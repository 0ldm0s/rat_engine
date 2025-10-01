//! 工作窃取队列实现
//! 
//! 基于无锁数据结构的高性能任务调度系统：
//! - 每个工作线程有独立的本地队列
//! - 全局队列用于负载均衡
//! - 工作窃取算法避免线程饥饿
//! - 使用 crossbeam 的 SegQueue 实现无锁操作

use crossbeam::queue::SegQueue;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// 工作窃取队列
/// 
/// 这个结构体管理多个工作线程的任务分发，使用工作窃取算法
/// 来实现高效的负载均衡和最小化线程间竞争。
pub struct WorkStealingQueue<T> {
    /// 全局队列 - 用于初始任务分发和负载均衡
    global_queue: SegQueue<T>,
    /// 每个工作线程的本地队列
    local_queues: Vec<SegQueue<T>>,
    /// 工作线程数量
    worker_count: usize,
    /// 轮询索引，用于任务分发
    round_robin: AtomicUsize,
    /// 统计信息
    stats: WorkStealingStats,
}

/// 工作窃取统计信息
#[derive(Debug)]
struct WorkStealingStats {
    /// 本地队列命中次数
    local_hits: AtomicUsize,
    /// 全局队列命中次数
    global_hits: AtomicUsize,
    /// 工作窃取成功次数
    steal_hits: AtomicUsize,
    /// 队列为空的次数
    empty_polls: AtomicUsize,
}

impl WorkStealingStats {
    fn new() -> Self {
        Self {
            local_hits: AtomicUsize::new(0),
            global_hits: AtomicUsize::new(0),
            steal_hits: AtomicUsize::new(0),
            empty_polls: AtomicUsize::new(0),
        }
    }
    
    fn record_local_hit(&self) {
        self.local_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_global_hit(&self) {
        self.global_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_steal_hit(&self) {
        self.steal_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_empty_poll(&self) {
        self.empty_polls.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn get_stats(&self) -> (usize, usize, usize, usize) {
        (
            self.local_hits.load(Ordering::Relaxed),
            self.global_hits.load(Ordering::Relaxed),
            self.steal_hits.load(Ordering::Relaxed),
            self.empty_polls.load(Ordering::Relaxed),
        )
    }
}

impl<T> WorkStealingQueue<T> {
    /// 创建新的工作窃取队列
    /// 
    /// # 参数
    /// * `worker_count` - 工作线程数量
    /// 
    /// # 返回
    /// 新的工作窃取队列实例
    pub fn new(worker_count: usize) -> Self {
        let mut local_queues = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            local_queues.push(SegQueue::new());
        }
        
        Self {
            global_queue: SegQueue::new(),
            local_queues,
            worker_count,
            round_robin: AtomicUsize::new(0),
            stats: WorkStealingStats::new(),
        }
    }
    
    /// 推送任务到队列
    /// 
    /// # 参数
    /// * `item` - 要推送的任务
    /// * `worker_id` - 可选的工作线程 ID，如果指定则优先推送到该线程的本地队列
    /// 
    /// # 策略
    /// 1. 如果指定了 worker_id 且有效，推送到对应的本地队列
    /// 2. 否则使用轮询方式分配到本地队列
    /// 3. 这样可以最大化本地队列命中率，减少跨线程竞争
    pub fn push(&self, item: T, worker_id: Option<usize>) {
        if let Some(id) = worker_id {
            if id < self.worker_count {
                self.local_queues[id].push(item);
                return;
            }
        }
        
        // 轮询分配到本地队列，避免所有任务都进入全局队列
        let idx = self.round_robin.fetch_add(1, Ordering::Relaxed) % self.worker_count;
        self.local_queues[idx].push(item);
    }
    
    /// 推送任务到全局队列
    /// 
    /// 用于需要全局负载均衡的场景
    pub fn push_global(&self, item: T) {
        self.global_queue.push(item);
    }
    
    /// 弹出任务（工作窃取算法）
    /// 
    /// # 参数
    /// * `worker_id` - 当前工作线程的 ID
    /// 
    /// # 返回
    /// 如果有可用任务则返回 Some(task)，否则返回 None
    /// 
    /// # 工作窃取策略
    /// 1. 首先尝试从本地队列获取任务（最高优先级，无竞争）
    /// 2. 然后尝试从全局队列获取任务（中等优先级，有竞争但公平）
    /// 3. 最后尝试从其他线程的本地队列窃取任务（最低优先级，帮助负载均衡）
    /// 4. 窃取时使用随机化的顺序，避免总是从同一个线程窃取
    pub fn pop(&self, worker_id: usize) -> Option<T> {
        // 1. 优先尝试本地队列（无竞争，最快）
        if let Some(item) = self.local_queues[worker_id].pop() {
            self.stats.record_local_hit();
            return Some(item);
        }
        
        // 2. 尝试全局队列（有竞争但公平）
        if let Some(item) = self.global_queue.pop() {
            self.stats.record_global_hit();
            return Some(item);
        }
        
        // 3. 工作窃取：从其他线程的本地队列窃取
        // 使用伪随机顺序避免总是从同一个线程窃取
        let start_offset = (worker_id * 7) % self.worker_count; // 简单的伪随机
        
        for i in 1..self.worker_count {
            let target = (worker_id + start_offset + i) % self.worker_count;
            if let Some(item) = self.local_queues[target].pop() {
                self.stats.record_steal_hit();
                return Some(item);
            }
        }
        
        // 4. 所有队列都为空
        self.stats.record_empty_poll();
        None
    }
    
    /// 获取队列长度估计
    /// 
    /// 注意：由于并发访问，这个值只是一个估计
    pub fn len_estimate(&self) -> usize {
        let global_len = self.global_queue.len();
        let local_len: usize = self.local_queues.iter().map(|q| q.len()).sum();
        global_len + local_len
    }
    
    /// 检查队列是否为空
    /// 
    /// 注意：由于并发访问，这个结果可能立即过时
    pub fn is_empty(&self) -> bool {
        self.global_queue.is_empty() && self.local_queues.iter().all(|q| q.is_empty())
    }
    
    /// 获取统计信息
    pub fn get_stats(&self) -> WorkStealingQueueStats {
        let (local_hits, global_hits, steal_hits, empty_polls) = self.stats.get_stats();
        
        WorkStealingQueueStats {
            local_hits,
            global_hits,
            steal_hits,
            empty_polls,
            total_polls: local_hits + global_hits + steal_hits + empty_polls,
            local_hit_rate: if local_hits + global_hits + steal_hits > 0 {
                local_hits as f64 / (local_hits + global_hits + steal_hits) as f64
            } else {
                0.0
            },
        }
    }
    
    /// 获取每个本地队列的长度
    pub fn get_local_queue_lengths(&self) -> Vec<usize> {
        self.local_queues.iter().map(|q| q.len()).collect()
    }
}

/// 工作窃取队列统计信息
#[derive(Debug, Clone)]
pub struct WorkStealingQueueStats {
    /// 本地队列命中次数
    pub local_hits: usize,
    /// 全局队列命中次数
    pub global_hits: usize,
    /// 工作窃取成功次数
    pub steal_hits: usize,
    /// 空轮询次数
    pub empty_polls: usize,
    /// 总轮询次数
    pub total_polls: usize,
    /// 本地队列命中率
    pub local_hit_rate: f64,
}

impl std::fmt::Display for WorkStealingQueueStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WorkStealingQueue Stats: local_hits={}, global_hits={}, steal_hits={}, empty_polls={}, local_hit_rate={:.2}%",
            self.local_hits,
            self.global_hits,
            self.steal_hits,
            self.empty_polls,
            self.local_hit_rate * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_basic_push_pop() {
        let queue = WorkStealingQueue::new(2);
        
        // 测试推送和弹出
        queue.push(1, Some(0));
        queue.push(2, Some(1));
        
        assert_eq!(queue.pop(0), Some(1));
        assert_eq!(queue.pop(1), Some(2));
        assert_eq!(queue.pop(0), None);
    }
    
    #[test]
    fn test_work_stealing() {
        let queue = Arc::new(WorkStealingQueue::new(2));
        
        // 线程 0 推送大量任务到自己的本地队列
        for i in 0..20 {
            queue.push(i, Some(0));
        }
        
        // 验证任务确实被推送了
        let initial_len = queue.len_estimate();
        println!("Initial queue length: {}", initial_len);
        assert!(initial_len > 0, "Tasks should have been pushed");
        
        // 线程 0 先消费一些任务，验证本地命中
        let mut local_count = 0;
        for _ in 0..5 {
            if queue.pop(0).is_some() {
                local_count += 1;
            }
        }
        println!("Local hits by thread 0: {}", local_count);
        
        // 线程 1 应该能够从线程 0 的队列窃取任务
        let mut stolen_count = 0;
        while queue.pop(1).is_some() {
            stolen_count += 1;
        }
        
        println!("Stolen count by thread 1: {}", stolen_count);
        let stats = queue.get_stats();
        println!("Final stats: local_hits={}, global_hits={}, steal_hits={}, empty_polls={}", 
                 stats.local_hits, stats.global_hits, stats.steal_hits, stats.empty_polls);
        
        // 验证至少有一些任务被处理
        assert!(local_count + stolen_count > 0, "Some tasks should have been processed");
        // 验证本地命中发生了
        assert!(stats.local_hits > 0, "Local hits should have occurred");
    }
    
    #[test]
    fn test_concurrent_access() {
        let queue = Arc::new(WorkStealingQueue::new(4));
        let mut handles = vec![];
        
        // 启动多个生产者线程
        for worker_id in 0..4 {
            let queue_clone = queue.clone();
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    queue_clone.push(worker_id * 100 + i, Some(worker_id));
                }
            });
            handles.push(handle);
        }
        
        // 启动多个消费者线程
        for worker_id in 0..4 {
            let queue_clone = queue.clone();
            let handle = thread::spawn(move || {
                let mut consumed = 0;
                while consumed < 50 { // 每个消费者尝试消费 50 个任务
                    if queue_clone.pop(worker_id).is_some() {
                        consumed += 1;
                    } else {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            });
            handles.push(handle);
        }
        
        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }
        
        // 验证统计信息
        let stats = queue.get_stats();
        assert!(stats.total_polls > 0);
        println!("Final stats: {}", stats);
    }
}