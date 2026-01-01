//! HAProxy TcpData 丢失测试 - 客户端
//!
//! 测试客户端，发送各种数据包来验证 TcpData 是否丢失

use rat_engine::client::{RatGrpcClient, RatGrpcClientBuilder};
use rat_engine::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext};
use std::time::Duration;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// 代理数据包（简化版）
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub enum ProxyPacket {
    TcpConnect { connection_id: u64, target_addr: String, target_port: u16 },
    TcpData { connection_id: u64, data: Vec<u8> },
    TcpClose { connection_id: u64 },
}

/// 测试客户端处理器
struct TestClientHandler {
    response_sender: mpsc::Sender<ProxyPacket>,
}

#[async_trait]
impl ClientBidirectionalHandler for TestClientHandler {
    type SendData = ProxyPacket;
    type ReceiveData = ProxyPacket;

    async fn on_connected(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("[客户端] ✅ 双向流已建立！流ID: {}", context.stream_id());
        println!("[客户端] 开始发送测试数据包...");
        println!();

        // 立即开始发送测试数据包
        self.run_test_sequence(context).await?;

        Ok(())
    }

    async fn on_message_received(
        &self,
        message: ProxyPacket,
        _context: &ClientStreamContext,
    ) -> Result<(), String> {
        println!("[客户端] 📥 收到响应: {:?}", message);

        // 转发到响应通道
        if let Err(_) = self.response_sender.send(message).await {
            println!("[客户端] 警告：响应通道已关闭");
        }

        Ok(())
    }

    async fn on_send_task(&self, _context: &ClientStreamContext) -> Result<(), String> {
        Ok(())
    }

    async fn on_disconnected(&self, _context: &ClientStreamContext, reason: Option<String>) {
        if let Some(reason) = reason {
            println!("[客户端] 👋 连接断开: {}", reason);
        } else {
            println!("[客户端] 👋 连接断开");
        }
    }

    async fn on_error(&self, _context: &ClientStreamContext, error: String) {
        println!("[客户端] ❌ 错误: {}", error);
    }
}

impl TestClientHandler {
    /// 运行测试序列
    async fn run_test_sequence(&self, context: &ClientStreamContext) -> Result<(), String> {
        let sender = context.sender();

        // 1. 发送 TcpConnect
        println!("[客户端] 📤 发送 TcpConnect #1");
        let connect = ProxyPacket::TcpConnect {
            connection_id: 1,
            target_addr: "example.com".to_string(),
            target_port: 443,
        };
        sender.send_serialized(connect).await?;
        println!();

        // 短暂延迟
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 2. 发送多个 TcpData
        for i in 1..=5 {
            println!("[客户端] 📤 发送 TcpData #{}", i);
            let data = format!("测试数据包 #{}", i).into_bytes();
            let packet = ProxyPacket::TcpData {
                connection_id: 1,
                data,
            };
            sender.send_serialized(packet).await?;

            // 短暂延迟（模拟真实场景）
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        println!();

        // 3. 发送 TcpClose
        println!("[客户端] 📤 发送 TcpClose");
        let close = ProxyPacket::TcpClose { connection_id: 1 };
        sender.send_serialized(close).await?;
        println!();

        // 等待一下确保所有数据都发送完毕
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("[客户端] ✅ 测试序列完成");
        println!();

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔍 HAProxy TcpData 丢失测试 - 客户端");
    println!("=====================================");
    println!();

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();
    let endpoint = if args.len() > 1 {
        args[1].clone()
    } else {
        "https://ligproxy-test.0ldm0s.net:8443".to_string()
    };

    println!("连接端点: {}", endpoint);
    println!();

    // 创建响应通道
    let (response_sender, mut response_receiver) = mpsc::channel::<ProxyPacket>(100);

    // 创建 gRPC 客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))?
        .request_timeout(Duration::from_secs(10))?
        .max_idle_connections(5)?
        .http2_only()
        .user_agent("rat-engine-haproxy-test/1.0")?
        .disable_compression()
        .h2c_mode()
        .build()?;

    println!("✅ 客户端创建成功");
    println!();

    // 创建处理器
    let handler = Arc::new(TestClientHandler {
        response_sender: response_sender.clone(),
    });

    // 创建委托模式双向流
    println!("📞 正在建立双向流连接...");
    match client.create_bidirectional_stream_delegated_with_uri(
        &endpoint,
        "test.ProxyService",
        "Stream",
        handler,
        None,
    ).await {
        Ok(stream_id) => {
            println!("✅ 双向流创建成功，流ID: {}", stream_id);
            println!();
        }
        Err(e) => {
            println!("❌ 建立双向流失败: {:?}", e);
            return Err(e.into());
        }
    }

    // 等待测试完成
    println!("⏳ 等待测试完成...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // 关闭客户端
    client.shutdown().await;
    println!("👋 客户端已关闭");
    println!();

    println!("📊 测试总结：");
    println!("   如果服务端收到了所有 5 个 TcpData 包，说明测试通过");
    println!("   如果服务端只收到部分或没有收到 TcpData 包，说明存在问题");
    println!();

    Ok(())
}
