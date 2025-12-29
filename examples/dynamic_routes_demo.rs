//! 动态路径参数演示（纯 HTTP 模式）
//!
//! 展示如何使用 rat_engine 处理带有路径参数的路由
//! 使用 HTTP 专用模式，无需 TLS 证书

use rat_engine::{RatEngine, Response, StatusCode, Method, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use std::time::Duration;
use serde_json::json;

/// 用户信息处理器
async fn handle_user_info(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_id = req.param_as_i64("id").unwrap_or(0);
    let path = req.path();

    // 输出请求头信息（用于测试）
    println!("=== 请求头信息 ===");
    println!("远程地址: {:?}", req.remote_addr);
    for (name, value) in req.headers.iter() {
        println!("  {}: {:?}", name, value);
    }
    println!("==================");

    // 收集所有请求头到响应中
    let mut headers_map = serde_json::Map::new();
    for (name, value) in req.headers.iter() {
        if let Ok(value_str) = value.to_str() {
            headers_map.insert(name.to_string(), json!(value_str));
        }
    }

    let response_data = json!({
        "user_id": user_id,
        "name": format!("用户{}", user_id),
        "email": format!("user{}@example.com", user_id),
        "status": "active",
        "path_matched": path,
        "remote_addr": format!("{:?}", req.remote_addr),
        "headers": headers_map
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap())
}

/// 用户资料更新处理器
async fn handle_user_profile_update(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_id = req.param_as_i64("id").unwrap_or(0);
    let body_str = req.body_as_string().unwrap_or_default();

    let response_data = json!({
        "user_id": user_id,
        "message": "用户资料更新成功",
        "updated_fields": body_str
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap())
}

/// API 项目处理器
async fn handle_api_item(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let item_id = req.param_as_i64("id").unwrap_or(0);
    let path = req.path();

    let response_data = json!({
        "item_id": item_id,
        "name": format!("项目{}", item_id),
        "description": format!("这是项目{}的描述", item_id),
        "price": 99.99,
        "in_stock": true,
        "path_matched": path
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap())
}

/// 用户帖子处理器（多参数）
async fn handle_user_post(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_id = req.param_as_i64("user_id").unwrap_or(0);
    let post_id = req.param_as_i64("post_id").unwrap_or(0);
    let path = req.path();

    let response_data = json!({
        "user_id": user_id,
        "post_id": post_id,
        "title": format!("用户{}的帖子{}", user_id, post_id),
        "content": "这是一个示例帖子内容",
        "likes": 42,
        "path_matched": path
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap())
}

/// 健康检查处理器
async fn handle_health(_req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let response_data = json!({
        "status": "healthy",
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "version": "1.0.0",
        "mode": "http_only"
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap())
}

/// 根路径处理器
async fn handle_root(_req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>动态路由演示（HTTP 模式）</title>
    <meta charset="utf-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .endpoint { background: #f5f5f5; padding: 10px; margin: 10px 0; border-radius: 5px; }
        .method { font-weight: bold; color: #007acc; }
        .path { font-family: monospace; background: #e8e8e8; padding: 2px 5px; }
    </style>
</head>
<body>
    <h1>🚀 动态路由演示（纯 HTTP 模式）</h1>
    <p>使用 HTTP 专用模式，无需 TLS 证书</p>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/users/{id}</span>
        <br>示例: <a href="/users/123">/users/123</a>
    </div>

    <div class="endpoint">
        <span class="method">POST</span> <span class="path">/users/{id}/profile</span>
        <br>示例: /users/123/profile (需要POST请求)
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/api/v1/items/{id}</span>
        <br>示例: <a href="/api/v1/items/456">/api/v1/items/456</a>
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/api/v1/users/{user_id}/posts/{post_id}</span>
        <br>示例: <a href="/api/v1/users/789/posts/101">/api/v1/users/789/posts/101</a>
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/health</span>
        <br>示例: <a href="/health">/health</a>
    </div>

    <h2>测试命令：</h2>
    <pre>
# 测试各个端点
curl http://localhost:8080/
curl http://localhost:8080/health
curl http://localhost:8080/users/123
curl http://localhost:8080/api/v1/items/456
curl http://localhost:8080/api/v1/users/789/posts/101

# POST 请求示例
curl -X POST http://localhost:8080/users/123/profile \
  -H "Content-Type: application/json" \
  -d '{"name":"测试","email":"test@example.com"}'
    </pre>
</body>
</html>
    "#;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(html)))
        .unwrap())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 RAT Engine 动态路由演示（纯 HTTP 模式）");
    println!("========================================");
    println!("模式: HTTP 专用（无需 TLS）");
    println!("端口: 8080");
    println!();

    // 创建路由器
    let mut router = rat_engine::Router::new();

    // 启用 HTTP 专用模式（跳过协议检测，提高性能）
    router.enable_http_only();

    // 注册路由
    router.add_route(Method::GET, "/", |req| Box::pin(handle_root(req)));
    router.add_route(Method::GET, "/health", |req| Box::pin(handle_health(req)));

    // 动态路由 - 使用 <param> 格式
    router.add_route(Method::GET, "/users/<id>", |req| Box::pin(handle_user_info(req)));
    router.add_route(Method::POST, "/users/<id>/profile", |req| Box::pin(handle_user_profile_update(req)));
    router.add_route(Method::GET, "/api/v1/items/<id>", |req| Box::pin(handle_api_item(req)));
    router.add_route(Method::GET, "/api/v1/users/<user_id>/posts/<post_id>", |req| Box::pin(handle_user_post(req)));

    println!("📋 已注册的路由:");
    println!("  GET  /");
    println!("  GET  /health");
    println!("  GET  /users/{{id}}");
    println!("  POST /users/{{id}}/profile");
    println!("  GET  /api/v1/items/{{id}}");
    println!("  GET  /api/v1/users/{{user_id}}/posts/{{post_id}}");
    println!();

    // 构建引擎
    let engine = RatEngine::builder()
        .worker_threads(4)
        .timeout(Duration::from_secs(30))
        .enable_logger()
        .router(router)
        .build()?;

    println!("✅ 服务器启动！");
    println!();
    println!("测试方法:");
    println!("  curl http://localhost:8080/");
    println!("  curl http://localhost:8080/users/123");
    println!("  curl http://localhost:8080/api/v1/items/456");
    println!("  curl http://localhost:8080/api/v1/users/789/posts/101");
    println!();
    println!("按 Ctrl+C 停止");
    println!();

    // 启动服务器
    engine.start_single_port_multi_protocol("127.0.0.1".to_string(), 8080).await?;

    Ok(())
}
