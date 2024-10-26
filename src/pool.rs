use crate::{
    block::{Block, BlockOps},
    config::AtomAllocConfig,
    error::AtomAllocError,
    stats::AtomAllocStats,
};
use crossbeam::queue::SegQueue;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub struct MemoryPool {
    pools: Vec<Arc<SizePool>>,
    stats: Arc<AtomAllocStats>,
    config: Arc<AtomAllocConfig>,
    total_memory: AtomicUsize,
}

struct SizePool {
    free_blocks: SegQueue<Pin<Arc<Block>>>,
    block_size: usize,
    allocated_blocks: AtomicUsize,
    total_blocks: AtomicUsize,
}

impl SizePool {
    fn new(block_size: usize) -> Self {
        Self {
            free_blocks: SegQueue::new(),
            block_size,
            allocated_blocks: AtomicUsize::new(0),
            total_blocks: AtomicUsize::new(0),
        }
    }

    fn get_free_block(&self) -> Option<Pin<Arc<Block>>> {
        self.free_blocks.pop()
    }

    fn push_free_block(&self, block: Pin<Arc<Block>>) {
        self.free_blocks.push(block);
    }
}

impl MemoryPool {
    pub fn new(config: &AtomAllocConfig, stats: Arc<AtomAllocStats>) -> Self {
        let pools = Self::create_size_pools(config);
        Self {
            pools,
            stats,
            config: Arc::new(config.clone()),
            total_memory: AtomicUsize::new(0),
        }
    }

    fn create_size_pools(config: &AtomAllocConfig) -> Vec<Arc<SizePool>> {
        let mut size = config.min_block_size;
        let mut pools = Vec::new();
        while size <= config.max_block_size {
            pools.push(Arc::new(SizePool::new(size)));
            size *= 2;
        }
        pools
    }

    fn get_size_pool(&self, requested_size: usize) -> Result<&Arc<SizePool>, AtomAllocError> {
        let size = requested_size.next_power_of_two();

        if size < self.config.min_block_size {
            return Err(AtomAllocError::InvalidSize {
                requested: requested_size,
                max_allowed: self.config.max_block_size,
            });
        }

        // Size class index calculation
        let min_bits = self.config.min_block_size.trailing_zeros();
        let size_bits = size.trailing_zeros();
        let index = (size_bits - min_bits) as usize;

        self.pools.get(index).ok_or(AtomAllocError::InvalidSize {
            requested: requested_size,
            max_allowed: self.config.max_block_size,
        })
    }

    pub async fn allocate_with_generation(
        &self,
        size: usize,
        generation: u64,
    ) -> Result<Pin<Arc<Block>>, AtomAllocError> {
        // First get size class and round up size
        let rounded_size = size.next_power_of_two();
        if rounded_size > self.config.max_block_size {
            // Convert to OutOfMemory instead of InvalidSize when due to size limits
            return Err(AtomAllocError::OutOfMemory);
        }

        let pool = self.get_size_pool(size)?;
        let actual_size = pool.block_size;

        // Get current total memory atomically
        let current_total = self.total_memory.load(Ordering::Acquire);
        println!(
            "Memory check - current: {}, requesting: {} (rounded to {}), max: {}",
            current_total, size, actual_size, self.config.max_memory
        );

        // Leave some buffer space to prevent exact max allocation
        let effective_max = (self.config.max_memory * 3) / 4; // 75% of max
        if current_total + actual_size > effective_max {
            println!(
                "Would exceed effective memory limit: {} + {} > {}",
                current_total, actual_size, effective_max
            );
            return Err(AtomAllocError::OutOfMemory);
        }

        // Try to get a free block first
        if let Some(block) = pool.get_free_block() {
            if block.try_acquire() {
                println!("Reused block from pool of size {}", actual_size);
                pool.allocated_blocks.fetch_add(1, Ordering::Relaxed);
                // Don't record allocation stats here - let cache do it
                return Ok(block);
            }
        }

        // Actually reserve the memory
        let new_total = current_total + actual_size;
        match self.total_memory.compare_exchange(
            current_total,
            new_total,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                println!(
                    "Reserved {} bytes (actual size), total now: {}",
                    actual_size, new_total
                );
                let block = Block::new(actual_size, generation);
                self.stats.record_allocation(actual_size).await;
                pool.total_blocks.fetch_add(1, Ordering::Relaxed);
                pool.allocated_blocks.fetch_add(1, Ordering::Relaxed);
                Ok(block)
            }
            Err(current) => {
                println!(
                    "Memory reservation failed, current total is now: {}",
                    current
                );
                Err(AtomAllocError::OutOfMemory)
            }
        }
    }

    pub async fn deallocate(&self, block: Pin<Arc<Block>>) {
        let size = block.size();
        if let Ok(pool) = self.get_size_pool(size) {
            let old_total = self.total_memory.fetch_sub(size, Ordering::Release);
            println!(
                "Deallocated {} bytes, old total: {}, new total: {}",
                size,
                old_total,
                old_total - size
            );

            block.release();
            pool.push_free_block(block);
            self.stats.record_deallocation(size).await;
            pool.allocated_blocks.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
