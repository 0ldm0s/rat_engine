//! 委托模式的 gRPC 客户端双向流实现
//! 
//! 采用类似服务端的委托架构，让连接池统一管理双向流连接

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::future::Future;
use futures_util::Stream;
use tokio::sync::{mpsc, RwLock};
use serde::{Serialize, Deserialize};
use bytes;
use crate::error::{RatError, RatResult};
use crate::client::connection_pool::ClientConnectionPool;
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{debug, info, warn, error};

/// 双向流处理器特征（客户端版本）
/// 
/// 类似服务端的 BidirectionalHandler，但适用于客户端场景
#[async_trait::async_trait]
pub trait ClientBidirectionalHandler: Send + Sync {
    /// 发送数据类型
    type SendData: Serialize + Send + Sync;
    /// 接收数据类型
    type ReceiveData: for<'de> Deserialize<'de> + Send + Sync;

    /// 处理连接建立事件
    async fn on_connected(&self, context: &ClientStreamContext) -> Result<(), String>;

    /// 处理接收到的消息
    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        context: &ClientStreamContext,
    ) -> Result<(), String>;

    /// 处理发送任务 - 定期发送消息或响应事件
    async fn on_send_task(&self, context: &ClientStreamContext) -> Result<(), String>;

    /// 处理连接断开事件
    async fn on_disconnected(&self, context: &ClientStreamContext, reason: Option<String>);

    /// 处理错误事件
    async fn on_error(&self, context: &ClientStreamContext, error: String);
}

/// 客户端流上下文
#[derive(Debug, Clone)]
pub struct ClientStreamContext {
    /// 流ID
    stream_id: u64,
    /// 发送端
    sender: ClientStreamSender,
}

impl ClientStreamContext {
    /// 创建新的流上下文
    pub fn new(stream_id: u64, sender: ClientStreamSender) -> Self {
        debug!("🏗️ [ClientStreamContext] 创建新的流上下文，流ID: {}", stream_id);
        
        let context = Self {
            stream_id,
            sender,
        };
        
        debug!("✅ [ClientStreamContext] 流上下文创建完成，流ID: {}", stream_id);
        context
    }

    /// 获取流ID
    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }

    /// 获取发送端
    pub fn sender(&self) -> &ClientStreamSender {
        &self.sender
    }
}

/// 客户端流发送端委托接口
/// 
/// 通过委托模式，用户不需要直接持有发送端，而是通过这个接口发送数据
#[derive(Debug, Clone)]
pub struct ClientStreamSender {
    /// 内部发送通道
    inner: mpsc::UnboundedSender<bytes::Bytes>,
}

impl ClientStreamSender {
    /// 创建新的发送端
    pub fn new(inner: mpsc::UnboundedSender<bytes::Bytes>) -> Self {
        Self { inner }
    }

    /// 发送原始字节数据
    pub async fn send_raw(&self, data: Vec<u8>) -> Result<(), String> {
        debug!("📤 [ClientStreamSender] 准备发送原始数据，大小: {} 字节", data.len());
        
        match self.inner.send(bytes::Bytes::from(data)) {
            Ok(_) => {
                debug!("✅ [ClientStreamSender] 原始数据发送成功");
                Ok(())
            },
            Err(e) => {
                let error_msg = rat_embed_lang::tf("send_failed", &[("msg", &e.to_string())]);
                error!("❌ [ClientStreamSender] {}", error_msg);
                Err(error_msg)
            }
        }
    }
    
    /// 发送序列化数据
    pub async fn send_serialized<T>(&self, data: T) -> Result<(), String>
    where
        T: Serialize + Send + 'static + bincode::Encode,
    {
        let serialized = GrpcCodec::encode(&data)
            .map_err(|e| rat_embed_lang::tf("encode_data_failed", &[("msg", &e.to_string())]))?;
        
        self.send_raw(serialized).await
    }

    /// 发送关闭指令
    pub async fn send_close(&self) -> Result<(), String> {
        use crate::server::grpc_types::GrpcStreamMessage;
        
        // 创建关闭指令消息（使用 Vec<u8> 作为数据类型）
        let close_message = GrpcStreamMessage::<Vec<u8>>::end_of_stream(0, 0, 0);
        
        // 使用统一编解码器序列化关闭消息
        let serialized = GrpcCodec::encode(&close_message)
            .map_err(|e| rat_embed_lang::tf("encode_close_failed", &[("msg", &e.to_string())]))?;
        
        info!("📤 [客户端] ClientStreamSender 发送关闭指令");
        
        // 发送关闭指令到内部通道，如果通道已关闭则忽略错误
        match self.inner.send(bytes::Bytes::from(serialized)) {
            Ok(_) => {
                info!("✅ [客户端] 关闭指令发送成功");
                Ok(())
            },
            Err(_) => {
                // 通道已关闭是正常情况，不需要报错
                info!("ℹ️ [客户端] 发送通道已关闭，跳过关闭指令发送");
                Ok(())
            }
        }
    }
}

/// 委托模式的双向流管理器
/// 
/// 负责管理所有双向流连接，类似服务端的处理器注册机制
#[derive(Debug)]
pub struct ClientBidirectionalManager {
    /// 连接池引用
    connection_pool: Arc<ClientConnectionPool>,
    /// 活跃的双向流连接
    active_streams: Arc<RwLock<HashMap<u64, ClientStreamInfo>>>,
    /// 流ID计数器
    stream_id_counter: std::sync::atomic::AtomicU64,
}

/// 客户端流信息
#[derive(Debug)]
pub struct ClientStreamInfo {
    /// 流ID
    pub stream_id: u64,
    /// 连接ID
    pub connection_id: String,
    /// 发送任务句柄
    pub send_task: Option<tokio::task::JoinHandle<()>>,
    /// 接收任务句柄
    pub recv_task: Option<tokio::task::JoinHandle<()>>,
    /// 处理器任务句柄
    pub handler_task: Option<tokio::task::JoinHandle<()>>,
    /// 发送端通道
    pub sender_tx: mpsc::UnboundedSender<bytes::Bytes>,
}

impl ClientBidirectionalManager {
    /// 创建新的双向流管理器
    pub fn new(connection_pool: Arc<ClientConnectionPool>) -> Self {
        Self {
            connection_pool,
            active_streams: Arc::new(RwLock::new(HashMap::new())),
            stream_id_counter: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// 存储流信息和任务句柄
    pub async fn store_stream_info(&self, stream_info: ClientStreamInfo) {
        debug!("📝 [委托管理器] 开始存储流信息，流ID: {}, 连接ID: {}", stream_info.stream_id, stream_info.connection_id);
        
        let mut streams = self.active_streams.write().await;
        streams.insert(stream_info.stream_id, stream_info);
        
        let total_streams = streams.len();
        debug!("✅ [委托管理器] 流信息存储完成，当前活跃流数量: {}", total_streams);
    }

    /// 检查流是否存在
    pub async fn stream_exists(&self, stream_id: u64) -> bool {
        let streams = self.active_streams.read().await;
        streams.contains_key(&stream_id)
    }
    
    /// 关闭双向流连接
    pub async fn close_stream(&self, stream_id: u64) -> RatResult<()> {
        let mut streams = self.active_streams.write().await;
        
        if let Some(stream_info) = streams.remove(&stream_id) {
            info!("🛑 关闭委托模式双向流: {}", stream_id);
            
            // 先发送关闭指令
            let sender = ClientStreamSender::new(stream_info.sender_tx.clone());
            let _ = sender.send_close().await; // 忽略错误，因为通道可能已关闭
            
            // 给发送任务一点时间处理关闭指令
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            // 然后取消所有任务
            if let Some(send_task) = stream_info.send_task {
                send_task.abort();
            }
            if let Some(recv_task) = stream_info.recv_task {
                recv_task.abort();
            }
            if let Some(handler_task) = stream_info.handler_task {
                handler_task.abort();
            }
            
            // 释放连接
            self.connection_pool.release_connection(&stream_info.connection_id);
            
            info!("✅ 双向流 {} 已完全关闭，资源已释放", stream_id);
        } else {
            warn!("⚠️ 尝试关闭不存在的流: {}", stream_id);
        }
        
        Ok(())
    }

    /// 获取流上下文
    pub async fn get_stream_context(&self, stream_id: u64) -> Option<ClientStreamContext> {
        debug!("🔍 [委托管理器] 查找流上下文，流ID: {}", stream_id);
        
        let streams = self.active_streams.read().await;
        if let Some(stream_info) = streams.get(&stream_id) {
            debug!("✅ [委托管理器] 找到流上下文，流ID: {}, 连接ID: {}", stream_id, stream_info.connection_id);
            let sender = ClientStreamSender::new(stream_info.sender_tx.clone());
            Some(ClientStreamContext::new(stream_id, sender))
        } else {
            warn!("❌ [委托管理器] 未找到流上下文，流ID: {}", stream_id);
            let active_ids: Vec<u64> = streams.keys().cloned().collect();
            debug!("📋 [委托管理器] 当前活跃流ID列表: {:?}", active_ids);
            None
        }
    }

    /// 获取活跃流数量
    pub async fn active_stream_count(&self) -> usize {
        let streams = self.active_streams.read().await;
        streams.len()
    }

    /// 获取所有活跃流的ID列表
    pub async fn get_active_stream_ids(&self) -> Vec<u64> {
        let streams = self.active_streams.read().await;
        streams.keys().cloned().collect()
    }

    /// 关闭所有活跃的双向流
    pub async fn close_all_streams(&self) -> RatResult<()> {
        let stream_ids = self.get_active_stream_ids().await;
        
        for stream_id in stream_ids {
            if let Err(e) = self.close_stream(stream_id).await {
                warn!("⚠️ 关闭委托模式双向流 {} 失败: {}", stream_id, e);
            }
        }
        
        Ok(())
    }
    

}



/// 接收端流包装器
struct ReceiverStream {
    receiver_rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

impl ReceiverStream {
    fn new(receiver_rx: mpsc::UnboundedReceiver<Vec<u8>>) -> Self {
        Self { receiver_rx }
    }
}

impl Stream for ReceiverStream {
    type Item = Result<Vec<u8>, RatError>;
    
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.receiver_rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(data)) => std::task::Poll::Ready(Some(Ok(data))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}