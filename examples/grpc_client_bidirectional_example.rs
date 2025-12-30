//! gRPC 客户端双向流示例（委托模式）
//!
//! 展示如何使用 rat_engine 的 gRPC 客户端进行双向流通信
//! 使用委托模式实现业务逻辑与传输层分离的最佳实践

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::time::Duration;
use tokio::time::sleep;
use futures_util::stream::StreamExt;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

use rat_engine::client::grpc_client::RatGrpcClient;
use rat_engine::client::grpc_builder::RatGrpcClientBuilder;
use rat_engine::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext};

/// 聊天消息类型
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct ChatMessage {
    pub user: String,
    pub message: String,
    pub timestamp: i64,
}

/// 简单的委托处理器（不包含业务逻辑）
/// 
/// 这个处理器只负责传输层的消息处理，不包含具体的业务逻辑
#[derive(Debug)]
struct SimpleDelegatedHandler {
    message_count: Arc<AtomicU32>,
}

impl SimpleDelegatedHandler {
    fn new() -> Self {
        Self {
            message_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl ClientBidirectionalHandler for SimpleDelegatedHandler {
    type SendData = ChatMessage;
    type ReceiveData = ChatMessage;

    async fn on_connected(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("🔗 [客户端] 委托处理器：连接建立，流ID: {}", context.stream_id());
        
        // 发送初始连接消息
        let connect_msg = ChatMessage {
            user: "委托客户端".to_string(),
            message: "Hello from delegated client!".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };
        
        context.sender().send_serialized(connect_msg).await?;
        println!("📤 [客户端] 向服务器发送初始连接消息");
        
        Ok(())
    }

    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        context: &ClientStreamContext,
    ) -> Result<(), String> {
        let count = self.message_count.fetch_add(1, Ordering::SeqCst) + 1;
        println!("📥 [客户端] 收到服务器消息 #{} (流ID: {}): {} - {}", 
            count, context.stream_id(), message.user, message.message);
        Ok(())
    }

    async fn on_send_task(&self, context: &ClientStreamContext) -> Result<(), String> {
        println!("📤 [客户端] 开始发送任务 (流ID: {})", context.stream_id());
        
        // 等待一秒后开始发送消息
        sleep(Duration::from_secs(1)).await;
        
        for i in 1..=5 {
            let msg = ChatMessage {
                user: "委托客户端".to_string(),
                message: format!("委托消息 #{}", i),
                timestamp: chrono::Utc::now().timestamp(),
            };
            
            let message_content = msg.message.clone();
            context.sender().send_serialized(msg).await?;
            println!("📤 [客户端] 向服务器发送消息 #{}: {}", i, message_content);
            
            sleep(Duration::from_secs(2)).await;
        }
        
        // 发送关闭指令
        println!("📤 [委托模式] 发送关闭指令");
        context.sender().send_close().await?;
        
        println!("📤 [客户端] 消息发送完成 (流ID: {})", context.stream_id());
        Ok(())
    }

    async fn on_disconnected(&self, context: &ClientStreamContext, reason: Option<String>) {
        if let Some(reason) = reason {
            println!("🔌 [客户端] 与服务器连接断开 (流ID: {}): {}", context.stream_id(), reason);
        } else {
            println!("🔌 [客户端] 与服务器连接断开 (流ID: {})", context.stream_id());
        }
    }

    async fn on_error(&self, context: &ClientStreamContext, error: String) {
        eprintln!("❌ [客户端] 处理器错误 (流ID: {}): {}", context.stream_id(), error);
    }
}

/// 启动测试服务器
async fn start_test_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rat_engine::server::{ServerConfig, Router};
    use rat_engine::server::grpc_handler::BidirectionalHandler;
    use rat_engine::server::grpc_types::{GrpcError, GrpcContext, GrpcStreamMessage};
    use std::pin::Pin;
    use futures_util::{Stream, StreamExt};
    use async_stream::stream;
    use tokio::sync::mpsc;
    
    // 简单的回声处理器
    #[derive(Clone)]
    struct EchoHandler;
    
    impl BidirectionalHandler for EchoHandler {
        fn handle(
            &self,
            mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
            _context: GrpcContext,
        ) -> Pin<Box<dyn futures_util::Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
            Box::pin(async move {
                println!("🔄 [服务器] 新的双向流连接建立");
                
                let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
                
                // 处理传入消息的任务
                tokio::spawn(async move {
                    while let Some(result) = request_stream.next().await {
                        match result {
                            Ok(msg) => {
                                if msg.end_of_stream {
                                    println!("📥 [服务器] 收到流结束信号，停止处理");
                                    break;
                                }
                                
                                // 解析消息并回声
                                match bincode::decode_from_slice::<ChatMessage, _>(&msg.data, bincode::config::standard()) {
                                    Ok((chat_msg, _)) => {
                                        let echo_msg = ChatMessage {
                                            user: "服务器".to_string(),
                                            message: format!("回声: {}", chat_msg.message),
                                            timestamp: chrono::Utc::now().timestamp(),
                                        };
                                        
                                        println!("📥 [服务器] 收到客户端消息: {} - {}", chat_msg.user, chat_msg.message);
                                        println!("📤 [服务器] 向客户端发送回声: {}", echo_msg.message);
                                        
                                        if let Ok(data) = bincode::encode_to_vec(&echo_msg, bincode::config::standard()) {
                                            // 检查发送是否成功，如果失败说明客户端已断开
                                            if tx.send(data).is_err() {
                                                println!("🔌 [服务器] 响应通道已关闭，停止发送回声");
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("❌ [服务器] 消息解析失败: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                let error_msg = format!("{}", e);
                                // 检查是否是正常的连接关闭
                                if error_msg.contains("stream no longer needed") || error_msg.contains("connection closed") {
                                    println!("📥 [服务器] 客户端正常断开连接");
                                } else {
                                    eprintln!("❌ [服务器] 接收客户端消息失败: {}", e);
                                }
                                break;
                            }
                        }
                    }
                    println!("🧹 [服务器] 客户端消息处理任务结束");
                });
                
                // 创建响应流
                let response_stream = stream! {
                    let mut sequence = 0u64;
                    
                    while let Some(data) = rx.recv().await {
                        sequence += 1;
                        yield Ok(GrpcStreamMessage {
                            id: sequence,
                            stream_id: 1,
                            sequence,
                            data,
                            end_of_stream: false,
                            metadata: HashMap::new(),
                        });
                    }
                    
                    // 发送结束消息
                    sequence += 1;
                    yield Ok(GrpcStreamMessage {
                        id: sequence,
                        stream_id: 1,
                        sequence,
                        data: Vec::new(),
                        end_of_stream: true,
                        metadata: HashMap::new(),
                    });
                };
                
                Ok(Box::pin(response_stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>)
            })
        }
    }
    
    let handler = EchoHandler;
    
    // 创建服务器配置
    let server_config = ServerConfig::new(
        "127.0.0.1:50051".parse()?,
        4
    ).with_log_config(rat_engine::utils::logger::LogConfig::default());
    
    // 创建路由器
    let mut router = Router::new();
    
    // 启用 HTTP/2 支持（gRPC 需要 HTTP/2）
    router.enable_h2();
    router.enable_h2c(); // 启用 H2C 以支持明文 HTTP/2
    
    // 注册 gRPC 双向流处理器
    router.add_grpc_bidirectional("/chat.ChatService/BidirectionalChat", handler);
    
    println!("🚀 gRPC 服务器启动在 127.0.0.1:50051");
    
    // 启动服务器
    let engine = rat_engine::RatEngine::builder()
        .router(router)
        .enable_h2c_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await
        .map_err(|e| format!("启用开发模式失败: {}", e))?
        .build()?;
    
    engine.start("127.0.0.1".to_string(), 50051).await
        .map_err(|e| format!("服务器启动失败: {}", e).into())
}

/// 运行委托模式测试
async fn run_delegated_mode() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 启动委托模式双向流测试...");
    
    // 创建客户端
    let mut client = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(10)?
        .http2_only()
        .user_agent("rat-engine-example/1.0")?
        .disable_compression()
        .h2c_mode()
        .build()?;
    
    // 创建简单的委托处理器（不包含业务逻辑）
    let handler = Arc::new(SimpleDelegatedHandler::new());
    
    // 创建委托模式双向流
    let stream_id = client.create_bidirectional_stream_delegated_with_uri(
        "http://127.0.0.1:50051",
        "chat.ChatService",
        "BidirectionalChat",
        handler.clone(),
        None::<HashMap<String, String>>
    ).await?;
    
    println!("✅ 委托模式双向流创建成功，流ID: {}", stream_id);
    
    // 获取流上下文
    if let Some(context) = client.get_stream_context(stream_id).await {
        // 在业务层控制逻辑 - 手动调用处理器方法
        if let Err(e) = handler.on_connected(&context).await {
            eprintln!("❌ [客户端] 连接建立失败: {}", e);
            // 确保清理资源
            let _ = client.close_bidirectional_stream_delegated(stream_id).await;
            return Err(e.into());
        }
        
        // 启动业务逻辑任务
        let handler_clone = handler.clone();
        let context_clone = context.clone();
        let business_task = tokio::spawn(async move {
            if let Err(e) = handler_clone.on_send_task(&context_clone).await {
                eprintln!("❌ [客户端] 发送任务失败: {}", e);
            }
        });
        
        // 等待业务任务完成，但设置超时
        let task_result = tokio::time::timeout(
            Duration::from_secs(15),
            business_task
        ).await;
        
        match task_result {
            Ok(Ok(_)) => {
                println!("✅ [客户端] 委托模式业务任务完成");
            }
            Ok(Err(e)) => {
                eprintln!("❌ [客户端] 委托模式业务任务失败: {}", e);
            }
            Err(_) => {
                println!("⚠️ [客户端] 委托模式业务任务超时，强制结束");
            }
        }
        
        // 调用断开连接处理器
        handler.on_disconnected(&context, Some("客户端主动断开".to_string())).await;
    } else {
        eprintln!("❌ [客户端] 无法获取流上下文");
        // 确保清理资源
        let _ = client.close_bidirectional_stream_delegated(stream_id).await;
        return Err("无法获取流上下文".into());
    }
    
    // 关闭连接
    if let Err(e) = client.close_bidirectional_stream_delegated(stream_id).await {
        eprintln!("❌ [客户端] 关闭委托模式双向流失败: {}", e);
        return Err(Box::new(e));
    }
    
    println!("🧹 委托模式双向流已关闭");
    
    // 显式关闭客户端连接池
    client.shutdown().await;
    
    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 检查必需的特性
    rat_engine::require_features!("client");

    // 框架层会自动确保 CryptoProvider 只安装一次

    // 初始化日志
    // 日志通过RatEngineBuilder初始化

    println!("🚀 启动 gRPC 客户端双向流示例");
    
    // 检查命令行参数（现在只支持委托模式）
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        println!("📖 使用说明: 此示例现在只支持委托模式");
        println!("  直接运行程序即可启动委托模式测试");
        return Ok(());
    }
    
    // 启动服务器任务
    let server_task = tokio::spawn(async {
        if let Err(e) = start_test_server().await {
            eprintln!("❌ [服务器] 启动失败: {}", e);
        }
    });
    
    // 等待服务器启动
    sleep(Duration::from_secs(2)).await;
    
    // 执行测试逻辑（现在只支持委托模式）
    let test_result = run_delegated_mode().await;
    
    // 处理测试结果
    if let Err(e) = test_result {
        eprintln!("❌ [客户端] 测试失败: {}", e);
    }
    
    println!("✅ 所有测试完成，开始清理资源...");
    
    // 强制关闭服务器任务
    server_task.abort();
    
    // 等待服务器任务完全结束
    let _ = tokio::time::timeout(Duration::from_secs(5), server_task).await;
    
    println!("🧹 资源清理完成");
    
    // 强制退出程序，避免死锁
    std::process::exit(0);
}