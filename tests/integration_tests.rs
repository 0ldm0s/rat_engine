//! RAT Engine 集成测试
//! 
//! 测试服务器启动、路由处理、性能、缓存中间件等核心功能

use rat_engine::{
    server::{ServerConfig, Router, config::optimal_worker_count},
    utils::sys_info::SystemInfo,
    DEFAULT_PORT,
};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_system_info() {
    let sys_info = SystemInfo::global();
    
    assert!(sys_info.cpu_cores > 0);
    assert!(!sys_info.os_name.is_empty());
    println!("✅ System Info: {} cores, OS: {}", sys_info.cpu_cores, sys_info.os_name);
}

#[tokio::test]
async fn test_router_creation() {
    let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let config = ServerConfig::new(addr, 1);
    
    let router = Router::new();
    let routes = router.list_routes();
    
    // 新创建的路由器应该是空的
    assert!(routes.is_empty(), "新创建的路由器应该不包含任何路由");
    
    println!("✅ Router created with {} routes", routes.len());
}

#[tokio::test]
async fn test_server_config() {
    let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
    let config = ServerConfig::new(addr, 4);
    
    assert_eq!(config.workers, 4);
    assert_eq!(config.addr().ip().to_string(), "127.0.0.1");
    
    println!("✅ Server config created successfully");
}

#[test]
fn test_optimal_worker_count() {
    let workers = optimal_worker_count();
    
    assert!(workers >= 1);
    assert!(workers <= 1024);
    
    println!("✅ Optimal worker count: {}", workers);
}

#[test]
fn test_constants() {
    assert_eq!(rat_engine::DEFAULT_PORT, 8080);
    assert_eq!(rat_engine::MIN_WORKERS, 1);
    assert_eq!(rat_engine::MAX_WORKERS, 1024);
    
    println!("✅ Constants validated");
}