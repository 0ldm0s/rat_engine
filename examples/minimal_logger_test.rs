//! 极简日志测试示例
//!
//! 测试不使用 with_log_config 时会发生什么
//! 只初始化最基本的服务器，运行1秒后退出

use std::time::Duration;
use rat_engine::RatEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚨 极简日志测试开始");

    // 创建引擎构建器，添加 with_log_config 调用
    let engine = RatEngine::builder()
        .worker_threads(2)
        .max_connections(100)
                .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .with_router(|mut router| {
            // 添加一个简单的路由
            router.add_route(rat_engine::Method::GET, "/", |_req| {
                Box::pin(async {
                    Ok(rat_engine::Response::builder()
                        .status(200)
                        .header("Content-Type", "text/plain")
                        .body("OK".into())
                        .unwrap())
                })
            });
            router
        })
        .build()?;

    println!("✅ 引擎构建完成，准备启动服务器...");

    // 在后台启动服务器
    let server_handle = tokio::spawn(async move {
        engine.start("127.0.0.1".to_string(), 8080).await
    });

    println!("🌐 服务器已启动，等待1秒...");

    // 等待1秒
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("⏰ 1秒已到，程序退出");

    Ok(())
}