//! gRPC 流处理模块

use std::pin::Pin;
use std::task::{Context, Poll};
use std::collections::HashMap;
use tokio::sync::mpsc;
use futures_util::{Stream, StreamExt};
use hyper::body::Bytes;
use serde::{Serialize, Deserialize};
use bincode;

use crate::server::grpc_codec::GrpcCodec;
use crate::server::grpc_types::GrpcStreamMessage;
use crate::error::{RatError, RatResult};
use crate::utils::logger::{info, debug};
use crate::client::grpc_client::RatGrpcClient;

/// gRPC 流响应
pub struct GrpcStreamResponse<T> {
    /// 流 ID
    pub stream_id: u64,
    /// 响应流
    pub stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<T>, RatError>> + Send>>,
}


/// gRPC 流发送端
pub struct GrpcStreamSender<T> {
    /// 内部发送通道
    inner: mpsc::UnboundedSender<Bytes>,
    /// 类型标记
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Clone for GrpcStreamSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> GrpcStreamSender<T> {
    /// 创建新的发送端
    pub fn new(inner: mpsc::UnboundedSender<Bytes>) -> Self {
        Self {
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> GrpcStreamSender<T>
where
    T: Serialize + bincode::Encode,
{
    /// 发送数据（使用 GrpcCodec 序列化）
    pub async fn send(&mut self, data: T) -> Result<(), String> {
        // 使用统一的编解码器序列化数据
        let serialized = GrpcCodec::encode(&data)
            .map_err(|e| rat_embed_lang::tf("grpc_serialize_failed", &[("msg", &e.to_string())]))?;
        
        info!("📤 [客户端] GrpcStreamSender 发送数据，大小: {} 字节", serialized.len());
        
        // 发送到内部通道
        self.inner.send(Bytes::from(serialized))
            .map_err(|e| rat_embed_lang::tf("send_failed", &[("msg", &e.to_string())]))
    }
}

impl<T> GrpcStreamSender<T>
where
    T: Serialize + bincode::Encode + Default,
{
    /// 发送关闭指令
    pub async fn send_close(&mut self) -> Result<(), String> {
        // 创建关闭指令消息，使用服务端期望的 GrpcStreamMessage<Vec<u8>> 格式
        let close_message = GrpcStreamMessage::<Vec<u8>> {
            id: 0,
            stream_id: 0,
            sequence: 0,
            data: Vec::new(), // 空数据
            end_of_stream: true,
            metadata: HashMap::new(),
        };
        
        // 使用统一的编解码器序列化关闭消息
        let serialized = GrpcCodec::encode(&close_message)
            .map_err(|e| rat_embed_lang::tf("grpc_serialize_close_failed", &[("msg", &e.to_string())]))?;
        
        info!("📤 [客户端] GrpcStreamSender 发送关闭指令，大小: {} 字节", serialized.len());
        
        // 发送关闭指令到内部通道
        self.inner.send(Bytes::from(serialized))
            .map_err(|e| rat_embed_lang::tf("send_close_failed", &[("msg", &e.to_string())]))
    }
}

// 为 Vec<u8> 提供特殊实现，直接发送原始字节
impl GrpcStreamSender<Vec<u8>> {
    /// 发送原始字节数据
    pub async fn send_raw(&mut self, data: Vec<u8>) -> Result<(), String> {
        info!("📤 GrpcStreamSender 发送原始字节数据，大小: {} 字节", data.len());
        
        // 直接发送原始字节，不进行额外序列化
        self.inner.send(Bytes::from(data))
            .map_err(|e| rat_embed_lang::tf("send_failed", &[("msg", &e.to_string())]))
    }
}

/// gRPC 流接收端
pub struct GrpcStreamReceiver<T> {
    /// 内部接收通道
    inner: mpsc::UnboundedReceiver<Bytes>,
    /// 类型标记
    _phantom: std::marker::PhantomData<T>,
}

impl<T> GrpcStreamReceiver<T>
where
    T: for<'de> Deserialize<'de> + bincode::Decode<()>,
{
    /// 创建新的接收端
    fn new(inner: mpsc::UnboundedReceiver<Bytes>) -> Self {
        Self {
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Stream for GrpcStreamReceiver<T>
where
    T: for<'de> Deserialize<'de> + Unpin + bincode::Decode<()>,
{
    type Item = Result<T, RatError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.poll_recv(cx) {
            Poll::Ready(Some(bytes)) => {
                // 使用统一的编解码器反序列化数据
                match GrpcCodec::decode::<T>(&bytes) {
                    Ok(data) => {
                        info!("📥 [客户端] GrpcStreamReceiver 接收数据，大小: {} 字节", bytes.len());
                        Poll::Ready(Some(Ok(data)))
                    },
                    Err(e) => Poll::Ready(Some(Err(RatError::DecodingError(rat_embed_lang::tf("grpc_deserialize_failed", &[("msg", &e.to_string())]))))),
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
