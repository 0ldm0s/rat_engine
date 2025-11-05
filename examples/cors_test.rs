//! CORS 测试示例 - 模拟浏览器行为
//!
//! 这个示例完全模拟浏览器的CORS行为，包括：
//! 1. 发送带 Origin 头部的请求
//! 2. 测试预检请求 (OPTIONS)
//! 3. 验证SSE端点的CORS响应
//! 4. 测试各种跨域场景

use rat_engine::RatEngine;
use rat_engine::server::{
    Router,
    config::ServerConfig,
    streaming::SseResponse,
    http_request::HttpRequest,
    cors::CorsConfig,
};
use hyper::{Method, StatusCode, body::Bytes};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 CORS 测试示例 - 模拟浏览器行为");
    println!("=====================================");

    // 初始化日志
    let _engine = RatEngine::builder()
        .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .build();

    // 创建服务器配置
    let addr = "127.0.0.1:3002".parse().unwrap();
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());

    // 创建路由器并配置CORS
    let mut router = Router::new();

    // 配置CORS - 允许所有来源以方便测试
    let cors_config = CorsConfig::new()
        .enable()
        .allowed_origins(vec!["*".to_string()])
        .allowed_methods(vec![
            Method::GET, Method::POST, Method::PUT,
            Method::DELETE, Method::OPTIONS, Method::HEAD
        ])
        .allowed_headers(vec![
            "Content-Type".to_string(),
            "Authorization".to_string(),
            "X-Requested-With".to_string(),
        ])
        .max_age(3600);

    router.enable_cors(cors_config);

    // 添加标准HTTP路由用于对比
    router.add_route(
        Method::GET,
        "/api/test",
        |_req: HttpRequest| {
            Box::pin(async move {
                Ok(hyper::Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(hyper::body::Full::new(Bytes::from(r#"{"message": "标准API响应"}"#)))
                    .unwrap())
            })
        },
    );

    // 添加SSE路由 - 这是我们重点测试的
    router.add_streaming_route(
        Method::GET,
        "/sse/test",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                let mut sse = SseResponse::new();

                // 发送测试数据
                for i in 1..=5 {
                    let data = format!("{{\"id\": {}, \"message\": \"SSE测试数据\", \"timestamp\": \"{}\"}}",
                                     i, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
                    sse.send_data(&data);
                    sleep(Duration::from_millis(500)).await;
                }

                sse.build()
            })
        },
    );

    // 启动服务器
    let server = RatEngine::builder()
        .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .router(router)
        .enable_development_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await
        .map_err(|e| format!("启用开发模式失败: {}", e))?
        .build()
        .await
        .start_server()
        .await?;

    println!("✅ 服务器启动成功，地址: http://127.0.0.1:3002");
    println!();

    // 等待服务器完全启动
    sleep(Duration::from_secs(1)).await;

    // 开始CORS测试
    run_cors_tests().await?;

    // 保持服务器运行以便手动测试
    println!();
    println!("🚀 服务器继续运行，您可以手动测试：");
    println!("   标准API: http://127.0.0.1:3002/api/test");
    println!("   SSE端点: http://127.0.0.1:3002/sse/test");
    println!("   按Ctrl+C停止");

    // 等待中断信号
    tokio::signal::ctrl_c().await?;
    server.shutdown().await?;

    println!("🛑 服务器已停止");
    Ok(())
}

/// 运行完整的CORS测试套件
async fn run_cors_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 开始CORS测试");
    println!("===============");

    // 测试场景配置
    let test_origins = vec![
        "https://example.com",
        "https://sub.example.com",
        "http://localhost:3000",
        "https://www.google.com",
        "null", // 测试特殊origin
    ];

    let endpoints = vec![
        ("/api/test", "标准API"),
        ("/sse/test", "SSE端点"),
    ];

    // 1. 测试简单跨域请求
    println!("\n📋 测试1: 简单跨域请求");
    for (endpoint, desc) in &endpoints {
        println!("   📍 测试端点: {} ({})", endpoint, desc);

        for origin in &test_origins {
            let result = test_cors_request("GET", endpoint, origin, None).await;
            match result {
                Ok(cors_headers) => {
                    println!("     ✅ Origin {}: {:?}", origin, cors_headers);
                }
                Err(e) => {
                    println!("     ❌ Origin {}: {}", origin, e);
                }
            }
        }
    }

    // 2. 测试预检请求
    println!("\n📋 测试2: 预检请求 (OPTIONS)");
    for (endpoint, desc) in &endpoints {
        println!("   📍 测试端点: {} ({})", endpoint, desc);

        let result = test_preflight_request(endpoint, "https://example.com").await;
        match result {
            Ok(headers) => {
                println!("     ✅ 预检请求成功: {:?}", headers);
            }
            Err(e) => {
                println!("     ❌ 预检请求失败: {}", e);
            }
        }
    }

    // 3. 测试SSE流式响应的CORS
    println!("\n📋 测试3: SSE流式响应CORS");
    let result = test_sse_cors_streaming("https://example.com").await;
    match result {
        Ok(()) => {
            println!("     ✅ SSE流式响应CORS正常");
        }
        Err(e) => {
            println!("     ❌ SSE流式响应CORS失败: {}", e);
        }
    }

    // 4. 测试不同的CORS配置场景
    println!("\n📋 测试4: CORS配置场景");
    test_cors_scenarios().await?;

    println!("\n🎉 CORS测试完成！");
    Ok(())
}

/// 测试单个CORS请求
async fn test_cors_request(
    method: &str,
    endpoint: &str,
    origin: &str,
    custom_headers: Option<Vec<(&str, &str)>>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:3002{}", endpoint);

    let mut request = client.request(method.parse().unwrap(), &url);
    request = request.header("Origin", origin);
    request = request.header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36");

    if let Some(headers) = custom_headers {
        for (key, value) in headers {
            request = request.header(key, value);
        }
    }

    let response = request.send().await?;

    let mut cors_headers = Vec::new();

    // 检查CORS响应头
    if let Some(header) = response.headers().get("Access-Control-Allow-Origin") {
        cors_headers.push(format!("Access-Control-Allow-Origin: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Allow-Methods") {
        cors_headers.push(format!("Access-Control-Allow-Methods: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Allow-Headers") {
        cors_headers.push(format!("Access-Control-Allow-Headers: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Allow-Credentials") {
        cors_headers.push(format!("Access-Control-Allow-Credentials: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Max-Age") {
        cors_headers.push(format!("Access-Control-Max-Age: {:?}", header.to_str()?));
    }

    Ok(cors_headers)
}

/// 测试预检请求
async fn test_preflight_request(endpoint: &str, origin: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:3002{}", endpoint);

    let response = client
        .request(reqwest::Method::OPTIONS, &url)
        .header("Origin", origin)
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "Content-Type, Authorization")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .send()
        .await?;

    let mut headers = Vec::new();

    if let Some(header) = response.headers().get("Access-Control-Allow-Origin") {
        headers.push(format!("Allow-Origin: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Allow-Methods") {
        headers.push(format!("Allow-Methods: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Allow-Headers") {
        headers.push(format!("Allow-Headers: {:?}", header.to_str()?));
    }

    if let Some(header) = response.headers().get("Access-Control-Max-Age") {
        headers.push(format!("Max-Age: {:?}", header.to_str()?));
    }

    Ok(headers)
}

/// 测试SSE流式响应的CORS
async fn test_sse_cors_streaming(origin: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:3002/sse/test";

    let mut event_count = 0;
    let mut cors_found = false;

    let mut response = client
        .get(url)
        .header("Origin", origin)
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .send()
        .await?;

    // 首先检查响应头中的CORS
    println!("     📊 SSE响应头部:");
    for (name, value) in response.headers().iter() {
        let name_str = name.as_str();
        if name_str.starts_with("Access-Control") {
            println!("       {}: {:?}", name_str, value.to_str()?);
            cors_found = true;
        }
    }

    if !cors_found {
        return Err("SSE响应中未找到CORS头部".into());
    }

    // 读取几个SSE事件
    while let Some(chunk) = response.chunk().await? {
        let data = String::from_utf8_lossy(&chunk);
        if !data.trim().is_empty() {
            event_count += 1;
            println!("     📥 SSE事件 #{}: {}", event_count, data.trim());

            if event_count >= 3 {
                break;
            }
        }
    }

    if event_count == 0 {
        return Err("未收到SSE事件".into());
    }

    Ok(())
}

/// 测试不同的CORS配置场景
async fn test_cors_scenarios() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🎯 测试通配符Origin");

    // 测试通配符匹配
    let test_cases = vec![
        ("https://sub.example.com", "*.example.com"),
        ("https://api.test.com", "*.test.com"),
        ("https://localhost:3000", "http://localhost:*"),
    ];

    for (origin, pattern) in test_cases {
        println!("     测试 {} 是否匹配模式 {}", origin, pattern);
        let result = test_cors_request("GET", "/api/test", origin, None).await;
        match result {
            Ok(headers) => {
                println!("       ✅ 匹配成功: {:?}", headers);
            }
            Err(e) => {
                println!("       ❌ 匹配失败: {}", e);
            }
        }
    }

    Ok(())
}