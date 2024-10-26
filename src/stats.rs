use std::sync::atomic::{AtomicUsize, Ordering};

pub struct AtomAllocStats {
    total_allocated: AtomicUsize,
    total_freed: AtomicUsize,
    current_allocated: AtomicUsize,
    cache_hits: AtomicUsize,
    cache_misses: AtomicUsize,
}

impl AtomAllocStats {
    pub async fn new() -> Self {
        Self {
            total_allocated: AtomicUsize::new(0),
            total_freed: AtomicUsize::new(0),
            current_allocated: AtomicUsize::new(0),
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
        }
    }

    // Stats recording - all async to maintain consistency
    pub async fn record_allocation(&self, size: usize) {
        let prev_total = self.total_allocated.fetch_add(size, Ordering::Release);
        let prev_current = self.current_allocated.fetch_add(size, Ordering::Release);
        println!("Recording allocation: prev_total={}, prev_current={}, size={}, new_total={}, new_current={}",
                prev_total, prev_current, size, prev_total + size, prev_current + size);
    }

    pub async fn record_deallocation(&self, size: usize) {
        let prev_freed = self.total_freed.fetch_add(size, Ordering::Release);
        let prev_current = self.current_allocated.fetch_sub(size, Ordering::Release);
        println!("Recording deallocation: prev_freed={}, prev_current={}, size={}, new_freed={}, new_current={}",
                prev_freed, prev_current, size, prev_freed + size, prev_current - size);
    }

    pub async fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Release);
        smol::future::yield_now().await;
    }

    pub async fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Release);
        smol::future::yield_now().await;
    }

    // Stats retrieval
    pub async fn allocated_bytes(&self) -> usize {
        let result = self.total_allocated.load(Ordering::Acquire);
        smol::future::yield_now().await;
        result
    }

    pub async fn freed_bytes(&self) -> usize {
        let result = self.total_freed.load(Ordering::Acquire);
        smol::future::yield_now().await;
        result
    }

    pub async fn current_bytes(&self) -> usize {
        let result = self.current_allocated.load(Ordering::Acquire);
        smol::future::yield_now().await;
        result
    }

    pub async fn cache_hits(&self) -> usize {
        let result = self.cache_hits.load(Ordering::Acquire);
        smol::future::yield_now().await;
        result
    }

    pub async fn cache_misses(&self) -> usize {
        let result = self.cache_misses.load(Ordering::Acquire);
        smol::future::yield_now().await;
        result
    }
}
