//! 专注调试路由冲突问题的简化测试
//! 只测试最后3个问题：
//! 1. /mixed/456/docs/manual.pdf 应该匹配 mixed_file_path
//! 2. /negative/-123 应该匹配 negative_int
//! 3. /negative/-456.78 应该匹配 negative_float

use rat_engine::server::{Router, config::ServerConfig, http_request::HttpRequest};
use rat_engine::RatEngine;
use rat_engine::{Request, Method, StatusCode, Response};
use rat_engine::{Incoming, Frame, Bytes, Full};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 CryptoProvider
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    // 创建服务器配置
    let addr = "127.0.0.1:8888".parse().unwrap();
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());

    // 创建路由器
    let mut router = Router::new();

    // 只注册冲突的路由（与完整测试保持相同的注册顺序）

    // mixed 路由1：整数+字符串+浮点数
    router.add_route(
        Method::GET,
        "/mixed/<int:user_id>/<str:category>/<float:price>",
        |req| Box::pin(handle_mixed_params(req))
    );

    // mixed 路由2：整数+路径
    router.add_route(
        Method::GET,
        "/mixed/<int:user_id>/<path:file_path>",
        |req| Box::pin(handle_mixed_file_path(req))
    );

    // negative 路由1：负整数
    router.add_route(
        Method::GET,
        "/negative/<int:value>",
        |req| Box::pin(handle_negative_int(req))
    );

    // negative 路由2：负浮点数
    router.add_route(
        Method::GET,
        "/negative/<float:value>",
        |req| Box::pin(handle_negative_float(req))
    );

    println!("🚀 启动专注路由冲突调试服务器...");
    println!("📋 已注册的冲突测试路由:");
    for (method, pattern) in router.list_routes() {
        println!("  {}  {}", method, pattern);
    }
    println!();

    // 启动服务器
    let engine = RatEngine::builder()
        .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .router(router)
        .enable_development_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await
        .map_err(|e| format!("启用开发模式失败: {}", e))?
        .build()
        .map_err(|e| format!("构建引擎失败: {}", e))?;

    println!("🌐 服务器启动在 http://127.0.0.1:8888");

    // 在后台启动服务器
    let server_handle = tokio::spawn(async move {
        if let Err(e) = engine.start("127.0.0.1".to_string(), 8888).await {
            eprintln!("❌ 服务器启动失败: {}", e);
        }
    });

    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // 运行专注测试
    run_focused_tests().await?;

    // 关闭服务器
    server_handle.abort();

    Ok(())
}

async fn run_focused_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 开始专注路由冲突调试测试...");
    println!();

    // 测试用例：只测试3个问题
    let test_cases = vec![
        ("/mixed/456/docs/manual.pdf", "mixed_file_path", "文件路径应该匹配path路由"),
        ("/negative/-123", "negative_int", "纯整数应该匹配int路由"),
        ("/negative/-456.78", "negative_float", "浮点数应该匹配float路由"),
    ];

    let client = reqwest::Client::new();
    let base_url = "http://127.0.0.1:8888";

    let mut passed = 0;
    let total = test_cases.len();

    for (url, expected_route, description) in test_cases {
        println!("🧪 测试: {}", description);
        println!("   URL: {}", url);

        match client.get(&format!("{}{}", base_url, url)).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(data) = response.json::<serde_json::Value>().await {
                        let actual_route = data.get("route").and_then(|v| v.as_str()).unwrap_or("unknown");

                        println!("   📄 实际匹配: {}", actual_route);
                        println!("   🎯 期望匹配: {}", expected_route);

                        if actual_route == expected_route {
                            println!("   ✅ 测试通过");
                            passed += 1;
                        } else {
                            println!("   ❌ 测试失败");
                        }
                    } else {
                        println!("   ❌ JSON解析失败");
                    }
                } else {
                    println!("   ❌ HTTP错误: {}", response.status());
                }
            }
            Err(e) => {
                println!("   ❌ 请求失败: {}", e);
            }
        }

        println!();
    }

    println!("📊 专注测试结果: {}/{} 通过", passed, total);

    if passed == total {
        println!("🎉 所有问题都已解决！");
    } else {
        println!("⚠️  还有 {} 个问题需要解决", total - passed);
    }

    Ok(())
}

/// mixed参数处理器
async fn handle_mixed_params(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let response_data = json!({
        "route": "mixed_params",
        "user_id": req.param("user_id"),
        "category": req.param("category"),
        "price": req.param("price"),
        "description": "整数+字符串+浮点数"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// mixed文件路径处理器
async fn handle_mixed_file_path(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let response_data = json!({
        "route": "mixed_file_path",
        "user_id": req.param("user_id"),
        "file_path": req.param("file_path"),
        "description": "整数+路径"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 负整数处理器
async fn handle_negative_int(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let response_data = json!({
        "route": "negative_int",
        "value": req.param("value"),
        "description": "负整数"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 负浮点数处理器
async fn handle_negative_float(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let response_data = json!({
        "route": "negative_float",
        "value": req.param("value"),
        "description": "负浮点数"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}