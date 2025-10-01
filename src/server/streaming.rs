//! HTTP 流式响应支持模块
//! 
//! 提供高性能的流式数据传输能力：
//! - Server-Sent Events (SSE) 支持（含连接池管理）
//! - 分块传输编码 (Chunked Transfer Encoding)
//! - 自定义流式响应处理器
//! - PyO3 绑定支持
//! - HTTP/1.1 和 HTTP/2（含 H2C）协议兼容

use hyper::{Response, StatusCode, HeaderMap, Request};
use hyper::body::{Bytes, Frame, Incoming};
use http_body_util::{StreamBody, BodyExt};
use tokio_stream::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::collections::HashMap;
use futures_util::stream;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use crate::server::sse_adapter::{SseAdapter, SseAdapterBuilder};
use crate::server::sse_connection_pool::SseConnectionPoolConfig;
use crate::utils::logger::{trace, error};

/// 流式响应体类型
pub type StreamingBody = StreamBody<Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> + Send + Sync>>>;

/// 流式响应构建器
pub struct StreamingResponse {
    status: StatusCode,
    headers: HeaderMap,
    stream: Option<Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> + Send + Sync>>>,
}

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("stream", &"<stream>")
            .finish()
    }
}

impl StreamingResponse {
    /// 创建新的流式响应
    pub fn new() -> Self {
        Self {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            stream: None,
        }
    }

    /// 设置状态码
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// 添加响应头
    pub fn with_header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: hyper::header::IntoHeaderName,
        V: TryInto<hyper::header::HeaderValue>,
    {
        if let Ok(value) = value.try_into() {
            self.headers.insert(key, value);
        }
        self
    }

    /// 设置流式数据源
    pub fn stream<S>(mut self, stream: S) -> Self
    where
        S: Stream<Item = Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> + Send + Sync + 'static,
    {
        self.stream = Some(Box::pin(stream));
        self
    }

    /// 构建 hyper Response
    pub fn build(self) -> Result<Response<StreamingBody>, hyper::Error> {
        let mut response = Response::builder().status(self.status);
        
        // 添加所有头部
        for (key, value) in self.headers.iter() {
            response = response.header(key, value);
        }

        // 如果没有设置流，创建空流
        let stream = self.stream.unwrap_or_else(|| {
            Box::pin(stream::empty())
        });

        let body = StreamBody::new(stream);
        // Convert hyper::http::Error to hyper::Error
        // Since there's no direct conversion, we need to handle this differently
        match response.body(body) {
            Ok(resp) => Ok(resp),
            Err(e) => {
                // Log the error and return a generic hyper error
                crate::utils::logger::error!("Failed to build response: {}", e);
                // Create a simple error response with the same body type
                let error_stream: Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> + Send + Sync>> = 
                    Box::pin(stream::once(async {
                        Ok::<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>(Frame::data(Bytes::from("Internal Server Error")))
                    }));
                let error_body = StreamBody::new(error_stream);
                Ok(Response::builder()
                    .status(500)
                    .body(error_body)
                    .unwrap()) // This unwrap is safe as we're using simple values
            }
        }
    }
}

/// SSE 管理器
/// 提供基于连接池的高级 SSE 功能
pub struct SseManager {
    adapter: Arc<SseAdapter>,
}

impl SseManager {
    /// 创建新的 SSE 管理器
    pub fn new() -> Self {
        let adapter = SseAdapterBuilder::new()
            .max_connections(10000)
            .idle_timeout(Duration::from_secs(300))
            .heartbeat_interval(Duration::from_secs(30))
            .enable_heartbeat(true)
            .enable_statistics(true)
            .build();
        
        Self {
            adapter: Arc::new(adapter),
        }
    }

    /// 使用自定义配置创建 SSE 管理器
    pub fn with_config(config: SseConnectionPoolConfig) -> Self {
        let adapter = SseAdapter::new(config);
        Self {
            adapter: Arc::new(adapter),
        }
    }

    /// 处理 SSE 连接请求
    pub async fn handle_sse_request(
        &self,
        req: Request<Incoming>,
        client_addr: String,
    ) -> crate::error::RatResult<Response<StreamingBody>> {
        self.adapter.handle_sse_request(req, client_addr).await
    }

    /// 广播消息到所有连接
    pub async fn broadcast(&self, event: &str, data: &str) -> usize {
        self.adapter.broadcast(event, data).await
    }

    /// 向特定用户发送消息
    pub async fn send_to_user(&self, user_id: &str, event: &str, data: &str) -> usize {
        self.adapter.send_to_user(user_id, event, data).await
    }

    /// 向特定房间发送消息
    pub async fn send_to_room(&self, room_id: &str, event: &str, data: &str) -> usize {
        self.adapter.send_to_room(room_id, event, data).await
    }

    /// 获取连接统计
    pub fn get_statistics(&self) -> crate::server::sse_connection_pool::SseConnectionStatistics {
        self.adapter.get_statistics()
    }

    /// 获取适配器引用（用于高级操作）
    pub fn get_adapter(&self) -> Arc<SseAdapter> {
        self.adapter.clone()
    }
}

impl Default for SseManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Server-Sent Events (SSE) 流式响应（传统实现，保持向后兼容）
/// 
/// 注意：推荐使用 `SseManager` 进行新的开发，它提供了更好的连接管理和协议支持
pub struct SseResponse {
    pub sender: mpsc::UnboundedSender<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
    pub receiver: mpsc::UnboundedReceiver<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
}

impl SseResponse {
    /// 创建新的 SSE 响应
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self { sender, receiver }
    }

    /// 发送 SSE 事件
    pub fn send_event(&self, event: &str, data: &str) -> Result<(), String> {
        println!("Sending SSE event: type={}, data={}", event, data);
        let formatted = format!("event: {}\ndata: {}\n\n\n", event, data);
        self.sender
            .send(Ok(Frame::data(Bytes::from(formatted))))
            .map_err(|e| {
                println!("Failed to send SSE event: {:?}", e);
                "Failed to send SSE event".to_string()
            })
    }

    /// 发送简单数据
    pub fn send_data(&self, data: &str) -> Result<(), String> {
        println!("Sending SSE data: {}", data);
        let formatted = format!("data: {}\n\n\n", data);
        self.sender
            .send(Ok(Frame::data(Bytes::from(formatted))))
            .map_err(|e| {
                println!("Failed to send SSE data: {:?}", e);
                "Failed to send SSE data".to_string()
            })
    }

    /// 发送保持连接的心跳
    pub fn send_heartbeat(&self) -> Result<(), String> {
        trace!("🫀 [SSE] 发送心跳");
        self.sender
            .send(Ok(Frame::data(Bytes::from(": heartbeat\n\n\n"))))
            .map_err(|e| {
                error!("❌ [SSE] 心跳发送失败: {:?}", e);
                "Failed to send heartbeat".to_string()
            })
    }

    /// 关闭连接
    pub fn close(&self) {
        // 发送器会在 drop 时自动关闭
    }

    /// 构建 SSE 响应
    pub fn build(mut self) -> Result<Response<StreamingBody>, hyper::Error> {
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(self.receiver);
        
        StreamingResponse::new()
            .status(StatusCode::OK)
            .with_header("Content-Type", "text/event-stream")
            .with_header("Cache-Control", "no-cache")
            .with_header("Connection", "keep-alive")
            .with_header("Access-Control-Allow-Origin", "*")
            .stream(stream)
            .build()
    }

    /// 获取发送器的克隆
    pub fn get_sender(&self) -> mpsc::UnboundedSender<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> {
        self.sender.clone()
    }
}

/// 分块流式响应
#[derive(Clone)]
pub struct ChunkedResponse {
    chunks: Vec<Bytes>,
    delay: Option<Duration>,
}

impl ChunkedResponse {
    /// 创建新的分块响应
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            delay: None,
        }
    }

    /// 添加数据块
    pub fn add_chunk<T: Into<Bytes>>(mut self, chunk: T) -> Self {
        self.chunks.push(chunk.into());
        self
    }

    /// 设置块之间的延迟
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// 构建分块响应
    pub fn build(self) -> Result<Response<StreamingBody>, hyper::Error> {
        let chunks = self.chunks;
        let delay = self.delay;

        let stream = stream::iter(chunks.into_iter().enumerate())
            .then(move |(i, chunk)| async move {
                if i > 0 {
                    if let Some(delay) = delay {
                        tokio::time::sleep(delay).await;
                    }
                }
                Ok::<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>(Frame::data(chunk))
            });

        StreamingResponse::new()
            .status(StatusCode::OK)
            .with_header("Transfer-Encoding", "chunked")
            .with_header("Content-Type", "text/plain; charset=utf-8")
            .stream(stream)
            .build()
    }
}

/// 流式响应工具函数
pub mod utils {
    use super::*;
    use serde_json::Value;

    /// 创建 JSON 流响应
    pub fn json_stream<I>(items: I) -> Result<Response<StreamingBody>, hyper::Error>
    where
        I: IntoIterator<Item = Value> + Send + 'static,
        I::IntoIter: Send + Sync,
    {
        let stream = stream::iter(items.into_iter())
            .map(|item| {
                let json_str = serde_json::to_string(&item)
                    .unwrap_or_else(|_| "null".to_string());
                Ok::<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>(Frame::data(Bytes::from(format!("{}
", json_str))))
            });

        StreamingResponse::new()
            .status(StatusCode::OK)
            .with_header("Content-Type", "application/json; charset=utf-8")
            .stream(stream)
            .build()
    }

    /// 创建文本流响应
    pub fn text_stream<I>(lines: I) -> Result<Response<StreamingBody>, hyper::Error>
    where
        I: IntoIterator<Item = String> + Send + 'static,
        I::IntoIter: Send + Sync,
    {
        let stream = stream::iter(lines.into_iter())
            .map(|line| {
                Ok::<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>(Frame::data(Bytes::from(format!("{}
", line))))
            });

        StreamingResponse::new()
            .status(StatusCode::OK)
            .with_header("Content-Type", "text/plain; charset=utf-8")
            .stream(stream)
            .build()
    }

    /// 创建二进制流响应
    pub fn binary_stream<I>(chunks: I) -> Result<Response<StreamingBody>, hyper::Error>
    where
        I: IntoIterator<Item = Vec<u8>> + Send + 'static,
        I::IntoIter: Send + Sync,
    {
        let stream = stream::iter(chunks.into_iter())
            .map(|chunk| {
                Ok::<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>(Frame::data(Bytes::from(chunk)))
            });

        StreamingResponse::new()
            .status(StatusCode::OK)
            .with_header("Content-Type", "application/octet-stream")
            .stream(stream)
            .build()
    }
}
