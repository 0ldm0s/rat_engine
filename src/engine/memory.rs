//! 内存池管理模块
//! 
//! 实现高效的内存分配和复用策略：
//! - 预分配缓冲区池
//! - 无锁内存回收
//! - 分层内存管理
//! - 内存使用统计

use bytes::BytesMut;
use crossbeam::queue::SegQueue;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 内存池
/// 
/// 管理不同大小的缓冲区池，减少内存分配开销
pub struct MemoryPool {
    /// 小缓冲区池 (1KB)
    small_buffers: SegQueue<BytesMut>,
    /// 中等缓冲区池 (8KB)
    medium_buffers: SegQueue<BytesMut>,
    /// 大缓冲区池 (64KB)
    large_buffers: SegQueue<BytesMut>,
    /// 超大缓冲区池 (1MB)
    xlarge_buffers: SegQueue<BytesMut>,
    
    /// 配置
    config: MemoryPoolConfig,
    
    /// 统计信息
    stats: MemoryPoolStats,
}

/// 内存池配置
#[derive(Debug, Clone)]
pub struct MemoryPoolConfig {
    /// 小缓冲区大小 (1KB)
    pub small_size: usize,
    /// 中等缓冲区大小 (8KB)
    pub medium_size: usize,
    /// 大缓冲区大小 (64KB)
    pub large_size: usize,
    /// 超大缓冲区大小 (1MB)
    pub xlarge_size: usize,
    
    /// 每个池的初始容量
    pub initial_capacity: usize,
    /// 每个池的最大容量
    pub max_capacity: usize,
    
    /// 是否启用统计
    pub enable_stats: bool,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            small_size: 1024,        // 1KB
            medium_size: 8192,       // 8KB
            large_size: 65536,       // 64KB
            xlarge_size: 1048576,    // 1MB
            initial_capacity: 100,
            max_capacity: 1000,
            enable_stats: true,
        }
    }
}

/// 内存池统计信息
#[derive(Debug)]
struct MemoryPoolStats {
    /// 分配计数
    allocations: AtomicU64,
    /// 回收计数
    deallocations: AtomicU64,
    /// 缓存命中计数
    cache_hits: AtomicU64,
    /// 缓存未命中计数
    cache_misses: AtomicU64,
    
    /// 当前使用的内存字节数
    bytes_in_use: AtomicU64,
    /// 峰值内存使用
    peak_bytes_in_use: AtomicU64,
    
    /// 各池的当前大小
    small_pool_size: AtomicUsize,
    medium_pool_size: AtomicUsize,
    large_pool_size: AtomicUsize,
    xlarge_pool_size: AtomicUsize,
}

impl MemoryPoolStats {
    fn new() -> Self {
        Self {
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            bytes_in_use: AtomicU64::new(0),
            peak_bytes_in_use: AtomicU64::new(0),
            small_pool_size: AtomicUsize::new(0),
            medium_pool_size: AtomicUsize::new(0),
            large_pool_size: AtomicUsize::new(0),
            xlarge_pool_size: AtomicUsize::new(0),
        }
    }
    
    fn record_allocation(&self, size: usize, from_cache: bool) {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        
        if from_cache {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
        
        let new_bytes = self.bytes_in_use.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
        
        // 更新峰值使用量
        let mut current_peak = self.peak_bytes_in_use.load(Ordering::Relaxed);
        while new_bytes > current_peak {
            match self.peak_bytes_in_use.compare_exchange_weak(
                current_peak,
                new_bytes,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_peak = actual,
            }
        }
    }
    
    fn record_deallocation(&self, size: usize) {
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.bytes_in_use.fetch_sub(size as u64, Ordering::Relaxed);
    }
}

impl MemoryPool {
    /// 创建新的内存池
    pub fn new(default_buffer_size: usize) -> Self {
        let config = MemoryPoolConfig {
            medium_size: default_buffer_size,
            ..Default::default()
        };
        
        Self::with_config(config)
    }
    
    /// 使用指定配置创建内存池
    pub fn with_config(config: MemoryPoolConfig) -> Self {
        let pool = Self {
            small_buffers: SegQueue::new(),
            medium_buffers: SegQueue::new(),
            large_buffers: SegQueue::new(),
            xlarge_buffers: SegQueue::new(),
            stats: MemoryPoolStats::new(),
            config,
        };
        
        // 预分配初始缓冲区
        pool.preallocate_buffers();
        
        pool
    }
    
    /// 预分配缓冲区
    fn preallocate_buffers(&self) {
        // 预分配各种大小的缓冲区
        for _ in 0..self.config.initial_capacity {
            self.small_buffers.push(BytesMut::with_capacity(self.config.small_size));
            self.medium_buffers.push(BytesMut::with_capacity(self.config.medium_size));
            self.large_buffers.push(BytesMut::with_capacity(self.config.large_size));
        }
        
        // 超大缓冲区预分配较少
        for _ in 0..(self.config.initial_capacity / 10).max(1) {
            self.xlarge_buffers.push(BytesMut::with_capacity(self.config.xlarge_size));
        }
        
        // 更新统计
        self.stats.small_pool_size.store(self.config.initial_capacity, Ordering::Relaxed);
        self.stats.medium_pool_size.store(self.config.initial_capacity, Ordering::Relaxed);
        self.stats.large_pool_size.store(self.config.initial_capacity, Ordering::Relaxed);
        self.stats.xlarge_pool_size.store(self.config.initial_capacity / 10, Ordering::Relaxed);
    }
    
    /// 获取缓冲区
    pub fn get_buffer(&self) -> BytesMut {
        self.get_buffer_with_size(self.config.medium_size)
    }
    
    /// 获取指定大小的缓冲区
    pub fn get_buffer_with_size(&self, size: usize) -> BytesMut {
        let (buffer, actual_size, from_cache) = if size <= self.config.small_size {
            self.get_from_pool(&self.small_buffers, self.config.small_size, &self.stats.small_pool_size)
        } else if size <= self.config.medium_size {
            self.get_from_pool(&self.medium_buffers, self.config.medium_size, &self.stats.medium_pool_size)
        } else if size <= self.config.large_size {
            self.get_from_pool(&self.large_buffers, self.config.large_size, &self.stats.large_pool_size)
        } else if size <= self.config.xlarge_size {
            self.get_from_pool(&self.xlarge_buffers, self.config.xlarge_size, &self.stats.xlarge_pool_size)
        } else {
            // 超大请求，直接分配
            (BytesMut::with_capacity(size), size, false)
        };
        
        if self.config.enable_stats {
            self.stats.record_allocation(actual_size, from_cache);
        }
        
        buffer
    }
    
    /// 从指定池获取缓冲区
    fn get_from_pool(
        &self,
        pool: &SegQueue<BytesMut>,
        pool_size: usize,
        pool_counter: &AtomicUsize,
    ) -> (BytesMut, usize, bool) {
        if let Some(mut buffer) = pool.pop() {
            pool_counter.fetch_sub(1, Ordering::Relaxed);
            buffer.clear(); // 清空内容但保留容量
            (buffer, pool_size, true)
        } else {
            // 池为空，分配新缓冲区
            (BytesMut::with_capacity(pool_size), pool_size, false)
        }
    }
    
    /// 归还缓冲区
    pub fn return_buffer(&self, buffer: BytesMut) {
        let capacity = buffer.capacity();
        
        // 根据容量归还到对应的池
        let (pool, pool_counter, max_size) = if capacity <= self.config.small_size * 2 {
            (&self.small_buffers, &self.stats.small_pool_size, self.config.small_size)
        } else if capacity <= self.config.medium_size * 2 {
            (&self.medium_buffers, &self.stats.medium_pool_size, self.config.medium_size)
        } else if capacity <= self.config.large_size * 2 {
            (&self.large_buffers, &self.stats.large_pool_size, self.config.large_size)
        } else if capacity <= self.config.xlarge_size * 2 {
            (&self.xlarge_buffers, &self.stats.xlarge_pool_size, self.config.xlarge_size)
        } else {
            // 太大的缓冲区直接丢弃
            if self.config.enable_stats {
                self.stats.record_deallocation(capacity);
            }
            return;
        };
        
        // 检查池是否已满
        let current_size = pool_counter.load(Ordering::Relaxed);
        if current_size < self.config.max_capacity {
            pool.push(buffer);
            pool_counter.fetch_add(1, Ordering::Relaxed);
        }
        
        if self.config.enable_stats {
            self.stats.record_deallocation(max_size);
        }
    }
    
    /// 获取统计信息
    pub fn get_stats(&self) -> MemoryPoolStatsSnapshot {
        MemoryPoolStatsSnapshot {
            allocations: self.stats.allocations.load(Ordering::Relaxed),
            deallocations: self.stats.deallocations.load(Ordering::Relaxed),
            cache_hits: self.stats.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.stats.cache_misses.load(Ordering::Relaxed),
            bytes_in_use: self.stats.bytes_in_use.load(Ordering::Relaxed),
            peak_bytes_in_use: self.stats.peak_bytes_in_use.load(Ordering::Relaxed),
            small_pool_size: self.stats.small_pool_size.load(Ordering::Relaxed),
            medium_pool_size: self.stats.medium_pool_size.load(Ordering::Relaxed),
            large_pool_size: self.stats.large_pool_size.load(Ordering::Relaxed),
            xlarge_pool_size: self.stats.xlarge_pool_size.load(Ordering::Relaxed),
        }
    }
    
    /// 清理池中的缓冲区
    pub fn cleanup(&self) {
        // 清空所有池
        while self.small_buffers.pop().is_some() {}
        while self.medium_buffers.pop().is_some() {}
        while self.large_buffers.pop().is_some() {}
        while self.xlarge_buffers.pop().is_some() {}
        
        // 重置计数器
        self.stats.small_pool_size.store(0, Ordering::Relaxed);
        self.stats.medium_pool_size.store(0, Ordering::Relaxed);
        self.stats.large_pool_size.store(0, Ordering::Relaxed);
        self.stats.xlarge_pool_size.store(0, Ordering::Relaxed);
    }
    
    /// 获取总内存使用量
    pub fn total_memory_usage(&self) -> usize {
        let stats = self.get_stats();
        (stats.small_pool_size * self.config.small_size) +
        (stats.medium_pool_size * self.config.medium_size) +
        (stats.large_pool_size * self.config.large_size) +
        (stats.xlarge_pool_size * self.config.xlarge_size)
    }
}

/// 内存池统计信息快照
#[derive(Debug, Clone)]
pub struct MemoryPoolStatsSnapshot {
    pub allocations: u64,
    pub deallocations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub bytes_in_use: u64,
    pub peak_bytes_in_use: u64,
    pub small_pool_size: usize,
    pub medium_pool_size: usize,
    pub large_pool_size: usize,
    pub xlarge_pool_size: usize,
}

impl MemoryPoolStatsSnapshot {
    /// 计算缓存命中率
    pub fn cache_hit_rate(&self) -> f64 {
        let total_requests = self.cache_hits + self.cache_misses;
        if total_requests > 0 {
            self.cache_hits as f64 / total_requests as f64
        } else {
            0.0
        }
    }
    
    /// 计算内存效率
    pub fn memory_efficiency(&self) -> f64 {
        if self.allocations > 0 {
            self.deallocations as f64 / self.allocations as f64
        } else {
            0.0
        }
    }
}

impl std::fmt::Display for MemoryPoolStatsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MemoryPool Stats: allocs={}, deallocs={}, cache_hit_rate={:.2}%, bytes_in_use={}, peak={}, pools=[{},{},{},{}]",
            self.allocations,
            self.deallocations,
            self.cache_hit_rate() * 100.0,
            self.bytes_in_use,
            self.peak_bytes_in_use,
            self.small_pool_size,
            self.medium_pool_size,
            self.large_pool_size,
            self.xlarge_pool_size
        )
    }
}

/// RAII 缓冲区包装器
/// 
/// 自动归还缓冲区到池中
pub struct PooledBuffer {
    buffer: Option<BytesMut>,
    pool: Arc<MemoryPool>,
}

impl PooledBuffer {
    pub fn new(pool: Arc<MemoryPool>, size: usize) -> Self {
        let buffer = pool.get_buffer_with_size(size);
        Self {
            buffer: Some(buffer),
            pool,
        }
    }
    
    pub fn get_mut(&mut self) -> &mut BytesMut {
        self.buffer.as_mut().expect("池化缓冲区为空：可能已被其他线程借用")
    }
    
    pub fn get(&self) -> &BytesMut {
        self.buffer.as_ref().expect("池化缓冲区为空：可能已被其他线程借用")
    }
    
    pub fn take(mut self) -> BytesMut {
        self.buffer.take().expect("池化缓冲区为空：可能已被其他线程借用")
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.return_buffer(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_pool_basic() {
        let pool = MemoryPool::new(8192);
        
        // 获取缓冲区
        let buffer1 = pool.get_buffer();
        assert_eq!(buffer1.capacity(), 8192);
        
        // 归还缓冲区
        pool.return_buffer(buffer1);
        
        // 再次获取应该复用
        let buffer2 = pool.get_buffer();
        assert_eq!(buffer2.capacity(), 8192);
        
        let stats = pool.get_stats();
        assert!(stats.cache_hits > 0);
    }
    
    #[test]
    fn test_different_sizes() {
        let pool = MemoryPool::with_config(MemoryPoolConfig::default());
        
        let small = pool.get_buffer_with_size(512);
        let medium = pool.get_buffer_with_size(4096);
        let large = pool.get_buffer_with_size(32768);
        
        assert!(small.capacity() >= 512);
        assert!(medium.capacity() >= 4096);
        assert!(large.capacity() >= 32768);
        
        pool.return_buffer(small);
        pool.return_buffer(medium);
        pool.return_buffer(large);
    }
    
    #[test]
    fn test_pooled_buffer() {
        let pool = Arc::new(MemoryPool::new(1024));
        
        {
            let mut pooled = PooledBuffer::new(pool.clone(), 1024);
            pooled.get_mut().extend_from_slice(b"test data");
            assert_eq!(pooled.get().len(), 9);
        } // 自动归还
        
        let stats = pool.get_stats();
        assert_eq!(stats.allocations, 1);
        assert_eq!(stats.deallocations, 1);
    }
}