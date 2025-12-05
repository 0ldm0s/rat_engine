//! gRPC 客户端预解析IP功能一元请求测试
//!
//! 基于grpc_comprehensive_example.rs简化而来，专门测试一元请求和预解析IP功能

use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use rat_engine::{
    server::{
        grpc_handler::UnaryHandler,
        grpc_types::{GrpcError, GrpcContext, GrpcRequest, GrpcResponse, GrpcStatusCode},
        Router, ServerConfig
    },
    engine::RatEngine,
    utils::logger::{info, warn, error, debug},
};

/// 简单的请求消息
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct SimpleRequest {
    pub message: String,
    pub timestamp: i64,
}

/// 简单的响应消息
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct SimpleResponse {
    pub echo: String,
    pub server_time: i64,
    pub status: String,
}

/// 简单的gRPC处理器
#[derive(Debug)]
struct SimpleGrpcHandler {
    request_count: std::sync::atomic::AtomicU64,
}

impl SimpleGrpcHandler {
    fn new() -> Self {
        Self {
            request_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl UnaryHandler for SimpleGrpcHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        let count = self.request_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

        Box::pin(async move {
            // 解码请求数据
            let simple_request: SimpleRequest = match bincode::decode_from_slice(&request.data, bincode::config::standard()) {
                Ok((req, _)) => req,
                Err(e) => {
                    let error_msg = format!("请求解码失败: {}", e);
                    error!("❌ {}", error_msg);
                    return Err(GrpcError::InvalidArgument(error_msg));
                }
            };

            info!("📨 收到请求 #{}: {}", count, simple_request.message);

            // 创建响应
            let response_data = SimpleResponse {
                echo: format!("回声: {}", simple_request.message),
                server_time: chrono::Utc::now().timestamp(),
                status: "success".to_string(),
            };

            info!("📤 发送响应 #{}: {}", count, response_data.echo);

            // 编码响应数据
            let response_bytes = match bincode::encode_to_vec(&response_data, bincode::config::standard()) {
                Ok(bytes) => bytes,
                Err(e) => {
                    let error_msg = format!("响应编码失败: {}", e);
                    error!("❌ {}", error_msg);
                    return Err(GrpcError::Internal(error_msg));
                }
            };

            Ok(GrpcResponse {
                data: response_bytes,
                status: 0u32, // 0 表示成功
                message: "请求处理成功".to_string(),
                metadata: std::collections::HashMap::new(),
            })
        })
    }
}

/// 启动测试服务器
async fn start_test_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("🚀 启动 gRPC 测试服务器...");

    // 创建处理器
    let handler = SimpleGrpcHandler::new();

    // 创建服务器配置
    let config = ServerConfig::new(
        "127.0.0.1:50053".parse()?,
        4
    );

    // 创建路由器
    let mut router = Router::new();
    router.enable_h2(); // 启用 HTTP/2
    router.enable_h2c(); // 启用 H2C
    router.add_grpc_unary("/test.Simple/Echo", handler);

    // 配置日志
    let mut log_config = rat_engine::utils::logger::LogConfig::default();
    log_config.level = rat_engine::utils::logger::LogLevel::Debug;

    // 使用 RatEngineBuilder
    let engine = RatEngine::builder()
        .router(router)
        .with_log_config(log_config)
        .enable_development_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()])
        .await
        .map_err(|e| format!("配置开发模式失败: {}", e))?
        .build()
        .map_err(|e| format!("创建服务器失败: {}", e))?;

    // 启动服务器
    info!("🌐 gRPC 服务器启动成功，监听地址: 127.0.0.1:50053");
    engine.start("127.0.0.1".to_string(), 50053).await
        .map_err(|e| format!("服务器运行失败: {}", e).into())
}

/// 运行一元请求测试
async fn run_unary_test() -> Result<(), Box<dyn std::error::Error>> {
    info!("🧪 开始测试预解析IP功能...");

    // 创建DNS映射表
    let mut dns_mapping = HashMap::new();
    // 使用一个绝对假的域名，如果没有预解析IP会连接失败
    dns_mapping.insert("this-domain-absolutely-does-not-exist-12345.com".to_string(), "127.0.0.1".to_string());
    dns_mapping.insert("fake-microservice.prod.internal".to_string(), "127.0.0.1".to_string());
    // 保留localhost作为对照组
    dns_mapping.insert("localhost".to_string(), "127.0.0.1".to_string());

    info!("📋 DNS映射表:");
    for (domain, ip) in &dns_mapping {
        info!("  {} -> {}", domain, ip);
    }

    // 创建支持预解析IP的gRPC客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))?
        .request_timeout(Duration::from_secs(10))?
        .max_idle_connections(5)?
        .http2_only()
        .user_agent("rat-engine-dns-test/1.0")?
        .disable_compression()
        .development_mode()
        .with_dns_mapping(dns_mapping)?
        .build()?;

    info!("✅ gRPC 客户端创建成功");

    // 测试用例列表
    let test_cases = vec![
        ("使用绝对假的域名（证明预解析IP）", "http://this-domain-absolutely-does-not-exist-12345.com:50053"),
        ("使用另一个假域名", "http://fake-microservice.prod.internal:50053"),
        ("使用localhost连接（对照组）", "http://localhost:50053"),
        ("使用IP直接连接", "http://127.0.0.1:50053"),
    ];

    for (description, server_url) in test_cases {
        info!("\n🔗 测试: {}", description);
        info!("📍 连接地址: {}", server_url);

        // 创建请求
        let request = SimpleRequest {
            message: format!("测试消息 - {}", description),
            timestamp: chrono::Utc::now().timestamp(),
        };
        let request_timestamp = request.timestamp; // 保存时间戳用于计算延迟

        // 发送一元请求
        match client.call_typed_with_uri::<SimpleRequest, SimpleResponse>(
            server_url,
            "test.Simple",
            "Echo",
            request,
            None,
        ).await {
            Ok(response) => {
                info!("✅ 请求成功");
                info!("📥 响应内容:");
                info!("   echo: {}", response.data.echo);
                info!("   server_time: {}", response.data.server_time);
                info!("   status: {}", response.data.status);
                info!("   延迟: {}ms", response.data.server_time - request_timestamp);
            }
            Err(e) => {
                error!("❌ 请求失败: {:?}", e);
            }
        }

        // 等待一秒再进行下一个测试
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // 测试错误情况 - 连接不存在的域名
    info!("\n❌ 测试: 连接不存在的域名（应该失败）");
    let error_request = SimpleRequest {
        message: "应该失败的请求".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    match client.call_typed_with_uri::<SimpleRequest, SimpleResponse>(
        "http://nonexistent.test:50053",
        "test.Simple",
        "Echo",
        error_request,
        None,
    ).await {
        Ok(response) => {
            warn!("⚠️ 意外成功: {:?}", response.data.echo);
        }
        Err(e) => {
            info!("✅ 预期失败: {:?}", e);
        }
    }

    // 关闭客户端
    client.shutdown().await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 检查必需的特性
    rat_engine::require_features!("client", "tls");

    // 确保 CryptoProvider 只安装一次
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    println!("🚀 启动 gRPC 客户端预解析IP功能一元请求测试");

    // 启动服务器任务
    let server_task = tokio::spawn(async {
        if let Err(e) = start_test_server().await {
            error!("❌ 服务器启动失败: {}", e);
        }
    });

    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 运行测试
    let test_result = run_unary_test().await;

    // 处理测试结果
    match test_result {
        Ok(_) => {
            println!("✅ 预解析IP功能测试完成");
        }
        Err(e) => {
            eprintln!("❌ 测试失败: {}", e);
            return Err(e);
        }
    }

    // 等待一段时间让服务器完成清理
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 终止服务器任务
    server_task.abort();

    println!("🧹 测试程序结束");

    Ok(())
}