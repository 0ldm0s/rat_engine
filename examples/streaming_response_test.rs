//! 验证 add_streaming_route 方法的JSON和HTTP状态码返回功能
//!
//! 这个示例测试：
//! 1. 流式路由能否返回正常的JSON数据
//! 2. 流式路由能否返回不同的HTTP状态码（如403）
//! 3. 流式路由的错误处理能力

use rat_engine::server::{
    Router,
    streaming::{StreamingResponse, SseResponse, ChunkedResponse},
    config::ServerConfig,
    http_request::HttpRequest,
};
use rat_engine::RatEngine;
use rat_engine::{Method, StatusCode, Response};
use rat_engine::{Bytes, Frame, Full};
use std::collections::HashMap;
use std::time::Duration;
use futures_util::{stream, StreamExt};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 CryptoProvider
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    // 创建服务器配置
    let addr = "127.0.0.1:3000".parse().unwrap();
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());

    // 创建路由器
    let mut router = Router::new();

    println!("🚀 启动流式响应测试服务器 http://127.0.0.1:3000");
    println!();
    println!("📋 测试端点：");
    println!("  GET  /json-stream     - 测试JSON流式响应");
    println!("  GET  /error-stream    - 测试错误状态码流式响应");
    println!("  GET  /auth-check      - 测试权限验证流式响应");
    println!("  GET  /sse-test        - 测试SSE流式响应");
    println!("  GET  /chunked-test    - 测试分块流式响应");
    println!();
    println!("🧪 测试方法：");
    println!("  curl -i http://127.0.0.1:3000/json-stream");
    println!("  curl -i http://127.0.0.1:3000/error-stream");
    println!("  curl -i http://127.0.0.1:3000/auth-check");
    println!("  curl -i http://127.0.0.1:3000/sse-test");
    println!("  curl -i http://127.0.0.1:3000/chunked-test");

    // 1. 测试正常的JSON流式响应
    router.add_streaming_route(
        Method::GET,
        "/json-stream",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                println!("✅ [JSON-Stream] 处理请求");

                // 创建JSON数据流
                let json_items = vec![
                    json!({"id": 1, "name": "张三", "status": "active"}),
                    json!({"id": 2, "name": "李四", "status": "pending"}),
                    json!({"id": 3, "name": "王五", "status": "active"}),
                ];

                let stream = stream::iter(json_items.into_iter())
                    .map(|item| {
                        let json_str = serde_json::to_string(&item).unwrap();
                        println!("📦 [JSON-Stream] 发送数据: {}", json_str);

                        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                            Frame::data(Bytes::from(format!("{}\n", json_str)))
                        )
                    });

                StreamingResponse::new()
                    .status(StatusCode::OK)
                    .with_header("Content-Type", "application/json; charset=utf-8")
                    .with_header("X-Stream-Type", "json-stream")
                    .stream(stream)
                    .build()
            })
        }
    );

    // 2. 测试错误状态码的流式响应
    router.add_streaming_route(
        Method::GET,
        "/error-stream",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                println!("❌ [Error-Stream] 处理错误请求");

                // 返回403状态码的流式响应
                let error_stream = stream::iter(vec![
                    json!({"error": "Access Denied"}),
                    json!({"code": 403}),
                    json!({"message": "您没有权限访问此资源"}),
                    json!({"timestamp": chrono::Utc::now().to_rfc3339()}),
                ])
                .map(|item| {
                    let json_str = serde_json::to_string(&item).unwrap();
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                        Frame::data(Bytes::from(format!("{}\n", json_str)))
                    )
                });

                StreamingResponse::new()
                    .status(StatusCode::FORBIDDEN) // 403状态码
                    .with_header("Content-Type", "application/json; charset=utf-8")
                    .with_header("X-Error-Code", "403")
                    .with_header("X-Error-Type", "permission-denied")
                    .stream(error_stream)
                    .build()
            })
        }
    );

    // 3. 测试带权限验证的流式响应
    router.add_streaming_route(
        Method::GET,
        "/auth-check",
        |req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                println!("🔐 [Auth-Check] 处理权限验证请求");

                // 检查是否有Authorization头
                let auth_header = req.header("Authorization");

                if auth_header.is_none() {
                    // 没有授权信息，返回401
                    let error_stream = stream::iter(vec![
                        json!({"error": "Authorization Required"}),
                        json!({"code": 401}),
                        json!({"message": "需要提供Authorization头"}),
                    ])
                    .map(|item| {
                        let json_str = serde_json::to_string(&item).unwrap();
                        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                            Frame::data(Bytes::from(format!("{}\n", json_str)))
                        )
                    });

                    return StreamingResponse::new()
                        .status(StatusCode::UNAUTHORIZED) // 401状态码
                        .with_header("Content-Type", "application/json; charset=utf-8")
                        .with_header("WWW-Authenticate", "Bearer realm=\"API\"")
                        .stream(error_stream)
                        .build();
                }

                // 有授权信息，返回正常响应
                let success_stream = stream::iter(vec![
                    json!({"status": "authenticated"}),
                    json!({"user": "admin"}),
                    json!({"permissions": ["read", "write", "admin"]}),
                ])
                .map(|item| {
                    let json_str = serde_json::to_string(&item).unwrap();
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                        Frame::data(Bytes::from(format!("{}\n", json_str)))
                    )
                });

                StreamingResponse::new()
                    .status(StatusCode::OK)
                    .with_header("Content-Type", "application/json; charset=utf-8")
                    .stream(success_stream)
                    .build()
            })
        }
    );

    // 4. 测试SSE流式响应（带URL参数验证）
    router.add_streaming_route(
        Method::GET,
        "/sse-test",
        |req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                println!("📡 [SSE-Test] 处理SSE请求");

                // 使用框架提供的查询参数解析方法
                let query_params = req.query_params();
                let token = query_params.get("token").cloned().unwrap_or_default();
                let event_type = query_params.get("event").cloned().unwrap_or_else(|| "message".to_string());

                println!("🔍 [SSE-Test] URL参数验证: token='{}', event='{}'", token, event_type);

                // 检查token参数
                if token.is_empty() {
                    println!("❌ [SSE-Test] 缺少token参数，返回401错误");
                    // 返回401错误响应
                    let error_stream = stream::iter(vec![
                        json!({"error": "Authentication Required"}),
                        json!({"code": 401}),
                        json!({"message": "SSE连接需要提供有效的token参数"}),
                        json!({"example": "/sse-test?token=valid_token&event=custom"}),
                    ])
                    .map(|item| {
                        let json_str = serde_json::to_string(&item).unwrap();
                        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                            Frame::data(Bytes::from(format!("{}\n", json_str)))
                        )
                    });

                    return StreamingResponse::new()
                        .status(StatusCode::UNAUTHORIZED)
                        .with_header("Content-Type", "application/json; charset=utf-8")
                        .with_header("X-Error-Type", "sse-auth-failed")
                        .stream(error_stream)
                        .build();
                }

                // 检查token是否有效（简单验证：长度至少为8）
                if token.len() < 8 {
                    println!("❌ [SSE-Test] token太短，返回403错误");
                    // 返回403错误响应
                    let error_stream = stream::iter(vec![
                        json!({"error": "Invalid Token"}),
                        json!({"code": 403}),
                        json!({"message": "提供的token无效或太短"}),
                        json!({"provided_token": token}),
                        json!({"requirement": "token长度至少8个字符"}),
                    ])
                    .map(|item| {
                        let json_str = serde_json::to_string(&item).unwrap();
                        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                            Frame::data(Bytes::from(format!("{}\n", json_str)))
                        )
                    });

                    return StreamingResponse::new()
                        .status(StatusCode::FORBIDDEN)
                        .with_header("Content-Type", "application/json; charset=utf-8")
                        .with_header("X-Error-Type", "sse-invalid-token")
                        .stream(error_stream)
                        .build();
                }

                println!("✅ [SSE-Test] token验证通过，建立SSE连接");

                let sse = SseResponse::new();
                let sender = sse.get_sender();
                let event_type_clone = event_type.clone();

                // 在后台任务中发送SSE事件
                tokio::spawn(async move {
                    // 发送连接成功事件
                    let connect_data = json!({
                        "event": "connected",
                        "message": "SSE连接已建立",
                        "token": token,
                        "event_type": event_type_clone,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    });
                    let formatted = format!("event: connected\ndata: {}\n\n", connect_data.to_string());
                    if sender.send(Ok(Frame::data(Bytes::from(formatted)))).is_err() {
                        eprintln!("❌ [SSE] 发送连接事件失败");
                        return;
                    }

                    for i in 1..=5 {
                        let event_data = json!({
                            "event": event_type_clone,
                            "data": format!("这是第 {} 条{}事件", i, event_type_clone),
                            "index": i,
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        });

                        // 使用sender直接发送SSE事件
                        let formatted = format!("event: {}\ndata: {}\n\n", event_type_clone, event_data.to_string());
                        if sender.send(Ok(Frame::data(Bytes::from(formatted)))).is_err() {
                            eprintln!("❌ [SSE] 发送事件失败，连接可能已断开");
                            break;
                        }

                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }

                    // 发送结束事件
                    let end_data = json!({
                        "event": "end",
                        "message": "SSE流结束",
                        "total_events": 5,
                        "event_type": event_type_clone,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    });
                    let formatted = format!("event: end\ndata: {}\n\n", end_data.to_string());
                    let _ = sender.send(Ok(Frame::data(Bytes::from(formatted))));
                });

                sse.build()
            })
        }
    );

    // 5. 测试分块流式响应
    router.add_streaming_route(
        Method::GET,
        "/chunked-test",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                println!("🧩 [Chunked-Test] 处理分块请求");

                ChunkedResponse::new()
                    .add_chunk("这是第一块数据\n")
                    .add_chunk("这是第二块数据\n")
                    .add_chunk("这是第三块数据\n")
                    .with_delay(Duration::from_millis(800))
                    .build()
            })
        }
    );

    // 添加主页路由，显示测试说明
    router.add_route(
        Method::GET,
        "/",
        |_req: HttpRequest| {
            Box::pin(async move {
                let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine 流式响应测试</title>
    <meta charset="UTF-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .test-section { margin: 20px 0; padding: 20px; border: 1px solid #ddd; border-radius: 5px; }
        .test-section h2 { color: #333; }
        .endpoint { background: #f5f5f5; padding: 10px; margin: 5px 0; border-radius: 3px; font-family: monospace; }
        .description { color: #666; margin: 5px 0; }
        .success { color: green; }
        .error { color: red; }
    </style>
</head>
<body>
    <h1>RAT Engine 流式响应测试</h1>

    <div class="test-section">
        <h2>📋 测试端点</h2>
        <div class="endpoint">
            <strong>GET /json-stream</strong> - 测试JSON流式响应
            <div class="description">验证流式路由能否正常返回JSON数据</div>
        </div>
        <div class="endpoint">
            <strong>GET /error-stream</strong> - 测试错误状态码流式响应
            <div class="description">验证流式路由能否返回403状态码和错误信息</div>
        </div>
        <div class="endpoint">
            <strong>GET /auth-check</strong> - 测试权限验证流式响应
            <div class="description">验证基于Authorization头的权限控制和401状态码</div>
        </div>
        <div class="endpoint">
            <strong>GET /sse-test?token=valid_token</strong> - 测试SSE流式响应（带验证）
            <div class="description">验证带URL参数验证的Server-Sent Events功能</div>
        </div>
        <div class="endpoint">
            <strong>GET /chunked-test</strong> - 测试分块流式响应
            <div class="description">验证分块传输编码功能</div>
        </div>
    </div>

    <div class="test-section">
        <h2>🧪 测试方法</h2>
        <p>使用curl命令测试各个端点：</p>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/json-stream
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/error-stream
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/auth-check
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/auth-check -H "Authorization: Bearer test-token"
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/sse-test --max-time 7 # 401错误（缺少token）
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/sse-test?token=short --max-time 7 # 403错误（token太短）
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/sse-test?token=valid_token&event=custom --max-time 7 # 正常SSE连接
        </div>
        <div class="endpoint">
            curl -i http://127.0.0.1:3000/chunked-test
        </div>
    </div>

    <div class="test-section">
        <h2>✅ 验证要点</h2>
        <ul>
            <li><span class="success">JSON流式响应</span>：检查是否返回正确的JSON数据和Content-Type</li>
            <li><span class="error">错误状态码</span>：检查/error-stream是否返回403状态码</li>
            <li><span class="error">权限验证</span>：检查/auth-check在无Authorization头时返回401</li>
            <li><span class="success">SSE功能</span>：检查/sse-test带参数验证的SSE连接</li>
            <li><span class="success">分块传输</span>：检查/chunked-test是否正确分块发送数据</li>
        </ul>
    </div>
</body>
</html>
                "#;

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Full::new(Bytes::from(html)))
                    .unwrap())
            })
        }
    );

    println!();
    println!("🎯 测试说明：");
    println!("  1. ✅ /json-stream - 应该返回200状态码和JSON数据流");
    println!("  2. ❌ /error-stream - 应该返回403状态码和错误信息流");
    println!("  3. 🔐 /auth-check - 无Authorization头返回401，有头返回200");
    println!("  4. 📡 /sse-test - 带URL参数验证：无token返回401，token短返回403，有效token返回SSE");
    println!("  5. 🧩 /chunked-test - 应该返回分块传输数据");
    println!();
    println!("按 Ctrl+C 停止服务器");

    // 启动服务器
    let engine = RatEngine::builder()
        .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .router(router)
        .enable_h2c_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await
        .map_err(|e| format!("启用开发模式失败: {}", e))?
        .build()
        .map_err(|e| format!("构建引擎失败: {}", e))?;

    engine.start("127.0.0.1".to_string(), 3000).await
        .map_err(|e| format!("启动服务器失败: {}", e))?;

    Ok(())
}