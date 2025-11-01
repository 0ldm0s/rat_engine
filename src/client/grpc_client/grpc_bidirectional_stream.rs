  //! gRPC 双向流模块

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use futures_util::{Stream, StreamExt};
use serde::{Serialize, Deserialize};

use crate::server::grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage};
use crate::server::grpc_codec::GrpcCodec;
use crate::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext, ClientStreamSender, ClientStreamInfo};
use crate::utils::logger::{info, debug, error};
use crate::error::{RatError, RatResult};
use crate::client::grpc_client::RatGrpcClient;
use hyper::{Request, Uri, Method};
use hyper::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};
use http_body_util::Full;
use hyper::body::Bytes;
use tokio::sync::mpsc;

impl RatGrpcClient {
    /// 创建委托模式的双向流连接
    /// 
    /// 类似服务端的处理器注册机制，用户只需要实现处理器接口，
    /// 不需要直接管理 sender/receiver，连接池会统一处理资源管理
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `handler` - 双向流处理器
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回流ID，用于后续管理
    /// 
    /// # 示例
    /// ```ignore
    /// use std::sync::Arc;
    /// use rat_engine::client::grpc_client::RatGrpcClient;
    /// use rat_engine::client::grpc_client_delegated::ClientBidirectionalHandler;
    /// 
    /// // 实现自定义的双向流处理器
    /// struct ChatHandler;
    /// 
    /// // 注意：实际使用时需要完整实现 ClientBidirectionalHandler trait
    /// // 这里仅展示方法调用示例
    /// async fn example(client: RatGrpcClient, handler: Arc<impl ClientBidirectionalHandler>) -> Result<u64, Box<dyn std::error::Error>> {
    ///     let stream_id = client.create_bidirectional_stream_delegated(
    ///         "chat.ChatService",
    ///         "BidirectionalChat", 
    ///         handler,
    ///         None
    ///     ).await?;
    ///     Ok(stream_id)
    /// }
    /// ```
    pub async fn create_bidirectional_stream_delegated<H>(
        &self,
        service: &str,
        method: &str,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        H: ClientBidirectionalHandler + 'static,
        <H as ClientBidirectionalHandler>::ReceiveData: bincode::Decode<()>,
    {
        return Err(RatError::RequestError("create_bidirectional_stream_delegated 方法已弃用，请使用 create_bidirectional_stream_delegated_with_uri 方法".to_string()));
    }

    /// 使用指定 URI 创建委托模式双向流
    pub async fn create_bidirectional_stream_delegated_with_uri<H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        H: ClientBidirectionalHandler + 'static,
        <H as ClientBidirectionalHandler>::ReceiveData: bincode::Decode<()>,
    {
        let stream_id = self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        info!("🔗 创建委托模式双向流: {}/{}, 流ID: {}", service, method, stream_id);
        
        // 解析 URI
        let parsed_uri = uri.parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_uri", &[("msg", &e.to_string())])))?;
        
        // 1. 从连接池获取连接
        let connection = self.connection_pool.get_connection(&parsed_uri).await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("get_connection_failed", &[("msg", &e.to_string())])))?;
        let mut send_request = connection.send_request.clone();

        // 构建请求路径
        let path = format!("/{}/{}", service, method);

        // 创建双向流请求
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(CONTENT_TYPE, "application/grpc")
            .header(USER_AGENT, &self.user_agent)
            .body(())
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_bidirectional_stream_request_failed", &[("msg", &e.to_string())])))?;

        // 发送请求并获取响应流
        let (response, send_stream) = send_request.send_request(request, false)
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("send_bidirectional_stream_request_failed", &[("msg", &e.to_string())])))?;

        // 等待响应头
        let response = response.await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("receive_bidirectional_stream_response_failed", &[("msg", &e.to_string())])))?;

        let receive_stream = response.into_body();

        // 2. 创建发送/接收通道
        let (send_tx, send_rx) = mpsc::unbounded_channel::<Bytes>();
        let (recv_tx, recv_rx) = mpsc::unbounded_channel::<Bytes>();

        // 创建流上下文
        let context = ClientStreamContext::new(stream_id, ClientStreamSender::new(send_tx.clone()));

        // 3. 启动发送/接收任务
        let connection_id = connection.connection_id.clone();
        let connection_pool = self.connection_pool.clone();
        
        // 启动发送任务
        let send_task = {
            let mut send_stream = send_stream;
            tokio::spawn(async move {
                let mut send_rx = send_rx;
                let mut message_sent = false;
                
                while let Some(data) = send_rx.recv().await {
                    message_sent = true;
                    
                    // 尝试检查是否为已序列化的 GrpcStreamMessage（关闭指令）
                    let is_close_message = if let Ok(stream_message) = GrpcCodec::decode::<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>>(&data) {
                        stream_message.end_of_stream
                    } else {
                        false
                    };
                    
                    if is_close_message {
                        // 这是来自 ClientStreamSender::send_close() 的关闭指令
                        // 数据已经是序列化的 GrpcStreamMessage，直接构建 gRPC 帧
                        let frame = GrpcCodec::create_frame(&data);
                        
                        if let Err(e) = send_stream.send_data(Bytes::from(frame), true) {
                            // 如果是 inactive stream 错误，这是正常的，不需要记录为错误
                            if e.to_string().contains("inactive stream") {
                                info!("ℹ️ [委托模式] 流已关闭，关闭指令发送被忽略");
                            } else {
                                error!("❌ [委托模式] 发送关闭指令失败: {}", e);
                            }
                        } else {
                            info!("✅ [委托模式] 关闭指令已发送");
                        }
                        break; // 关闭指令发送后退出循环
                    } else {
                        // 这是普通消息数据，需要包装成 gRPC 帧
                        let frame = GrpcCodec::create_frame(&data);
                        
                        if let Err(e) = send_stream.send_data(Bytes::from(frame), false) {
                            error!("发送数据失败: {}", e);
                            break;
                        }
                    }
                }

                
                // 释放连接回连接池
                connection_pool.release_connection(&connection_id);
                info!("消息发送完成，连接已释放");
            })
        };

        // 启动接收任务
        let handler_clone = handler.clone();
        let context_clone = context.clone();
        let recv_task = {
            let mut receive_stream = receive_stream;
            tokio::spawn(async move {
                info!("🔄 [委托模式] 启动双向流接收任务，流ID: {}", stream_id);
                debug!("🔍 [委托模式] 接收任务已启动，等待服务器数据...");
                let mut buffer = Vec::new();
                
                info!("🔄 [委托模式] 开始接收响应流数据...");
                while let Some(chunk_result) = receive_stream.data().await {
                    info!("📡 [委托模式-网络层] ===== 网络数据接收事件 =====");
                    info!("📡 [委托模式-网络层] 数据块结果状态: {:?}", chunk_result.is_ok());
                    match chunk_result {
                        Ok(chunk) => {
                            info!("📡 [委托模式-网络层] ✅ 成功接收网络数据块，大小: {} 字节", chunk.len());
                            debug!("📡 [委托模式-网络层] 数据块内容(前64字节): {:?}", 
                                &chunk[..std::cmp::min(64, chunk.len())]);
                            buffer.extend_from_slice(&chunk);
                            info!("📡 [委托模式-网络层] 数据已添加到缓冲区，当前缓冲区大小: {} 字节", buffer.len());
                            
                            // 尝试解析完整的 gRPC 消息
                            info!("🔍 [委托模式-解析层] ===== 开始解析缓冲区消息 =====");
                            info!("🔍 [委托模式-解析层] 当前缓冲区大小: {} 字节", buffer.len());
                            while buffer.len() >= 5 {
                                let _compression_flag = buffer[0];
                                let message_length = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
                                info!("📏 [委托模式-解析层] 解析到消息长度: {} 字节，压缩标志: {}", message_length, _compression_flag);
                                
                                if buffer.len() >= 5 + message_length {
                                    let message_data = &buffer[5..5 + message_length];
                                    
                                    info!("📨 [委托模式-解析层] ✅ 提取完整消息，大小: {} 字节", message_data.len());
                                    debug!("📨 [委托模式-解析层] 消息数据(前32字节): {:?}", 
                                        &message_data[..std::cmp::min(32, message_data.len())]);
                                    // 首先尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                    info!("🔄 [委托模式-解码层] 开始解码GrpcStreamMessage...");
                                    match GrpcCodec::decode::<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>>(message_data) {
                                        Ok(stream_message) => {
                                            info!("✅ [委托模式] 成功解码GrpcStreamMessage，序列号: {}, 数据大小: {} 字节", stream_message.sequence, stream_message.data.len());
                                            // 检查是否为流结束信号
                                            if stream_message.end_of_stream {
                                                info!("📥 [委托模式] 收到流结束信号");
                                                break;
                                            }
                                            
                                            // 记录数据长度和序列号（在移动前）
                                             let data_len = stream_message.data.len();
                                             let sequence = stream_message.sequence;
                                             
                                             // 从 GrpcStreamMessage 中提取实际的消息数据
                                             let message_bytes = bytes::Bytes::from(stream_message.data);
                                             
                                             info!("📥 [委托模式] 成功解析并转发流消息，序列号: {}, 数据大小: {} 字节", sequence, data_len);
                                            
                                            // 反序列化实际的消息数据
                                            info!("🔄 [委托模式-解码层] ===== 开始解码实际消息数据 =====");
                                            info!("🔄 [委托模式-解码层] 实际消息数据大小: {} 字节", message_bytes.len());
                                            debug!("🔄 [委托模式-解码层] 实际消息数据(前32字节): {:?}", 
                                                &message_bytes[..std::cmp::min(32, message_bytes.len())]);
                                            match GrpcCodec::decode::<H::ReceiveData>(&message_bytes) {
                                                Ok(message) => {
                                                    info!("✅ [委托模式-解码层] 成功解码实际消息，开始调用处理器");
                                                    info!("📞 [委托模式-处理层] ===== 调用用户处理器 =====");
                                                    if let Err(e) = handler_clone.on_message_received(message, &context_clone).await {
                                                        error!("❌ [委托模式] 处理器处理消息失败: {}", e);
                                                        handler_clone.on_error(&context_clone, e).await;
                                                    } else {
                                                        debug!("✅ [委托模式] 处理器处理消息成功");
                                                    }
                                                }
                                                Err(e) => {
                                                    let error_msg = rat_embed_lang::tf("delegate_deserialize_actual_message_failed", &[("msg", &e.to_string())]);
                                                    error!("{}", error_msg);
                                                    handler_clone.on_error(&context_clone, error_msg).await;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let error_msg = rat_embed_lang::tf("delegate_grpc_stream_message_deserialize_failed", &[("msg", &e.to_string())]);
                                            error!("{}", error_msg);
                                            handler_clone.on_error(&context_clone, error_msg).await;
                                        }
                                    }
                                 
                                 // 移除已处理的数据
                                 buffer.drain(0..5 + message_length);
                                 debug!("🗑️ [委托模式] 已移除处理完的数据，剩余缓冲区大小: {} 字节", buffer.len());
                             } else {
                                 // 数据不完整，等待更多数据
                                 debug!("⏳ [委托模式] 消息不完整，等待更多数据 (需要: {}, 当前: {})", 5 + message_length, buffer.len());
                                 break;
                             }
                         }
                     }
                     Err(e) => {
                            let error_msg = rat_embed_lang::tf("receive_data_failed", &[("msg", &e.to_string())]);
                            error!("{}", error_msg);
                            handler_clone.on_error(&context_clone, error_msg).await;
                            break;
                        }
                    }
                }
                
                // 通知处理器连接断开
                handler_clone.on_disconnected(&context_clone, None).await;
                info!("消息接收完成");
            })
        };

        // 4. 传输层不应该主动调用业务逻辑，这些应该由用户在示例代码中控制
        // 用户可以通过返回的 stream_id 获取上下文，然后自行调用处理器方法

        // 存储任务句柄到委托管理器中，以便后续关闭时能够正确清理
        let stream_info = ClientStreamInfo {
            stream_id,
            connection_id: connection.connection_id.clone(),
            send_task: Some(send_task),
            recv_task: Some(recv_task),
            handler_task: None, // 不再由传输层管理业务逻辑任务
            sender_tx: send_tx,
        };
        
        self.delegated_manager.store_stream_info(stream_info).await;
        
        info!("✅ 委托模式双向流 {} 创建完成，任务句柄已存储", stream_id);
        
        Ok(stream_id)
    }
}
