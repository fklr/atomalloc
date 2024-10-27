# ⚛️ AtomAlloc ⚛️

[![Build](https://github.com/ovnanova/atomalloc/actions/workflows/rust.yml/badge.svg)](https://github.com/ovnanova/atomalloc/actions/workflows/rust.yml) [![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

An experimental async-first memory allocator exploring atomic & lock-free allocation patterns in Rust.

> **Research Implementation**: This is an experimental allocator focused on exploring async memory management patterns. It is NOT optimized for production use and currently exhibits higher overhead than traditional allocators.

## Overview

AtomAlloc investigates the intersection of async Rust, atomic operations, and memory management by implementing a fully lock-free, task-aware allocator. While performance is not yet competitive with production allocators, it provides insights into async allocation patterns and challenges.

## Design Goals vs Reality

### ✅ Successfully Achieved
- 🛡️ **Zero Unsafe Code**: Fully safe Rust implementation
- 🔓 **Lock-Free Design**: Atomic operations throughout
- 💾 **Task Awareness**: Generations and task-local caching
- 🔄 **Async Interface**: Full async/await support

### ❌ Current Limitations
- **Performance**: slower than system allocator for common cases
- **Memory Overhead**: higher memory usage due to atomic metadata
- **Cache Efficiency**: poor cache locality from atomic operations
- **Cold Start**: initial allocation overhead from block initialization
- **Compatibility**: incompatible with GlobalAlloc trait or the unstable allocator_api feature

## Usage

If you'd like to experiment with this allocator, download this repo.

Basic usage:

```rust
// Create allocator instance
let alloc = AtomAlloc::new().await;

// Allocation
let layout = Layout::new::<[u8; 1024]>();
let block = alloc.allocate(layout).await?;

// Write data
block.write(0, &[1, 2, 3, 4]).await?;

// Read data
let data = block.read(0, 4).await?;

// Deallocation
alloc.deallocate(block).await;

// Get allocation stats
let stats = alloc.stats().await;
println!("Cache hit rate: {}%",
    stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64 * 100.0);
```

## Configuration

The allocator can be configured via `AtomAllocConfig`:

```rust
let config = AtomAllocConfig {
    max_memory: 1024 * 1024 * 1024, // 1GB
    max_block_size: 64 * 1024,      // 64KB
    min_block_size: 64,             // 64B
    alignment: 16,
    cache_ttl: Duration::from_secs(300),
    max_caches: 1000,
    initial_pool_size: 1024 * 1024, // 1MB
    zero_on_dealloc: true,
};

let alloc = AtomAlloc::with_config(config).await;
```

## Technical Architecture

### Core Components

```rust
pub struct AtomAlloc {
    pool: Arc<MemoryPool>,
    cache: Arc<BlockCache>,
    block_manager: Arc<BlockManager>,
    stats: Arc<AtomAllocStats>,
    config: Arc<AtomAllocConfig>,
}

pub struct Block {
    state: AtomicU64,      // Packs generation + flags
    size: AtomicUsize,     // Block size
    data: Box<[AtomicU8]>, // Actual memory storage
}
```

### Memory Model

Allocation follows a three-tier hierarchy:
1. Block Cache with Hot/Cold Queues
2. Size Class Pool with Power-of-2 Classes
3. Global Memory Pool

Each level uses atomic operations and lock-free data structures for synchronization.

### Generation Safety

```rust
impl BlockManager {
    pub async fn verify_generation(&self, block: &Pin<Arc<Block>>) -> Result<(), AtomAllocError> {
        let block_gen = block.generation();
        let current_gen = self.current_generation.load(Ordering::Acquire);

        if block_gen > current_gen {
            return Err(AtomAllocError::BlockError(BlockError::InvalidGeneration {
                block: block_gen,
                expected: current_gen,
            }));
        }
        Ok(())
    }
}
```

Memory safety is enforced through a generation system that tracks block validity.

## Critical Implementation Challenges

### 1. Cache Efficiency

The block cache implements a hot/cold queue system to balance reuse and memory pressure:

```rust
pub struct BlockCache {
    manager: Arc<BlockManager>,
    pool: Arc<MemoryPool>,
    size_classes: Vec<Arc<SizeClass>>,
    stats: Arc<AtomAllocStats>,
}
```

### 2. Memory Ordering

Ensuring correct ordering without locks requires careful atomic operations:

```rust
pub struct Block {
    pub async fn write(&self, offset: usize, data: &[u8]) -> Result<(), BlockError> {
        let size = self.size.load(Ordering::Acquire);
        // Atomic writes with proper ordering
        self.state.fetch_and(!ZEROED_FLAG, Ordering::Release);
        Ok(())
    }
}
```

### 3. Zero-on-Free Overhead

Memory zeroing for security has performance implications:

```rust
async fn zero_block(&self, block: &Pin<Arc<Block>>) {
    if self.config.zero_on_dealloc {
        block.clear().await;
    }
}
```

## Further Improvements

Current areas of investigation:
1. Cache-friendly atomic operations
2. Improved size class distribution
3. Memory coalescing techniques
4. Alternative cache hierarchies
5. Reduce generation verification overhead

## Contributing

This is an experiment that I would like to improve further over time.

I welcome contributions that would:
- Explore new async allocation patterns
- Improve performance characteristics

## License

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](LICENSE)

## Why This Exists

This allocator serves as an investigation into several questions:
- Can we build a fully async-first allocator?
- What are the real costs of lock-free memory management?
- How do we handle task isolation efficiently?
- What patterns emerge in async memory usage?

While the current implementation is not performance-competitive, I think it offers some insights on where to go next.

## Final Disclaimer

This was built as an exploration of the boundaries of async Rust. Don't use this in production unless you enjoy debugging memory allocation patterns more than having a functional application.

---
*Last updated: October 26, 2024*
