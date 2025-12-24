//! 纯 gRPC + TLS 服务端（一元请求）
//!
//! 使用 grpc + bincode
//! 使用真实 TLS 证书（ligproxy-test.0ldm0s.net）

use rat_engine::{RatEngine, Router};
use rat_engine::server::grpc_handler::UnaryHandler;
use rat_engine::server::grpc_types::{GrpcRequest, GrpcResponse, GrpcContext, GrpcError};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use std::pin::Pin;
use std::future::Future;

/// Hello 请求
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct HelloRequest {
    pub name: String,
}

/// Hello 响应
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct HelloResponse {
    pub message: String,
    pub timestamp: u64,
}

/// Ping 请求
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct PingRequest {
    pub message: String,
}

/// Ping 响应
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct PingResponse {
    pub pong: String,
    pub timestamp: u64,
}

/// Hello 处理器
struct HelloHandler;

impl UnaryHandler for HelloHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        Box::pin(async move {
            // 解码请求
            let hello_req: HelloRequest = match bincode::decode_from_slice(&request.data, bincode::config::standard()) {
                Ok((req, _)) => req,
                Err(e) => {
                    return Err(GrpcError::InvalidArgument(format!("解码失败: {}", e)));
                }
            };

            // 创建响应
            let response = HelloResponse {
                message: format!("你好，{}！欢迎使用 RAT Engine gRPC + TLS 服务！", hello_req.name),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            // 编码响应
            let response_bytes = match bincode::encode_to_vec(&response, bincode::config::standard()) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Err(GrpcError::Internal(format!("编码失败: {}", e)));
                }
            };

            Ok(GrpcResponse {
                data: response_bytes,
                status: 0,
                message: "OK".to_string(),
                metadata: Default::default(),
            })
        })
    }
}

/// Ping 处理器
struct PingHandler;

impl UnaryHandler for PingHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        Box::pin(async move {
            // 解码请求
            let ping_req: PingRequest = match bincode::decode_from_slice(&request.data, bincode::config::standard()) {
                Ok((req, _)) => req,
                Err(e) => {
                    return Err(GrpcError::InvalidArgument(format!("解码失败: {}", e)));
                }
            };

            // 创建响应
            let response = PingResponse {
                pong: format!("Pong: {}", ping_req.message),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };

            // 编码响应
            let response_bytes = match bincode::encode_to_vec(&response, bincode::config::standard()) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Err(GrpcError::Internal(format!("编码失败: {}", e)));
                }
            };

            Ok(GrpcResponse {
                data: response_bytes,
                status: 0,
                message: "OK".to_string(),
                metadata: Default::default(),
            })
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 RAT Engine gRPC + TLS 服务端");
    println!("================================");
    println!("证书: ligproxy-test.0ldm0s.net");
    println!("绑定: 0.0.0.0:50051");
    println!();

    // 验证证书文件
    let cert_path = "../../certs/ligproxy-test.0ldm0s.net.pem";
    let key_path = "../../certs/ligproxy-test.0ldm0s.net-key.pem";

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

    router.add_grpc_unary("/hello.HelloService/Hello", HelloHandler);
    router.add_grpc_unary("/ping.PingService/Ping", PingHandler);

    println!("📡 gRPC 服务:");
    println!("   /hello.HelloService/Hello");
    println!("   /ping.PingService/Ping");
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
