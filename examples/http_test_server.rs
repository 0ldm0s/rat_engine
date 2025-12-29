//! 最简单的 HTTP 单端口示例
//!
//! 这是一个最基础的示例：
//! - 只启用 HTTP（不启用 gRPC）
//! - 单端口模式（端口 8080）
//! - 不使用 TLS（HTTP/1.1）
//! - 不需要证书

use rat_engine::{RatEngine, Response, Method};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 RAT Engine HTTP 单端口示例");
    println!("================================");
    println!("模式: HTTP only，无 TLS");
    println!("端口: 9090 (HAProxy 后端测试)");
    println!();

    // 创建引擎 - HTTP only，单端口
    let engine = RatEngine::builder()
        .worker_threads(4)
        .timeout(Duration::from_secs(30))
        .enable_logger()
        .with_router(|mut router| {
            // 启用 HTTP 专用模式
            router.enable_http_only();

            // 根路径
            router.add_route(Method::GET, "/", |_req| {
                Box::pin(async {
                    Ok(Response::builder()
                        .status(200)
                        .header("Content-Type", "text/plain; charset=utf-8")
                        .body("你好，这是 RAT Engine！\nHello from RAT Engine!".into())
                        .unwrap())
                })
            });

            // 健康检查
            router.add_route(Method::GET, "/health", |_req| {
                Box::pin(async {
                    Ok(Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .body(r#"{"status": "ok", "service": "rat-engine"}"#.into())
                        .unwrap())
                })
            });

            // API 示例
            router.add_route(Method::GET, "/api/info", |_req| {
                Box::pin(async {
                    Ok(Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .body(r#"{"name": "RAT Engine", "version": "1.0", "mode": "HTTP-only"}"#.into())
                        .unwrap())
                })
            });

            router
        })
        .build()?;

    // 启动服务器
    println!("✅ 服务器配置完成");
    println!("📍 http://127.0.0.1:9090/");
    println!("📍 http://127.0.0.1:9090/health");
    println!("📍 http://127.0.0.1:9090/api/info");
    println!("📍 (通过 HAProxy) http://127.0.0.1:8080/");
    println!();
    println!("按 Ctrl+C 停止服务器");
    println!();

    engine.start("127.0.0.1".to_string(), 9090).await?;

    Ok(())
}
