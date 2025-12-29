//! gRPC + TLS 服务端流客户端示例
//!
//! 客户端发送一个股票查询请求，接收多个报价响应

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use futures_util::StreamExt;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔌 RAT Engine gRPC + TLS 服务端流客户端");
    println!("========================================");
    println!("连接地址: https://ligproxy-test.0ldm0s.net:8443 (通过 HAProxy)");
    println!();

    // 创建 gRPC 客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))?
        .request_timeout(Duration::from_secs(10))?
        .max_idle_connections(5)?
        .http2_only()
        .user_agent("rat-engine-grpc-tls-server-stream-client/1.0")?
        .disable_compression()
        .development_mode()  // 跳过证书验证（测试自签名证书）
        .build()?;

    println!("✅ 客户端创建成功");
    println!();

    // 测试服务端流 - 查询股票报价
    println!("📤 测试服务端流 - 股票报价查询:");
    println!("查询 /stock.StockService/GetQuotes");
    println!();

    let query_request = StockQueryRequest {
        symbol: "AAPL".to_string(),
        count: 5,
    };

    println!("📤 发送查询请求: {} ({} 条报价)", query_request.symbol, query_request.count);
    println!();

    match client.call_server_stream_with_uri::<StockQueryRequest, StockQuoteResponse>(
        "https://ligproxy-test.0ldm0s.net:8443",
        "stock.StockService",
        "GetQuotes",
        query_request,
        None,
    ).await {
        Ok(stream_response) => {
            println!("✅ 服务端流已建立，开始接收报价...");
            println!();

            let mut stream = stream_response.stream;
            let mut count = 0;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(stream_msg) => {
                        count += 1;
                        let quote = stream_msg.data;

                        println!("📥 [报价 #{}] {} @ ${:.2}",
                            count, quote.symbol, quote.price);
                        println!("      涨跌: {:+.2} ({:+.2}%)",
                            quote.change, quote.change_percent);
                        println!("      成交量: {}", quote.volume);
                        println!();

                        // 检查是否为流结束信号
                        if stream_msg.end_of_stream {
                            println!("✅ 收到流结束信号");
                            break;
                        }
                    }
                    Err(e) => {
                        println!("❌ 接收错误: {:?}", e);
                        break;
                    }
                }
            }

            println!("✅ 共接收 {} 条报价", count);
        }
        Err(e) => {
            println!("❌ 建立服务端流失败: {:?}", e);
        }
    }

    // 关闭客户端
    client.shutdown().await;
    println!("👋 客户端已关闭");

    Ok(())
}
