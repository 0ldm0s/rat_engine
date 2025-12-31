//! gRPC + mTLS 双向流完整示例
//!
//! 展示如何使用 rat_engine 的 gRPC 客户端进行 H2 + mTLS 双向流通信
//! 支持客户端证书认证，使用委托模式实现业务逻辑与传输层分离
//!
//! 主要特性:
//! - mTLS 客户端证书认证
//! - 自定义 CA 证书验证
//! - 双向流通信
//! - 委托模式架构
//! - 完整的消息类型处理（connect/cert_verification/business/disconnect）

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::time::Duration;
use tokio::time::sleep;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use rat_engine::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};

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
        println!("🔗 [mTLS客户端] 委托处理器：连接建立，流ID: {}", context.stream_id());

        // 发送初始连接消息，包含客户端身份信息
        let connect_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "Hello from mTLS authenticated client!".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "connect".to_string(),
        };

        context.sender().send_serialized(connect_msg).await?;
        println!("📤 [mTLS客户端] 向服务器发送初始连接消息");

        Ok(())
    }

    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        context: &ClientStreamContext,
    ) -> Result<(), String> {
        let count = self.message_count.fetch_add(1, Ordering::SeqCst) + 1;
        println!("📥 [mTLS客户端] 收到服务器消息 #{} (流ID: {}): {} - {} [{}]",
            count, context.stream_id(), message.user, message.message, message.message_type);

        // 如果收到服务器的认证确认消息，记录日志
        if message.message_type == "auth_confirmed" {
            println!("✅ [mTLS客户端] 服务器确认客户端证书认证成功");
        }

        Ok(())
    }

    async fn on_send_task(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("📤 [mTLS客户端] 开始发送任务 (流ID: {})", context.stream_id());

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
        println!("📤 [mTLS客户端] 发送证书验证请求");

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
            println!("📤 [mTLS客户端] 向服务器发送消息 #{}: {}", i, message_content);

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
        println!("📤 [mTLS客户端] 发送断开连接消息");

        // 发送关闭指令
        println!("📤 [mTLS委托模式] 发送关闭指令");
        context.sender().send_close().await?;

        println!("📤 [mTLS客户端] 消息发送完成 (流ID: {})", context.stream_id());
        Ok(())
    }

    async fn on_disconnected(&self, context: &ClientStreamContext, reason: Option<String>) {
        let reason_str = reason.unwrap_or_else(|| "未知原因".to_string());
        println!("🔌 [mTLS客户端] 连接断开 (流ID: {}): {}", context.stream_id(), reason_str);
    }

    async fn on_error(&self, context: &ClientStreamContext, error: String) {
        eprintln!("❌ [mTLS客户端] 发生错误 (流ID: {}): {}", context.stream_id(), error);
    }
}

/// 启动 mTLS 测试服务器
///
/// 这个服务器支持 mTLS 客户端证书认证
async fn start_mtls_test_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rat_engine::server::grpc_handler::BidirectionalHandler;
    use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
    use std::pin::Pin;
    use futures_util::{Stream, StreamExt};
    use async_stream::stream;
    use rat_engine::{RatEngine, Router};

    // mTLS 双向流处理器
    #[derive(Clone)]
    struct MtlsChatHandler;

    impl BidirectionalHandler for MtlsChatHandler {
        fn handle(
            &self,
            mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
            _context: GrpcContext,
        ) -> Pin<Box<dyn futures_util::Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
            Box::pin(async move {
                println!("🔗 [mTLS服务器] 新的双向流连接建立");

                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

                // 处理传入消息的任务
                tokio::spawn(async move {
                    let mut message_count = 0u32;

                    while let Some(result) = request_stream.next().await {
                        match result {
                            Ok(msg) => {
                                message_count += 1;

                                if msg.end_of_stream {
                                    println!("📥 [mTLS服务器] 收到流结束信号，停止处理");
                                    break;
                                }

                                // 解析消息
                                match bincode::decode_from_slice::<ChatMessage, _>(&msg.data, bincode::config::standard()) {
                                    Ok((chat_msg, _)) => {
                                        println!("📥 [mTLS服务器] 收到客户端消息 #{}: {} - {} [{}]",
                                            message_count, chat_msg.user, chat_msg.message, chat_msg.message_type);

                                        // 根据消息类型进行不同的处理
                                        let response = match chat_msg.message_type.as_str() {
                                            "connect" => {
                                                println!("✅ [mTLS服务器] 客户端连接认证: {}", chat_msg.user);
                                                ChatMessage {
                                                    user: "mTLS服务器".to_string(),
                                                    message: format!("欢迎 {}！mTLS 认证成功", chat_msg.user),
                                                    timestamp: chrono::Utc::now().timestamp(),
                                                    message_type: "auth_confirmed".to_string(),
                                                }
                                            }
                                            "cert_verification" => {
                                                println!("🔐 [mTLS服务器] 客户端请求证书验证");
                                                ChatMessage {
                                                    user: "mTLS服务器".to_string(),
                                                    message: "客户端证书验证通过，可以进行安全通信".to_string(),
                                                    timestamp: chrono::Utc::now().timestamp(),
                                                    message_type: "cert_verified".to_string(),
                                                }
                                            }
                                            "business" => {
                                                println!("💼 [mTLS服务器] 处理业务消息: {}", chat_msg.message);
                                                ChatMessage {
                                                    user: "mTLS服务器".to_string(),
                                                    message: format!("已收到业务消息: {}", chat_msg.message),
                                                    timestamp: chrono::Utc::now().timestamp(),
                                                    message_type: "business_ack".to_string(),
                                                }
                                            }
                                            "disconnect" => {
                                                println!("👋 [mTLS服务器] 客户端请求断开连接: {}", chat_msg.user);

                                                let response = ChatMessage {
                                                    user: "mTLS服务器".to_string(),
                                                    message: "再见！mTLS 会话结束".to_string(),
                                                    timestamp: chrono::Utc::now().timestamp(),
                                                    message_type: "disconnect_ack".to_string(),
                                                };

                                                if let Ok(data) = bincode::encode_to_vec(&response, bincode::config::standard()) {
                                                    let _ = tx.send(data);
                                                }

                                                println!("🔌 [mTLS服务器] 客户端断开连接，结束会话");
                                                break;
                                            }
                                            _ => {
                                                println!("⚠️  [mTLS服务器] 未知消息类型: {}", chat_msg.message_type);
                                                ChatMessage {
                                                    user: "mTLS服务器".to_string(),
                                                    message: "未知消息类型".to_string(),
                                                    timestamp: chrono::Utc::now().timestamp(),
                                                    message_type: "error".to_string(),
                                                }
                                            }
                                        };

                                        // 发送响应
                                        if let Ok(data) = bincode::encode_to_vec(&response, bincode::config::standard()) {
                                            if tx.send(data).is_err() {
                                                println!("🔌 [mTLS服务器] 响应通道已关闭，停止发送");
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("❌ [mTLS服务器] 消息解析失败: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                let error_msg = format!("{}", e);
                                if error_msg.contains("stream no longer needed") || error_msg.contains("connection closed") {
                                    println!("📥 [mTLS服务器] 客户端正常断开连接");
                                } else {
                                    eprintln!("❌ [mTLS服务器] 接收客户端消息失败: {}", e);
                                }
                                break;
                            }
                        }
                    }
                    println!("🧹 [mTLS服务器] 客户端消息处理任务结束");
                });

                // 创建响应流
                let response_stream = stream! {
                    let mut sequence = 0u64;

                    while let Some(data) = rx.recv().await {
                        sequence += 1;
                        yield Ok(GrpcStreamMessage {
                            id: sequence,
                            stream_id: 1,
                            sequence,
                            data,
                            end_of_stream: false,
                            metadata: HashMap::new(),
                        });
                    }

                    // 发送结束消息
                    sequence += 1;
                    yield Ok(GrpcStreamMessage {
                        id: sequence,
                        stream_id: 1,
                        sequence,
                        data: Vec::new(),
                        end_of_stream: true,
                        metadata: HashMap::new(),
                    });
                };

                Ok(Box::pin(response_stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>)
            })
        }
    }

    // 创建 mTLS 证书管理器配置
    // 使用 CA 证书验证客户端证书链
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let server_cert_config = CertConfig::from_paths(
        manifest_dir.join("examples/certs/ligproxy-test.0ldm0s.net.pem"),
        manifest_dir.join("examples/certs/ligproxy-test.0ldm0s.net-key.pem"),
    )
    .with_domains(vec!["ligproxy-test.0ldm0s.net".to_string()])
    .with_ca(manifest_dir.join("examples/certs/mtls/ca-cert.pem")); // ← CA 证书验证客户端

    let cert_manager_config = CertManagerConfig::shared(server_cert_config);

    // 创建证书管理器
    let cert_manager = CertificateManager::from_config(cert_manager_config)?;

    // 创建路由器（启用纯 gRPC 模式）
    let mut router = Router::new();
    router.enable_grpc_only(); // 纯 gRPC 模式
    router.enable_h2(); // 启用 HTTP/2
    router.add_grpc_bidirectional("/chat.ChatService/BidirectionalChat", MtlsChatHandler);

    println!("🚀 [mTLS服务器] 启动 mTLS gRPC 服务器（纯 gRPC 模式）");
    println!("   监听地址: 127.0.0.1:50053");
    println!("   mTLS: 已启用");

    let engine = RatEngine::builder()
        .router(router)
        .certificate_manager(cert_manager)
        .worker_threads(4)
        .build()?;

    engine.start_single_port_multi_protocol("127.0.0.1".to_string(), 50053).await?;

    Ok(())
}

/// 运行 mTLS 委托模式测试
async fn run_mtls_delegated_mode() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 启动 mTLS 客户端测试...");

    // 获取项目根目录
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cert_dir = manifest_dir.join("examples/certs/mtls");

    println!("📁 证书目录: {:?}", cert_dir);

    // 检查证书文件是否存在
    let client_cert_path = cert_dir.join("client-cert-chain.pem");  // 完整证书链（客户端证书 + CA）
    let client_key_path = cert_dir.join("client-key.pem");
    let ca_cert_path = cert_dir.join("ca-cert.pem");

    println!("   客户端证书: {:?}", client_cert_path);
    println!("   客户端私钥: {:?}", client_key_path);
    println!("   CA 证书: {:?}", ca_cert_path);

    if !client_cert_path.exists() {
        return Err(format!("客户端证书文件不存在: {:?}", client_cert_path).into());
    }
    if !client_key_path.exists() {
        return Err(format!("客户端私钥文件不存在: {:?}", client_key_path).into());
    }
    if !ca_cert_path.exists() {
        return Err(format!("CA 证书文件不存在: {:?}", ca_cert_path).into());
    }

    println!("✅ 所有证书文件检查通过");

    // 使用 mTLS 客户端证书进行双向认证
    // 注意：CA 证书传 None，mTLS 模式下会自动跳过服务器证书验证（开发环境）
    println!("🔐 [客户端] 开始创建带 mTLS 证书的 gRPC 客户端");
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(5)?
        .http2_only()
        .disable_compression()
        .user_agent("rat-engine-mtls-example/1.0")?
        .with_client_certs_and_ca(
            client_cert_path.to_string_lossy().to_string(),
            client_key_path.to_string_lossy().to_string(),
            None  // mTLS 模式下跳过服务器证书验证
        )?
        .build()?;
    println!("✅ gRPC 客户端创建成功");

    // 创建 mTLS 委托处理器
    let handler = Arc::new(MtlsDelegatedHandler::new("mTLS客户端001".to_string()));

    // 创建委托模式双向流
    let stream_id = client.create_bidirectional_stream_delegated_with_uri(
        "https://ligproxy-test.0ldm0s.net:50053",
        "chat.ChatService",
        "BidirectionalChat",
        handler.clone(),
        None::<HashMap<String, String>>
    ).await?;

    println!("✅ mTLS 委托模式双向流创建成功，流ID: {}", stream_id);

    // 获取流上下文
    if let Some(context) = client.get_stream_context(stream_id).await {
        // 在业务层控制逻辑 - 手动调用处理器方法
        if let Err(e) = handler.on_connected(&context).await {
            eprintln!("❌ [mTLS客户端] 连接建立失败: {}", e);
            let _ = client.close_bidirectional_stream_delegated(stream_id).await;
            return Err(e.into());
        }

        // 启动业务逻辑任务
        let handler_clone = handler.clone();
        let context_clone = context.clone();
        let business_task = tokio::spawn(async move {
            if let Err(e) = handler_clone.on_send_task(&context_clone).await {
                eprintln!("❌ [mTLS客户端] 发送任务失败: {}", e);
            }
        });

        // 等待业务任务完成，但设置超时
        let task_result = tokio::time::timeout(
            Duration::from_secs(20),
            business_task
        ).await;

        match task_result {
            Ok(Ok(_)) => {
                println!("✅ [mTLS客户端] 委托模式业务任务完成");
            }
            Ok(Err(e)) => {
                eprintln!("❌ [mTLS客户端] 委托模式业务任务失败: {}", e);
            }
            Err(_) => {
                println!("⚠️  [mTLS客户端] 委托模式业务任务超时，强制结束");
            }
        }

        // 调用断开连接处理器
        handler.on_disconnected(&context, Some("mTLS客户端主动断开".to_string())).await;
    } else {
        eprintln!("❌ [mTLS客户端] 无法获取流上下文");
        let _ = client.close_bidirectional_stream_delegated(stream_id).await;
        return Err("无法获取流上下文".into());
    }

    // 关闭连接
    if let Err(e) = client.close_bidirectional_stream_delegated(stream_id).await {
        eprintln!("❌ [mTLS客户端] 关闭委托模式双向流失败: {}", e);
        return Err(Box::new(e));
    }

    println!("🧹 mTLS 委托模式双向流已关闭");

    // 显式关闭客户端连接池
    client.shutdown().await;

    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rat_engine::require_features!("client", "tls");

    // 确保 CryptoProvider 只安装一次
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    println!("🚀 启动 gRPC + mTLS 双向流示例");

    // 检查命令行参数（现在只支持委托模式）
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        println!("📖 使用说明: 此示例现在只支持委托模式");
        println!("  直接运行程序即可启动 mTLS 委托模式测试");
        return Ok(());
    }

    // 启动 mTLS 服务器任务
    let server_task = tokio::spawn(async {
        if let Err(e) = start_mtls_test_server().await {
            eprintln!("❌ [mTLS服务器] 启动失败: {}", e);
        }
    });

    // 等待服务器启动
    sleep(Duration::from_secs(3)).await;

    // 执行测试逻辑（现在只支持委托模式）
    let test_result = run_mtls_delegated_mode().await;

    // 处理测试结果
    match test_result {
        Ok(_) => {
            println!("✅ gRPC mTLS 双向流测试成功完成");
        }
        Err(e) => {
            eprintln!("❌ gRPC mTLS 双向流测试失败: {}", e);
            return Err(e);
        }
    }

    // 等待一段时间让服务器完成清理
    sleep(Duration::from_secs(1)).await;

    // 终止服务器任务
    server_task.abort();

    println!("🧹 gRPC mTLS 双向流示例程序结束");

    Ok(())
}