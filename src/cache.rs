use crate::error::AtomAllocError;
use crate::manager::BlockManager;
use crate::pool::MemoryPool;
use crate::{
    block::{Block, BlockOps},
    stats::AtomAllocStats,
};
use crossbeam::queue::SegQueue;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

pub struct SizeClass {
    size: usize,
    hot_queue: Arc<SegQueue<Pin<Arc<Block>>>>,
    cold_queue: Arc<SegQueue<Pin<Arc<Block>>>>,
    allocation_count: AtomicUsize,
}

impl SizeClass {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            hot_queue: Arc::new(SegQueue::new()),
            cold_queue: Arc::new(SegQueue::new()),
            allocation_count: AtomicUsize::new(0),
        }
    }

    pub async fn get_block(&self) -> Option<Pin<Arc<Block>>> {
        // Check hot queue with retry
        for _ in 0..2 {
            if let Some(block) = self.hot_queue.pop() {
                if block.size() == self.size && block.try_acquire() {
                    self.allocation_count.fetch_add(1, Ordering::Relaxed);
                    return Some(block);
                }
            }
        }

        // Try cold queue once
        if let Some(block) = self.cold_queue.pop() {
            if block.size() == self.size && block.try_acquire() {
                self.allocation_count.fetch_add(1, Ordering::Relaxed);
                return Some(block);
            }
        }

        None
    }

    pub async fn return_block(&self, block: Pin<Arc<Block>>) {
        let alloc_count = self.allocation_count.load(Ordering::Relaxed);

        // Ensure the block size matches the size class
        if block.size() != self.size {
            panic!("Block size does not match size class");
        }

        // Adaptive promotion based on allocation frequency
        if alloc_count & 7 == 0 {
            // Power of 2 mask
            self.hot_queue.push(block);
            println!("Returned block of size {} to hot queue", self.size);
        } else {
            self.cold_queue.push(block);
            println!("Returned block of size {} to cold queue", self.size);
        }
    }
}

pub struct BlockCache {
    manager: Arc<BlockManager>,
    pool: Arc<MemoryPool>,
    size_classes: Vec<Arc<SizeClass>>,
    stats: Arc<AtomAllocStats>,
}

impl BlockCache {
    // Power of 2 size classes for better alignment and less fragmentation
    const SIZE_CLASSES: &'static [usize] = &[32, 64, 128, 256, 512, 1024, 2048, 4096, 8192];

    pub fn new(
        manager: Arc<BlockManager>,
        pool: Arc<MemoryPool>,
        stats: Arc<AtomAllocStats>,
    ) -> Self {
        let size_classes = Self::SIZE_CLASSES
            .iter()
            .map(|&size| Arc::new(SizeClass::new(size)))
            .collect();

        Self {
            manager,
            pool,
            size_classes,
            stats,
        }
    }

    #[inline]
    fn get_size_class_index(&self, size: usize) -> Option<usize> {
        // Fast path for small sizes using trailing zeros
        if size <= 32 {
            return Some(0);
        }

        // Use size - 1 to handle exact power of 2 sizes
        let size_log2 = (size - 1).next_power_of_two().trailing_zeros() as usize;
        let index = size_log2.saturating_sub(5); // 32 is 2^5

        if index < Self::SIZE_CLASSES.len() {
            Some(index)
        } else {
            None
        }
    }

    pub async fn allocate(&self, size: usize) -> Result<Pin<Arc<Block>>, AtomAllocError> {
        println!("BlockCache: Attempting allocation of size {}", size);

        if let Some(class_idx) = self.get_size_class_index(size) {
            if let Some(block) = self.size_classes[class_idx].get_block().await {
                println!("BlockCache: Found block in size class {}", size);
                // Need to record allocation even for cached blocks
                self.stats.record_allocation(block.size()).await;
                self.stats.record_cache_hit().await;
                return Ok(block);
            }
        }

        println!("BlockCache: Cache miss for size {}", size);
        self.stats.record_cache_miss().await;

        let generation = self.manager.new_generation().await;
        let block = self.pool.allocate_with_generation(size, generation).await?;
        // Pool records its own allocation stats
        println!("BlockCache: Created new block of size {}", size);
        Ok(block)
    }

    pub async fn deallocate(&self, block: Pin<Arc<Block>>) {
        let size = block.size();
        block.release();
        self.manager.zero_block(&block).await;

        if let Some(class_idx) = self.get_size_class_index(size) {
            println!("BlockCache: Returning block of size {} to cache", size);
            self.size_classes[class_idx].return_block(block).await;
            self.stats.record_deallocation(size).await;
        } else {
            println!(
                "BlockCache: Block size {} doesn't match any size class, deallocating",
                size
            );
            self.pool.deallocate(block).await;
        }
    }
}
