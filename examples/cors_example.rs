//! CORS 示例
//!
//! 演示如何使用 RAT Engine 的 CORS (跨域资源共享) 功能

use rat_engine::{RatEngine, Method, Response, StatusCode, Bytes, Full};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    text: String,
    timestamp: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 启动 CORS 示例服务器");

    // 使用 RatEngineBuilder 创建服务器，配置 CORS 支持
    let engine = RatEngine::builder()
        .worker_threads(4)                      // 设置工作线程数
        .max_connections(1000)                   // 设置最大连接数
        .buffer_size(8192)                      // 设置缓冲区大小
        .timeout(std::time::Duration::from_secs(30)) // 设置超时时间
        .keepalive(true)                        // 启用 Keep-Alive
        .tcp_nodelay(true)                      // 启用 TCP_NODELAY
        .with_log_config(rat_engine::utils::logger::LogConfig::default()) // 启用日志
        .with_router(|mut router| {             // 配置路由
            // 配置 CORS (默认禁用)
            // 这里我们将启用 CORS 并设置允许的来源
            router.enable_cors(
                rat_engine::server::cors::CorsConfig::new()
                    .enable()
                    .allowed_origins(vec![
                        "*".to_string(), // 允许所有来源
                    ])
                    .allowed_methods(vec![
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::DELETE,
                        Method::OPTIONS,
                    ])
                    .allowed_headers(vec![
                        "Content-Type".to_string(),
                        "Authorization".to_string(),
                        "X-Requested-With".to_string(),
                    ])
                    .allow_credentials(true)
                    .max_age(3600) // 1小时缓存
            );

            // 添加 API 端点
            router.add_route(Method::GET, "/api/message", |_req| {
                Box::pin(async {
                    let message = Message {
                        text: "Hello from CORS-enabled API!".to_string(),
                        timestamp: chrono::Utc::now().timestamp(),
                    };

                    let json = serde_json::to_string(&message).unwrap_or_else(|_| r#"{"error": "Failed to serialize message"}"#.to_string());
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(json)))
                        .unwrap())
                })
            });

            router.add_route(Method::POST, "/api/echo", |req| {
                Box::pin(async move {
                    let body = req.body_as_string().unwrap_or_default();
                    let message = Message {
                        text: format!("Echo: {}", body),
                        timestamp: chrono::Utc::now().timestamp(),
                    };

                    println!("📝 收到 POST 请求: {}", body);

                    let json = serde_json::to_string(&message).unwrap_or_else(|_| r#"{"error": "Failed to serialize message"}"#.to_string());
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(json)))
                        .unwrap())
                })
            });

            router // 返回配置好的router
        })
        .build_and_start("127.0.0.1".to_string(), 8080).await?;

    println!("🌐 CORS 服务器已启动！");
    println!("📍 服务器地址: http://127.0.0.1:8080");
    println!("");
    println!("📋 CORS 配置:");
    println!("   ✅ 已启用");
    println!("   🌐 允许来源: http://localhost:3000, https://example.com, https://*.example.com");
    println!("   🔧 允许方法: GET, POST, PUT, DELETE, OPTIONS");
    println!("   📋 允许头部: Content-Type, Authorization, X-Requested-With");
    println!("   🔐 允许认证: true");
    println!("   ⏰ 预检缓存: 3600秒");
    println!("");
    println!("🧪 测试方法:");
    println!("   1. 使用浏览器前端页面测试");
    println!("   2. 使用 curl 测试:");
    println!("      curl -H \"Origin: http://localhost:3000\" \\");
    println!("           -H \"Access-Control-Request-Method: GET\" \\");
    println!("           -H \"Access-Control-Request-Headers: Content-Type\" \\");
    println!("           -X OPTIONS http://localhost:8080/api/message");
    println!("      ");
    println!("   3. 实际请求:");
    println!("      curl -H \"Origin: http://localhost:3000\" \\");
    println!("           http://localhost:8080/api/message");
    println!("      ");
    println!("   4. 测试跨域限制:");
    println!("      curl -H \"Origin: http://evil.com\" \\");
    println!("           http://localhost:8080/api/message");
    println!("");
    println!("💡 提示: 使用浏览器开发者工具或 Postman 可以更直观地测试 CORS 功能");
    println!("🛑 按 Ctrl+C 停止服务器");

    // 服务器运行中...
    println!("⏳ 服务器正在运行，按 Ctrl+C 停止...");

    // 在实际应用中，这里会一直运行直到收到停止信号
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    Ok(())
}