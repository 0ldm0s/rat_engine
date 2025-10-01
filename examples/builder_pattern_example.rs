//! RAT Engine 构建器模式示例
//! 
//! 这个示例展示了如何使用新的 RatEngineBuilder 来创建和配置服务器
//! 这是唯一推荐的创建服务器的方式
//! 
//! 新的架构流程：
//! 1. 使用 RatEngineBuilder 配置所有服务器参数
//! 2. 使用 with_router() 方法在构建器中直接配置路由
//! 3. 构建并启动服务器

use rat_engine::RatEngine;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 日志通过RatEngineBuilder初始化
    
    println!("🚀 RAT Engine 构建器模式示例");
    println!("================================");
    
    // 使用构建器创建引擎，所有配置都在一个地方完成
    let engine = RatEngine::builder()
        .worker_threads(4)                      // 设置工作线程数
        .max_connections(10000)                 // 设置最大连接数
        .buffer_size(8192)                      // 设置缓冲区大小
        .timeout(Duration::from_secs(30))       // 设置超时时间
        .keepalive(true)                        // 启用 Keep-Alive
        .tcp_nodelay(true)                      // 启用 TCP_NODELAY
        .with_log_config(rat_engine::utils::logger::LogConfig::default()) // 启用日志
                .congestion_control(true, "bbr".to_string()) // 启用拥塞控制
        .spa_config("index.html".to_string())  // 配置SPA支持
        .with_router(|mut router| {             // 配置路由
            // 配置路由规则
            router.add_route(rat_engine::Method::GET, "/", |_req| {
                Box::pin(async {
                    Ok(rat_engine::Response::builder()
                        .status(200)
                        .header("Content-Type", "text/plain")
                        .body("Hello from RAT Engine Builder with unified configuration!".into())
                        .unwrap())
                })
            });
            
            router.add_route(rat_engine::Method::GET, "/api", |_req| {
                Box::pin(async {
                    Ok(rat_engine::Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .body(r#"{"message": "API endpoint", "status": "ok"}"#.into())
                        .unwrap())
                })
            });
            
            // 单个方法的路由
            router.add_route(rat_engine::Method::POST, "/api/data", |_req| {
                Box::pin(async {
                    Ok(rat_engine::Response::builder()
                        .status(201)
                        .header("Content-Type", "application/json")
                        .body(r#"{"message": "Data created", "status": "success"}"#.into())
                        .unwrap())
                })
            });
            
            // 支持多个方法的路由示例
            router.add_route_with_methods(
                [rat_engine::Method::GET, rat_engine::Method::POST], // 同时支持 GET 和 POST
                "/api/users", 
                |_req| {
                    Box::pin(async {
                        Ok(rat_engine::Response::builder()
                            .status(200)
                            .header("Content-Type", "application/json")
                            .body(r#"{"message": "Users endpoint", "methods": ["GET", "POST"]}"#.into())
                            .unwrap())
                    })
                }
            );
            
            // 支持所有常用方法的路由
            router.add_route_with_methods(
                [
                    rat_engine::Method::GET,
                    rat_engine::Method::POST, 
                    rat_engine::Method::PUT,
                    rat_engine::Method::DELETE
                ],
                "/api/universal",
                |_req| {
                    Box::pin(async {
                        Ok(rat_engine::Response::builder()
                            .status(200)
                            .header("Content-Type", "application/json")
                            .body(r#"{"message": "Universal endpoint", "accepts": ["GET", "POST", "PUT", "DELETE"]}"#.into())
                            .unwrap())
                    })
                }
            );
            
            router // 返回配置好的router
        })
        .build_and_start("127.0.0.1".to_string(), 8080).await?;
    
    println!("✅ 服务器已启动，访问 http://127.0.0.1:8080");
    println!("📝 服务器配置信息：");
    println!("   - 工作线程数: {}", engine.get_workers());
    println!("   - 最大连接数: {}", engine.get_max_connections());
    println!("   - 主机地址: {}", engine.get_host());
    println!("   - 端口: {}", engine.get_port());
    println!("📚 可访问的路由:");
    println!("   GET /              - Hello World");
    println!("   GET /api           - API 状态");
    println!("   POST /api/data     - 数据创建");
    println!("   GET|POST /api/users - 多方法用户端点");
    println!("   GET|POST|PUT|DELETE /api/universal - 通用端点");
    
    // 服务器运行中...
    println!("⏳ 服务器正在运行，按 Ctrl+C 停止...");
    
    // 在实际应用中，这里会一直运行直到收到停止信号
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    Ok(())
}