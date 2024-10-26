use atomalloc::{block::BlockOps, config::AtomAllocConfig, error::AtomAllocError, AtomAlloc};
use macro_rules_attribute::apply;
use smol_macros::{test, Executor};
use std::{alloc::Layout, sync::Arc, time::Duration};

#[apply(test!)]
async fn test_basic_alloc_dealloc(ex: &Executor<'_>) {
    ex.spawn(async {
        let config = AtomAllocConfig::get_default_for_tests();
        let allocator = AtomAlloc::with_config(config).await;
        let layout = Layout::from_size_align(64, 8).unwrap();

        println!("\n=== Starting basic allocation test ===");

        // Get initial stats
        let stats_before = allocator.stats().await;
        println!("Initial stats: {:?}", stats_before);

        // Single allocation
        let block = allocator.allocate(layout).await.unwrap();
        let stats_after_alloc = allocator.stats().await;
        println!("Stats after allocation: {:?}", stats_after_alloc);
        assert_eq!(block.size(), 64);

        // Deallocate
        allocator.deallocate(block).await;
        let stats_final = allocator.stats().await;
        println!("Final stats: {:?}", stats_final);

        assert_eq!(stats_final.allocated - stats_before.allocated, 64);
        assert_eq!(stats_final.freed - stats_before.freed, 64);
    }).await;
}

#[apply(test!)]
async fn test_concurrent_allocations(ex: &Executor<'_>) {
    ex.spawn(async {
        let allocator = Arc::new(AtomAlloc::new().await);
        let mut handles = vec![];

        for _ in 0..100 {
            let allocator = Arc::clone(&allocator);
            handles.push(smol::spawn(async move {
                let layout = Layout::from_size_align(64, 8).unwrap();
                let block = allocator.allocate(layout).await.unwrap();
                smol::future::yield_now().await;
                allocator.deallocate(block).await;
            }));
        }

        for handle in handles {
            handle.await;
        }
    }).await;
}

#[apply(test!)]
async fn test_pool_reuse(ex: &Executor<'_>) {
    ex.spawn(async {
        let allocator = AtomAlloc::new().await;
        let layout = Layout::from_size_align(64, 8).unwrap();

        // First allocation
        let block1 = allocator.allocate(layout).await.unwrap();
        let gen1 = block1.generation();
        allocator.deallocate(block1).await;

        // Second allocation should reuse block
        let block2 = allocator.allocate(layout).await.unwrap();
        let gen2 = block2.generation();
        assert_eq!(gen1, gen2, "Block should be reused");
        allocator.deallocate(block2).await;
    }).await;
}

#[apply(test!)]
async fn test_memory_limits(ex: &Executor<'_>) {
    ex.spawn(async {
        // Configure with 2KB total but only 512B max block size
        let config = AtomAllocConfig {
            max_memory: 2048,    // 2KB total
            max_block_size: 512, // But max block still 512B
            min_block_size: 64,  // Min block 64B
            alignment: 8,
            cache_ttl: Duration::from_secs(60),
            max_caches: 100,
            initial_pool_size: 512,
            zero_on_dealloc: true,
        };

        println!("\n=== Starting memory limits test with size classes ===");
        println!("Config: {:?}", config);
        let allocator = AtomAlloc::with_config(config).await;

        // First 512B allocation
        let layout1 = Layout::from_size_align(400, 8).unwrap();
        let block1 = allocator.allocate(layout1).await.unwrap();
        let stats1 = allocator.stats().await;
        println!("Stats after first allocation (400B->512B): {:?}", stats1);
        assert_eq!(stats1.current, 512);

        // Try to allocate more than max block size
        let layout2 = Layout::from_size_align(600, 8).unwrap();
        let result = allocator.allocate(layout2).await;
        println!(
            "Second allocation result (600B): {:?}",
            result.as_ref().err()
        );
        assert!(
            matches!(result.as_ref(), Err(AtomAllocError::OutOfMemory)),
            "Expected OutOfMemory for too-large block"
        );

        allocator.deallocate(block1).await;
    }).await;
}

#[apply(test!)]
async fn test_strict_memory_limits(ex: &Executor<'_>) {
    ex.spawn(async {
        // Configure for 1KB total, but effectively ~768B usable
        let config = AtomAllocConfig {
            max_memory: 1024,    // 1KB total limit
            max_block_size: 512, // Largest block is 512B
            min_block_size: 64,  // Smallest block is 64B
            alignment: 8,
            cache_ttl: Duration::from_secs(60),
            max_caches: 100,
            initial_pool_size: 512,
            zero_on_dealloc: true,
        };

        println!("\n=== Starting strict memory limit test ===");
        let allocator = AtomAlloc::with_config(config).await;

        // First 512B allocation should work
        let layout1 = Layout::from_size_align(300, 8).unwrap();
        let block1 = allocator.allocate(layout1).await.unwrap();
        let stats1 = allocator.stats().await;
        println!("After first allocation (300B->512B): {:?}", stats1);
        assert_eq!(stats1.current, 512);

        // Second 512B should fail due to effective memory limit
        let layout2 = Layout::from_size_align(400, 8).unwrap();
        let result = allocator.allocate(layout2).await;
        println!(
            "Second allocation result (400B->512B): {:?}",
            result.as_ref().err()
        );
        assert!(matches!(result.as_ref(), Err(AtomAllocError::OutOfMemory)));

        // Free first block
        allocator.deallocate(block1).await;
        let stats2 = allocator.stats().await;
        println!("After deallocation: {:?}", stats2);
        assert_eq!(stats2.current, 0);

        // Now we can allocate again
        let block2 = allocator.allocate(layout2).await.unwrap();
        let stats3 = allocator.stats().await;
        println!("After final allocation: {:?}", stats3);
        assert_eq!(stats3.current, 512);

        allocator.deallocate(block2).await;
    }).await;
}
