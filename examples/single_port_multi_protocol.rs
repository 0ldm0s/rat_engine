//! 单端口多协议模式示例
//!
//! HTTP 和 gRPC 共用同一端口，自动检测协议
//! - 端口: 8443
//! - 协议: 自动检测 HTTP/1.1、HTTP/2、gRPC
//! - ⚠️ 注意：必须配置 HTTPS 证书

use rat_engine::{RatEngine, Router, Response, Method};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use rat_engine::server::grpc_handler::BidirectionalHandler;
use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
use http_body_util::Full;
use std::pin::Pin;
use futures_util::{Stream, StreamExt, stream};
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

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

            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            tokio::spawn(async move {
                let mut message_count = 0u32;

                while let Some(result) = request_stream.next().await {
                    match result {
                        Ok(stream_msg) => {
                            message_count += 1;

                            if stream_msg.end_of_stream {
                                println!("[双向流] 收到流结束信号");
                                break;
                            }

                            let ping_req: PingRequest = match bincode::decode_from_slice(
                                &stream_msg.data,
                                bincode::config::standard()
                            ) {
                                Ok((req, _)) => req,
                                Err(e) => {
                                    println!("[双向流] 解码失败: {}", e);
                                    let _ = tx.send(Err(GrpcError::InvalidArgument(format!("解码失败: {}", e))));
                                    continue;
                                }
                            };

                            println!("[双向流] [Ping #{:03}] 收到: {}", ping_req.sequence, ping_req.message);

                            let response = PongResponse {
                                reply: format!("收到你的消息: {}", ping_req.message),
                                sequence: ping_req.sequence,
                                timestamp: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs(),
                            };

                            let response_bytes = match bincode::encode_to_vec(
                                &response,
                                bincode::config::standard()
                            ) {
                                Ok(bytes) => bytes,
                                Err(e) => {
                                    println!("[双向流] 编码失败: {}", e);
                                    let _ = tx.send(Err(GrpcError::Internal(format!("编码失败: {}", e))));
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
    println!("🚀 RAT Engine 单端口多协议模式示例");
    println!("==================================");
    println!("端口: 50051");
    println!("协议: 自动检测 HTTP/1.1、HTTP/2、gRPC");
    println!("证书: 强制 HTTPS（单端口模式必须）");
    println!();

    // ⚠️ 重要：单端口多协议模式必须配置 HTTPS 证书
    // 因为 gRPC 强制要求 TLS
    let cert_config = CertConfig::from_paths(
        "examples/certs/ligproxy-test.0ldm0s.net.pem",
        "examples/certs/ligproxy-test.0ldm0s.net-key.pem",
    )
    .with_domains(vec!["ligproxy-test.0ldm0s.net".to_string()]);

    let cert_manager_config = CertManagerConfig::shared(cert_config);
    let cert_manager = CertificateManager::from_config(cert_manager_config)?;

    // 创建路由器
    let mut router = Router::new();

    // 添加 HTTP 路由
    router.add_route(Method::GET, "/", |_req| {
        Box::pin(async {
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Full::new(hyper::body::Bytes::from(
                    r#"
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine 单端口多协议模式</title>
    <meta charset="utf-8">
</head>
<body>
    <h1>🚀 RAT Engine 单端口多协议模式</h1>
    <p>HTTP 和 gRPC 共用同一端口（8443）</p>
    <p>服务器会自动检测协议类型：</p>
    <ul>
        <li>HTTP/1.1 请求 → HTTP 处理器</li>
        <li>HTTP/2 请求 → HTTP 处理器</li>
        <li>gRPC 请求 → gRPC 处理器</li>
    </ul>
    <p>⚠️ 注意：单端口模式强制使用 HTTPS</p>
</body>
</html>
"#,
                )))
                .unwrap())
        })
    });

    // 健康检查
    router.add_route(Method::GET, "/health", |_req| {
        Box::pin(async {
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(Full::new(hyper::body::Bytes::from(
                    r#"{"status": "ok", "mode": "single_port_multi_protocol"}"#,
                )))
                .unwrap())
        })
    });

    // 添加 gRPC 双向流路由
    router.add_grpc_bidirectional("/chat.ChatService/Chat", PingPongStreamHandler);

    println!("✅ 配置完成，启动服务器...");
    println!();

    // 使用单端口多协议模式启动
    let engine = RatEngine::builder()
        .worker_threads(4)
        .router(router)
        .certificate_manager(cert_manager)
        .build()?;

    println!("✅ 服务器已启动！");
    println!();
    println!("测试方法:");
    println!("  HTTP: curl -k https://ligproxy-test.0ldm0s.net:50051/");
    println!("  gRPC: cargo run --example grpc_tls_bidirectional_client --features client");
    println!();
    println!("按 Ctrl+C 停止");

    // 单端口多协议模式：HTTP 和 gRPC 共用同一端口
    engine.start_single_port_multi_protocol("0.0.0.0".to_string(), 50051).await?;

    Ok(())
}
