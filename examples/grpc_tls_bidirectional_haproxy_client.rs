//! gRPC + TLS 双向流客户端示例（Ping/Pong 模式）
//!
//! 使用 grpc + bincode
//! 连接到 TLS gRPC 服务器进行双向流通信

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext};
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use std::time::Duration;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Ping/Pong 双向流处理器（客户端）
struct PingPongClientHandler {
    messages: Vec<String>,
    current_index: Arc<AtomicUsize>,
}

#[async_trait]
impl ClientBidirectionalHandler for PingPongClientHandler {
    type SendData = PingRequest;
    type ReceiveData = PongResponse;

    async fn on_connected(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("✅ [客户端] 双向流已建立！流ID: {}", context.stream_id());
        println!();

        // 发送第一条消息（开始 ping/pong）
        self.send_next_ping(context).await?;

        Ok(())
    }

    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        context: &ClientStreamContext,
    ) -> Result<(), String> {
        println!("📥 [Pong #{:03}] 收到响应:", message.sequence);
        println!("      回复: {}", message.reply);
        println!("      时间戳: {}", message.timestamp);
        println!();

        // 收到响应后，发送下一条消息
        self.send_next_ping(context).await?;

        Ok(())
    }

    async fn on_send_task(&self, _context: &ClientStreamContext) -> Result<(), String> {
        // 不需要单独的发送任务
        Ok(())
    }

    async fn on_disconnected(&self, _context: &ClientStreamContext, _reason: Option<String>) {
        println!("👋 双向流已断开");
    }

    async fn on_error(&self, _context: &ClientStreamContext, error: String) {
        println!("❌ 错误: {}", error);
    }
}

impl PingPongClientHandler {
    /// 发送下一条 ping 消息
    async fn send_next_ping(&self, context: &ClientStreamContext) -> Result<(), String> {
        let index = self.current_index.fetch_add(1, Ordering::SeqCst);

        if index < self.messages.len() {
            let msg = &self.messages[index];
            let sequence = (index + 1) as u32;

            let ping_req = PingRequest {
                message: msg.clone(),
                sequence,
            };

            println!("📤 [Ping #{:03}] 发送消息: {}", sequence, msg);

            let sender = context.sender();
            match sender.send_serialized(ping_req).await {
                Ok(_) => println!("      ✅ 发送成功"),
                Err(e) => {
                    println!("      ❌ 发送失败: {:?}", e);
                    return Err(format!("发送失败: {}", e));
                }
            }
            println!();

            Ok(())
        } else {
            // 所有消息发送完毕，发送关闭指令
            println!("📤 所有消息发送完毕，发送关闭指令...");
            let _ = context.sender().send_close().await;
            println!("✅ 流正常结束");
            println!();
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔌 RAT Engine gRPC + TLS 双向流客户端 (Ping/Pong 模式)");
    println!("======================================================");
    println!("连接地址: https://ligproxy-test.0ldm0s.net:8443 (通过 HAProxy)");
    println!();

    // 创建 gRPC 客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))?
        .request_timeout(Duration::from_secs(10))?
        .max_idle_connections(5)?
        .http2_only()
        .user_agent("rat-engine-grpc-tls-bidi-client/1.0")?
        .disable_compression()
        .development_mode()  // 跳过证书验证（测试自签名证书）
        .build()?;

    println!("✅ 客户端创建成功");
    println!();

    // 测试双向流 Ping/Pong 服务
    println!("📤 测试双向流 Ping/Pong 服务:");
    println!("连接到 /chat.ChatService/Chat...");
    println!();

    // 创建处理器
    let handler = Arc::new(PingPongClientHandler {
        messages: vec![
            "你好！".to_string(),
            "今天天气不错".to_string(),
            "再见".to_string(),
        ],
        current_index: Arc::new(AtomicUsize::new(0)),
    });

    // 创建委托模式双向流
    match client.create_bidirectional_stream_delegated_with_uri(
        "https://ligproxy-test.0ldm0s.net:8443",
        "chat.ChatService",
        "Chat",
        handler,
        None,
    ).await {
        Ok(stream_id) => {
            println!("✅ 双向流创建成功，流ID: {}", stream_id);
            println!();

            // 等待 ping/pong 完成
            println!("⏳ 等待 Ping/Pong 完成...");
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            println!("✅ 测试完成");
        }
        Err(e) => {
            println!("❌ 建立双向流失败: {:?}", e);
        }
    }

    // 关闭客户端
    client.shutdown().await;
    println!("👋 客户端已关闭");

    Ok(())
}
