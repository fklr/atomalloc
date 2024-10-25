use std::sync::{Arc, atomic::{AtomicU64, AtomicUsize, AtomicU8, Ordering}};
use std::pin::Pin;
use crate::error::BlockError;

// Pack flags into a single atomic
const IN_USE_FLAG: u64 = 1 << 63;
const ZEROED_FLAG: u64 = 1 << 62;

pub struct Block {
    state: AtomicU64,        // generation + flags
    size: AtomicUsize,
    data: Box<[AtomicU8]>,
}

pub trait BlockOps: Send + Sync {
    fn size(&self) -> usize;
    fn try_acquire(&self) -> bool;
    fn release(&self);
    fn generation(&self) -> u64;
}

impl Block {
    pub fn new(size: usize, generation: u64) -> Pin<Arc<Self>> {
        let state = generation | IN_USE_FLAG | ZEROED_FLAG;

        // Initialize data with capacity
        let data = (0..size)
            .map(|_| AtomicU8::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let block = Arc::new(Self {
            state: AtomicU64::new(state),
            size: AtomicUsize::new(size),
            data,
        });

        Pin::new(block)
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

        // Process in chunks for better cache efficiency
        const CHUNK_SIZE: usize = 1024;
        for chunk_start in (0..data.len()).step_by(CHUNK_SIZE) {
            let chunk_end = (chunk_start + CHUNK_SIZE).min(data.len());
            for (i, &byte) in data[chunk_start..chunk_end].iter().enumerate() {
                self.data[offset + chunk_start + i].store(byte, Ordering::Release);
            }
            tokio::task::yield_now().await;
        }

        self.state.fetch_and(!ZEROED_FLAG, Ordering::Release);
        Ok(())
    }

    pub async fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>, BlockError> {
        let size = self.size.load(Ordering::Acquire);
        if offset + len > size {
            return Err(BlockError::OutOfBounds {
                offset,
                len,
                size,
            });
        }

        let mut result = Vec::with_capacity(len);

        // Read in chunks for better cache efficiency
        const CHUNK_SIZE: usize = 1024;
        for chunk_start in (0..len).step_by(CHUNK_SIZE) {
            let chunk_end = (chunk_start + CHUNK_SIZE).min(len);
            for i in chunk_start..chunk_end {
                result.push(self.data[offset + i].load(Ordering::Acquire));
            }
            tokio::task::yield_now().await;
        }

        Ok(result)
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
}
