//! HAProxy TcpData 丢失测试 - 服务端（修复版）
//!
//! 这个示例使用类似客户端流的处理方式：先发送响应头
//! 预期：经过 HAProxy 后，TcpData 不会丢失

use rat_engine::{RatEngine, Router};
use rat_engine::server::grpc_handler::BidirectionalHandler;
use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use std::pin::Pin;
use futures_util::{Stream, StreamExt, stream};

/// 代理数据包（简化版）
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub enum ProxyPacket {
    TcpConnect { connection_id: u64, target_addr: String, target_port: u16 },
    TcpData { connection_id: u64, data: Vec<u8> },
    TcpClose { connection_id: u64 },
}

/// 修复版双向流处理器
/// 关键改进：立即返回响应流，让框架可以尽快发送响应头
struct FixedBidirectionalHandler;

impl BidirectionalHandler for FixedBidirectionalHandler {
    fn handle(
        &self,
        mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        Box::pin(async move {
            println!("[服务端-修复版] 双向流处理器 - 开始处理");

            // 创建响应通道
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            // 立即返回响应流，不等待任何数据
            // 这样框架可以立即发送响应头
            println!("[服务端-修复版] 立即返回响应流（允许框架发送响应头）");

            // 启动处理任务（异步，不阻塞响应流返回）
            tokio::spawn(async move {
                println!("[服务端-修复版] 处理任务启动，等待接收数据包...");

                let mut packet_count = 0u32;

                // 处理接收到的请求
                while let Some(result) = request_stream.next().await {
                    match result {
                        Ok(stream_msg) => {
                            packet_count += 1;

                            // 检查是否为流结束信号
                            if stream_msg.end_of_stream {
                                println!("[服务端-修复版] 收到流结束信号，总共收到 {} 个数据包", packet_count);
                                break;
                            }

                            // 解码数据包
                            match bincode::decode_from_slice::<ProxyPacket, _>(
                                &stream_msg.data,
                                bincode::config::standard()
                            ) {
                                Ok((packet, _)) => {
                                    println!("[服务端-修复版] 收到数据包 #{}: {:?}", packet_count, packet);

                                    // 处理不同类型的包
                                    match packet {
                                        ProxyPacket::TcpConnect { connection_id, target_addr, target_port } => {
                                            println!("[服务端-修复版]   -> TcpConnect: {}:{} (id={})", target_addr, target_port, connection_id);

                                            // 模拟建立连接并响应
                                            let response = ProxyPacket::TcpConnect { connection_id, target_addr, target_port };
                                            let _ = send_packet(&tx, response);
                                        }
                                        ProxyPacket::TcpData { connection_id, data } => {
                                            println!("[服务端-修复版]   -> TcpData: {} 字节 (id={})", data.len(), connection_id);
                                            println!("[服务端-修复版]   -> ✅ 数据已成功接收（响应头已发送，连接稳定）");

                                            // 回显数据
                                            let response = ProxyPacket::TcpData { connection_id, data };
                                            let _ = send_packet(&tx, response);
                                        }
                                        ProxyPacket::TcpClose { connection_id } => {
                                            println!("[服务端-修复版]   -> TcpClose: (id={})", connection_id);
                                            let response = ProxyPacket::TcpClose { connection_id };
                                            let _ = send_packet(&tx, response);
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("[服务端-修复版] 解码失败: {}", e);
                                    let _ = tx.send(Err(GrpcError::InvalidArgument(format!("解码失败: {}", e))));
                                }
                            }
                        }
                        Err(e) => {
                            println!("[服务端-修复版] 接收错误: {:?}", e);
                            break;
                        }
                    }
                }

                println!("[服务端-修复版] 处理任务结束，共处理 {} 个数据包", packet_count);
            });

            // 立即返回响应流
            let response_stream = stream::unfold(rx, |mut rx| async move {
                match rx.recv().await {
                    Some(result) => Some((result, rx)),
                    None => None,
                }
            });

            // 显式类型转换以满足 trait 要求
            let boxed_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>> =
                Box::pin(response_stream);

            Ok(boxed_stream)
        })
    }
}

fn send_packet(
    tx: &tokio::sync::mpsc::UnboundedSender<Result<GrpcStreamMessage<Vec<u8>>, GrpcError>>,
    packet: ProxyPacket,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = bincode::encode_to_vec(&packet, bincode::config::standard())?;
    let stream_response = GrpcStreamMessage {
        id: 0,
        stream_id: 0,
        sequence: 0,
        end_of_stream: false,
        data,
        metadata: Default::default(),
    };
    tx.send(Ok(stream_response)).map_err(|e| format!("发送失败: {}", e).into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔍 HAProxy TcpData 丢失测试 - 修复版服务端");
    println!("========================================");
    println!("✅ 这个版本使用立即返回响应流的方式");
    println!("✅ 预期：经过 HAProxy 后，TcpData 不会丢失");
    println!();
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
    println!();

    // 配置证书
    let cert_config = CertConfig::from_paths(cert_path, key_path)
        .with_domains(vec!["ligproxy-test.0ldm0s.net".to_string()]);
    let cert_manager_config = CertManagerConfig::shared(cert_config);
    let cert_manager = CertificateManager::from_config(cert_manager_config)?;

    let mut router = Router::new();
    router.enable_grpc_only();
    router.enable_h2();

    // 添加双向流服务
    router.add_grpc_bidirectional("/test.ProxyService/Stream", FixedBidirectionalHandler);

    println!("📡 gRPC 双向流服务:");
    println!("   /test.ProxyService/Stream");
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
