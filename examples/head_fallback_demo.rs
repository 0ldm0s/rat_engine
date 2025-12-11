//! HEAD 请求回退功能演示
//!
//! 这个示例展示了如何启用 HEAD 请求自动回退到 GET 处理器的功能

use rat_engine::{RatEngine, Method, Response, Bytes, Full};
use std::collections::HashSet;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 启动 HEAD 回退功能演示服务器");

    // 使用 RatEngineBuilder 创建服务器
    let _engine = RatEngine::builder()
        .worker_threads(4)
        .max_connections(1000)
        .buffer_size(8192)
        .timeout(std::time::Duration::from_secs(30))
        .keepalive(true)
        .tcp_nodelay(true)
        .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .with_router(|mut router| {
            // 添加一个 GET 路由（公开路径）
            router.add_route(
                Method::GET,
                "/api/public/info",
                |_req| Box::pin(async {
                    let response = Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .header("X-Custom-Header", "test-value")
                        .body(Full::new(Bytes::from(r#"{"message":"Hello from GET","timestamp":"2024-01-01"}"#)))
                        .unwrap();
                    Ok(response)
                })
            );

            // 添加另一个 GET 路由（私有路径）
            router.add_route(
                Method::GET,
                "/api/private/data",
                |_req| Box::pin(async {
                    let response = Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(r#"{"secret":"private data"}"#)))
                        .unwrap();
                    Ok(response)
                })
            );

            // 添加静态文件路由
            router.add_route(
                Method::GET,
                "/static/files/<filename>",
                |_req| Box::pin(async {
                    let response = Response::builder()
                        .status(200)
                        .header("Content-Type", "text/plain")
                        .header("Cache-Control", "public, max-age=3600")
                        .body(Full::new(Bytes::from("Static file content here")))
                        .unwrap();
                    Ok(response)
                })
            );

            // 显式添加一个 HEAD 路由作为对比
            router.add_route(
                Method::HEAD,
                "/api/explicit/head",
                |_req| Box::pin(async {
                    let response = Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .header("X-Explicit-HEAD", "true")
                        .body(Full::new(Bytes::new()))
                        .unwrap();
                    Ok(response)
                })
            );

            // 配置 HEAD 回退功能
            // 创建白名单，只允许公开路径和静态文件使用 HEAD 回退
            let mut whitelist = HashSet::new();
            whitelist.insert("/api/public".to_string());
            whitelist.insert("/static".to_string());

            // 启用 HEAD 回退，但限制在白名单内
            router.enable_head_fallback(true, Some(whitelist));

            router
        })
        .build()?;

    println!();
    println!("📋 测试说明:");
    println!("1. HEAD /api/public/info - ✅ 应该成功回退到 GET 处理器");
    println!("2. HEAD /api/private/data - ❌ 应该返回 404（不在白名单中）");
    println!("3. HEAD /static/files/test.txt - ✅ 应该成功回退（前缀匹配）");
    println!("4. HEAD /api/explicit/head - ✅ 显式声明的 HEAD 路由正常工作");
    println!("5. GET /api/public/info - ✅ 正常工作");
    println!();
    println!("🧪 测试命令:");
    println!("curl -I http://127.0.0.1:8080/api/public/info");
    println!("curl -I http://127.0.0.1:8080/api/private/data");
    println!("curl -I http://127.0.0.1:8080/static/files/test.txt");
    println!("curl -I http://127.0.0.1:8080/api/explicit/head");
    println!("curl http://127.0.0.1:8080/api/public/info");
    println!();
    println!("按 Ctrl+C 停止服务器...");

    // 运行服务器
    _engine.start("127.0.0.1".to_string(), 8080).await?;

    Ok(())
}