//! gRPC + TLS 客户端流服务端示例
//!
//! 客户端发送多个数据块，服务端返回一个汇总响应
//! 场景：大文件上传、批量数据处理等

use rat_engine::{RatEngine, Router};
use rat_engine::server::grpc_handler::ClientStreamHandler;
use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError, GrpcResponse};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use std::pin::Pin;
use futures_util::{Stream, StreamExt};

/// 数据块请求
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct DataChunk {
    pub chunk_id: u32,
    pub data: String,
    pub size: u32,
}

/// 汇总响应
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ChunkUploadSummary {
    pub total_chunks: u32,
    pub total_size: u32,
    pub success: bool,
    pub message: String,
}

/// 数据块上传处理器
struct ChunkUploadStreamHandler;

impl ClientStreamHandler for ChunkUploadStreamHandler {
    fn handle(
        &self,
        mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        Box::pin(async move {
            println!("[客户端流] 数据块上传服务已连接，开始接收数据块...");

            let mut total_chunks = 0u32;
            let mut total_size = 0u32;
            let mut chunk_ids = Vec::new();

            // 处理接收到的数据块
            while let Some(result) = request_stream.next().await {
                match result {
                    Ok(stream_msg) => {
                        // 检查是否为流结束信号
                        if stream_msg.end_of_stream {
                            println!("[客户端流] 收到流结束信号");
                            break;
                        }

                        // 解码数据块
                        let chunk: DataChunk = match bincode::decode_from_slice(
                            &stream_msg.data,
                            bincode::config::standard()
                        ) {
                            Ok((req, _)) => req,
                            Err(e) => {
                                println!("[客户端流] 解码失败: {}", e);
                                return Err(GrpcError::InvalidArgument(format!("解码失败: {}", e)));
                            }
                        };

                        total_chunks += 1;
                        total_size += chunk.size;
                        chunk_ids.push(chunk.chunk_id);

                        println!("[客户端流] 收到数据块 #{}: 大小={} 字节, 数据={}",
                            chunk.chunk_id, chunk.size, chunk.data);
                    }
                    Err(e) => {
                        println!("[客户端流] 接收错误: {:?}", e);
                        break;
                    }
                }
            }

            println!("[客户端流] 所有数据块接收完成，共 {} 块，总大小 {} 字节",
                total_chunks, total_size);

            // 创建汇总响应
            let summary = ChunkUploadSummary {
                total_chunks,
                total_size,
                success: true,
                message: format!("成功接收 {} 个数据块", total_chunks),
            };

            // 编码响应
            let response_bytes = match bincode::encode_to_vec(
                &summary,
                bincode::config::standard()
            ) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Err(GrpcError::Internal(format!("编码失败: {}", e)));
                }
            };

            let grpc_response = GrpcResponse {
                data: response_bytes,
                status: 0,
                message: "OK".to_string(),
                metadata: Default::default(),
            };

            println!("[客户端流] 返回汇总响应");
            Ok(grpc_response)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 RAT Engine gRPC + TLS 客户端流服务端");
    println!("========================================");
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

    router.add_grpc_client_stream("/upload.ChunkService/Upload", ChunkUploadStreamHandler);

    println!("📡 gRPC 客户端流服务:");
    println!("   /upload.ChunkService/Upload");
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
