//! PROXY protocol v2 兼容性测试
//!
//! 测试服务器对 HAProxy send-proxy-v2 的支持

use std::pin::Pin;
use std::future::Future;
use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

use rat_engine::{
    server::{Router, ServerConfig, http_request::HttpRequest},
    engine::RatEngine,
    utils::logger,
};
use hyper::{Response, StatusCode, Error};
use http_body_util::Full;
use bytes::Bytes;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct TestResponse {
    pub echo: String,
    pub client_ip: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::info!("🚀 启动 PROXY protocol v2 兼容性测试");

    // 创建路由器
    let mut router = Router::new();

    // 添加测试路由
    router.add_route(
        hyper::Method::GET,
        "/test",
        |req: HttpRequest| {
            Box::pin(async move {
                // 获取客户端IP
                let client_ip = req.client_ip().to_string();

                // 构建响应
                let response = TestResponse {
                    echo: "Hello from server".to_string(),
                    client_ip,
                };

                let json = serde_json::to_string_pretty(&response).unwrap();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from(json)))
                    .unwrap())
            })
        },
    );

    // gRPC 处理器
    use rat_engine::server::grpc_handler::UnaryHandler;
    use rat_engine::server::grpc_types::{GrpcError, GrpcContext, GrpcRequest, GrpcResponse};
    use rat_engine::server::grpc_codec::GrpcCodec;

    struct SimpleGrpcHandler;

    impl UnaryHandler for SimpleGrpcHandler {
        fn handle(
            &self,
            request: GrpcRequest<Vec<u8>>,
            context: GrpcContext,
        ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
            Box::pin(async move {
                // 解码请求
                let test_req: HashMap<String, String> = GrpcCodec::decode(&request.data)
                    .map_err(|e| GrpcError::Internal(format!("解码失败: {}", e)))?;

                // 获取原始客户端IP
                let client_ip = context.remote_addr
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // 构建响应
                let mut response = HashMap::new();
                response.insert("echo".to_string(),
                    format!("gRPC echo: {:?}", test_req.get("message").unwrap_or(&"no message".to_string())));
                response.insert("client_ip".to_string(), client_ip);

                // 编码响应
                let response_data = GrpcCodec::encode(&response)
                    .map_err(|e| GrpcError::Internal(format!("编码失败: {}", e)))?;

                Ok(GrpcResponse {
                    data: response_data,
                    status: 0,
                    message: "OK".to_string(),
                    metadata: HashMap::new(),
                })
            })
        }
    }

    // 注册 gRPC 处理器
    router.add_grpc_unary("/test.Service/Echo", SimpleGrpcHandler);

    // 创建引擎
    let engine = RatEngine::builder()
        .router(router)
        .worker_threads(4)
        .build()
        .map_err(|e| format!("创建引擎失败: {}", e))?;

    logger::info!("✅ 服务器配置完成");
    logger::info!("📍 测试说明:");
    logger::info!("   1. 使用 HAProxy 配置 send-proxy-v2");
    logger::info!("   2. HAProxy 配置示例:");
    logger::info!("      backend test_backend");
    logger::info!("          mode http");
    logger::info!("          server test 127.0.0.1:8080 send-proxy-v2");
    logger::info!("   3. 服务器将识别原始客户端IP而不是HAProxy的IP");
    logger::info!("   4. 访问 http://server/test 查看客户端IP");
    logger::info!("   5. gRPC 调用 /test.Service/Echo 将返回原始客户端IP");

    // 启动服务器
    engine.start("0.0.0.0".to_string(), 8080)
        .await
        .map_err(|e| format!("启动服务器失败: {}", e))?;

    Ok(())
}