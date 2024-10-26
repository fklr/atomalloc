use std::time::Duration;

#[derive(Clone, Debug)]
pub struct AtomAllocConfig {
    // Memory limits
    pub max_memory: usize,
    pub max_block_size: usize,
    pub min_block_size: usize,
    pub alignment: usize,

    // Cache settings
    pub cache_ttl: Duration,
    pub max_caches: usize,
    pub initial_pool_size: usize,

    // Security settings
    pub zero_on_dealloc: bool,
}

impl Default for AtomAllocConfig {
    fn default() -> Self {
        Self {
            max_memory: 1024 * 1024 * 1024, // 1GB
            max_block_size: 64 * 1024,      // 64KB
            min_block_size: 64,             // 64B
            alignment: 16,

            cache_ttl: Duration::from_secs(300),
            max_caches: 1000,
            initial_pool_size: 1024 * 1024, // 1MB

            zero_on_dealloc: true,
        }
    }
}

impl AtomAllocConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_memory < self.initial_pool_size {
            return Err(format!(
                "max_memory ({}) must be >= initial_pool_size ({})",
                self.max_memory, self.initial_pool_size
            ));
        }

        if !self.min_block_size.is_power_of_two() {
            return Err(format!(
                "min_block_size ({}) must be a power of 2",
                self.min_block_size
            ));
        }

        if !self.max_block_size.is_power_of_two() {
            return Err(format!(
                "max_block_size ({}) must be a power of 2",
                self.max_block_size
            ));
        }

        if self.max_block_size < self.min_block_size {
            return Err(format!(
                "max_block_size ({}) must be >= min_block_size ({})",
                self.max_block_size, self.min_block_size
            ));
        }

        if !self.alignment.is_power_of_two() {
            return Err(format!(
                "alignment ({}) must be a power of 2",
                self.alignment
            ));
        }

        if self.max_caches == 0 {
            return Err("max_caches must be > 0".into());
        }

        Ok(())
    }

    pub fn get_default_for_tests() -> Self {
        Self {
            max_memory: 16 * 1024, // 16KB for tests
            max_block_size: 1024,  // 1KB
            min_block_size: 64,    // 64B
            alignment: 8,
            cache_ttl: Duration::from_secs(60),
            max_caches: 100,
            initial_pool_size: 4 * 1024, // 4KB
            zero_on_dealloc: true,
        }
    }
}
