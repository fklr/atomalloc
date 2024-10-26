use std::{alloc::Layout, pin::Pin, sync::Arc};

pub mod block;
mod cache;
pub mod config;
pub mod error;
mod manager;
mod pool;
mod stats;

use block::Block;
use cache::BlockCache;
use config::AtomAllocConfig;
use error::AtomAllocError;
use manager::BlockManager;
use pool::MemoryPool;
use stats::AtomAllocStats;

pub struct AtomAlloc {
    pool: Arc<MemoryPool>,
    cache: Arc<BlockCache>,
    block_manager: Arc<BlockManager>,
    stats: Arc<AtomAllocStats>,
    config: Arc<AtomAllocConfig>,
}

impl AtomAlloc {
    pub async fn new() -> Self {
        Self::with_config(AtomAllocConfig::default()).await
    }

    pub async fn with_config(config: AtomAllocConfig) -> Self {
        config.validate().expect("Invalid configuration");

        let config = Arc::new(config);
        let stats = Arc::new(AtomAllocStats::new().await);
        let pool = Arc::new(MemoryPool::new(&config, stats.clone()));
        let block_manager = Arc::new(BlockManager::new(&config).await);
        let cache = Arc::new(BlockCache::new(
            block_manager.clone(),
            pool.clone(),
            stats.clone(),
        ));

        smol::future::yield_now().await;

        Self {
            pool,
            cache,
            block_manager,
            stats,
            config,
        }
    }

    pub async fn allocate(&self, layout: Layout) -> Result<Pin<Arc<Block>>, AtomAllocError> {
        // Try cache first
        match self.cache.allocate(layout.size()).await {
            Ok(block) => {
                self.block_manager.verify_generation(&block).await?;
                self.stats.record_cache_hit().await;
                Ok(block)
            }
            Err(_) => {
                self.stats.record_cache_miss().await;
                // Allocate from pool - only pool should record allocation
                let generation = self.block_manager.new_generation().await;
                self.pool
                    .allocate_with_generation(layout.size(), generation)
                    .await
            }
        }
    }

    pub async fn deallocate(&self, block: Pin<Arc<Block>>) {
        self.cache.deallocate(block).await;
    }

    pub async fn stats(&self) -> Stats {
        Stats {
            allocated: self.stats.allocated_bytes().await,
            freed: self.stats.freed_bytes().await,
            current: self.stats.current_bytes().await,
            cache_hits: self.stats.cache_hits().await,
            cache_misses: self.stats.cache_misses().await,
        }
    }

    pub fn config(&self) -> &AtomAllocConfig {
        &self.config
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub allocated: usize,
    pub freed: usize,
    pub current: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}
