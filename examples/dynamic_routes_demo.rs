//! 动态路径参数演示
//! 展示如何使用 rat_engine 处理带有路径参数的路由
//! 包含自动化测试验证功能

use rat_engine::server::{Router, ServerConfig};
use rat_engine::RatEngine;
use rat_engine::server::http_request::HttpRequest;
use rat_engine::client::RatIndependentHttpClientBuilder;
use rat_engine::compression::CompressionConfig;
use rat_engine::{Response, StatusCode, Method};
use rat_engine::Full;
use rat_engine::Bytes;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;
use tokio;
use tokio::signal;
use tokio::sync::oneshot;

/// 用户信息处理器
async fn handle_user_info(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    // 使用新的路径参数系统
    let user_id = req.param_as_i64("id").unwrap_or(0);
    let path = req.path();

    let response_data = json!({
        "user_id": user_id,
        "name": format!("用户{}", user_id),
        "email": format!("user{}@example.com", user_id),
        "status": "active",
        "created_at": "2024-01-01T00:00:00Z",
        "path_matched": path
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 用户资料更新处理器
async fn handle_user_profile_update(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    // 使用新的路径参数系统
    let user_id = req.param_as_i64("id").unwrap_or(0);
    let path = req.path();

    // 读取请求体
    let body_str = req.body_as_string().unwrap_or_default();

    let response_data = json!({
        "user_id": user_id,
        "message": "用户资料更新成功",
        "updated_fields": body_str,
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "path_matched": path
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// API 项目处理器
async fn handle_api_item(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    // 使用新的路径参数系统
    let item_id = req.param_as_i64("id").unwrap_or(0);
    let path = req.path();

    let response_data = json!({
        "item_id": item_id,
        "name": format!("项目{}", item_id),
        "description": format!("这是项目{}的描述", item_id),
        "price": 99.99,
        "in_stock": true,
        "path_matched": path,
        "created_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 用户帖子处理器
async fn handle_user_post(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    // 使用新的路径参数系统
    let user_id = req.param_as_i64("user_id").unwrap_or(0);
    let post_id = req.param_as_i64("post_id").unwrap_or(0);
    let path = req.path();

    let response_data = json!({
        "user_id": user_id,
        "post_id": post_id,
        "title": format!("用户{}的帖子{}", user_id, post_id),
        "content": "这是一个示例帖子内容",
        "created_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "likes": 42,
        "path_matched": path
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
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
        "features": ["dynamic_routes", "path_parameters"]
    });
    
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();
    
    Ok(response)
}

/// 根路径处理器
async fn handle_root(_req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let html_content = r#"
<!DOCTYPE html>
<html>
<head>
    <title>动态路由演示</title>
    <meta charset="utf-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .endpoint { background: #f5f5f5; padding: 10px; margin: 10px 0; border-radius: 5px; }
        .method { font-weight: bold; color: #007acc; }
        .path { font-family: monospace; background: #e8e8e8; padding: 2px 5px; }
    </style>
</head>
<body>
    <h1>🚀 动态路由演示</h1>
    <p>以下是可用的动态路由端点：</p>
    
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
    
    <h2>测试说明</h2>
    <p>运行 Python 测试脚本来验证所有路由：</p>
    <pre>cd rat_engine/python/examples
python test_dynamic_routes.py --url http://localhost:8081</pre>
</body>
</html>
    "#;
    
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html")
        .body(Full::new(Bytes::from(html_content.to_string())))
        .unwrap();
    
    Ok(response)
}

// ========== 保留的辅助函数（供参考）==========
// 注意：这些函数展示了如何手动解析路径参数，但现在推荐使用 req.param_*() 方法

/// 从路径中提取用户ID（手动解析示例，已废弃）
/// 现在推荐使用：req.param_as_i64("id") 或 req.param("id")
fn extract_user_id_from_path(path: &str) -> String {
    // 简化的路径解析，实际应该使用路由器提供的参数
    if let Some(captures) = regex::Regex::new(r"/users/([^/]+)").unwrap().captures(path) {
        captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default()
    } else {
        "unknown".to_string()
    }
}

/// 从路径中提取项目ID（手动解析示例，已废弃）
/// 现在推荐使用：req.param_as_i64("id") 或 req.param("id")
fn extract_item_id_from_path(path: &str) -> String {
    if let Some(captures) = regex::Regex::new(r"/api/v1/items/([^/]+)").unwrap().captures(path) {
        captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default()
    } else {
        "unknown".to_string()
    }
}

/// 从路径中提取用户ID和帖子ID（手动解析示例，已废弃）
/// 现在推荐使用：req.param_as_i64("user_id") 和 req.param_as_i64("post_id")
fn extract_user_post_ids_from_path(path: &str) -> (String, String) {
    if let Some(captures) = regex::Regex::new(r"/api/v1/users/([^/]+)/posts/([^/]+)").unwrap().captures(path) {
        let user_id = captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let post_id = captures.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
        (user_id, post_id)
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

/// 自动化测试验证函数
async fn run_automated_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 开始自动化测试验证...");
    
    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // 使用独立HTTP客户端
    let http_client = RatIndependentHttpClientBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent("DynamicRoutesDemo/1.0.0")
        .supported_compressions(vec!["gzip".to_string(), "deflate".to_string()])
        .auto_decompress(true)
        .build()?;
    
    let base_url = "http://127.0.0.1:8081";
    let mut test_results = Vec::new();
    
    // 测试用例定义
    let test_cases = vec![
        ("GET /", "/"),
        ("GET /health", "/health"),
        ("GET /users/123", "/users/123"),
        ("GET /api/v1/items/456", "/api/v1/items/456"),
        ("GET /api/v1/users/789/posts/101", "/api/v1/users/789/posts/101"),
    ];
    
    // 执行测试用例
    for (test_name, path) in test_cases {
        print!("  测试 {}: ", test_name);
        
        match test_endpoint(&http_client, &format!("{}{}", base_url, path)).await {
            Ok(response_data) => {
                println!("✅ 通过");
                test_results.push((test_name, true, None));
                
                // 验证响应内容
                if path != "/" {
                    if let Ok(json_value) = serde_json::from_str::<Value>(&response_data) {
                        if path == "/health" {
                            if json_value.get("status").and_then(|v| v.as_str()) == Some("healthy") {
                                println!("    ✓ 健康检查状态正确");
                            }
                        } else if path.starts_with("/users/") {
                            if json_value.get("user_id").is_some() {
                                println!("    ✓ 用户ID参数提取正确");
                            }
                        } else if path.starts_with("/api/v1/items/") {
                            if json_value.get("item_id").is_some() {
                                println!("    ✓ 项目ID参数提取正确");
                            }
                        } else if path.contains("/posts/") {
                            if json_value.get("user_id").is_some() && json_value.get("post_id").is_some() {
                                println!("    ✓ 多参数提取正确");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ 失败: {}", e);
                test_results.push((test_name, false, Some(e.to_string())));
            }
        }
    }
    
    // 测试 POST 请求
    print!("  测试 POST /users/123/profile: ");
    match test_post_endpoint(&http_client, &format!("{}/users/123/profile", base_url)).await {
        Ok(_) => {
            println!("✅ 通过");
            test_results.push(("POST /users/123/profile", true, None));
        }
        Err(e) => {
            println!("❌ 失败: {}", e);
            test_results.push(("POST /users/123/profile", false, Some(e.to_string())));
        }
    }
    
    // 输出测试结果统计
    let passed = test_results.iter().filter(|(_, success, _)| *success).count();
    let total = test_results.len();
    
    println!("\n📊 测试结果统计:");
    println!("  总测试数: {}", total);
    println!("  通过数: {}", passed);
    println!("  失败数: {}", total - passed);
    
    if passed == total {
        println!("🎉 所有测试通过！动态路由功能正常工作。");
    } else {
        println!("⚠️  有 {} 个测试失败，请检查服务器配置。", total - passed);
        for (test_name, success, error) in &test_results {
            if !success {
                println!("  ❌ {}: {}", test_name, error.as_ref().unwrap_or(&"未知错误".to_string()));
            }
        }
    }
    
    Ok(())
}

/// 测试 GET 端点
async fn test_endpoint(client: &rat_engine::client::RatIndependentHttpClient, url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = client.get(url).await?;
    Ok(response.text()?)
}

/// 测试 POST 端点
async fn test_post_endpoint(client: &rat_engine::client::RatIndependentHttpClient, url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let test_data = json!({
        "name": "测试用户",
        "email": "test@example.com",
        "bio": "这是一个测试用户的简介"
    });
    
    let response = client.post_json(url, &test_data).await?;
    Ok(response.text()?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 检查必需的特性
    #[cfg(not(feature = "reqwest"))]
    {
        println!("❌ 此示例需要 reqwest 特性");
        println!("请使用: cargo run --example dynamic_routes_demo --features reqwest");
        return Ok(());
    }

    // 初始化 CryptoProvider
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();
    
    // 日志通过RatEngineBuilder初始化
    
    println!("🚀 启动动态路由演示服务器...");
    
    // 创建服务器配置
    let addr: SocketAddr = "127.0.0.1:8081".parse()?;
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());
    
    // 创建路由器
    let mut router = Router::new();
    
        
    // 注册路由
    router.add_route(Method::GET, "/", |req| Box::pin(handle_root(req)));
    router.add_route(Method::GET, "/health", |req| Box::pin(handle_health(req)));
    
    // 动态路由 - 使用 <param> 格式而不是 {param}
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
    
    println!("\n🌐 服务器启动在 http://127.0.0.1:8081");
    
    // 创建一个通道用于控制服务器关闭
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    
    // 在后台启动服务器
    let server_handle = tokio::spawn(async move {
        // 创建引擎
        let engine = match RatEngine::builder()
            .router(router)
            .build()
        {
            Ok(engine) => engine,
            Err(e) => {
                eprintln!("❌ 构建引擎失败: {}", e);
                return;
            }
        };
        
        // 使用 select! 来同时监听服务器和关闭信号
        tokio::select! {
            result = engine.start("127.0.0.1".to_string(), 8081) => {
                if let Err(e) = result {
                    eprintln!("❌ 服务器运行错误: {}", e);
                }
            }
            _ = shutdown_rx => {
                println!("📴 收到关闭信号，服务器正在关闭...");
            }
        }
    });
    
    // 在另一个任务中运行自动化测试
    let test_handle = tokio::spawn(async move {
        // 运行自动化测试
        if let Err(e) = run_automated_tests().await {
            eprintln!("❌ 自动化测试失败: {}", e);
        }
        
        // 测试完成后发送关闭信号
        if let Err(_) = shutdown_tx.send(()) {
            eprintln!("⚠️  无法发送关闭信号");
        }
    });
    
    // 等待测试完成
    let _ = test_handle.await;
    
    // 给服务器一点时间来处理关闭
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // 强制终止服务器任务（如果还在运行）
    server_handle.abort();
    
    println!("✅ 动态路由演示完成，程序退出。");
    
    Ok(())
}