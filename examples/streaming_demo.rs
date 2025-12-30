//! RAT Engine 流式响应演示
//! 
//! 展示如何使用 RAT Engine 的流式响应功能：
//! - Server-Sent Events (SSE)
//! - 分块传输编码
//! - 自定义流式响应

use rat_engine::server::{
    Router,
    streaming::{SseResponse, ChunkedResponse, StreamingResponse, utils},
    config::ServerConfig,
    http_request::HttpRequest,
    global_sse_manager::{get_global_sse_manager, send_sse_message_with_type, broadcast_sse_message}
};
use rat_engine::RatEngine;
use rat_engine::{Request, Method, StatusCode, Response};
use rat_engine::{Incoming, Frame, Bytes, Full};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::time::{sleep, Duration};
use serde_json::json;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 CryptoProvider
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();
    
    // 日志通过RatEngineBuilder初始化
    
    // 创建服务器配置
    let addr = "127.0.0.1:3000".parse().unwrap();
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());
    
    // 创建路由器
    let mut router = Router::new();
    
    // 注册 SSE 路由 - 使用全局管理器
    router.add_streaming_route(
        Method::GET,
        "/sse",
        |req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                // 生成唯一的连接ID（使用客户端IP作为标识）
                let client_ip = req.client_ip().to_string();
                let connection_id = format!("sse-{}-{}", client_ip, Uuid::new_v4().to_string()[..8].to_string());

                // 获取全局管理器并创建连接
                let manager = get_global_sse_manager();
                let response = manager.register_connection(connection_id.clone())?;

                // 发送初始连接事件
                send_sse_message_with_type(&connection_id, "connected", &json!({
                    "connection_id": connection_id,
                    "client_ip": client_ip,
                    "message": "Connection established with global manager"
                }).to_string()).unwrap();

                // 启动后台任务演示全局管理器的功能
                let connection_id_clone = connection_id.clone();
                tokio::spawn(async move {
                    // 模拟向特定连接发送消息
                    for i in 1..=15 {
                        sleep(Duration::from_secs(2)).await;

                        let data = json!({
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "counter": i,
                            "connection_id": connection_id_clone,
                            "message": format!("Global manager message #{}", i)
                        });

                        // 使用全局管理器发送带类型的事件
                        if let Err(e) = send_sse_message_with_type(&connection_id_clone, "update", &data.to_string()) {
                            println!("❌ 发送消息失败，连接可能已断开: {}", e);
                            break;  // 连接断开，退出后台任务
                        }
                    }

                    // 发送结束事件
                    let _ = send_sse_message_with_type(&connection_id_clone, "end", "Stream completed via global manager");
                });

                Ok(response)
            })
        }
    );
    
    // 注册分块传输路由
    router.add_streaming_route(
        Method::GET,
        "/chunked",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                let response = ChunkedResponse::new()
                    .add_chunk("开始数据传输...\n")
                    .add_chunk("正在处理第一部分数据...\n")
                    .add_chunk("正在处理第二部分数据...\n")
                    .add_chunk("正在处理第三部分数据...\n")
                    .add_chunk("数据传输完成！\n")
                    .with_delay(Duration::from_millis(500));
                
                response.build()
            })
        }
    );
    
    // 注册 JSON 流路由
    router.add_streaming_route(
        Method::GET,
        "/json-stream",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                let items = vec![
                    json!({"id": 1, "name": "Alice", "age": 30}),
                    json!({"id": 2, "name": "Bob", "age": 25}),
                    json!({"id": 3, "name": "Charlie", "age": 35}),
                    json!({"id": 4, "name": "Diana", "age": 28}),
                    json!({"id": 5, "name": "Eve", "age": 32}),
                ];
                
                utils::json_stream(items)
            })
        }
    );
    
    // 注册文本流路由
    router.add_streaming_route(
        Method::GET,
        "/text-stream",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                let lines = vec![
                    "第一行文本".to_string(),
                    "第二行文本".to_string(),
                    "第三行文本".to_string(),
                    "第四行文本".to_string(),
                    "最后一行文本".to_string(),
                ];
                
                utils::text_stream(lines)
            })
        }
    );
    
    // 注册实时日志流路由
    router.add_streaming_route(
        Method::GET,
        "/logs",
        |_req: HttpRequest, _params: HashMap<String, String>| {
            Box::pin(async move {
                let sse = SseResponse::new();
                
                // 发送初始日志
                sse.send_event("log", "[INFO] 日志流已启动").unwrap();
                
                // 模拟实时日志
                let sender = sse.get_sender();
                tokio::spawn(async move {
                    let log_levels = ["INFO", "WARN", "ERROR", "DEBUG"];
                    let messages = [
                        "用户登录成功",
                        "数据库连接建立",
                        "处理用户请求",
                        "缓存更新完成",
                        "定时任务执行",
                        "系统健康检查",
                    ];
                    
                    for i in 0..20 {
                        sleep(Duration::from_millis(800)).await;
                        
                        let level = log_levels[i % log_levels.len()];
                        let message = messages[i % messages.len()];
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        
                        let log_entry = format!("[{}] {} - {}", timestamp, level, message);
                        let formatted = format!("event: log\ndata: {}\n\n", log_entry);
                        
                        if sender.send(Ok(Frame::data(Bytes::from(formatted)))).is_err() {
                            break;
                        }
                    }
                });
                
                sse.build()
            })
        }
    );
    
    // 注册主页路由
    router.add_route(
        Method::GET,
        "/",
        |_req: HttpRequest| {
            Box::pin(async move {
                let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine 流式响应演示</title>
    <meta charset="UTF-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .demo-section { margin: 20px 0; padding: 20px; border: 1px solid #ddd; border-radius: 5px; }
        .log-output { background: #f5f5f5; padding: 10px; height: 200px; overflow-y: auto; font-family: monospace; }
        button { padding: 10px 20px; margin: 5px; cursor: pointer; }
    </style>
</head>
<body>
    <h1>RAT Engine 流式响应演示</h1>
    
    <div class="demo-section">
        <h2>Server-Sent Events (SSE) 演示</h2>
        <button onclick="startSSE()">开始 SSE 连接</button>
        <button onclick="stopSSE()">停止 SSE 连接</button>
        <div id="sse-output" class="log-output"></div>
    </div>
    
    <div class="demo-section">
        <h2>实时日志流演示</h2>
        <button onclick="startLogs()">开始日志流</button>
        <button onclick="stopLogs()">停止日志流</button>
        <div id="log-output" class="log-output"></div>
    </div>
    
    <div class="demo-section">
        <h2>其他流式端点</h2>
        <ul>
            <li><a href="/chunked" target="_blank">分块传输演示</a></li>
            <li><a href="/json-stream" target="_blank">JSON 流演示</a></li>
            <li><a href="/text-stream" target="_blank">文本流演示</a></li>
        </ul>
    </div>
    
    <script>
        let sseConnection = null;
        let logConnection = null;
        
        function startSSE() {
            if (sseConnection) return;
            
            const output = document.getElementById('sse-output');
            output.innerHTML = '';
            
            sseConnection = new EventSource('/sse');
            
            sseConnection.onopen = function() {
                output.innerHTML += '[连接已建立]\n';
            };
            
            sseConnection.addEventListener('connected', function(e) {
                output.innerHTML += '[连接事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            sseConnection.addEventListener('update', function(e) {
                output.innerHTML += '[更新事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            sseConnection.addEventListener('end', function(e) {
                output.innerHTML += '[结束事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
                sseConnection.close();
                sseConnection = null;
            });
            
            sseConnection.onerror = function() {
                output.innerHTML += '[连接错误]\n';
                output.scrollTop = output.scrollHeight;
            };
        }
        
        function stopSSE() {
            if (sseConnection) {
                sseConnection.close();
                sseConnection = null;
                document.getElementById('sse-output').innerHTML += '[连接已关闭]\n';
            }
        }
        
        function startLogs() {
            if (logConnection) return;
            
            const output = document.getElementById('log-output');
            output.innerHTML = '';
            
            logConnection = new EventSource('/logs');
            
            logConnection.addEventListener('log', function(e) {
                output.innerHTML += e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            logConnection.onerror = function() {
                output.innerHTML += '[日志连接错误]\n';
                output.scrollTop = output.scrollHeight;
            };
        }
        
        function stopLogs() {
            if (logConnection) {
                logConnection.close();
                logConnection = null;
                document.getElementById('log-output').innerHTML += '[日志连接已关闭]\n';
            }
        }
    </script>
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
    
    println!("🚀 RAT Engine 流式响应演示服务器启动中...");
    println!("📡 服务器地址: http://127.0.0.1:3000");
    println!("🔗 演示页面: http://127.0.0.1:3000/");
    println!("📊 SSE 端点: http://127.0.0.1:3000/sse");
    println!("📦 分块传输: http://127.0.0.1:3000/chunked");
    println!("📄 JSON 流: http://127.0.0.1:3000/json-stream");
    println!("📝 文本流: http://127.0.0.1:3000/text-stream");
    println!("📋 日志流: http://127.0.0.1:3000/logs");
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