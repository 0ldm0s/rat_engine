//! gRPC mTLS 客户端示例
//!
//! 独立的客户端程序，便于调试 mTLS 配置

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::time::Duration;
use tokio::time::sleep;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use rat_engine::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext};
use rat_engine::server::cert_manager::{CertConfig, CertManagerConfig};

/// 聊天消息类型
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct ChatMessage {
    pub user: String,
    pub message: String,
    pub timestamp: i64,
    pub message_type: String,
}

/// mTLS 委托处理器
#[derive(Debug)]
struct MtlsDelegatedHandler {
    message_count: Arc<AtomicU32>,
    client_name: String,
}

impl MtlsDelegatedHandler {
    fn new(client_name: String) -> Self {
        Self {
            message_count: Arc::new(AtomicU32::new(0)),
            client_name,
        }
    }
}

#[async_trait::async_trait]
impl ClientBidirectionalHandler for MtlsDelegatedHandler {
    type SendData = ChatMessage;
    type ReceiveData = ChatMessage;

    async fn on_connected(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("🔗 [mTLS客户端] 委托处理器：连接建立，流ID: {}", context.stream_id());

        // 发送初始连接消息，包含客户端身份信息
        let connect_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "Hello from mTLS authenticated client!".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "connect".to_string(),
        };

        context.sender().send_serialized(connect_msg).await?;
        println!("📤 [mTLS客户端] 向服务器发送初始连接消息");

        Ok(())
    }

    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        context: &ClientStreamContext,
    ) -> Result<(), String> {
        let count = self.message_count.fetch_add(1, Ordering::SeqCst) + 1;
        println!("📥 [mTLS客户端] 收到服务器消息 #{} (流ID: {}): {} - {} [{}]",
            count, context.stream_id(), message.user, message.message, message.message_type);

        // 如果收到服务器的认证确认消息，记录日志
        if message.message_type == "auth_confirmed" {
            println!("✅ [mTLS客户端] 服务器确认客户端证书认证成功");
        }

        Ok(())
    }

    async fn on_send_task(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("📤 [mTLS客户端] 开始发送任务 (流ID: {})", context.stream_id());

        // 等待一秒后开始发送消息
        sleep(Duration::from_secs(1)).await;

        // 发送证书信息验证消息
        let cert_info_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "请验证我的客户端证书".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "cert_verification".to_string(),
        };

        context.sender().send_serialized(cert_info_msg).await?;
        println!("📤 [mTLS客户端] 发送证书验证请求");

        sleep(Duration::from_secs(2)).await;

        // 发送业务消息
        for i in 1..=3 {
            let msg = ChatMessage {
                user: self.client_name.clone(),
                message: format!("mTLS 认证消息 #{}", i),
                timestamp: chrono::Utc::now().timestamp(),
                message_type: "business".to_string(),
            };

            let message_content = msg.message.clone();
            context.sender().send_serialized(msg).await?;
            println!("📤 [mTLS客户端] 向服务器发送消息 #{}: {}", i, message_content);

            sleep(Duration::from_secs(2)).await;
        }

        // 发送断开连接消息
        let disconnect_msg = ChatMessage {
            user: self.client_name.clone(),
            message: "Goodbye from mTLS client!".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            message_type: "disconnect".to_string(),
        };

        context.sender().send_serialized(disconnect_msg).await?;
        println!("📤 [mTLS客户端] 发送断开连接消息");

        // 发送关闭指令
        println!("📤 [mTLS委托模式] 发送关闭指令");
        context.sender().send_close().await?;

        println!("📤 [mTLS客户端] 消息发送完成 (流ID: {})", context.stream_id());
        Ok(())
    }

    async fn on_disconnected(&self, context: &ClientStreamContext, reason: Option<String>) {
        let reason_str = reason.unwrap_or_else(|| "未知原因".to_string());
        println!("🔌 [mTLS客户端] 连接断开 (流ID: {}): {}", context.stream_id(), reason_str);
    }

    async fn on_error(&self, context: &ClientStreamContext, error: String) {
        eprintln!("❌ [mTLS客户端] 发生错误 (流ID: {}): {}", context.stream_id(), error);
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    rat_engine::require_features!("client", "tls");

    // 确保 CryptoProvider 只安装一次
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    println!("🚀 启动 gRPC mTLS 客户端");

    // 获取项目根目录
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cert_dir = manifest_dir.join("examples/certs/mtls");

    println!("📁 证书目录: {:?}", cert_dir);

    // 检查证书文件是否存在
    let client_cert_path = cert_dir.join("client-cert-chain.pem");  // 完整证书链
    let client_key_path = cert_dir.join("client-key.pem");
    let ca_cert_path = cert_dir.join("ca-cert.pem");

    println!("   客户端证书: {:?}", client_cert_path);
    println!("   客户端私钥: {:?}", client_key_path);
    println!("   CA 证书: {:?}", ca_cert_path);

    if !client_cert_path.exists() {
        return Err(format!("客户端证书文件不存在: {:?}", client_cert_path).into());
    }
    if !client_key_path.exists() {
        return Err(format!("客户端私钥文件不存在: {:?}", client_key_path).into());
    }
    if !ca_cert_path.exists() {
        return Err(format!("CA 证书文件不存在: {:?}", ca_cert_path).into());
    }

    println!("✅ 所有证书文件检查通过");

    // 使用 mTLS 客户端证书进行双向认证
    println!("🔐 [客户端] 开始创建带 mTLS 证书的 gRPC 客户端");
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(10)).map_err(|e| format!("连接超时设置失败: {}", e))?
        .request_timeout(Duration::from_secs(30)).map_err(|e| format!("请求超时设置失败: {}", e))?
        .max_idle_connections(5).map_err(|e| format!("最大空闲连接设置失败: {}", e))?
        .http2_only()
        .disable_compression()
        .user_agent("rat-engine-mtls-example/1.0").map_err(|e| format!("User-Agent 设置失败: {}", e))?
        .with_client_certs_and_ca(
            client_cert_path.to_string_lossy().to_string(),
            client_key_path.to_string_lossy().to_string(),
            Some(ca_cert_path.to_string_lossy().to_string())
        ).map_err(|e| format!("mTLS 证书设置失败: {}", e))?
        .build().map_err(|e| format!("客户端构建失败: {}", e))?;
    println!("✅ gRPC 客户端创建成功");

    // 创建 mTLS 委托处理器
    let handler = Arc::new(MtlsDelegatedHandler::new("mTLS客户端001".to_string()));

    // 创建委托模式双向流
    let stream_id = client.create_bidirectional_stream_delegated_with_uri(
        "https://ligproxy-test.0ldm0s.net:50053",
        "chat.ChatService",
        "BidirectionalChat",
        handler.clone(),
        None::<HashMap<String, String>>
    ).await.map_err(|e| format!("创建双向流失败: {}", e))?;

    println!("✅ mTLS 委托模式双向流创建成功，流ID: {}", stream_id);

    // 获取流上下文
    if let Some(context) = client.get_stream_context(stream_id).await {
        // 在业务层控制逻辑 - 手动调用处理器方法
        if let Err(e) = handler.on_connected(&context).await {
            eprintln!("❌ [mTLS客户端] 连接建立失败: {}", e);
            let _ = client.close_bidirectional_stream_delegated(stream_id).await;
            return Err(e.into());
        }

        // 启动业务逻辑任务
        let handler_clone = handler.clone();
        let context_clone = context.clone();
        let business_task = tokio::spawn(async move {
            if let Err(e) = handler_clone.on_send_task(&context_clone).await {
                eprintln!("❌ [mTLS客户端] 发送任务失败: {}", e);
            }
        });

        // 等待业务任务完成，但设置超时
        let task_result = tokio::time::timeout(
            Duration::from_secs(20),
            business_task
        ).await;

        match task_result {
            Ok(Ok(_)) => {
                println!("✅ [mTLS客户端] 委托模式业务任务完成");
            }
            Ok(Err(e)) => {
                eprintln!("❌ [mTLS客户端] 委托模式业务任务失败: {}", e);
            }
            Err(_) => {
                println!("⚠️  [mTLS客户端] 委托模式业务任务超时，强制结束");
            }
        }

        // 调用断开连接处理器
        handler.on_disconnected(&context, Some("mTLS客户端主动断开".to_string())).await;
    } else {
        eprintln!("❌ [mTLS客户端] 无法获取流上下文");
        let _ = client.close_bidirectional_stream_delegated(stream_id).await;
        return Err("无法获取流上下文".into());
    }

    // 关闭连接
    if let Err(e) = client.close_bidirectional_stream_delegated(stream_id).await {
        eprintln!("❌ [mTLS客户端] 关闭委托模式双向流失败: {}", e);
        return Err(format!("关闭流失败: {}", e));
    }

    println!("🧹 mTLS 委托模式双向流已关闭");

    // 显式关闭客户端连接池
    client.shutdown().await;

    Ok(())
}
