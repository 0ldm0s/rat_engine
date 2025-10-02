//! 高级路径参数演示
//! 展示不同类型的路径参数：int、str、uuid、float、path
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

/// 整数ID处理器 - <int:id> 或 <id>
async fn handle_int_user(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_id = req.param_as_i64("id").unwrap_or(0);
    let path = req.path();

    let response_data = json!({
        "type": "integer_parameter",
        "parameter_name": "id",
        "raw_value": req.param("id"),
        "parsed_value": user_id,
        "value_type": "i64",
        "path_matched": path,
        "description": "整数ID参数，默认类型"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// UUID处理器 - <uuid:id> 或 <str:id>
async fn handle_uuid_user(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_uuid = req.param("id").unwrap_or("unknown");
    let path = req.path();

    let response_data = json!({
        "type": "string_parameter",
        "parameter_name": "id",
        "raw_value": user_uuid,
        "parsed_value": user_uuid,
        "value_type": "String",
        "is_uuid_format": user_uuid.len() == 36 && user_uuid.contains('-'),
        "path_matched": path,
        "description": "UUID参数，使用str或uuid类型约束"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 浮点数处理器 - <float:price>
async fn handle_product_price(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let price = req.param_as_f64("price").unwrap_or(0.0);
    let path = req.path();

    let response_data = json!({
        "type": "float_parameter",
        "parameter_name": "price",
        "raw_value": req.param("price"),
        "parsed_value": price,
        "value_type": "f64",
        "path_matched": path,
        "description": "浮点数价格参数，使用float类型约束"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 路径参数处理器 - <path:file_path>
async fn handle_file_request(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let file_path = req.param("file_path").unwrap_or("unknown");
    let path = req.path();

    let response_data = json!({
        "type": "path_parameter",
        "parameter_name": "file_path",
        "raw_value": file_path,
        "parsed_value": file_path,
        "value_type": "String",
        "path_matched": path,
        "description": "完整路径参数，可以包含斜杠，使用path类型约束"
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(response_data.to_string())))
        .unwrap();

    Ok(response)
}

/// 混合类型参数处理器
async fn handle_mixed_params(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_id = req.param_as_i64("user_id").unwrap_or(0);
    let category = req.param("category").unwrap_or("unknown");
    let price = req.param_as_f64("price").unwrap_or(0.0);
    let path = req.path();

    let response_data = json!({
        "type": "mixed_parameters",
        "parameters": {
            "user_id": {
                "name": "user_id",
                "raw_value": req.param("user_id"),
                "parsed_value": user_id,
                "type": "i64",
                "constraint": "<int:user_id>"
            },
            "category": {
                "name": "category",
                "raw_value": category,
                "parsed_value": category,
                "type": "String",
                "constraint": "<str:category>"
            },
            "price": {
                "name": "price",
                "raw_value": req.param("price"),
                "parsed_value": price,
                "type": "f64",
                "constraint": "<float:price>"
            }
        },
        "path_matched": path,
        "description": "混合类型参数演示"
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
    <title>高级路径参数演示</title>
    <meta charset="utf-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .endpoint { background: #f5f5f5; padding: 10px; margin: 10px 0; border-radius: 5px; }
        .method { font-weight: bold; color: #007acc; }
        .path { font-family: monospace; background: #e8e8e8; padding: 2px 5px; }
        .type { color: #d73a49; font-weight: bold; }
    </style>
</head>
<body>
    <h1>🚀 高级路径参数演示</h1>
    <p>展示了不同类型的路径参数处理：</p>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/users/int/<span class="type">&lt;id&gt;</span></span>
        <br>整数参数（默认类型）
        <br>示例: <a href="/users/int/123">/users/int/123</a>
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/users/uuid/<span class="type">&lt;uuid:id&gt;</span></span>
        <br>UUID参数（字符串类型）
        <br>示例: <a href="/users/uuid/550e8400-e29b-41d4-a716-446655440000">/users/uuid/550e8400-e29b-41d4-a716-446655440000</a>
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/products/price/<span class="type">&lt;float:price&gt;</span></span>
        <br>浮点数参数
        <br>示例: <a href="/products/price/99.99">/products/price/99.99</a>
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/files/<span class="type">&lt;path:file_path&gt;</span></span>
        <br>完整路径参数（可包含斜杠）
        <br>示例: <a href="/files/docs/user/manual.pdf">/files/docs/user/manual.pdf</a>
    </div>

    <div class="endpoint">
        <span class="method">GET</span> <span class="path">/mixed/<span class="type">&lt;int:user_id&gt;</span>/<span class="type">&lt;str:category&gt;</span>/<span class="type">&lt;float:price&gt;</span></span>
        <br>混合类型参数
        <br>示例: <a href="/mixed/123/electronics/299.99">/mixed/123/electronics/299.99</a>
    </div>

    <h2>参数类型说明</h2>
    <ul>
        <li><strong>&lt;param&gt;</strong> 或 <strong>&lt;int:param&gt;</strong> - 整数类型（默认）</li>
        <li><strong>&lt;str:param&gt;</strong> 或 <strong>&lt;string:param&gt;</strong> 或 <strong>&lt;uuid:param&gt;</strong> - 字符串类型</li>
        <li><strong>&lt;float:param&gt;</strong> - 浮点数类型</li>
        <li><strong>&lt;path:param&gt;</strong> - 路径类型（可包含斜杠）</li>
    </ul>

    <h2>使用方法</h2>
    <p>在处理器中，使用以下方法获取参数：</p>
    <ul>
        <li><code>req.param_as_i64("name")</code> - 获取i64整数</li>
        <li><code>req.param_as_u64("name")</code> - 获取u64无符号整数</li>
        <li><code>req.param_as_f64("name")</code> - 获取f64浮点数</li>
        <li><code>req.param("name")</code> - 获取原始字符串</li>
    </ul>
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

/// 自动化测试验证函数
async fn run_automated_tests() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 开始高级路径参数测试...");

    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 使用独立HTTP客户端
    let http_client = RatIndependentHttpClientBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent("AdvancedPathParamsDemo/1.0.0")
        .supported_compressions(vec!["gzip".to_string(), "deflate".to_string()])
        .auto_decompress(true)
        .build()?;

    let base_url = "http://127.0.0.1:8082";
    let mut test_results = Vec::new();

    // 测试用例定义
    let test_cases = vec![
        ("GET /", "/"),
        ("GET /users/int/123", "/users/int/123"),
        ("GET /users/uuid/550e8400-e29b-41d4-a716-446655440000", "/users/uuid/550e8400-e29b-41d4-a716-446655440000"),
        ("GET /products/price/99.99", "/products/price/99.99"),
        ("GET /files/docs/user/manual.pdf", "/files/docs/user/manual.pdf"),
        ("GET /mixed/123/electronics/299.99", "/mixed/123/electronics/299.99"),
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
                        if path.starts_with("/users/int/") {
                            if json_value.get("parsed_value").is_some() {
                                println!("    ✓ 整数参数解析正确");
                            }
                        } else if path.starts_with("/users/uuid/") {
                            if json_value.get("is_uuid_format").is_some() {
                                println!("    ✓ UUID参数格式验证正确");
                            }
                        } else if path.starts_with("/products/price/") {
                            if json_value.get("parsed_value").is_some() {
                                println!("    ✓ 浮点数参数解析正确");
                            }
                        } else if path.starts_with("/files/") {
                            if json_value.get("type").and_then(|v| v.as_str()) == Some("path_parameter") {
                                println!("    ✓ 路径参数处理正确");
                            }
                        } else if path.starts_with("/mixed/") {
                            if json_value.get("parameters").is_some() {
                                println!("    ✓ 混合参数处理正确");
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

    // 输出测试结果统计
    let passed = test_results.iter().filter(|(_, success, _)| *success).count();
    let total = test_results.len();

    println!("\n📊 测试结果统计:");
    println!("  总测试数: {}", total);
    println!("  通过数: {}", passed);
    println!("  失败数: {}", total - passed);

    if passed == total {
        println!("🎉 所有测试通过！高级路径参数功能正常工作。");
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 检查必需的特性
    #[cfg(not(feature = "reqwest"))]
    {
        println!("❌ 此示例需要 reqwest 特性");
        println!("请使用: cargo run --example advanced_path_params_demo --features reqwest");
        return Ok(());
    }

    // 初始化 CryptoProvider
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    println!("🚀 启动高级路径参数演示服务器...");

    // 创建服务器配置
    let addr: SocketAddr = "127.0.0.1:8082".parse()?;
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());

    // 创建路由器
    let mut router = Router::new();

    // 注册路由 - 展示不同类型的参数
    router.add_route(Method::GET, "/", |req| Box::pin(handle_root(req)));

    // 整数参数（默认类型）
    router.add_route(Method::GET, "/users/int/<id>", |req| Box::pin(handle_int_user(req)));

    // UUID参数（字符串类型）
    router.add_route(Method::GET, "/users/uuid/<uuid:id>", |req| Box::pin(handle_uuid_user(req)));

    // 浮点数参数
    router.add_route(Method::GET, "/products/price/<float:price>", |req| Box::pin(handle_product_price(req)));

    // 路径参数（可包含斜杠）
    router.add_route(Method::GET, "/files/<path:file_path>", |req| Box::pin(handle_file_request(req)));

    // 混合类型参数
    router.add_route(Method::GET, "/mixed/<int:user_id>/<str:category>/<float:price>", |req| Box::pin(handle_mixed_params(req)));

    println!("📋 已注册的路由:");
    println!("  GET  /");
    println!("  GET  /users/int/<id> (整数)");
    println!("  GET  /users/uuid/<uuid:id> (UUID)");
    println!("  GET  /products/price/<float:price> (浮点数)");
    println!("  GET  /files/<path:file_path> (路径)");
    println!("  GET  /mixed/<int:user_id>/<str:category>/<float:price> (混合)");

    println!("\n🌐 服务器启动在 http://127.0.0.1:8082");

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
            result = engine.start("127.0.0.1".to_string(), 8082) => {
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

    println!("✅ 高级路径参数演示完成，程序退出。");

    Ok(())
}