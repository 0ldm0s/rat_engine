//! gRPC 客户端双向流 mTLS 示例（委托模式）
//!
//! 展示如何使用 rat_engine 的 gRPC 客户端进行 H2 + mTLS 双向流通信
//! 支持客户端证书认证，使用委托模式实现业务逻辑与传输层分离
//!
//! 主要特性:
//! - mTLS 客户端证书认证
//! - 自定义 CA 证书验证
//! - 双向流通信
//! - 委托模式架构
//! - 完整的错误处理和资源清理

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}, RwLock};
use std::time::Duration;
use tokio::time::sleep;
use futures_util::stream::StreamExt;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use tokio_stream;

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use rat_engine::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext};
use rat_engine::server::cert_manager::{CertificateManager, CertManagerConfig};
use rat_engine::utils::logger::{info, warn, debug, error};
use rat_engine::{RatEngine, ServerConfig, Router};
use std::future::Future;
use std::fs;

/// 加载证书文件 - 使用OpenSSL格式
fn load_certificates(cert_path: &str) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let cert_pem = fs::read_to_string(cert_path)?;

    if cert_pem.is_empty() {
        return Err(format!("证书文件 {} 为空", cert_path).into());
    }

    // 直接返回PEM格式的内容，OpenSSL可以处理
    Ok(vec![cert_pem.into_bytes()])
}

/// 加载私钥文件 - 使用OpenSSL格式
fn load_private_key(key_path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let key_pem = fs::read_to_string(key_path)?;

    if key_pem.is_empty() {
        return Err(format!("私钥文件 {} 为空", key_path).into());
    }

    // 直接返回PEM格式的内容，OpenSSL可以处理
    Ok(key_pem.into_bytes())
}

/// 聊天消息类型
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct ChatMessage {
    pub user: String,
    pub message: String,
    pub timestamp: i64,
    pub message_type: String,
}

/// mTLS 委托处理器
/// 
/// 这个处理器专门用于 mTLS 认证场景，包含证书验证相关的业务逻辑
#[derive(Debug)]
struct MtlsDelegatedHandler {
    message_count: Arc<AtomicU32>,
    client_name: String,
}

impl MtlsDelegatedHandler {
    fn new(client_name: String) -> Self {
        Self {
            message_count: Arc::new(AtomicU32::new(0)),
            client_name,
        }
    }
}

#[async_trait::async_trait]
impl ClientBidirectionalHandler for MtlsDelegatedHandler {
    type SendData = ChatMessage;
    type ReceiveData = ChatMessage;

    async fn on_connected(&self, context: &ClientStreamContext) -> Result<(), String> {
        info!("🔗 [mTLS客户端] 委托处理器：连接建立，流ID: {}", context.stream_id());
        
        // 发送初始连接消息，包含客户端身份信息
        let connect_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "Hello from mTLS authenticated client!".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "connect".to_string(),
        };
        
        context.sender().send_serialized(connect_msg).await?;
        info!("📤 [mTLS客户端] 向服务器发送初始连接消息");
        
        Ok(())
    }

    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        context: &ClientStreamContext,
    ) -> Result<(), String> {
        let count = self.message_count.fetch_add(1, Ordering::SeqCst) + 1;
        info!("📥 [mTLS客户端] 收到服务器消息 #{} (流ID: {}): {} - {} [{}]", 
            count, context.stream_id(), message.user, message.message, message.message_type);
        
        // 如果收到服务器的认证确认消息，记录日志
        if message.message_type == "auth_confirmed" {
            info!("✅ [mTLS客户端] 服务器确认客户端证书认证成功");
        }
        
        Ok(())
    }

    async fn on_send_task(&self, context: &ClientStreamContext) -> Result<(), String> {
        info!("📤 [mTLS客户端] 开始发送任务 (流ID: {})", context.stream_id());
        
        // 等待一秒后开始发送消息
        sleep(Duration::from_secs(1)).await;
        
        // 发送证书信息验证消息
        let cert_info_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "请验证我的客户端证书".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "cert_verification".to_string(),
        };
        
        context.sender().send_serialized(cert_info_msg).await?;
        info!("📤 [mTLS客户端] 发送证书验证请求");
        
        sleep(Duration::from_secs(2)).await;
        
        // 发送业务消息
        for i in 1..=3 {
            let msg = ChatMessage {
                user: self.client_name.clone(),
                message: format!("mTLS 认证消息 #{}", i),
                timestamp: chrono::Utc::now().timestamp(),
                message_type: "business".to_string(),
            };
            
            let message_content = msg.message.clone();
            context.sender().send_serialized(msg).await?;
            info!("📤 [mTLS客户端] 向服务器发送消息 #{}: {}", i, message_content);
            
            sleep(Duration::from_secs(2)).await;
        }
        
        // 发送断开连接消息
        let disconnect_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "Goodbye from mTLS client!".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "disconnect".to_string(),
        };
        
        context.sender().send_serialized(disconnect_msg).await?;
        info!("📤 [mTLS客户端] 发送断开连接消息");
        
        // 发送关闭指令
        info!("📤 [mTLS委托模式] 发送关闭指令");
        context.sender().send_close().await?;
        
        info!("📤 [mTLS客户端] 消息发送完成 (流ID: {})", context.stream_id());
        Ok(())
    }

    async fn on_disconnected(&self, context: &ClientStreamContext, reason: Option<String>) {
        let reason_str = reason.unwrap_or_else(|| "未知原因".to_string());
        info!("🔌 [mTLS客户端] 连接断开 (流ID: {}): {}", context.stream_id(), reason_str);
    }

    async fn on_error(&self, context: &ClientStreamContext, error: String) {
        error!("❌ [mTLS客户端] 发生错误 (流ID: {}): {}", context.stream_id(), error);
    }
}

/// 启动 mTLS 测试服务器
/// 
/// 这个服务器支持 mTLS 客户端证书认证
async fn start_mtls_test_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rat_engine::server::grpc_handler::BidirectionalHandler;
    use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
    use std::pin::Pin;
    use futures_util::Stream;
    
    // mTLS 双向流处理器
    #[derive(Clone)]
    struct MtlsChatHandler;
    
    #[async_trait::async_trait]
    impl BidirectionalHandler for MtlsChatHandler {
            fn handle(
            &self,
            request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
            context: GrpcContext,
        ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
            Box::pin(async move {
                info!("🔗 [mTLS服务器] 新的双向流连接建立");
                
                let (response_tx, response_rx): (tokio::sync::mpsc::UnboundedSender<Result<GrpcStreamMessage<Vec<u8>>, GrpcError>>, tokio::sync::mpsc::UnboundedReceiver<Result<GrpcStreamMessage<Vec<u8>>, GrpcError>>) = tokio::sync::mpsc::unbounded_channel();
                
                // 启动消息处理任务
                let mut request_stream = request_stream;
                tokio::spawn(async move {
                    let mut message_count = 0;
                    
                    while let Some(message_result) = request_stream.next().await {
                        match message_result {
                            Ok(grpc_message) => {
                                // 反序列化消息
                                if let Ok(message) = bincode::decode_from_slice::<ChatMessage, _>(&grpc_message.data, bincode::config::standard()) {
                                    let message = message.0;
                                    message_count += 1;
                                    info!("📥 [mTLS服务器] 收到客户端消息 #{}: {} - {} [{}]", 
                                        message_count, message.user, message.message, message.message_type);
                            
                            // 根据消息类型进行不同的处理
                            let response = match message.message_type.as_str() {
                                "connect" => {
                                    ChatMessage {
                                        user: "mTLS服务器".to_string(),
                                        message: format!("欢迎 {}！mTLS 认证成功", message.user),
                                        timestamp: chrono::Utc::now().timestamp(),
                                        message_type: "auth_confirmed".to_string(),
                                    }
                                }
                                "cert_verification" => {
                                    ChatMessage {
                                        user: "mTLS服务器".to_string(),
                                        message: "客户端证书验证通过，可以进行安全通信".to_string(),
                                        timestamp: chrono::Utc::now().timestamp(),
                                        message_type: "cert_verified".to_string(),
                                    }
                                }
                                "business" => {
                                    ChatMessage {
                                        user: "mTLS服务器".to_string(),
                                        message: format!("已收到业务消息: {}", message.message),
                                        timestamp: chrono::Utc::now().timestamp(),
                                        message_type: "business_ack".to_string(),
                                    }
                                }
                                "disconnect" => {
                                    let response = ChatMessage {
                                        user: "mTLS服务器".to_string(),
                                        message: "再见！mTLS 会话结束".to_string(),
                                        timestamp: chrono::Utc::now().timestamp(),
                                        message_type: "disconnect_ack".to_string(),
                                    };
                                    
                                    // 序列化响应并发送
                                    if let Ok(response_data) = bincode::encode_to_vec(&response, bincode::config::standard()) {
                                        let grpc_response = GrpcStreamMessage {
                                        id: 2,
                                        stream_id: 1,
                                        sequence: 1,
                                        data: response_data,
                                        end_of_stream: true,
                                        metadata: HashMap::new(),
                                    };
                                        if let Err(e) = response_tx.send(Ok(grpc_response)) {
                                            error!("❌ [mTLS服务器] 发送断开确认失败: {}", e);
                                        }
                                    }
                                    
                                    info!("🔌 [mTLS服务器] 客户端断开连接，结束会话");
                                    break;
                                }
                                _ => {
                                    ChatMessage {
                                        user: "mTLS服务器".to_string(),
                                        message: "未知消息类型".to_string(),
                                        timestamp: chrono::Utc::now().timestamp(),
                                        message_type: "error".to_string(),
                                    }
                                }
                            };
                            
                            // 序列化响应并发送
                            if let Ok(response_data) = bincode::encode_to_vec(&response, bincode::config::standard()) {
                                let grpc_response = GrpcStreamMessage {
                                    id: 1,
                                    stream_id: 1,
                                    sequence: 0,
                                    data: response_data,
                                    end_of_stream: false,
                                    metadata: HashMap::new(),
                                };
                                if let Err(e) = response_tx.send(Ok(grpc_response)) {
                                    error!("❌ [mTLS服务器] 发送响应失败: {}", e);
                                    break;
                                }
                            }
                                } else {
                                    error!("❌ [mTLS服务器] 反序列化消息失败");
                                }
                            }
                            Err(e) => {
                                error!("❌ [mTLS服务器] 接收消息失败: {}", e);
                                break;
                            }
                        }
                    }
                    
                    info!("🧹 [mTLS服务器] 双向流处理任务结束");
                });
                
                // 返回响应流
                let response_stream = tokio_stream::wrappers::UnboundedReceiverStream::new(response_rx);
                Ok(Box::pin(response_stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>)
            })
        }
    }
    
    // 创建 mTLS 证书管理器配置（自签名模式）
    let cert_manager_config = CertManagerConfig {
        development_mode: true,
        cert_path: Some("./certs/server.crt".to_string()),
        key_path: Some("./certs/server.key".to_string()),
        ca_path: Some("./certs/ca.crt".to_string()),
        validity_days: 365,
        hostnames: vec!["localhost".to_string(), "127.0.0.1".to_string()],
        acme_enabled: false,
        acme_production: false,
        acme_email: None,
        cloudflare_api_token: None,
        acme_renewal_days: 30,
        acme_cert_dir: None,
        mtls_enabled: true,
        client_cert_path: Some("./certs/client.crt".to_string()),
        client_key_path: Some("./certs/client.key".to_string()),
        client_ca_path: Some("./certs/ca.crt".to_string()),
        mtls_mode: Some("self_signed".to_string()),
        auto_generate_client_cert: true,
        client_cert_subject: Some("CN=RAT Engine Client,O=RAT Engine,C=CN".to_string()),
        auto_refresh_enabled: true,
        refresh_check_interval: 3600,
        force_cert_rotation: false,
        mtls_whitelist_paths: Vec::new(),
    };
    
    // 创建并初始化证书管理器
    let mut cert_manager = CertificateManager::new(cert_manager_config.clone());
    cert_manager.initialize().await.map_err(|e| format!("证书管理器初始化失败: {}", e))?;
    let cert_manager = Arc::new(RwLock::new(cert_manager));
    
    // 创建服务器配置
    let config = ServerConfig::new(
        "127.0.0.1:50053".parse().unwrap(),
        4
    );
    
    // 创建路由器
    let mut router = Router::new();
    router.enable_h2(); // 启用 HTTP/2 支持，mTLS 需要 HTTP/2
    router.enable_h2c(); // 启用 H2C 以支持明文 HTTP/2
    router.add_grpc_bidirectional("/chat.ChatService/BidirectionalChat", MtlsChatHandler);
    
    info!("🚀 [mTLS服务器] 启动 mTLS gRPC 服务器，监听地址: 127.0.0.1:50053");
    
    // 启动服务器 - 使用新的 RatEngineBuilder 架构，配置日志和证书管理器
    let mut log_config = rat_engine::utils::logger::LogConfig::default();
    log_config.level = rat_engine::utils::logger::LogLevel::Debug; // 设置debug级别日志

    let mut cert_manager_for_engine = CertificateManager::new(cert_manager_config.clone());
    cert_manager_for_engine.initialize().await.map_err(|e| format!("证书管理器初始化失败: {}", e))?;

    let engine = RatEngine::builder()
        .with_log_config(log_config)
        .router(router)
        .certificate_manager(cert_manager_for_engine)
        .build()?;
    
    // ALPN 协议现在由 RatEngineBuilder 自动配置，无需手动设置
    
    engine.start("127.0.0.1".to_string(), 50053).await?;
    
    Ok(())
}

/// 运行 mTLS 委托模式测试
async fn run_mtls_delegated_mode() -> Result<(), Box<dyn std::error::Error>> {
    info!("🚀 启动 mTLS 委托模式双向流测试...");
    
    // 加载客户端证书和私钥
    let client_cert_chain = load_certificates("certs/client.crt")?;
    let client_private_key = load_private_key("certs/client.key")?;
    
    // 创建 mTLS 客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(10)?
        .http2_only() // 强制使用 HTTP/2
        .user_agent("rat-engine-mtls-example/1.0")?
        .disable_compression()
        // 配置 mTLS 客户端证书（开发模式）
        .with_self_signed_mtls(
            client_cert_chain,
            client_private_key,
            Some("localhost".to_string()),
            Some("./certs/client.crt".to_string()),
            Some("./certs/client.key".to_string())
        )?
        .development_mode() // 启用开发模式
        .build()?;
    
    // 创建 mTLS 委托处理器
    let handler = Arc::new(MtlsDelegatedHandler::new("mTLS客户端001".to_string()));
    
    // 创建委托模式双向流
    let stream_id = client.create_bidirectional_stream_delegated_with_uri(
        "https://127.0.0.1:50053",
        "chat.ChatService",
        "BidirectionalChat", 
        handler.clone(),
        None::<HashMap<String, String>>
    ).await?;
    
    info!("✅ mTLS 委托模式双向流创建成功，流ID: {}", stream_id);
    
    // 获取流上下文
    if let Some(context) = client.get_stream_context(stream_id).await {
        // 在业务层控制逻辑 - 手动调用处理器方法
        if let Err(e) = handler.on_connected(&context).await {
            error!("❌ [mTLS客户端] 连接建立失败: {}", e);
            // 确保清理资源
            let _ = client.close_bidirectional_stream_delegated(stream_id).await;
            return Err(e.into());
        }
        
        // 启动业务逻辑任务
        let handler_clone = handler.clone();
        let context_clone = context.clone();
        let business_task = tokio::spawn(async move {
            if let Err(e) = handler_clone.on_send_task(&context_clone).await {
                error!("❌ [mTLS客户端] 发送任务失败: {}", e);
            }
        });
        
        // 等待业务任务完成，但设置超时
        let task_result = tokio::time::timeout(
            Duration::from_secs(20),
            business_task
        ).await;
        
        match task_result {
            Ok(Ok(_)) => {
                info!("✅ [mTLS客户端] 委托模式业务任务完成");
            }
            Ok(Err(e)) => {
                error!("❌ [mTLS客户端] 委托模式业务任务失败: {}", e);
            }
            Err(_) => {
                warn!("⚠️ [mTLS客户端] 委托模式业务任务超时，强制结束");
            }
        }
        
        // 调用断开连接处理器
        handler.on_disconnected(&context, Some("mTLS客户端主动断开".to_string())).await;
    } else {
        error!("❌ [mTLS客户端] 无法获取流上下文");
        // 确保清理资源
        let _ = client.close_bidirectional_stream_delegated(stream_id).await;
        return Err("无法获取流上下文".into());
    }
    
    // 关闭连接
    if let Err(e) = client.close_bidirectional_stream_delegated(stream_id).await {
        error!("❌ [mTLS客户端] 关闭委托模式双向流失败: {}", e);
        return Err(Box::new(e));
    }
    
    info!("🧹 mTLS 委托模式双向流已关闭");
    
    // 显式关闭客户端连接池
    client.shutdown().await;
    
    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rat_engine::require_features!("client", "tls");

    // 确保 CryptoProvider 只安装一次
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();
    
    // 日志通过RatEngineBuilder初始化
    
    info!("🚀 启动 gRPC 客户端双向流 mTLS 示例");
    
    // 检查命令行参数（现在只支持委托模式）
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        info!("📖 使用说明: 此示例现在只支持委托模式");
        info!("  直接运行程序即可启动 mTLS 委托模式测试");
        return Ok(());
    }
    
    // 启动 mTLS 服务器任务
    let server_task = tokio::spawn(async {
        if let Err(e) = start_mtls_test_server().await {
            error!("❌ [mTLS服务器] 启动失败: {}", e);
        }
    });
    
    // 等待服务器启动
    sleep(Duration::from_secs(3)).await; // mTLS 服务器可能需要更多时间启动
    
    // 执行测试逻辑（现在只支持委托模式）
    let test_result = run_mtls_delegated_mode().await;
    
    // 处理测试结果
    match test_result {
        Ok(_) => {
            info!("✅ gRPC mTLS 双向流测试成功完成");
        }
        Err(e) => {
            error!("❌ gRPC mTLS 双向流测试失败: {}", e);
            return Err(e);
        }
    }
    
    // 等待一段时间让服务器完成清理
    sleep(Duration::from_secs(1)).await;
    
    // 终止服务器任务
    server_task.abort();
    
    info!("🧹 gRPC mTLS 双向流示例程序结束");
    
    Ok(())
}