//! gRPC + TLS 客户端流客户端示例
//!
//! 客户端发送多个数据块，接收汇总响应

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use rat_engine::client::grpc_client::GrpcStreamSender;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

/// 数据块请求
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, Default)]
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔌 RAT Engine gRPC + TLS 客户端流客户端");
    println!("========================================");
    println!("连接地址: https://ligproxy-test.0ldm0s.net:8443 (通过 HAProxy)");
    println!();

    // 创建 gRPC 客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))?
        .request_timeout(Duration::from_secs(10))?
        .max_idle_connections(5)?
        .http2_only()
        .user_agent("rat-engine-grpc-tls-client-stream-client/1.0")?
        .disable_compression()
        .h2c_mode()  // 跳过证书验证（测试自签名证书）
        .build()?;

    println!("✅ 客户端创建成功");
    println!();

    // 测试客户端流 - 上传数据块
    println!("📤 测试客户端流 - 数据块上传:");
    println!("上传到 /upload.ChunkService/Upload");
    println!();

    // 准备要上传的数据块
    let chunks_to_send = vec![
        DataChunk {
            chunk_id: 1,
            data: "这是第一个数据块".to_string(),
            size: 20,
        },
        DataChunk {
            chunk_id: 2,
            data: "这是第二个数据块".to_string(),
            size: 20,
        },
        DataChunk {
            chunk_id: 3,
            data: "这是第三个数据块".to_string(),
            size: 20,
        },
    ];

    match client.call_client_stream_with_uri::<DataChunk, ChunkUploadSummary>(
        "https://ligproxy-test.0ldm0s.net:8443",
        "upload.ChunkService",
        "Upload",
        None,
    ).await {
        Ok((mut sender, response_rx)) => {
            println!("✅ 客户端流已建立，开始发送数据块...");
            println!();

            // 发送所有数据块
            for chunk in chunks_to_send {
                println!("📤 发送数据块 #{}: 大小={} 字节, 数据={}",
                    chunk.chunk_id, chunk.size, chunk.data);

                match sender.send(chunk).await {
                    Ok(_) => println!("      ✅ 发送成功"),
                    Err(e) => {
                        println!("      ❌ 发送失败: {:?}", e);
                        break;
                    }
                }
                println!();
            }

            // 发送完成，发送结束信号
            println!("📤 所有数据块发送完毕，发送结束信号...");
            let _ = sender.send_close().await;
            println!("✅ 结束信号已发送");
            println!();

            // 等待服务端响应
            println!("⏳ 等待服务端汇总响应...");
            match response_rx.await {
                Ok(Ok(summary)) => {
                    println!("📥 收到汇总响应:");
                    println!("      总数据块数: {}", summary.total_chunks);
                    println!("      总大小: {} 字节", summary.total_size);
                    println!("      成功: {}", summary.success);
                    println!("      消息: {}", summary.message);
                }
                Ok(Err(e)) => {
                    println!("❌ 服务端返回错误: {:?}", e);
                }
                Err(e) => {
                    println!("❌ 接收响应失败: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ 建立客户端流失败: {:?}", e);
        }
    }

    // 关闭客户端
    client.shutdown().await;
    println!("👋 客户端已关闭");

    Ok(())
}
