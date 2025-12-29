//! 双端口模式 - 分离证书（Dual Certificates）
//!
//! HTTP 和 gRPC 使用不同端口和不同证书
//! - HTTP 端口: 3001 (使用 ligproxy-http.0ldm0s.net 证书)
//! - gRPC 端口: 50051 (使用 ligproxy-test.0ldm0s.net 证书)
//!
//! 适用场景：
//! - HTTP 和 gRPC 服务需要使用不同的域名和证书
//! - 不同服务有不同的安全策略或证书提供商

use rat_engine::{RatEngine, Router, Response};
use rat_engine::server::config::ServerConfig;
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
    println!("🚀 RAT Engine 双端口模式 - 分离证书");
    println!("===================================");
    println!("HTTP 端口: https://ligproxy-http.0ldm0s.net:3001");
    println!("gRPC 端口: https://ligproxy-test.0ldm0s.net:50051");
    println!();

    // 配置 gRPC 证书
    let grpc_cert_config = CertConfig::from_paths(
        "examples/certs/ligproxy-test.0ldm0s.net.pem",
        "examples/certs/ligproxy-test.0ldm0s.net-key.pem",
    )
    .with_domains(vec!["ligproxy-test.0ldm0s.net".to_string()]);

    // 配置 HTTP 证书（不同域名）
    let http_cert_config = CertConfig::from_paths(
        "examples/certs/ligproxy-http.0ldm0s.net.pem",
        "examples/certs/ligproxy-http.0ldm0s.net-key.pem",
    )
    .with_domains(vec!["ligproxy-http.0ldm0s.net".to_string()]);

    // 使用分离证书模式
    let cert_manager_config = CertManagerConfig::separated(grpc_cert_config, Some(http_cert_config));
    let cert_manager = CertificateManager::from_config(cert_manager_config)?;

    // 创建路由器
    let mut router = Router::new();

    // 添加 HTTP 路由
    router.add_route(rat_engine::Method::GET, "/", |_req| {
        Box::pin(async {
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "text/html; charset=utf-8")
                .body(Full::new(hyper::body::Bytes::from(
                    r#"
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine 双端口模式 - 分离证书</title>
    <meta charset="utf-8">
</head>
<body>
    <h1>🚀 RAT Engine 双端口模式 - 分离证书</h1>
    <h2>Dual Certificates Mode</h2>
    <p><strong>HTTP 服务器</strong>: 端口 3001，证书: ligproxy-http.0ldm0s.net</p>
    <p><strong>gRPC 服务器</strong>: 端口 50051，证书: ligproxy-test.0ldm0s.net</p>
    <h3>特点</h3>
    <ul>
        <li>HTTP 和 gRPC 使用完全独立的证书</li>
        <li>可以针对不同服务配置不同的安全策略</li>
        <li>支持不同的证书颁发机构和有效期</li>
    </ul>
</body>
</html>
"#,
                )))
                .unwrap())
        })
    });

    router.add_route(rat_engine::Method::GET, "/health", |_req| {
        Box::pin(async {
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .body(Full::new(hyper::body::Bytes::from(
                    r#"{"status": "ok", "http_cert": "ligproxy-http.0ldm0s.net", "grpc_cert": "ligproxy-test.0ldm0s.net"}"#,
                )))
                .unwrap())
        })
    });

    // 添加 gRPC 双向流路由
    router.add_grpc_bidirectional("/chat.ChatService/Chat", PingPongStreamHandler);

    // 配置双端口模式（绑定到 0.0.0.0，允许外部访问）
    let config = ServerConfig::separated_ports_any(3001, 50051, 4)?;

    println!("✅ 配置完成，启动服务器...");
    println!();

    // 启动双端口服务器
    let engine = RatEngine::builder()
        .server_config(config)
        .worker_threads(4)
        .router(router)
        .certificate_manager(cert_manager)
        .build()?;

    println!("✅ 服务器已启动！");
    println!();
    println!("测试方法:");
    println!("  HTTP: curl -k https://ligproxy-http.0ldm0s.net:3001/");
    println!("  HTTP: curl -k https://ligproxy-http.0ldm0s.net:3001/health");
    println!("  gRPC: 运行 gRPC 客户端连接 ligproxy-test.0ldm0s.net:50051");
    println!();
    println!("按 Ctrl+C 停止");

    engine.start_separated().await?;

    Ok(())
}
