use crate::error::BlockError;
use std::pin::Pin;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering},
        Arc,
    },
};

// Pack flags into a single atomic
const IN_USE_FLAG: u64 = 1 << 63;
const ZEROED_FLAG: u64 = 1 << 62;

pub struct Block {
    state: AtomicU64, // generation + flags
    size: AtomicUsize,
    data: Box<[AtomicU8]>,
}

pub trait BlockOps: Send + Sync {
    fn size(&self) -> usize;
    fn try_acquire(&self) -> bool;
    fn release(&self);
    fn generation(&self) -> u64;
    fn clear(self: Pin<&Arc<Self>>) -> impl Future<Output = ()> + Send + 'static;
}

impl Block {
    pub fn new(size: usize, generation: u64) -> Pin<Arc<Self>> {
        let state = AtomicU64::new(generation);
        let size_atomic = AtomicUsize::new(size);
        let data = (0..size)
            .map(|_| AtomicU8::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Pin::new(Arc::new(Self {
            state,
            size: size_atomic,
            data,
        }))
    }

    pub async fn write(&self, offset: usize, data: &[u8]) -> Result<(), BlockError> {
        let size = self.size.load(Ordering::Acquire);
        if offset + data.len() > size {
            return Err(BlockError::OutOfBounds {
                offset,
                len: data.len(),
                size,
            });
        }

        // Process in chunks for cache efficiency
        const CHUNK_SIZE: usize = 1024;
        for chunk_start in (0..data.len()).step_by(CHUNK_SIZE) {
            let chunk_end = (chunk_start + CHUNK_SIZE).min(data.len());
            for (i, &byte) in data[chunk_start..chunk_end].iter().enumerate() {
                self.data[offset + chunk_start + i].store(byte, Ordering::Release);
            }
            smol::future::yield_now().await;
        }

        self.state.fetch_and(!ZEROED_FLAG, Ordering::Release);
        Ok(())
    }

    pub async fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>, BlockError> {
        let size = self.size.load(Ordering::Acquire);
        if offset + len > size {
            return Err(BlockError::OutOfBounds { offset, len, size });
        }

        let mut result = Vec::with_capacity(len);

        // Read in chunks for cache efficiency
        const CHUNK_SIZE: usize = 1024;
        for chunk_start in (0..len).step_by(CHUNK_SIZE) {
            let chunk_end = (chunk_start + CHUNK_SIZE).min(len);
            for i in chunk_start..chunk_end {
                result.push(self.data[offset + i].load(Ordering::Acquire));
            }
            smol::future::yield_now().await;
        }

        Ok(result)
    }

    pub fn update_generation(&self, new_gen: u64) {
        let current = self.state.load(Ordering::Acquire);
        let flags = current & (IN_USE_FLAG | ZEROED_FLAG);
        let new_state = new_gen | flags;
        self.state.store(new_state, Ordering::Release);
    }

    pub async fn clear(&self) {
        // Clear in chunks for async friendliness
        const CHUNK_SIZE: usize = 1024;
        let size = self.size();

        for offset in (0..size).step_by(CHUNK_SIZE) {
            let end = (offset + CHUNK_SIZE).min(size);
            for i in offset..end {
                self.data[i].store(0, Ordering::Release);
            }
            smol::future::yield_now().await;
        }

        self.state.fetch_or(ZEROED_FLAG, Ordering::Release);
    }
}

impl BlockOps for Block {
    fn size(&self) -> usize {
        self.size.load(Ordering::Acquire)
    }

    fn try_acquire(&self) -> bool {
        let current = self.state.fetch_or(IN_USE_FLAG, Ordering::AcqRel);
        current & IN_USE_FLAG == 0
    }

    fn release(&self) {
        self.state.fetch_and(!IN_USE_FLAG, Ordering::Release);
    }

    fn generation(&self) -> u64 {
        self.state.load(Ordering::Acquire) & !(IN_USE_FLAG | ZEROED_FLAG)
    }

    fn clear(self: Pin<&Arc<Self>>) -> impl Future<Output = ()> + Send + 'static {
        // Clone the Arc for the async block
        let block = Arc::clone(self.get_ref());

        async move {
            const CHUNK_SIZE: usize = 1024;
            let size = block.size();

            for offset in (0..size).step_by(CHUNK_SIZE) {
                let end = (offset + CHUNK_SIZE).min(size);
                for i in offset..end {
                    block.data[i].store(0, Ordering::Release);
                }
                smol::future::yield_now().await;
            }

            block.state.fetch_or(ZEROED_FLAG, Ordering::Release);
        }
    }
}
