//! gRPC + TLS 双向流服务端示例（Ping/Pong 模式）
//!
//! 使用 grpc + bincode
//! 使用真实 TLS 证书（ligproxy-test.0ldm0s.net）

use rat_engine::{RatEngine, Router};
use rat_engine::server::grpc_handler::BidirectionalHandler;
use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use std::pin::Pin;
use futures_util::{Stream, StreamExt, stream};

/// Ping 请求
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct PingRequest {
    pub message: String,
    pub sequence: u32,
}

/// Pong 响应
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct PongResponse {
    pub reply: String,
    pub sequence: u32,
    pub timestamp: u64,
}

/// Ping/Pong 双向流处理器
struct PingPongStreamHandler;

impl BidirectionalHandler for PingPongStreamHandler {
    fn handle(
        &self,
        mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        Box::pin(async move {
            use std::time::{SystemTime, UNIX_EPOCH};

            println!("[双向流] Ping/Pong 服务已连接");

            // 创建响应流通道
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            // 启动处理任务
            tokio::spawn(async move {
                let mut message_count = 0u32;

                // 处理接收到的请求并发送响应
                while let Some(result) = request_stream.next().await {
                    match result {
                        Ok(stream_msg) => {
                            message_count += 1;

                            // 检查是否为流结束信号
                            if stream_msg.end_of_stream {
                                println!("[双向流] 收到流结束信号");
                                break;
                            }

                            // 解码请求
                            let ping_req: PingRequest = match bincode::decode_from_slice(
                                &stream_msg.data,
                                bincode::config::standard()
                            ) {
                                Ok((req, _)) => req,
                                Err(e) => {
                                    println!("[双向流] 解码失败: {}", e);
                                    let error = GrpcError::InvalidArgument(format!("解码失败: {}", e));
                                    let _ = tx.send(Err(error));
                                    continue;
                                }
                            };

                            println!("[双向流] [Ping #{:03}] 收到: {}", ping_req.sequence, ping_req.message);

                            // 创建响应
                            let response = PongResponse {
                                reply: format!("收到你的消息: {}", ping_req.message),
                                sequence: ping_req.sequence,
                                timestamp: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs(),
                            };

                            // 编码响应
                            let response_bytes = match bincode::encode_to_vec(
                                &response,
                                bincode::config::standard()
                            ) {
                                Ok(bytes) => bytes,
                                Err(e) => {
                                    println!("[双向流] 编码失败: {}", e);
                                    let error = GrpcError::Internal(format!("编码失败: {}", e));
                                    let _ = tx.send(Err(error));
                                    continue;
                                }
                            };

                            let stream_response = GrpcStreamMessage {
                                id: stream_msg.id,
                                stream_id: stream_msg.stream_id,
                                sequence: message_count as u64,
                                end_of_stream: false,
                                data: response_bytes,
                                metadata: Default::default(),
                            };

                            // 发送响应到通道
                            if let Err(e) = tx.send(Ok(stream_response)) {
                                println!("[双向流] 发送响应失败: {}", e);
                                break;
                            }

                            println!("[双向流] [Pong #{:03}] 已回复", message_count);
                        }
                        Err(e) => {
                            println!("[双向流] 接收错误: {:?}", e);
                            break;
                        }
                    }
                }

                println!("[双向流] Ping/Pong 服务结束，共处理 {} 条消息", message_count);
            });

            // 返回响应流
            let response_stream = stream::unfold(rx, |mut rx| async move {
                match rx.recv().await {
                    Some(result) => Some((result, rx)),
                    None => None,
                }
            });

            Ok(Box::pin(response_stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 RAT Engine gRPC + TLS 双向流服务端 (Ping/Pong 模式)");
    println!("=======================================================");
    println!("证书: ligproxy-test.0ldm0s.net");
    println!("绑定: 0.0.0.0:50051");
    println!();

    // 验证证书文件
    let cert_path = "examples/certs/ligproxy-test.0ldm0s.net.pem";
    let key_path = "examples/certs/ligproxy-test.0ldm0s.net-key.pem";

    if !std::path::Path::new(cert_path).exists() {
        return Err(format!("证书文件不存在: {}", cert_path).into());
    }
    if !std::path::Path::new(key_path).exists() {
        return Err(format!("私钥文件不存在: {}", key_path).into());
    }

    println!("✅ 证书验证通过");

    // 配置证书（包含 SNI 域名）
    let cert_config = CertConfig::from_paths(cert_path, key_path)
        .with_domains(vec!["ligproxy-test.0ldm0s.net".to_string()]);
    let cert_manager_config = CertManagerConfig::shared(cert_config);
    let cert_manager = CertificateManager::from_config(cert_manager_config)?;

    println!();

    let mut router = Router::new();
    router.enable_grpc_only();
    router.enable_h2();

    router.add_grpc_bidirectional("/chat.ChatService/Chat", PingPongStreamHandler);

    println!("📡 gRPC 双向流服务:");
    println!("   /chat.ChatService/Chat");
    println!();
    println!("按 Ctrl+C 停止");
    println!();

    let engine = RatEngine::builder()
        .worker_threads(4)
        .enable_logger()
        .router(router)
        .certificate_manager(cert_manager)
        .build()?;

    engine.start("0.0.0.0".to_string(), 50051).await?;

    Ok(())
}
