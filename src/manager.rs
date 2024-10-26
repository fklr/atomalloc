use crate::{
    block::{Block, BlockOps},
    config::AtomAllocConfig,
    error::{AtomAllocError, BlockError},
};
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub(crate) struct BlockManager {
    config: Arc<AtomAllocConfig>,
    current_generation: AtomicU64,
}

impl BlockManager {
    pub async fn new(config: &AtomAllocConfig) -> Self {
        Self {
            config: Arc::new(config.clone()),
            current_generation: AtomicU64::new(0),
        }
    }

    pub async fn verify_generation(&self, block: &Pin<Arc<Block>>) -> Result<(), AtomAllocError> {
        // Don't verify if zeroing is disabled
        if !self.config.zero_on_dealloc {
            return Ok(());
        }

        // Get generation when block was created
        let block_gen = block.generation();

        // Should be less than or equal to current generation
        let current_gen = self.current_generation.load(Ordering::Acquire);
        if block_gen > current_gen {
            return Err(AtomAllocError::BlockError(BlockError::InvalidGeneration {
                block: block_gen,
                expected: current_gen,
            }));
        }

        Ok(())
    }

    pub async fn new_generation(&self) -> u64 {
        self.current_generation.fetch_add(1, Ordering::AcqRel)
    }

    pub async fn zero_block(&self, block: &Pin<Arc<Block>>) {
        if self.config.zero_on_dealloc {
            block.clear().await;
        }
    }
}
