//! 原子性能监控模块
//! 
//! 实现无锁的性能指标收集和统计：
//! - 原子计数器
//! - 延迟统计
//! - 吞吐量监控
//! - 实时性能报告

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// 原子性能监控器
/// 
/// 使用原子操作收集各种性能指标，避免锁竞争
pub struct AtomicMetrics {
    /// 请求计数
    request_count: AtomicU64,
    /// 成功请求计数
    success_count: AtomicU64,
    /// 错误计数
    error_count: AtomicU64,
    /// 连接计数
    connection_count: AtomicU64,
    /// 活跃连接数
    active_connections: AtomicUsize,
    
    /// 延迟统计
    latency_stats: LatencyStats,
    
    /// 吞吐量统计
    throughput_stats: ThroughputStats,
    
    /// 错误类型计数
    error_types: ErrorTypeCounters,
    
    /// 启动时间
    start_time: Instant,
}

/// 延迟统计
struct LatencyStats {
    /// 总延迟时间（微秒）
    total_latency_us: AtomicU64,
    /// 最小延迟（微秒）
    min_latency_us: AtomicU64,
    /// 最大延迟（微秒）
    max_latency_us: AtomicU64,
    /// P50 延迟估计（微秒）
    p50_latency_us: AtomicU64,
    /// P95 延迟估计（微秒）
    p95_latency_us: AtomicU64,
    /// P99 延迟估计（微秒）
    p99_latency_us: AtomicU64,
}

/// 吞吐量统计
struct ThroughputStats {
    /// 每秒请求数（RPS）
    requests_per_second: AtomicU64,
    /// 上次更新时间
    last_update: AtomicU64,
    /// 上次请求计数
    last_request_count: AtomicU64,
}

/// 错误类型计数器
struct ErrorTypeCounters {
    /// 超时错误
    timeout_errors: AtomicU64,
    /// Python 错误
    python_errors: AtomicU64,
    /// 网络错误
    network_errors: AtomicU64,
    /// 解析错误
    parse_errors: AtomicU64,
    /// 其他错误
    other_errors: AtomicU64,
}

impl AtomicMetrics {
    /// 创建新的性能监控器
    pub fn new() -> Self {
        Self {
            request_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            connection_count: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            latency_stats: LatencyStats::new(),
            throughput_stats: ThroughputStats::new(),
            error_types: ErrorTypeCounters::new(),
            start_time: Instant::now(),
        }
    }
    
    /// 记录请求开始
    pub fn record_request_start(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.update_throughput();
    }
    
    /// 记录请求成功
    pub fn record_request_success(&self) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 记录请求错误
    pub fn record_request_error(&self, error_type: ErrorType) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
        
        match error_type {
            ErrorType::Timeout => self.error_types.timeout_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Python => self.error_types.python_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Network => self.error_types.network_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Parse => self.error_types.parse_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Other => self.error_types.other_errors.fetch_add(1, Ordering::Relaxed),
        };
    }
    
    /// 记录请求延迟
    pub fn record_request_duration(&self, duration: Duration) {
        let latency_us = duration.as_micros() as u64;
        
        // 更新总延迟
        self.latency_stats.total_latency_us.fetch_add(latency_us, Ordering::Relaxed);
        
        // 更新最小延迟
        let mut current_min = self.latency_stats.min_latency_us.load(Ordering::Relaxed);
        while latency_us < current_min || current_min == 0 {
            match self.latency_stats.min_latency_us.compare_exchange_weak(
                current_min,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }
        
        // 更新最大延迟
        let mut current_max = self.latency_stats.max_latency_us.load(Ordering::Relaxed);
        while latency_us > current_max {
            match self.latency_stats.max_latency_us.compare_exchange_weak(
                current_max,
                latency_us,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
        
        // 更新百分位数估计（简化版本）
        self.update_percentiles(latency_us);
    }
    
    /// 增加连接计数
    pub fn increment_connections(&self) {
        self.connection_count.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 减少活跃连接数
    pub fn decrement_active_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// 增加错误计数
    pub fn increment_errors(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 更新吞吐量统计
    fn update_throughput(&self) {
        let now = Instant::now().duration_since(self.start_time).as_secs();
        let last_update = self.throughput_stats.last_update.load(Ordering::Relaxed);
        
        // 每秒更新一次吞吐量
        if now > last_update {
            let current_requests = self.request_count.load(Ordering::Relaxed);
            let last_requests = self.throughput_stats.last_request_count.load(Ordering::Relaxed);
            
            if now > last_update {
                let rps = (current_requests - last_requests) / (now - last_update).max(1);
                self.throughput_stats.requests_per_second.store(rps, Ordering::Relaxed);
                self.throughput_stats.last_update.store(now, Ordering::Relaxed);
                self.throughput_stats.last_request_count.store(current_requests, Ordering::Relaxed);
            }
        }
    }
    
    /// 更新百分位数估计（简化的指数移动平均）
    fn update_percentiles(&self, latency_us: u64) {
        // P50 更新（权重 0.1）
        let current_p50 = self.latency_stats.p50_latency_us.load(Ordering::Relaxed);
        let new_p50 = if current_p50 == 0 {
            latency_us
        } else {
            (current_p50 * 9 + latency_us) / 10
        };
        self.latency_stats.p50_latency_us.store(new_p50, Ordering::Relaxed);
        
        // P95 更新（权重 0.05）
        let current_p95 = self.latency_stats.p95_latency_us.load(Ordering::Relaxed);
        let new_p95 = if current_p95 == 0 {
            latency_us
        } else if latency_us > current_p95 {
            (current_p95 * 19 + latency_us) / 20
        } else {
            current_p95
        };
        self.latency_stats.p95_latency_us.store(new_p95, Ordering::Relaxed);
        
        // P99 更新（权重 0.01）
        let current_p99 = self.latency_stats.p99_latency_us.load(Ordering::Relaxed);
        let new_p99 = if current_p99 == 0 {
            latency_us
        } else if latency_us > current_p99 {
            (current_p99 * 99 + latency_us) / 100
        } else {
            current_p99
        };
        self.latency_stats.p99_latency_us.store(new_p99, Ordering::Relaxed);
    }
    
    /// 获取所有指标
    pub fn get_all(&self) -> HashMap<String, u64> {
        let mut metrics = HashMap::new();
        
        // 基础计数
        metrics.insert("requests_total".to_string(), self.request_count.load(Ordering::Relaxed));
        metrics.insert("requests_success".to_string(), self.success_count.load(Ordering::Relaxed));
        metrics.insert("requests_error".to_string(), self.error_count.load(Ordering::Relaxed));
        metrics.insert("connections_total".to_string(), self.connection_count.load(Ordering::Relaxed));
        metrics.insert("connections_active".to_string(), self.active_connections.load(Ordering::Relaxed) as u64);
        
        // 延迟指标
        let request_count = self.request_count.load(Ordering::Relaxed);
        if request_count > 0 {
            let avg_latency = self.latency_stats.total_latency_us.load(Ordering::Relaxed) / request_count;
            metrics.insert("latency_avg_us".to_string(), avg_latency);
        }
        metrics.insert("latency_min_us".to_string(), self.latency_stats.min_latency_us.load(Ordering::Relaxed));
        metrics.insert("latency_max_us".to_string(), self.latency_stats.max_latency_us.load(Ordering::Relaxed));
        metrics.insert("latency_p50_us".to_string(), self.latency_stats.p50_latency_us.load(Ordering::Relaxed));
        metrics.insert("latency_p95_us".to_string(), self.latency_stats.p95_latency_us.load(Ordering::Relaxed));
        metrics.insert("latency_p99_us".to_string(), self.latency_stats.p99_latency_us.load(Ordering::Relaxed));
        
        // 吞吐量
        metrics.insert("requests_per_second".to_string(), self.throughput_stats.requests_per_second.load(Ordering::Relaxed));
        
        // 错误类型
        metrics.insert("errors_timeout".to_string(), self.error_types.timeout_errors.load(Ordering::Relaxed));
        metrics.insert("errors_python".to_string(), self.error_types.python_errors.load(Ordering::Relaxed));
        metrics.insert("errors_network".to_string(), self.error_types.network_errors.load(Ordering::Relaxed));
        metrics.insert("errors_parse".to_string(), self.error_types.parse_errors.load(Ordering::Relaxed));
        metrics.insert("errors_other".to_string(), self.error_types.other_errors.load(Ordering::Relaxed));
        
        // 运行时间
        metrics.insert("uptime_seconds".to_string(), self.start_time.elapsed().as_secs());
        
        metrics
    }
    
    /// 获取性能摘要
    pub fn get_summary(&self) -> MetricsSummary {
        let request_count = self.request_count.load(Ordering::Relaxed);
        let success_count = self.success_count.load(Ordering::Relaxed);
        let error_count = self.error_count.load(Ordering::Relaxed);
        
        MetricsSummary {
            requests_total: request_count,
            requests_success: success_count,
            requests_error: error_count,
            success_rate: if request_count > 0 {
                success_count as f64 / request_count as f64
            } else {
                0.0
            },
            error_rate: if request_count > 0 {
                error_count as f64 / request_count as f64
            } else {
                0.0
            },
            connections_active: self.active_connections.load(Ordering::Relaxed),
            requests_per_second: self.throughput_stats.requests_per_second.load(Ordering::Relaxed),
            latency_avg_us: if request_count > 0 {
                self.latency_stats.total_latency_us.load(Ordering::Relaxed) / request_count
            } else {
                0
            },
            latency_p95_us: self.latency_stats.p95_latency_us.load(Ordering::Relaxed),
            latency_p99_us: self.latency_stats.p99_latency_us.load(Ordering::Relaxed),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }
    
    /// 重置所有指标
    pub fn reset(&self) {
        self.request_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        self.connection_count.store(0, Ordering::Relaxed);
        self.active_connections.store(0, Ordering::Relaxed);
        
        self.latency_stats.total_latency_us.store(0, Ordering::Relaxed);
        self.latency_stats.min_latency_us.store(0, Ordering::Relaxed);
        self.latency_stats.max_latency_us.store(0, Ordering::Relaxed);
        self.latency_stats.p50_latency_us.store(0, Ordering::Relaxed);
        self.latency_stats.p95_latency_us.store(0, Ordering::Relaxed);
        self.latency_stats.p99_latency_us.store(0, Ordering::Relaxed);
        
        self.throughput_stats.requests_per_second.store(0, Ordering::Relaxed);
        self.throughput_stats.last_update.store(0, Ordering::Relaxed);
        self.throughput_stats.last_request_count.store(0, Ordering::Relaxed);
        
        self.error_types.timeout_errors.store(0, Ordering::Relaxed);
        self.error_types.python_errors.store(0, Ordering::Relaxed);
        self.error_types.network_errors.store(0, Ordering::Relaxed);
        self.error_types.parse_errors.store(0, Ordering::Relaxed);
        self.error_types.other_errors.store(0, Ordering::Relaxed);
    }
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            total_latency_us: AtomicU64::new(0),
            min_latency_us: AtomicU64::new(0),
            max_latency_us: AtomicU64::new(0),
            p50_latency_us: AtomicU64::new(0),
            p95_latency_us: AtomicU64::new(0),
            p99_latency_us: AtomicU64::new(0),
        }
    }
}

impl ThroughputStats {
    fn new() -> Self {
        Self {
            requests_per_second: AtomicU64::new(0),
            last_update: AtomicU64::new(0),
            last_request_count: AtomicU64::new(0),
        }
    }
}

impl ErrorTypeCounters {
    fn new() -> Self {
        Self {
            timeout_errors: AtomicU64::new(0),
            python_errors: AtomicU64::new(0),
            network_errors: AtomicU64::new(0),
            parse_errors: AtomicU64::new(0),
            other_errors: AtomicU64::new(0),
        }
    }
}

/// 错误类型枚举
#[derive(Debug, Clone, Copy)]
pub enum ErrorType {
    Timeout,
    Python,
    Network,
    Parse,
    Other,
}

/// 性能指标摘要
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_error: u64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub connections_active: usize,
    pub requests_per_second: u64,
    pub latency_avg_us: u64,
    pub latency_p95_us: u64,
    pub latency_p99_us: u64,
    pub uptime_seconds: u64,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "📊 RAT Engine Metrics:\n\
             ├─ Requests: {} total, {} success ({:.2}%), {} errors ({:.2}%)\n\
             ├─ Connections: {} active\n\
             ├─ Throughput: {} req/s\n\
             ├─ Latency: avg={}μs, p95={}μs, p99={}μs\n\
             └─ Uptime: {}s",
            self.requests_total,
            self.requests_success,
            self.success_rate * 100.0,
            self.requests_error,
            self.error_rate * 100.0,
            self.connections_active,
            self.requests_per_second,
            self.latency_avg_us,
            self.latency_p95_us,
            self.latency_p99_us,
            self.uptime_seconds
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_basic_metrics() {
        let metrics = AtomicMetrics::new();
        
        // 记录一些请求
        metrics.record_request_start();
        metrics.record_request_duration(Duration::from_millis(10));
        metrics.record_request_success();
        
        metrics.record_request_start();
        metrics.record_request_duration(Duration::from_millis(20));
        metrics.record_request_error(ErrorType::Timeout);
        
        let summary = metrics.get_summary();
        assert_eq!(summary.requests_total, 2);
        assert_eq!(summary.requests_success, 1);
        assert_eq!(summary.requests_error, 1);
        assert_eq!(summary.success_rate, 0.5);
    }
    
    #[test]
    fn test_latency_tracking() {
        let metrics = AtomicMetrics::new();
        
        // 先记录请求，再记录延迟
        metrics.record_request_start();
        metrics.record_request_duration(Duration::from_millis(5));
        metrics.record_request_start();
        metrics.record_request_duration(Duration::from_millis(10));
        metrics.record_request_start();
        metrics.record_request_duration(Duration::from_millis(15));
        
        let all_metrics = metrics.get_all();
        assert!(all_metrics["latency_min_us"] > 0);
        assert!(all_metrics["latency_max_us"] >= all_metrics["latency_min_us"]);
        assert!(all_metrics["latency_avg_us"] > 0);
    }
    
    #[test]
    fn test_concurrent_access() {
        let metrics = std::sync::Arc::new(AtomicMetrics::new());
        let mut handles = vec![];
        
        // 启动多个线程并发更新指标
        for _ in 0..10 {
            let metrics_clone = metrics.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    metrics_clone.record_request_start();
                    metrics_clone.record_request_duration(Duration::from_millis(1));
                    metrics_clone.record_request_success();
                }
            });
            handles.push(handle);
        }
        
        // 等待所有线程完成
        for handle in handles {
            handle.join().unwrap();
        }
        
        let summary = metrics.get_summary();
        assert_eq!(summary.requests_total, 1000);
        assert_eq!(summary.requests_success, 1000);
        assert_eq!(summary.success_rate, 1.0);
    }
}