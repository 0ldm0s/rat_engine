//! gRPC mTLS 服务器示例
//!
//! 独立的服务端程序，便于调试 mTLS 配置

use std::collections::HashMap;
use std::pin::Pin;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

use rat_engine::{RatEngine, Router};
use rat_engine::server::grpc_handler::BidirectionalHandler;
use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use futures_util::{Stream, StreamExt};
use async_stream::stream;

/// 聊天消息类型
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct ChatMessage {
    pub user: String,
    pub message: String,
    pub timestamp: i64,
    pub message_type: String,
}

/// mTLS 双向流处理器
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    rat_engine::require_features!("client", "tls");

    // 确保 CryptoProvider 只安装一次
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    println!("🚀 启动 gRPC mTLS 服务器");

    // 创建 mTLS 证书管理器配置
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
