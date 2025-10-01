//! RAT Engine 单元测试
//! 
//! 测试各个模块的具体功能

use rat_engine::server::{
    config::{ServerConfig, default_config, optimal_worker_count},
    router::{Router, RouteKey},
    worker_pool::WorkerPool,
};
use rat_engine::{Method, Request, Response, StatusCode, Incoming, Empty, Full, Bytes};
use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use std::pin::Pin;

// 由于Incoming类型在测试环境中难以构造，我们暂时注释掉相关的Router测试
// 这些测试应该在集成测试中通过实际的HTTP请求来验证

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = default_config(8080);
        assert_eq!(config.addr().port(), 8080);
        assert!(config.workers > 0);
    }

    #[test]
    fn test_optimal_worker_count() {
        let workers = optimal_worker_count();
        assert!(workers >= 1);
        // 应该等于 CPU 核心数
        let expected = rat_engine::utils::sys_info::SystemInfo::global().cpu_cores;
        assert_eq!(workers, expected);
    }
}

#[cfg(test)]
mod router_tests {
    use super::*;
    use std::pin::Pin;
    use std::future::Future;

    // Router测试已移至集成测试，因为需要实际的HTTP请求来构造Incoming类型

    #[test]
    fn test_route_key() {
        let key1 = RouteKey::new(Method::GET, "/test".to_string());
        let key2 = RouteKey::new(Method::GET, "/test".to_string());
        let key3 = RouteKey::new(Method::POST, "/test".to_string());
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}

#[cfg(test)]
mod worker_pool_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_worker_pool_creation() {
        let pool = WorkerPool::new(4);
        // active_count 现在返回工作线程数量，而不是活跃任务数
        assert_eq!(pool.active_count(), 4);
    }

    #[test]
    fn test_worker_pool_spawn() {
        let pool = WorkerPool::new(2);
        let counter = Arc::new(AtomicUsize::new(0));
        
        for _ in 0..5 {
            let counter = counter.clone();
            pool.execute(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
            });
        }
        
        // 等待任务完成
        std::thread::sleep(Duration::from_millis(100));
        
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }
}