//! gRPC + TLS 服务端流示例
//!
//! 客户端发送一个请求，服务端返回多个响应
//! 场景：股票报价、新闻推送、数据流等

use rat_engine::{RatEngine, Router};
use rat_engine::server::grpc_handler::{TypedServerStreamHandler, TypedServerStreamAdapter};
use rat_engine::server::grpc_types::{GrpcStreamMessage, GrpcContext, GrpcError};
use rat_engine::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use std::pin::Pin;
use futures_util::{Stream, StreamExt, stream};

/// 股票查询请求
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct StockQueryRequest {
    pub symbol: String,
    pub count: u32,
}

/// 股票报价响应
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct StockQuoteResponse {
    pub symbol: String,
    pub price: f64,
    pub change: f64,
    pub change_percent: f64,
    pub volume: u64,
    pub timestamp: u64,
}

/// 股票报价流处理器（泛型版本）
#[derive(Clone)]
struct StockQuoteStreamHandlerTyped;

impl TypedServerStreamHandler<StockQuoteResponse> for StockQuoteStreamHandlerTyped {
    fn handle_typed(
        &self,
        request: rat_engine::server::grpc_types::GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<StockQuoteResponse>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        Box::pin(async move {
            use std::time::{SystemTime, UNIX_EPOCH};

            // 解码请求
            let query_req: StockQueryRequest = match bincode::decode_from_slice(
                &request.data,
                bincode::config::standard()
            ) {
                Ok((req, _)) => req,
                Err(e) => {
                    return Err(GrpcError::InvalidArgument(format!("解码失败: {}", e)));
                }
            };

            println!("[服务端流] 收到股票查询请求: {} ({} 条报价)", query_req.symbol, query_req.count);

            // 模拟股票价格数据
            let base_price = 100.0;
            let mut quotes = Vec::new();

            for i in 0..query_req.count {
                let change = (i as f64 - 5.0) * 2.0;  // 模拟价格变化
                let change_percent = (change / base_price) * 100.0;

                quotes.push(StockQuoteResponse {
                    symbol: query_req.symbol.clone(),
                    price: base_price + change,
                    change,
                    change_percent,
                    volume: 1000000 + (i as u64 * 100000),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                });
            }

            println!("[服务端流] 生成 {} 条报价，开始发送...", quotes.len());

            // 创建响应流（直接返回 StockQuoteResponse，由框架自动序列化）
            let stream = stream::iter(quotes.into_iter().enumerate())
                .map(move |(index, quote)| {
                    println!("[服务端流] 发送报价 #{}: {} @ ${:.2} ({:+.2}%)",
                        index + 1, quote.symbol, quote.price, quote.change_percent);

                    let stream_response = GrpcStreamMessage {
                        id: request.id,
                        stream_id: 0,
                        sequence: (index + 1) as u64,
                        end_of_stream: false,
                        data: quote,  // 直接使用强类型
                        metadata: Default::default(),
                    };

                    Ok(stream_response)
                });

            Ok(Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<StockQuoteResponse>, GrpcError>> + Send>>)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 RAT Engine gRPC + TLS 服务端流服务端");
    println!("========================================");
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

    // 使用 TypedServerStreamAdapter 包装，自动处理序列化
    router.add_grpc_server_stream(
        "/stock.StockService/GetQuotes",
        TypedServerStreamAdapter::new(StockQuoteStreamHandlerTyped)
    );

    println!("📡 gRPC 服务端流服务:");
    println!("   /stock.StockService/GetQuotes");
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
