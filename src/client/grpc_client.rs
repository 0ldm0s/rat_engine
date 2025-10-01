//! RAT Engine gRPC+Bincode 客户端实现
//! 
//! 基于 hyper 和 bincode 2.x 的高性能 gRPC 客户端，与服务端保持技术栈一致性
//! 支持 lz4 压缩或禁用压缩，默认为禁用

use std::time::Duration;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::io::Read;
use crate::client::builder::ClientProtocolMode;
use hyper::{Request, Response, Method, Uri, StatusCode};
use hyper::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT, CONTENT_TYPE, CONTENT_ENCODING, ACCEPT_ENCODING};
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::{Full, BodyExt};
use hyper::body::Bytes;
use serde::{Serialize, Deserialize};

use tokio::time::timeout;
use tokio::sync::mpsc;
use futures_util::{Stream, StreamExt, SinkExt};
use h2;
use bytes;
use async_stream;
use bincode;

use crate::error::{RatError, RatResult};
use crate::compression::{CompressionType, CompressionConfig};
use h2::{client::SendRequest, RecvStream};
use std::sync::Arc;
use crate::client::connection_pool::{ClientConnectionPool, ConnectionPoolConfig};
use crate::client::grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext, ClientStreamSender, ClientBidirectionalManager, ClientStreamInfo};
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{debug, info, warn, error};

// 条件导入 Python API
#[cfg(feature = "python")]
use crate::python_api::client::GrpcUnaryHandler;



/// gRPC 压缩模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcCompressionMode {
    /// 禁用压缩（默认）
    Disabled,
    /// 启用 LZ4 压缩
    Lz4,
}

impl GrpcCompressionMode {
    /// 获取压缩算法名称
    pub fn name(&self) -> &'static str {
        match self {
            Self::Disabled => "identity",
            Self::Lz4 => "lz4",
        }
    }

    /// 获取 Accept-Encoding 头部值
    pub fn accept_encoding(&self) -> &'static str {
        match self {
            Self::Disabled => "identity",
            Self::Lz4 => "lz4, identity",
        }
    }
}

// 使用统一的 gRPC 类型定义
pub use crate::server::grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage};

/// gRPC 流响应
pub struct GrpcStreamResponse<T> {
    /// 流 ID
    pub stream_id: u64,
    /// 响应流
    pub stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<T>, RatError>> + Send>>,
}

/// gRPC 双向流连接
pub struct GrpcBidirectionalStream<S, R> {
    /// 发送端（与示例期望的字段名匹配）
    pub sender: GrpcStreamSender<S>,
    /// 接收端（与示例期望的字段名匹配）
    pub receiver: GrpcStreamReceiver<R>,
    /// 发送任务句柄
    pub send_task: Option<tokio::task::JoinHandle<()>>,
    /// 接收任务句柄
    pub recv_task: Option<tokio::task::JoinHandle<()>>,
    /// 连接ID
    connection_id: String,
    /// 连接池引用
    connection_pool: Arc<ClientConnectionPool>,
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
    fn new(inner: mpsc::UnboundedSender<Bytes>) -> Self {
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
            .map_err(|e| format!("GrpcCodec 序列化失败: {}", e))?;
        
        info!("📤 [客户端] GrpcStreamSender 发送数据，大小: {} 字节", serialized.len());
        
        // 发送到内部通道
        self.inner.send(Bytes::from(serialized))
            .map_err(|e| format!("发送失败: {}", e))
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
            .map_err(|e| format!("GrpcCodec 序列化关闭指令失败: {}", e))?;
        
        info!("📤 [客户端] GrpcStreamSender 发送关闭指令，大小: {} 字节", serialized.len());
        
        // 发送关闭指令到内部通道
        self.inner.send(Bytes::from(serialized))
            .map_err(|e| format!("发送关闭指令失败: {}", e))
    }
}

// 为 Vec<u8> 提供特殊实现，直接发送原始字节
impl GrpcStreamSender<Vec<u8>> {
    /// 发送原始字节数据
    pub async fn send_raw(&mut self, data: Vec<u8>) -> Result<(), String> {
        info!("📤 GrpcStreamSender 发送原始字节数据，大小: {} 字节", data.len());
        
        // 直接发送原始字节，不进行额外序列化
        self.inner.send(Bytes::from(data))
            .map_err(|e| format!("发送失败: {}", e))
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
                    Err(e) => Poll::Ready(Some(Err(RatError::DecodingError(format!("GrpcCodec 反序列化失败: {}", e))))),
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// RAT Engine gRPC+Bincode 客户端
/// 
/// 提供基于 hyper 和 bincode 2.x 的高性能 gRPC 客户端实现，支持：
/// - 连接池管理和复用
/// - 自动保活机制
/// - 超时控制
/// - Bincode 2.x 序列化/反序列化
/// - LZ4 压缩（可选）
/// - 自动重试（可选）
/// - 请求/响应日志
/// - H2C (HTTP/2 over cleartext) 支持
#[derive(Debug)]
pub struct RatGrpcClient {
    /// 底层 hyper 客户端
    client: Client<HttpConnector, Full<Bytes>>,
    /// base_uri: 服务器基础 URI（已移除，现在在每次请求时传入）
    // base_uri: Uri, // 已移除
    /// 连接超时时间
    connect_timeout: Duration,
    /// 请求超时时间
    request_timeout: Duration,
    /// 最大空闲连接数
    max_idle_connections: usize,
    /// 用户代理字符串
    user_agent: String,
    /// 压缩配置
    compression_config: CompressionConfig,
    /// 是否启用压缩
    enable_compression: bool,
    /// 是否启用自动重试
    enable_retry: bool,
    /// 最大重试次数
    max_retries: u32,
    /// 客户端连接池
    connection_pool: Arc<ClientConnectionPool>,
    /// 压缩模式
    compression_mode: GrpcCompressionMode,
    /// 请求 ID 计数器
    request_id_counter: std::sync::atomic::AtomicU64,
    /// 流 ID 计数器
    stream_id_counter: std::sync::atomic::AtomicU64,
    /// 委托模式双向流管理器
    delegated_manager: Arc<ClientBidirectionalManager>,
    /// 是否启用开发模式（跳过证书验证）
    development_mode: bool,
    /// mTLS 客户端配置
    mtls_config: Option<crate::client::grpc_builder::MtlsClientConfig>,
}

impl Clone for RatGrpcClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            connect_timeout: self.connect_timeout,
            request_timeout: self.request_timeout,
            max_idle_connections: self.max_idle_connections,
            user_agent: self.user_agent.clone(),
            compression_config: self.compression_config.clone(),
            enable_compression: self.enable_compression,
            enable_retry: self.enable_retry,
            max_retries: self.max_retries,
            connection_pool: self.connection_pool.clone(),
            compression_mode: self.compression_mode,
            request_id_counter: std::sync::atomic::AtomicU64::new(0),
            stream_id_counter: std::sync::atomic::AtomicU64::new(0),
            delegated_manager: self.delegated_manager.clone(),
            development_mode: self.development_mode,
            mtls_config: self.mtls_config.as_ref().map(|config| {
                crate::client::grpc_builder::MtlsClientConfig {
                    client_cert_chain: config.client_cert_chain.clone(),
                    client_private_key: config.client_private_key.clone_key(),
                    ca_certs: config.ca_certs.clone(),
                    skip_server_verification: config.skip_server_verification,
                    server_name: config.server_name.clone(),
                    client_cert_path: config.client_cert_path.clone(),
                    client_key_path: config.client_key_path.clone(),
                    ca_cert_path: config.ca_cert_path.clone(),
                }
            }),
        }
    }
}

impl RatGrpcClient {
    /// 创建新的 gRPC 客户端实例
    /// 
    /// # 参数
    /// * `client` - hyper 客户端实例
    /// * `base_uri` - 服务器基础 URI
    /// * `connect_timeout` - 连接超时时间
    /// * `request_timeout` - 请求超时时间
    /// * `max_idle_connections` - 最大空闲连接数
    /// * `user_agent` - 用户代理字符串
    /// * `compression_config` - 压缩配置
    /// * `enable_compression` - 是否启用压缩
    /// * `enable_retry` - 是否启用自动重试
    /// * `max_retries` - 最大重试次数
    /// * `compression_mode` - 压缩模式
    /// * `development_mode` - 是否启用开发模式（跳过证书验证）
    /// * `mtls_config` - mTLS 客户端配置
    #[doc(hidden)]
    pub fn new(
        client: Client<HttpConnector, Full<Bytes>>,
        connect_timeout: Duration,
        request_timeout: Duration,
        max_idle_connections: usize,
        user_agent: String,
        compression_config: CompressionConfig,
        enable_compression: bool,
        enable_retry: bool,
        max_retries: u32,
        compression_mode: GrpcCompressionMode,
        development_mode: bool,
        mtls_config: Option<crate::client::grpc_builder::MtlsClientConfig>,
    ) -> Self {
        // 创建连接池配置
        let pool_config = ConnectionPoolConfig {
            max_connections: max_idle_connections * 2, // 总连接数为空闲连接数的2倍
            idle_timeout: Duration::from_secs(300), // 5分钟空闲超时
            keepalive_interval: Duration::from_secs(30), // 30秒保活间隔
            connect_timeout,
            cleanup_interval: Duration::from_secs(60), // 1分钟清理间隔
            max_connections_per_target: max_idle_connections,
            development_mode, // 传递开发模式配置
            mtls_config: mtls_config.clone(), // 传递 mTLS 配置给连接池
            protocol_mode: ClientProtocolMode::Auto, // gRPC 默认使用自动模式
        };

        // 创建连接池
        let mut connection_pool = ClientConnectionPool::new(pool_config);
        connection_pool.start_maintenance_tasks();
        let connection_pool = Arc::new(connection_pool);

        // 创建委托管理器
        let delegated_manager = Arc::new(ClientBidirectionalManager::new(connection_pool.clone()));

        Self {
            client,
            connect_timeout,
            request_timeout,
            max_idle_connections,
            user_agent,
            compression_config,
            enable_compression,
            enable_retry,
            max_retries,
            connection_pool,
            compression_mode,
            request_id_counter: std::sync::atomic::AtomicU64::new(1),
            stream_id_counter: std::sync::atomic::AtomicU64::new(1),
            delegated_manager,
            development_mode,
            mtls_config,
        }
    }

    /// 发送一元 gRPC 请求
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回 gRPC 响应
    pub async fn call<T, R>(&self, service: &str, method: &str, request_data: T, metadata: Option<HashMap<String, String>>) -> RatResult<GrpcResponse<R>>
    where
        T: Serialize + Send + Sync + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + bincode::Decode<()>,
    {
        return Err(RatError::RequestError("call 方法已弃用，请使用 call_with_uri 方法".to_string()));
    }

    /// 使用指定 URI 进行 gRPC 调用
    pub async fn call_with_uri<T, R>(&self, uri: &str, service: &str, method: &str, request_data: T, metadata: Option<HashMap<String, String>>) -> RatResult<GrpcResponse<R>>
    where
        T: Serialize + Send + Sync + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + bincode::Decode<()>,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // 构建 gRPC 请求
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: request_data,
            metadata: metadata.unwrap_or_default(),
        };

        // 使用统一的编解码器编码并创建帧
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(format!("编码 gRPC 请求失败: {}", e)))?;

        // 一元请求直接使用 gRPC 消息格式，不进行额外的 HTTP 压缩
        let compressed_data = Bytes::from(grpc_message);
        let content_encoding: Option<&'static str> = None;

        // 构建 HTTP 请求
        let base_uri_str = uri.trim_end_matches('/').to_string();
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", base_uri_str, path);
        

        
        let uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(format!("无效的用户代理: {}", e)))?);
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(self.compression_mode.accept_encoding()));
        
        if let Some(encoding) = content_encoding {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }

        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Full::new(compressed_data))
            .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)))?;

        // 添加头部
        let (mut parts, body) = request.into_parts();
        parts.headers = headers;
        let request = Request::from_parts(parts, body);

        // 发送请求
        let (status, headers, body) = self.send_request(request).await?;

        // 解析响应
        self.parse_grpc_response(status, headers, body)
    }

    /// 发送一元 gRPC 请求（类型化版本）
    /// 
    /// 类似于 call_typed_server_stream，但用于一元调用
    /// 自动处理请求数据的序列化，避免手动序列化步骤
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据（强类型）
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回 gRPC 响应（强类型）
    pub async fn call_typed<T, R>(&self, service: &str, method: &str, request_data: T, metadata: Option<HashMap<String, String>>) -> RatResult<GrpcResponse<R>>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        R: for<'de> Deserialize<'de> + Send + Sync + bincode::Decode<()>,
    {
        return Err(RatError::RequestError("call_typed 方法已弃用，请使用 call_typed_with_uri 方法".to_string()));
    }

    /// 使用指定 URI 进行强类型 gRPC 调用
    pub async fn call_typed_with_uri<T, R>(&self, uri: &str, service: &str, method: &str, request_data: T, metadata: Option<HashMap<String, String>>) -> RatResult<GrpcResponse<R>>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        R: for<'de> Deserialize<'de> + Send + Sync + bincode::Decode<()>,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // 先序列化强类型数据为 Vec<u8>，然后包装到 GrpcRequest 中
        // 这样服务端就能接收到 GrpcRequest<Vec<u8>> 格式的数据
        let serialized_data = GrpcCodec::encode(&request_data)
            .map_err(|e| RatError::SerializationError(format!("序列化请求数据失败: {}", e)))?;
        
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: serialized_data, // 使用序列化后的 Vec<u8> 数据
            metadata: metadata.unwrap_or_default(),
        };

        // 使用统一的编解码器编码并创建帧
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(format!("编码 gRPC 请求失败: {}", e)))?;

        // 一元请求直接使用 gRPC 消息格式，不进行额外的 HTTP 压缩
        let compressed_data = Bytes::from(grpc_message);
        let content_encoding: Option<&'static str> = None;

        // 构建 HTTP 请求
        let base_uri_str = uri.trim_end_matches('/').to_string();
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", base_uri_str, path);
        
        let uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(format!("无效的用户代理: {}", e)))?);
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(self.compression_mode.accept_encoding()));
        
        if let Some(encoding) = content_encoding {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }

        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Full::new(compressed_data))
            .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)))?;

        // 添加头部
        let (mut parts, body) = request.into_parts();
        parts.headers = headers;
        let request = Request::from_parts(parts, body);

        // 发送请求
        let (status, headers, body) = self.send_request(request).await?;

        // 解析响应
        self.parse_grpc_response(status, headers, body)
    }

    /// 构建标准 gRPC 消息格式
    /// 
    /// gRPC 消息格式：[压缩标志(1字节)][长度(4字节)][数据]
    fn build_grpc_message(&self, data: &[u8]) -> Vec<u8> {
        let mut message = Vec::with_capacity(5 + data.len());
        
        // 压缩标志（0 = 不压缩）
        message.push(0);
        
        // 消息长度（大端序）
        let length = data.len() as u32;
        let length_bytes = length.to_be_bytes();
        message.extend_from_slice(&length_bytes);
        
        // 消息数据
        message.extend_from_slice(data);
        

        
        message
    }

    /// 解析标准 gRPC 消息格式
    /// 
    /// 从 gRPC 消息格式中提取实际数据：[压缩标志(1字节)][长度(4字节)][数据]
    fn parse_grpc_message(&self, data: &[u8]) -> RatResult<Vec<u8>> {
        if data.len() < 5 {
            return Err(RatError::DecodingError("gRPC 消息太短".to_string()));
        }
        
        let compressed = data[0] != 0;
        let length = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
        
        // 添加详细的调试日志
        eprintln!("=== DEBUG: [客户端] 解析 gRPC 消息: 总长度={} bytes, 压缩标志={}, 声明长度={} bytes ===", 
                 data.len(), compressed, length);
        eprintln!("=== DEBUG: [客户端] 消息头部字节: {:?} ===", &data[..std::cmp::min(10, data.len())]);
        println!("DEBUG: [客户端] 解析 gRPC 消息: 总长度={} bytes, 压缩标志={}, 声明长度={} bytes", 
                 data.len(), compressed, length);
        println!("DEBUG: [客户端] 消息头部字节: {:?}", &data[..std::cmp::min(10, data.len())]);
        info!("🔍 [客户端] 解析 gRPC 消息: 总长度={} bytes, 压缩标志={}, 声明长度={} bytes", 
                         data.len(), compressed, length);
        info!("🔍 [客户端] 消息头部字节: {:?}", &data[..std::cmp::min(10, data.len())]);
        
        // 添加合理的长度限制，防止容量溢出（最大 100MB）
        const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
        if length > MAX_MESSAGE_SIZE {
            error!("❌ [客户端] gRPC 消息长度异常: {} 字节 > {} 字节", length, MAX_MESSAGE_SIZE);
            return Err(RatError::DecodingError(format!(
                "gRPC 消息长度过大: {} 字节，最大允许: {} 字节", 
                length, MAX_MESSAGE_SIZE
            )));
        }
        
        if data.len() < 5 + length {
            error!("❌ [客户端] gRPC 消息长度不匹配: 期望 {} 字节，实际 {} 字节", 5 + length, data.len());
            return Err(RatError::DecodingError(format!(
                "gRPC 消息长度不匹配: 期望 {} 字节，实际 {} 字节", 
                5 + length, data.len()
            )));
        }
        
        if compressed {
            return Err(RatError::DecodingError("不支持压缩的 gRPC 消息".to_string()));
        }
        
        info!("✅ [客户端] gRPC 消息解析成功，提取数据长度: {} bytes", length);
        Ok(data[5..5 + length].to_vec())
    }

    /// 压缩数据
    fn compress_data(&self, data: Bytes) -> RatResult<(Bytes, Option<&'static str>)> {
        match self.compression_mode {
            GrpcCompressionMode::Disabled => Ok((data, None)),
            GrpcCompressionMode::Lz4 => {
                #[cfg(feature = "compression")]
                {
                    let compressed = lz4_flex::block::compress(&data);
                    Ok((Bytes::from(compressed), Some("lz4")))
                }
                #[cfg(not(feature = "compression"))]
                {
                    Err(RatError::Other("LZ4 压缩功能未启用".to_string()))
                }
            }
        }
    }

    /// 解压缩数据
    fn decompress_data(&self, data: Bytes, encoding: Option<&HeaderValue>) -> RatResult<Bytes> {
        let encoding = match encoding {
            Some(value) => match value.to_str() {
                Ok(s) => s,
                Err(_) => return Ok(data), // 无法解析编码，返回原始数据
            },
            None => return Ok(data), // 没有编码头，返回原始数据
        };

        match encoding.to_lowercase().as_str() {
            "lz4" => {
                #[cfg(feature = "compression")]
                {
                    let decompressed = lz4_flex::block::decompress(&data, data.len() * 4)
                        .map_err(|e| RatError::DecodingError(format!("LZ4 解压缩失败: {}", e)))?;
                    Ok(Bytes::from(decompressed))
                }
                #[cfg(not(feature = "compression"))]
                {
                    Err(RatError::DecodingError("LZ4 压缩功能未启用".to_string()))
                }
            },
            "identity" | "" => Ok(data),
            _ => Ok(data), // 未知编码，返回原始数据
        }
    }

    /// 发送 gRPC 请求 - 统一使用 h2 依赖
    /// 
    /// gRPC 本身就不支持 HTTP/1.1，所以统一使用 h2 crate 处理 HTTP/2 和 H2C
    /// 直接返回响应数据，不再考虑 Hyper 兼容性
    async fn send_request(&self, request: Request<Full<Bytes>>) -> RatResult<(StatusCode, HeaderMap, Bytes)> {
        // gRPC 统一使用 h2 依赖，根据 URI scheme 决定是否使用 TLS
        let response = self.send_h2_request(request).await?;
        
        // 直接提取响应数据
        let (parts, body) = response.into_parts();
        let body_bytes = body.collect().await
            .map_err(|e| RatError::NetworkError(format!("读取响应体失败: {}", e)))?
            .to_bytes();
        
        Ok((parts.status, parts.headers, body_bytes))
    }

    /// 创建 TLS 配置（支持开发模式）
    fn create_tls_config(&self) -> RatResult<rustls::ClientConfig> {
        use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
        use rustls::{pki_types, Error as RustlsError};
        
        // 检查是否有 mTLS 配置
        if let Some(mtls_config) = &self.mtls_config {
            info!("🔐 启用 mTLS 客户端证书认证");
            
            // 构建根证书存储
            let mut root_store = rustls::RootCertStore::empty();
            
            if let Some(ca_certs) = &mtls_config.ca_certs {
                // 使用自定义 CA 证书
                for ca_cert in ca_certs {
                    root_store.add(ca_cert.clone())
                        .map_err(|e| RatError::TlsError(format!("添加 CA 证书失败: {}", e)))?;
                }
                info!("✅ 已加载 {} 个自定义 CA 证书", ca_certs.len());
            } else {
                // 使用系统默认根证书
                root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                info!("✅ 已加载系统默认根证书");
            }
            
            // 创建客户端证书链
            let client_cert_chain = mtls_config.client_cert_chain.clone();
            let client_private_key = mtls_config.client_private_key.clone_key();
            
            let mut tls_config = if mtls_config.skip_server_verification {
                // 跳过服务器证书验证（仅用于测试）
                warn!("⚠️  警告：已启用跳过服务器证书验证模式！仅用于测试环境！");
                
                #[derive(Debug)]
                struct DangerousClientCertVerifier;
                
                impl ServerCertVerifier for DangerousClientCertVerifier {
                    fn verify_server_cert(
                        &self,
                        _end_entity: &pki_types::CertificateDer<'_>,
                        _intermediates: &[pki_types::CertificateDer<'_>],
                        _server_name: &pki_types::ServerName<'_>,
                        _ocsp_response: &[u8],
                        _now: pki_types::UnixTime,
                    ) -> Result<ServerCertVerified, RustlsError> {
                        Ok(ServerCertVerified::assertion())
                    }
                    
                    fn verify_tls12_signature(
                        &self,
                        _message: &[u8],
                        _cert: &pki_types::CertificateDer<'_>,
                        _dss: &rustls::DigitallySignedStruct,
                    ) -> Result<HandshakeSignatureValid, RustlsError> {
                        Ok(HandshakeSignatureValid::assertion())
                    }
                    
                    fn verify_tls13_signature(
                        &self,
                        _message: &[u8],
                        _cert: &pki_types::CertificateDer<'_>,
                        _dss: &rustls::DigitallySignedStruct,
                    ) -> Result<HandshakeSignatureValid, RustlsError> {
                        Ok(HandshakeSignatureValid::assertion())
                    }
                    
                    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                        vec![
                            rustls::SignatureScheme::RSA_PKCS1_SHA1,
                            rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                            rustls::SignatureScheme::RSA_PKCS1_SHA256,
                            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                            rustls::SignatureScheme::RSA_PKCS1_SHA384,
                            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                            rustls::SignatureScheme::RSA_PKCS1_SHA512,
                            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                            rustls::SignatureScheme::RSA_PSS_SHA256,
                            rustls::SignatureScheme::RSA_PSS_SHA384,
                            rustls::SignatureScheme::RSA_PSS_SHA512,
                            rustls::SignatureScheme::ED25519,
                            rustls::SignatureScheme::ED448,
                        ]
                    }
                }
                
                rustls::ClientConfig::builder()
                    .dangerous()
                    .with_custom_certificate_verifier(std::sync::Arc::new(DangerousClientCertVerifier))
                    .with_client_auth_cert(client_cert_chain, client_private_key)
                    .map_err(|e| RatError::TlsError(format!("配置客户端证书失败: {}", e)))?
            } else {
                // 正常的服务器证书验证
                rustls::ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_client_auth_cert(client_cert_chain, client_private_key)
                    .map_err(|e| RatError::TlsError(format!("配置客户端证书失败: {}", e)))?
            };
            
            // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
            tls_config.alpn_protocols = vec![b"h2".to_vec()];
            
            info!("✅ mTLS 客户端配置完成");
            Ok(tls_config)
        } else if self.development_mode {
            // 开发模式：跳过证书验证
            warn!("⚠️  警告：gRPC 客户端已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");
            
            #[derive(Debug)]
            struct DangerousClientCertVerifier;
            
            impl ServerCertVerifier for DangerousClientCertVerifier {
                fn verify_server_cert(
                    &self,
                    _end_entity: &pki_types::CertificateDer<'_>,
                    _intermediates: &[pki_types::CertificateDer<'_>],
                    _server_name: &pki_types::ServerName<'_>,
                    _ocsp_response: &[u8],
                    _now: pki_types::UnixTime,
                ) -> Result<ServerCertVerified, RustlsError> {
                    Ok(ServerCertVerified::assertion())
                }
                
                fn verify_tls12_signature(
                    &self,
                    _message: &[u8],
                    _cert: &pki_types::CertificateDer<'_>,
                    _dss: &rustls::DigitallySignedStruct,
                ) -> Result<HandshakeSignatureValid, RustlsError> {
                    Ok(HandshakeSignatureValid::assertion())
                }
                
                fn verify_tls13_signature(
                    &self,
                    _message: &[u8],
                    _cert: &pki_types::CertificateDer<'_>,
                    _dss: &rustls::DigitallySignedStruct,
                ) -> Result<HandshakeSignatureValid, RustlsError> {
                    Ok(HandshakeSignatureValid::assertion())
                }
                
                fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                    vec![
                        rustls::SignatureScheme::RSA_PKCS1_SHA1,
                        rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                        rustls::SignatureScheme::RSA_PKCS1_SHA256,
                        rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                        rustls::SignatureScheme::RSA_PKCS1_SHA384,
                        rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                        rustls::SignatureScheme::RSA_PKCS1_SHA512,
                        rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                        rustls::SignatureScheme::RSA_PSS_SHA256,
                        rustls::SignatureScheme::RSA_PSS_SHA384,
                        rustls::SignatureScheme::RSA_PSS_SHA512,
                        rustls::SignatureScheme::ED25519,
                        rustls::SignatureScheme::ED448,
                    ]
                }
            }
            
            let mut tls_config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(std::sync::Arc::new(DangerousClientCertVerifier))
                .with_no_client_auth();
            
            // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
            tls_config.alpn_protocols = vec![b"h2".to_vec()];
            
            Ok(tls_config)
        } else {
            // 非开发模式：严格证书验证
            let mut root_store = rustls::RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            
            let mut tls_config = rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();
            
            // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
            tls_config.alpn_protocols = vec![b"h2".to_vec()];
            
            Ok(tls_config)
        }
    }

    /// 建立 H2 连接的统一方法
    /// 
    /// 封装了 TCP 连接、TLS 握手（HTTPS）和 H2 握手的完整流程
    async fn establish_h2_connection(&self, uri: &Uri) -> RatResult<h2::client::SendRequest<bytes::Bytes>> {
        let is_https = uri.scheme_str() == Some("https");
        let host = uri.host().ok_or_else(|| RatError::RequestError("URI 缺少主机".to_string()))?;
        let port = uri.port_u16().unwrap_or(if is_https { 443 } else { 80 });
        let addr = format!("{}:{}", host, port);
        
        debug!("🔗 建立 H2 连接: {} ({})", addr, if is_https { "HTTPS" } else { "H2C" });
        
        // 建立 TCP 连接
        let tcp_stream = timeout(self.connect_timeout, tokio::net::TcpStream::connect(&addr))
            .await
            .map_err(|_| RatError::TimeoutError(format!("H2 TCP 连接超时: {}", addr)))?
            .map_err(|e| RatError::NetworkError(format!("H2 TCP 连接失败: {}", e)))?;
        
        debug!("✅ H2 TCP 连接已建立: {}", addr);
        
        // 根据协议类型进行握手
        let client = if is_https {
            // HTTPS: 先进行 TLS 握手，再进行 H2 握手
            use rustls::pki_types::ServerName;
            
            let tls_config = self.create_tls_config()?;
            let tls_connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(tls_config));
            
            let server_name = ServerName::try_from(host.to_string())
                .map_err(|e| RatError::RequestError(format!("无效的服务器名称 '{}': {}", host, e)))?;
            
            let tls_stream = tls_connector.connect(server_name, tcp_stream).await
                .map_err(|e| RatError::NetworkError(format!("TLS 连接失败: {}", e)))?;
            
            debug!("🔐 TLS 连接建立成功，开始 HTTP/2 握手");
            
            let (client, h2_connection) = h2::client::handshake(tls_stream)
                .await
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 over TLS 握手失败: {}", e)))?;
            
            // 在后台运行 H2 连接
            tokio::spawn(async move {
                if let Err(e) = h2_connection.await {
                    error!("❌ H2 连接错误: {}", e);
                }
            });
            
            client
        } else {
            // H2C: 直接进行 H2 握手
            let (client, h2_connection) = h2::client::handshake(tcp_stream)
                .await
                .map_err(|e| RatError::NetworkError(format!("H2C 握手失败: {}", e)))?;
            
            // 在后台运行 H2 连接
            tokio::spawn(async move {
                if let Err(e) = h2_connection.await {
                    error!("❌ H2 连接错误: {}", e);
                }
            });
            
            client
        };
        
        debug!("🚀 H2 连接建立完成: {}", addr);
        Ok(client)
    }

    /// 发送 H2 请求（一元调用版本 - 读取完整响应体）
    async fn send_h2_request(&self, request: Request<Full<Bytes>>) -> RatResult<Response<Full<Bytes>>> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        debug!("🔗 使用 H2 发送 gRPC 请求: {} {}", method, uri);
        
        // 建立 H2 连接
        let client = self.establish_h2_connection(&uri).await?;
        
        // 发送请求并获取响应
        let h2_response = self.send_h2_request_internal(client, request).await?;
        
        debug!("📥 收到 H2 响应: {} {} - 状态码: {}", method, uri, h2_response.status());
        
        // 提取状态码和头部信息
        let status = h2_response.status();
        let headers = h2_response.headers().clone();
        
        // 读取响应体
        let mut body_stream = h2_response.into_body();
        let mut body_data = Vec::new();
        
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.map_err(|e| RatError::NetworkError(format!("H2 读取响应体失败: {}", e)))?;
            body_data.extend_from_slice(&chunk);
            // 释放流控制窗口
            let _ = body_stream.flow_control().release_capacity(chunk.len());
        }
        
        // 构建 Hyper 兼容的响应
        let mut response_builder = Response::builder()
            .status(status);
        
        // 复制响应头
        for (name, value) in &headers {
            response_builder = response_builder.header(name, value);
        }
        
        // 创建响应体
        let body = http_body_util::Full::new(Bytes::from(body_data));
        
        // 构建最终响应
        let response = response_builder
            .body(body)
            .map_err(|e| RatError::NetworkError(format!("构建响应失败: {}", e)))?;
        
        Ok(response)
    }

    /// 内部方法：发送 H2 请求的通用逻辑
    async fn send_h2_request_internal(&self, mut client: h2::client::SendRequest<bytes::Bytes>, request: Request<Full<Bytes>>) -> RatResult<hyper::Response<h2::RecvStream>> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        // 构建 H2 请求
        let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
        let mut h2_request = hyper::Request::builder()
            .method(method.clone())
            .uri(path);
        
        // 复制头部
        for (name, value) in request.headers() {
            h2_request = h2_request.header(name, value);
        }
        
        let h2_request = h2_request
            .body(())
            .map_err(|e| RatError::RequestError(format!("构建 H2 请求失败: {}", e)))?;
        
        // 发送请求
        let (response, mut send_stream) = client
            .send_request(h2_request, false)
            .map_err(|e| RatError::NetworkError(format!("H2 发送请求失败: {}", e)))?;
        
        // 发送请求体
        let body_bytes = request.into_body().collect().await
            .map_err(|e| RatError::NetworkError(format!("读取请求体失败: {}", e)))?
            .to_bytes();
        
        if !body_bytes.is_empty() {
            send_stream.send_data(body_bytes, true)
                .map_err(|e| RatError::NetworkError(format!("H2 发送数据失败: {}", e)))?;
        } else {
            send_stream.send_data(Bytes::new(), true)
                .map_err(|e| RatError::NetworkError(format!("H2 发送空数据失败: {}", e)))?;
        }
        
        // 等待响应
        let h2_response = timeout(self.request_timeout, response)
            .await
            .map_err(|_| RatError::TimeoutError(format!("H2 响应超时: {} {}", method, uri)))?
            .map_err(|e| RatError::NetworkError(format!("H2 接收响应失败: {}", e)))?;
        
        Ok(h2_response)
    }

    /// 发送 H2 请求（流调用版本 - 返回流响应）
    async fn send_h2_request_stream(&self, request: Request<Full<Bytes>>) -> RatResult<Response<h2::RecvStream>> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        debug!("🔗 使用 H2 发送 gRPC 流请求: {} {}", method, uri);
        
        // 建立 H2 连接
        let client = self.establish_h2_connection(&uri).await?;
        
        // 发送请求并获取响应
        let h2_response = self.send_h2_request_internal(client, request).await?;
        
        debug!("📥 收到 H2 流响应: {} {} - 状态码: {}", method, uri, h2_response.status());
        
        // 对于流请求，错误状态在 trailers 中处理，不在初始响应头中
        // 直接返回流响应，不读取响应体
        let (parts, body_stream) = h2_response.into_parts();
        let response = Response::from_parts(parts, body_stream);
        
        Ok(response)
    }

    /// 解析 gRPC 响应
    fn parse_grpc_response<R>(&self, status: StatusCode, headers: HeaderMap, body_bytes: Bytes) -> RatResult<GrpcResponse<R>>
    where
        R: for<'de> Deserialize<'de> + bincode::Decode<()>,
    {
        // 检查 HTTP 状态码
        if !status.is_success() {
            return Err(RatError::NetworkError(format!("gRPC HTTP 错误: {}", status)));
        }

        // 检查 Content-Type
        if let Some(content_type) = headers.get(CONTENT_TYPE) {
            if !content_type.to_str().unwrap_or("").starts_with("application/grpc") {
                return Err(RatError::DecodingError("无效的 gRPC Content-Type".to_string()));
            }
        }

        // 从响应头中提取 gRPC 状态和消息
        let grpc_status = headers
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0); // 默认为成功状态

        let grpc_message = headers
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // 提取元数据（所有非标准 gRPC 头部）
        let mut metadata = std::collections::HashMap::new();
        for (name, value) in &headers {
            let name_str = name.as_str();
            // 跳过标准 HTTP 和 gRPC 头部
            if !name_str.starts_with(":")
                && name_str != "content-type"
                && name_str != "grpc-status"
                && name_str != "grpc-message"
                && name_str != "grpc-encoding"
                && name_str != "user-agent"
            {
                if let Ok(value_str) = value.to_str() {
                    metadata.insert(name_str.to_string(), value_str.to_string());
                }
            }
        }

        // 使用统一的编解码器解析帧并反序列化
        let message_data = GrpcCodec::parse_frame(&body_bytes)
            .map_err(|e| RatError::DecodingError(format!("解析 gRPC 帧失败: {}", e)))?;

        // 添加反序列化前的调试信息
        eprintln!("=== DEBUG: [客户端] 准备反序列化响应数据，数据大小: {} bytes ===", message_data.len());
        eprintln!("=== DEBUG: [客户端] 反序列化数据前32字节: {:?} ===", &message_data[..std::cmp::min(32, message_data.len())]);
        println!("DEBUG: [客户端] 准备反序列化响应数据，数据大小: {} bytes", message_data.len());
        println!("DEBUG: [客户端] 反序列化数据前32字节: {:?}", &message_data[..std::cmp::min(32, message_data.len())]);
        info!("🔍 [客户端] 准备反序列化响应数据，数据大小: {} bytes", message_data.len());
        info!("🔍 [客户端] 反序列化数据前32字节: {:?}", &message_data[..std::cmp::min(32, message_data.len())]);

        eprintln!("=== DEBUG: [客户端] 开始使用 GrpcCodec 反序列化 ===");
        // 直接反序列化为最终的 R 类型，因为服务端现在发送完整的 GrpcResponse 结构
        let response_data: R = GrpcCodec::decode(message_data)
            .map_err(|e| {
                eprintln!("=== DEBUG: [客户端] GrpcCodec 反序列化最终数据类型失败: {} ===", e);
                println!("DEBUG: [客户端] GrpcCodec 反序列化最终数据类型失败: {}", e);
                error!("❌ [客户端] GrpcCodec 反序列化最终数据类型失败: {}", e);
                RatError::DeserializationError(format!("反序列化最终数据类型失败: {}", e))
            })?;
        eprintln!("=== DEBUG: [客户端] 最终数据类型反序列化成功 ===");

        // 构建默认的 GrpcResponse 结构，因为我们只收到了实际数据
        let grpc_response = GrpcResponse {
            status: 0, // OK
            message: "Success".to_string(),
            data: response_data,
            metadata: std::collections::HashMap::new(),
        };

        Ok(grpc_response)
    }

    /// 获取下一个请求 ID
    pub fn next_request_id(&self) -> u64 {
        self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// 获取压缩模式
    pub fn compression_mode(&self) -> GrpcCompressionMode {
        self.compression_mode
    }

    /// 获取基础 URI


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
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;
        
        // 1. 从连接池获取连接
        let connection = self.connection_pool.get_connection(&parsed_uri).await
            .map_err(|e| RatError::NetworkError(format!("获取连接失败: {}", e)))?;
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
            .map_err(|e| RatError::RequestError(format!("构建双向流请求失败: {}", e)))?;

        // 发送请求并获取响应流
        let (response, send_stream) = send_request.send_request(request, false)
            .map_err(|e| RatError::NetworkError(format!("发送双向流请求失败: {}", e)))?;

        // 等待响应头
        let response = response.await
            .map_err(|e| RatError::NetworkError(format!("接收双向流响应失败: {}", e)))?;

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
                                                    let error_msg = format!("❌ [委托模式] 反序列化实际消息失败: {}", e);
                                                    error!("{}", error_msg);
                                                    handler_clone.on_error(&context_clone, error_msg).await;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let error_msg = format!("❌ [委托模式] GrpcStreamMessage 反序列化失败: {}", e);
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
                            let error_msg = format!("接收数据失败: {}", e);
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

    /// 获取委托模式流的上下文
    /// 
    /// # 参数
    /// * `stream_id` - 流ID
    /// 
    /// # 返回
    /// 返回流上下文，用户可以通过此上下文发送消息
    pub async fn get_stream_context(&self, stream_id: u64) -> Option<ClientStreamContext> {
        self.delegated_manager.get_stream_context(stream_id).await
    }

    /// 关闭委托模式的双向流连接
    /// 
    /// # 参数
    /// * `stream_id` - 流ID
    pub async fn close_bidirectional_stream_delegated(&self, stream_id: u64) -> RatResult<()> {
        info!("🛑 开始关闭委托模式双向流: {}", stream_id);
        
        // 从委托管理器中关闭流，这会自动处理所有任务的取消和资源清理
        self.delegated_manager.close_stream(stream_id).await;
        
        info!("✅ 委托模式双向流 {} 已成功关闭", stream_id);
        Ok(())
    }

    /// 使用委托模式发送一元 gRPC 请求
    /// 
    /// 采用类似双向流的委托架构，让连接池统一管理一元请求连接
    /// 用户只需要实现处理器接口，不需要直接管理连接和响应处理
    /// 
    /// # 参数
    /// * `uri` - 服务器 URI
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据（强类型）
    /// * `handler` - 一元请求处理器
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回请求ID，用于后续管理
    /// 
    /// # 示例
    /// ```rust
    /// let request_id = client.call_unary_delegated_with_uri(
    ///     "http://127.0.0.1:50051",
    ///     "user.UserService",
    ///     "GetUser", 
    ///     user_request,
    ///     Arc::new(UserHandler::new()),
    ///     None
    /// ).await?;
    /// ```
    #[cfg(feature = "python")]
    pub async fn call_unary_delegated_with_uri<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: crate::python_api::client::GrpcUnaryHandler<ResponseData = Vec<u8>> + 'static,
    {
        self.call_unary_delegated_with_uri_impl(uri, service, method, request_data, handler, metadata).await
    }

    #[cfg(not(feature = "python"))]
    pub async fn call_unary_delegated_with_uri<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: Send + Sync + 'static,
    {
        self.call_unary_delegated_with_uri_impl(uri, service, method, request_data, handler, metadata).await
    }

    // Python 特性启用时的实现
    #[cfg(feature = "python")]
    async fn call_unary_delegated_with_uri_impl<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: crate::python_api::client::GrpcUnaryHandler<ResponseData = Vec<u8>> + 'static,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        info!("🔗 创建委托模式一元请求: {}/{}, 请求ID: {}", service, method, request_id);
        
        // 解析 URI
        let parsed_uri = uri.parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;
        
        // 1. 从连接池获取连接
        let connection = self.connection_pool.get_connection(&parsed_uri).await
            .map_err(|e| RatError::NetworkError(format!("获取连接失败: {}", e)))?;

        // 2. 直接使用原始请求数据（避免双重序列化）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: request_data, // 直接使用原始数据，不进行额外序列化
            metadata: metadata.unwrap_or_default(),
        };

        // 3. 编码 gRPC 消息
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(format!("编码 gRPC 请求失败: {}", e)))?;

        // 4. 构建 HTTP 请求
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", uri.trim_end_matches('/'), path);
        
        let request_uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的请求 URI: {}", e)))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(format!("无效的 User-Agent: {}", e)))?);

        // 5. 创建请求上下文（仅在启用 python 特性时）
        #[cfg(feature = "python")]
        let context = crate::python_api::client::GrpcUnaryContext::new(
            request_id,
            service.to_string(),
            method.to_string(),
            uri.to_string(),
            grpc_request.metadata.clone(),
        );

        // 6. 启动异步请求处理任务
        let handler_clone = handler.clone();
        let client_clone = self.clone();
        let connection_id = connection.connection_id.clone();
        
        tokio::spawn(async move {
            // 通知处理器请求开始
            #[cfg(feature = "python")]
            {
                if let Err(e) = handler_clone.on_request_start(&context).await {
                    error!("❌ 一元请求处理器启动失败 (请求ID: {}): {}", request_id, e);
                    let _ = handler_clone.on_error(&context, e).await;
                    return;
                }
            }

            // 发送 HTTP 请求
            let mut request_builder = Request::builder()
                .method(Method::POST)
                .uri(request_uri);
            
            // 添加 headers
            for (key, value) in headers.iter() {
                request_builder = request_builder.header(key, value);
            }
            
            let request = request_builder
                .body(Full::new(Bytes::from(grpc_message)))
                .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)));

            let request = match request {
                Ok(req) => req,
                Err(e) => {
                    error!("❌ 构建一元请求失败 (请求ID: {}): {}", request_id, e);
                    #[cfg(feature = "python")]
                    {
                        let _ = handler_clone.on_error(&context, e.to_string()).await;
                    }
                    return;
                }
            };

            // 发送请求并处理响应
            match client_clone.send_request(request).await {
                Ok((status, headers, body)) => {
                    if status.is_success() {
                        // 解析 gRPC 响应
                        match client_clone.parse_grpc_message(&body) {
                            Ok(response_data) => {
                                let grpc_response = response_data;
                                
                                // 通知处理器响应接收和完成
                                #[cfg(feature = "python")]
                                {
                                    if let Err(e) = handler_clone.on_response_received(grpc_response, &context).await {
                                        error!("❌ 一元请求响应处理失败 (请求ID: {}): {}", request_id, e);
                                        let _ = handler_clone.on_error(&context, e.to_string()).await;
                                        return;
                                    }
                                    
                                    // 通知处理器请求完成
                                    let _ = handler_clone.on_completed(&context).await;
                                }
                            }
                            Err(e) => {
                                error!("❌ 解析一元响应失败 (请求ID: {}): {}", request_id, e);
                                #[cfg(feature = "python")]
                                {
                                    let _ = handler_clone.on_error(&context, e.to_string()).await;
                                }
                            }
                        }
                    } else {
                        let error = RatError::NetworkError(format!("HTTP 错误: {}", status));
                        error!("❌ 一元请求 HTTP 错误 (请求ID: {}): {}", request_id, error);
                        #[cfg(feature = "python")]
                        {
                            let _ = handler_clone.on_error(&context, error.to_string()).await;
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 发送一元请求失败 (请求ID: {}): {}", request_id, e);
                    #[cfg(feature = "python")]
                    {
                        let _ = handler_clone.on_error(&context, e.to_string()).await;
                    }
                }
            }
        });

        info!("✅ 委托模式一元请求 {} 已启动", request_id);
        Ok(request_id)
    }

    // Python 特性未启用时的简化实现
    #[cfg(not(feature = "python"))]
    async fn call_unary_delegated_with_uri_impl<T, H>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        _handler: Arc<H>,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<u64>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        H: Send + Sync + 'static,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        info!("🔗 创建委托模式一元请求: {}/{}, 请求ID: {}", service, method, request_id);
        
        // 解析 URI
        let parsed_uri = uri.parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;
        
        // 1. 从连接池获取连接
        let connection = self.connection_pool.get_connection(&parsed_uri).await
            .map_err(|e| RatError::NetworkError(format!("获取连接失败: {}", e)))?;

        // 2. 直接使用原始请求数据（避免双重序列化）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: request_data, // 直接使用原始数据，不进行额外序列化
            metadata: metadata.unwrap_or_default(),
        };

        // 3. 编码 gRPC 消息
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(format!("编码 gRPC 请求失败: {}", e)))?;

        // 4. 构建 HTTP 请求
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", uri.trim_end_matches('/'), path);
        
        let request_uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的请求 URI: {}", e)))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(format!("无效的 User-Agent: {}", e)))?);        

        // 5. 启动异步请求处理任务（简化版本，无 handler 回调）
        let client_clone = self.clone();
        let connection_id = connection.connection_id.clone();
        
        tokio::spawn(async move {
            // 发送 HTTP 请求
            let mut request_builder = Request::builder()
                .method(Method::POST)
                .uri(request_uri);
            
            // 添加 headers
            for (key, value) in headers.iter() {
                request_builder = request_builder.header(key, value);
            }
            
            let request = request_builder
                .body(Full::new(Bytes::from(grpc_message)))
                .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)));

            let request = match request {
                Ok(req) => req,
                Err(e) => {
                    error!("❌ 构建一元请求失败 (请求ID: {}): {}", request_id, e);
                    return;
                }
            };

            // 发送请求并处理响应
            match client_clone.send_request(request).await {
                Ok((status, _headers, body)) => {
                    if status.is_success() {
                        // 解析 gRPC 响应
                        match client_clone.parse_grpc_message(&body) {
                            Ok(_response_data) => {
                                info!("✅ 一元请求成功完成 (请求ID: {})", request_id);
                            }
                            Err(e) => {
                                error!("❌ 解析一元响应失败 (请求ID: {}): {}", request_id, e);
                            }
                        }
                    } else {
                        let error = RatError::NetworkError(format!("HTTP 错误: {}", status));
                        error!("❌ 一元请求 HTTP 错误 (请求ID: {}): {}", request_id, error);
                    }
                }
                Err(e) => {
                    error!("❌ 发送一元请求失败 (请求ID: {}): {}", request_id, e);
                }
            }
        });

        info!("✅ 委托模式一元请求 {} 已启动", request_id);
        Ok(request_id)
    }

    /// 创建客户端流连接（统一化版本，用于分块上传等场景）
    #[deprecated(note = "请使用 call_client_stream_with_uri 方法")]
    pub async fn call_client_stream<S, R>(
        &self, 
        service: &str, 
        method: &str, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<(GrpcStreamSender<S>, tokio::sync::oneshot::Receiver<RatResult<R>>)>
    where
        S: Serialize + Send + Sync + 'static + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        self.call_client_stream_with_uri("http://localhost", service, method, metadata).await
    }

    /// 创建客户端流连接（统一化版本，用于分块上传等场景）
    /// 
    /// 复用双向流的底层机制，但只使用发送端，适合大文件分块上传
    /// 使用 GrpcCodec 统一编码解码器，确保与服务端的一致性
    /// 
    /// # 参数
    /// * `uri` - 服务器 URI
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回客户端流发送端和强类型响应数据的接收器
    pub async fn call_client_stream_with_uri<S, R>(
        &self, 
        uri: &str,
        service: &str, 
        method: &str, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<(GrpcStreamSender<S>, tokio::sync::oneshot::Receiver<RatResult<R>>)>
    where
        S: Serialize + Send + Sync + 'static + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        let base_uri: Uri = uri.parse()
            .map_err(|e| RatError::ConfigError(format!("无效的 URI: {}", e)))?;
        
        // 从连接池获取连接
        let connection = self.connection_pool.get_connection(&base_uri).await
            .map_err(|e| RatError::NetworkError(format!("获取连接失败: {}", e)))?;
        let mut send_request = connection.send_request.clone();

        // 构建请求路径
        let path = format!("/{}/{}", service, method);

        // 创建客户端流请求（复用双向流的请求构建方式）
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(CONTENT_TYPE, "application/grpc")
            .header("grpc-stream-type", "client-stream")
            .header(USER_AGENT, &self.user_agent)
            .body(())
            .map_err(|e| RatError::RequestError(format!("构建客户端流请求失败: {}", e)))?;

        // 发送请求并获取响应流（复用双向流的发送方式）
        let (response, send_stream) = send_request.send_request(request, false)
            .map_err(|e| RatError::NetworkError(format!("发送客户端流请求失败: {}", e)))?;

        // 等待响应头
        let response = response.await
            .map_err(|e| RatError::NetworkError(format!("接收客户端流响应失败: {}", e)))?;

        let receive_stream = response.into_body();

        // 创建发送通道
        let (send_tx, send_rx) = mpsc::unbounded_channel::<Bytes>();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        // 启动发送任务（复用双向流的发送逻辑）
        let connection_id = connection.connection_id.clone();
        let connection_pool = self.connection_pool.clone();
        let send_task = {
            let mut send_stream = send_stream;
            tokio::spawn(async move {
                let mut send_rx = send_rx;
                let mut message_sent = false;
                
                while let Some(data) = send_rx.recv().await {
                    message_sent = true;
                    
                    // 构建 gRPC 消息帧
                    let frame = GrpcCodec::create_frame(&data);
                    
                    if let Err(e) = send_stream.send_data(Bytes::from(frame), false) {
                        error!("客户端流发送数据失败: {}", e);
                        break;
                    }
                }
                
                // 注意：结束信号已经通过 send_close() 方法发送，这里不需要重复发送
                // 只需要关闭底层的 H2 流
                if message_sent {
                    if let Err(e) = send_stream.send_data(Bytes::new(), true) {
                        if e.to_string().contains("inactive stream") {
                            info!("ℹ️ [客户端流] 流已关闭，H2 结束信号发送被忽略");
                        } else {
                            error!("❌ [客户端流] 发送 H2 结束信号失败: {}", e);
                        }
                    } else {
                        info!("✅ [客户端流] H2 流已结束");
                    }
                }
                
                // 释放连接回连接池
                connection_pool.release_connection(&connection_id);
                info!("客户端流发送完成，连接已释放");
            })
        };

        // 启动响应接收任务（统一化版本，使用GrpcCodec解码）
        let recv_task = {
            let mut receive_stream = receive_stream;
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                
                // 接收响应数据
                while let Some(chunk_result) = receive_stream.data().await {
                    match chunk_result {
                        Ok(chunk) => buffer.extend_from_slice(&chunk),
                        Err(e) => {
                            let _ = response_tx.send(Err(RatError::NetworkError(format!("接收响应数据失败: {}", e))));
                            return;
                        }
                    }
                }
                
                // 使用 GrpcCodec 统一解码响应数据
                if buffer.is_empty() {
                    let _ = response_tx.send(Err(RatError::NetworkError("接收到空响应".to_string())));
                    return;
                }
                
                // 解析 gRPC 响应帧
                match GrpcCodec::decode_frame::<GrpcResponse<Vec<u8>>>(&buffer) {
                    Ok(grpc_response) => {
                        // 解码业务数据
                        match GrpcCodec::decode::<R>(&grpc_response.data) {
                            Ok(response_data) => {
                                let _ = response_tx.send(Ok(response_data));
                            }
                            Err(e) => {
                                let _ = response_tx.send(Err(RatError::SerializationError(format!("解码响应数据失败: {}", e))));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = response_tx.send(Err(RatError::SerializationError(format!("解码 gRPC 响应帧失败: {}", e))));
                    }
                }
            })
        };

        Ok((GrpcStreamSender::new(send_tx), response_rx))
    }

    /// 发送服务端流 gRPC 请求
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回服务端流响应
    /// 
    /// # 弃用警告
    /// 此方法已弃用，请使用 `call_server_stream_with_uri` 方法
    #[deprecated(note = "请使用 call_server_stream_with_uri 方法")]
    pub async fn call_server_stream<T, R>(
        &self, 
        service: &str, 
        method: &str, 
        request_data: T, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<GrpcStreamResponse<R>>
    where
        T: Serialize + Send + Sync + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        let stream_id = self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // 统一化处理：先序列化强类型数据为 Vec<u8>，然后包装到 GrpcRequest 中
        // 这样服务端就能接收到 GrpcRequest<Vec<u8>> 格式的数据，与 call_typed 保持一致
        let serialized_data = GrpcCodec::encode(&request_data)
            .map_err(|e| RatError::SerializationError(format!("序列化请求数据失败: {}", e)))?;
        
        // 构建 gRPC 请求（使用序列化后的数据）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: serialized_data, // 使用序列化后的 Vec<u8> 数据
            metadata: metadata.unwrap_or_default(),
        };

        // 使用统一的编解码器编码并创建帧
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(format!("编码 gRPC 请求失败: {}", e)))?;

        // 服务端流直接使用 gRPC 消息格式，不进行额外的 HTTP 压缩
        let compressed_data = Bytes::from(grpc_message);
        let content_encoding: Option<&'static str> = None;

        // 构建 HTTP 请求
        // 弃用方法 - 请使用 call_server_stream_with_uri
        let base_uri_str = "https://localhost:8080".trim_end_matches('/').to_string();
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", base_uri_str, path);
        let uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(format!("无效的用户代理: {}", e)))?);
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(self.compression_mode.accept_encoding()));
        headers.insert("grpc-stream-type", HeaderValue::from_static("server-stream"));
        
        if let Some(encoding) = content_encoding {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }

        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Full::new(compressed_data))
            .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)))?;

        // 添加头部
        let (mut parts, body) = request.into_parts();
        parts.headers = headers;
        let request = Request::from_parts(parts, body);

        // 发送 H2 流请求并获取流响应
        let h2_response = self.send_h2_request_stream(request).await?;
        let recv_stream = h2_response.into_body();
        let stream = self.create_server_stream(recv_stream);

        Ok(GrpcStreamResponse {
            stream_id,
            stream,
        })
    }

    /// 调用泛型服务端流 gRPC 方法（支持框架层统一序列化）
    /// 
    /// 类似于 call_typed，但用于服务端流调用
    /// 自动处理请求数据的 GrpcRequest 包装，保持与一元调用的一致性
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据（强类型）
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回服务端流响应（强类型）
    pub async fn call_typed_server_stream<T, R>(
        &self, 
        service: &str, 
        method: &str, 
        request_data: T, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<GrpcStreamResponse<R>>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        R: bincode::Decode<()> + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        // 直接调用原始方法，使用强类型数据，让 call_server_stream 处理 GrpcRequest 包装
        self.call_server_stream::<T, R>(service, method, request_data, metadata).await
    }

    /// 调用服务端流 gRPC 方法（带 URI 参数）
    /// 
    /// 支持自定义服务器地址和协议，避免硬编码
    /// 
    /// # 参数
    /// * `uri` - 服务器 URI (例如: "https://localhost:8080")
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回服务端流响应
    pub async fn call_server_stream_with_uri<T, R>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<GrpcStreamResponse<R>>
    where
        T: Serialize + Send + Sync + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        let stream_id = self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // 统一化处理：先序列化强类型数据为 Vec<u8>，然后包装到 GrpcRequest 中
        // 这样服务端就能接收到 GrpcRequest<Vec<u8>> 格式的数据，与 call_typed 保持一致
        let serialized_data = GrpcCodec::encode(&request_data)
            .map_err(|e| RatError::SerializationError(format!("序列化请求数据失败: {}", e)))?;
        
        // 构建 gRPC 请求（使用序列化后的数据）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: serialized_data, // 使用序列化后的 Vec<u8> 数据
            metadata: metadata.unwrap_or_default(),
        };

        // 使用统一的编解码器编码并创建帧
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(format!("编码 gRPC 请求失败: {}", e)))?;

        // 服务端流直接使用 gRPC 消息格式，不进行额外的 HTTP 压缩
        let compressed_data = Bytes::from(grpc_message);
        let content_encoding: Option<&'static str> = None;

        // 构建 HTTP 请求
        let base_uri_str = uri.trim_end_matches('/').to_string();
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", base_uri_str, path);
        let request_uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e)))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(format!("无效的用户代理: {}", e)))?);
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(self.compression_mode.accept_encoding()));
        headers.insert("grpc-stream-type", HeaderValue::from_static("server-stream"));
        
        if let Some(encoding) = content_encoding {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }

        let request = Request::builder()
            .method(Method::POST)
            .uri(request_uri)
            .body(Full::new(compressed_data))
            .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)))?;

        // 添加头部
        let (mut parts, body) = request.into_parts();
        parts.headers = headers;
        let request = Request::from_parts(parts, body);

        // 发送 H2 流请求并获取流响应
        let h2_response = self.send_h2_request_stream(request).await?;
        let recv_stream = h2_response.into_body();
        let stream = self.create_server_stream(recv_stream);

        Ok(GrpcStreamResponse {
            stream_id,
            stream,
        })
    }

    /// 创建双向流 gRPC 连接（传统模式）
    /// 
    /// 参考成功示例的实现，使用 H2 流和 bincode 序列化
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回双向流连接
    /// 
    /// # 弃用警告
    /// 此方法已弃用，请使用 `call_bidirectional_stream_with_uri` 方法
    #[deprecated(note = "请使用 call_bidirectional_stream_with_uri 方法")]
    pub async fn call_bidirectional_stream<S, R>(
        &self, 
        service: &str, 
        method: &str, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<GrpcBidirectionalStream<S, R>>
    where
        S: Serialize + Send + Sync + 'static + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + Unpin + 'static + bincode::Decode<()>,
    {
        self.call_bidirectional_stream_with_uri("http://localhost", service, method, metadata).await
    }

    /// 返回双向流连接（带 URI 参数）
    pub async fn call_bidirectional_stream_with_uri<S, R>(
        &self,
        uri: &str,
        service: &str, 
        method: &str, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<GrpcBidirectionalStream<S, R>>
    where
        S: Serialize + Send + Sync + 'static + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + Unpin + 'static + bincode::Decode<()>,
    {
        let base_uri: Uri = uri.parse().map_err(|e| RatError::InvalidArgument(format!("无效的 URI: {}", e)))?;
        
        // 从连接池获取连接
        let connection = self.connection_pool.get_connection(&base_uri).await
            .map_err(|e| RatError::NetworkError(format!("获取连接失败: {}", e)))?;
        let mut send_request = connection.send_request.clone();

        // 构建请求路径
        let path = format!("/{}/{}", service, method);

        // 创建双向流请求（参考成功示例）
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "identity")
            .header("te", "trailers")
            .header(USER_AGENT, &self.user_agent)
            .body(())
            .map_err(|e| RatError::RequestError(format!("构建双向流请求失败: {}", e)))?;

        // 发送请求并获取响应流
        let (response, send_stream) = send_request.send_request(request, false)
            .map_err(|e| RatError::NetworkError(format!("发送双向流请求失败: {}", e)))?;

        // 创建发送和接收通道
        let (send_tx, send_rx) = mpsc::unbounded_channel::<Bytes>();
        let (recv_tx, recv_rx) = mpsc::unbounded_channel::<Bytes>();

        // 启动发送任务（使用 GrpcCodec 统一编码）
        let connection_id = connection.connection_id.clone();
        let connection_pool = self.connection_pool.clone();
        let send_task = {
            let mut send_stream = send_stream;
            tokio::spawn(async move {
                let mut send_rx = send_rx;
                let mut message_sent = false;
                let mut stream_closed = false;
                
                while let Some(data) = send_rx.recv().await {
                    if stream_closed {
                        warn!("⚠️ [客户端] 流已关闭，跳过发送数据");
                        continue;
                    }
                    
                    message_sent = true;
                    
                    // data 是通过 GrpcStreamSender 序列化后的原始数据，需要包装成 gRPC 帧格式
                    let frame = GrpcCodec::create_frame(&data);
                    let frame_len = frame.len();
                    if let Err(e) = send_stream.send_data(Bytes::from(frame), false) {
                        let error_msg = e.to_string();
                        if error_msg.contains("inactive stream") || error_msg.contains("channel closed") {
                            info!("ℹ️ [客户端] 流已关闭，停止发送数据");
                            stream_closed = true;
                        } else {
                            error!("❌ [客户端] 向服务器发送数据失败: {}", e);
                        }
                        break;
                    }
                    
                    info!("📤 [客户端] 成功发送 gRPC 帧，大小: {} 字节", frame_len);
                }
                
                // 发送结束信号（参考成功示例）
                if message_sent && !stream_closed {
                    if let Err(e) = send_stream.send_data(Bytes::new(), true) {
                        // 如果是 inactive stream 错误，这是正常的，不需要记录为错误
                        if e.to_string().contains("inactive stream") {
                            info!("ℹ️ [客户端] 流已关闭，结束信号发送被忽略");
                        } else {
                            error!("❌ [客户端] 发送结束信号失败: {}", e);
                        }
                    } else {
                        info!("✅ [客户端] 发送流已结束");
                    }
                }
                
                // 释放连接回连接池
                connection_pool.release_connection(&connection_id);
                info!("🔄 [客户端] 消息发送完成，连接已释放");
            })
        };

        // 启动接收任务（使用 GrpcCodec 统一解码）
        let recv_task = {
            tokio::spawn(async move {
                info!("🔄 [客户端] 启动双向流接收任务，等待服务器响应...");
                match response.await {
                    Ok(response) => {
                        let status = response.status();
                        info!("📥 [客户端] 收到服务器响应头，状态: {}", status);
                        debug!("🔍 [客户端] 响应头详情: {:?}", response.headers());
                        
                        let mut body = response.into_body();
                        let mut buffer = Vec::new();
                        
                        // 接收响应流（使用 GrpcCodec 统一解码）
                        info!("🔄 [客户端] 开始接收响应流数据...");
                        while let Some(chunk_result) = body.data().await {
                            debug!("📦 [客户端] 收到数据块结果: {:?}", chunk_result.is_ok());
                            match chunk_result {
                                Ok(chunk) => {
                                    info!("📦 [客户端] 收到数据块，大小: {} 字节", chunk.len());
                                    buffer.extend_from_slice(&chunk);
                                    
                                    // 使用 GrpcCodec 解析 gRPC 消息帧
                    loop {
                        match GrpcCodec::try_parse_frame(&buffer) {
                            Some((message_data, consumed)) => {
                                // message_data 是 gRPC 帧的负载部分，包含序列化的 GrpcStreamMessage<Vec<u8>>
                                // 需要先反序列化为 GrpcStreamMessage，然后提取其中的 data 字段
                                match GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(message_data) {
                                    Ok(stream_message) => {
                                        // 检查是否是流结束消息
                                        if stream_message.end_of_stream {
                                            info!("📥 [客户端] 收到流结束信号");
                                            return;
                                        }
                                        
                                        // 记录数据长度（在移动前）
                                        let data_len = stream_message.data.len();
                                        let sequence = stream_message.sequence;
                                        
                                        // 提取实际的消息数据（已序列化的目标类型）
                                        if let Err(e) = recv_tx.send(Bytes::from(stream_message.data)) {
                                            error!("❌ [客户端] 接收通道发送失败: {}", e);
                                            return;
                                        }
                                        
                                        info!("📥 [客户端] 成功解析并转发流消息，序列号: {}, 数据大小: {} 字节", 
                                                         sequence, data_len);
                                    }
                                    Err(e) => {
                                        error!("❌ [客户端] 反序列化 GrpcStreamMessage 失败: {}", e);
                                        return;
                                    }
                                }
                                
                                // 移除已处理的数据
                                buffer.drain(0..consumed);
                            }
                            None => {
                                // 数据不完整，等待更多数据
                                break;
                            }
                        }
                    }
                                }
                                Err(e) => {
                                    error!("❌ [客户端] 接收服务器数据失败: {}", e);
                                    break;
                                }
                         }
                        }
                        info!("✅ [客户端] 消息接收完成");
                    }
                    Err(e) => {
                        error!("❌ [客户端] 接收服务器响应失败: {}", e);
                    }
                }
            })
        };

        Ok(GrpcBidirectionalStream {
            sender: GrpcStreamSender::new(send_tx),
            receiver: GrpcStreamReceiver::new(recv_rx),
            send_task: Some(send_task),
            recv_task: Some(recv_task),
            connection_id: connection.connection_id.clone(),
            connection_pool: self.connection_pool.clone(),
        })
    }

    /// 创建服务端流 - 直接使用 H2 RecvStream
    fn create_server_stream<R>(&self, mut recv_stream: RecvStream) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<R>, RatError>> + Send>>
    where
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        // 创建流来处理响应体
        let stream = async_stream::stream! {
            let mut buffer = Vec::new();
            let mut stream_ended = false;
            
            // 接收响应流数据
            while let Some(chunk_result) = recv_stream.data().await {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.extend_from_slice(&chunk);
                        // 释放流控制窗口
                        let _ = recv_stream.flow_control().release_capacity(chunk.len());
                        
                        // 尝试解析完整的 gRPC 消息
                        while buffer.len() >= 5 {
                            let compression_flag = buffer[0];
                            let message_length = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
                            
                            println!("DEBUG: [客户端] 解析 gRPC 消息头 - 压缩标志: {}, 消息长度: {}, 缓冲区总长度: {}", 
                                    compression_flag, message_length, buffer.len());
                            
                            if buffer.len() >= 5 + message_length {
                                let message_data = &buffer[5..5 + message_length];
                                
                                println!("DEBUG: [客户端] 提取消息数据，长度: {}, 前32字节: {:?}", 
                                        message_data.len(), 
                                        &message_data[..std::cmp::min(32, message_data.len())]);
                                
                                // 检查压缩标志
                                if compression_flag != 0 {
                                    yield Err(RatError::DeserializationError("不支持压缩的 gRPC 消息".to_string()));
                                    stream_ended = true;
                                    break;
                                }
                                
                                // 优化反序列化策略：先尝试直接反序列化为目标类型 R
                                // 如果失败，再尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                
                                // 策略1：如果 R 是 Vec<u8>，直接尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                if std::any::TypeId::of::<R>() == std::any::TypeId::of::<Vec<u8>>() {
                                    match GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(message_data) {
                                        Ok(stream_message) => {
                                            // 安全的类型转换
                                            let typed_message = GrpcStreamMessage {
                                                id: stream_message.id,
                                                stream_id: stream_message.stream_id,
                                                sequence: stream_message.sequence,
                                                end_of_stream: stream_message.end_of_stream,
                                                data: unsafe { std::mem::transmute_copy(&stream_message.data) },
                                                metadata: stream_message.metadata,
                                            };
                                            yield Ok(typed_message);
                                            
                                            // 如果是流结束标志，退出循环
                                            if stream_message.end_of_stream {
                                                stream_ended = true;
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            println!("DEBUG: [客户端] 反序列化 GrpcStreamMessage<Vec<u8>> 失败: {}", e);
                                            yield Err(RatError::DeserializationError(format!("反序列化 gRPC 流消息失败: {}", e)));
                                            stream_ended = true;
                                            break;
                                        }
                                    }
                                } else {
                                    // 策略2：对于其他类型，先尝试直接反序列化为目标类型
                                    match GrpcCodec::decode::<R>(message_data) {
                                        Ok(data) => {
                                            println!("DEBUG: [客户端] 直接反序列化为目标类型成功！");
                                            // 创建一个简化的流消息结构
                                            let typed_message = GrpcStreamMessage {
                                                id: 0, // 简化处理
                                                stream_id: 0,
                                                sequence: 0,
                                                end_of_stream: false, // 由上层逻辑判断
                                                data,
                                                metadata: std::collections::HashMap::new(),
                                            };
                                            yield Ok(typed_message);
                                        }
                                        Err(_) => {
                                            // 如果直接反序列化失败，尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                            println!("DEBUG: [客户端] 直接反序列化失败，尝试 GrpcStreamMessage 包装格式");
                                            match GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(message_data) {
                                                Ok(stream_message) => {
                                                    // 尝试反序列化 data 字段为目标类型 R
                                                    println!("DEBUG: [客户端] 尝试反序列化 data 字段，数据长度: {}, 前32字节: {:?}", 
                                                            stream_message.data.len(), 
                                                            &stream_message.data[..std::cmp::min(32, stream_message.data.len())]);
                                                    match GrpcCodec::decode::<R>(&stream_message.data) {
                                                        Ok(data) => {
                                                            println!("DEBUG: [客户端] 反序列化成功！");
                                                            let typed_message = GrpcStreamMessage {
                                                                id: stream_message.id,
                                                                stream_id: stream_message.stream_id,
                                                                sequence: stream_message.sequence,
                                                                end_of_stream: stream_message.end_of_stream,
                                                                data,
                                                                metadata: stream_message.metadata,
                                                            };
                                                            yield Ok(typed_message);
                                                            
                                                            // 如果是流结束标志，退出循环
                                                            if stream_message.end_of_stream {
                                                                stream_ended = true;
                                                                break;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            println!("DEBUG: [客户端] 反序列化 data 字段失败: {}", e);
                                                            yield Err(RatError::DeserializationError(format!("反序列化数据字段失败: {}", e)));
                                                            stream_ended = true;
                                                            break;
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    println!("DEBUG: [客户端] 反序列化 GrpcStreamMessage 失败: {}", e);
                                                    yield Err(RatError::DeserializationError(format!("反序列化 gRPC 流消息失败: {}", e)));
                                                    stream_ended = true;
                                                    break;
                                                }
                                            }
                                        }
                                     }
                                 }
                                
                                // 移除已处理的数据
                                buffer.drain(0..5 + message_length);
                            } else {
                                // 数据不完整，等待更多数据
                                break;
                            }
                        }
                        
                        if stream_ended {
                            break;
                        }
                    }
                    Err(e) => {
                        yield Err(RatError::NetworkError(format!("接收流数据错误: {}", e)));
                        stream_ended = true;
                        break;
                    }
                }
            }
            
            // 检查 trailers 以获取 gRPC 状态
            if let Ok(trailers) = recv_stream.trailers().await {
                if let Some(trailers) = trailers {
                    if let Some(grpc_status) = trailers.get("grpc-status") {
                        if let Ok(status_str) = grpc_status.to_str() {
                            if let Ok(status_code) = status_str.parse::<u32>() {
                                if status_code != 0 {
                                    let grpc_message = trailers.get("grpc-message")
                                        .and_then(|v| v.to_str().ok())
                                        .unwrap_or("Unknown error");
                                    yield Err(RatError::Other(format!("gRPC 错误 (状态码: {}): {}", status_code, grpc_message)));
                                }
                            }
                        }
                    }
                }
            }
        };

        Box::pin(stream)
    }

    /// 创建基于 H2 的双向流
    async fn create_h2_bidirectional_stream<T, R>(
        &self,
        mut send_stream: h2::SendStream<bytes::Bytes>,
        response: h2::client::ResponseFuture,
        _stream_id: u64,
    ) -> RatResult<(mpsc::Sender<T>, Pin<Box<dyn Stream<Item = Result<R, RatError>> + Send>>)>
    where
        T: Serialize + Send + Sync + 'static + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        // 创建发送通道
        let (sender, mut receiver) = mpsc::channel::<T>(100);
        
        // 启动发送任务
        tokio::spawn(async move {
            let mut message_sent = false;
            
            while let Some(message) = receiver.recv().await {
                message_sent = true;
                
                // 使用统一的编解码器编码并创建帧
                match GrpcCodec::encode_frame(&message) {
                    Ok(frame) => {
                        
                        // 发送消息
                        if let Err(e) = send_stream.send_data(frame.into(), false) {
                            eprintln!("发送消息失败: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("编码 gRPC 消息失败: {}", e);
                        break;
                    }
                }
            }
            
            // 当发送通道关闭时，如果发送过消息，则发送结束信号
            if message_sent {
                if let Err(e) = send_stream.send_data(bytes::Bytes::new(), true) {
                    // 如果是 inactive stream 错误，这是正常的，不需要记录为错误
                    if e.to_string().contains("inactive stream") {
                        println!("ℹ️ H2 流已关闭，结束信号发送被忽略");
                    } else {
                        eprintln!("❌ H2 发送结束信号失败: {}", e);
                    }
                } else {
                    println!("✅ H2 发送流已正常关闭");
                }
            }
        });
        
        // 创建接收流
        let receive_stream = async_stream::stream! {
            match response.await {
                Ok(response) => {
                    let mut body = response.into_body();
                    let mut buffer = Vec::new();
                    
                    // 接收响应流
                    while let Some(chunk_result) = body.data().await {
                        match chunk_result {
                            Ok(chunk) => {
                                buffer.extend_from_slice(&chunk);
                                
                                // 尝试解析完整的 gRPC 消息
                                while let Some((message_data, consumed)) = GrpcCodec::try_parse_frame(&buffer) {
                                    // 尝试反序列化消息
                                    match GrpcCodec::decode::<R>(&message_data) {
                                        Ok(message) => {
                                            yield Ok(message);
                                        }
                                        Err(e) => {
                                            yield Err(RatError::DeserializationError(format!("反序列化失败: {}", e)));
                                            break;
                                        }
                                    }
                                    
                                    // 移除已处理的数据
                                    buffer.drain(0..consumed);
                                }
                            }
                            Err(e) => {
                                yield Err(RatError::NetworkError(format!("接收数据错误: {}", e)));
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    yield Err(RatError::NetworkError(format!("接收响应失败: {}", e)));
                }
            }
        };

        Ok((sender, Box::pin(receive_stream)))
    }

    /// 获取下一个流 ID
    pub fn next_stream_id(&self) -> u64 {
        self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// 关闭客户端并清理所有资源
    /// 
    /// 这个方法会：
    /// 1. 关闭所有活跃的委托模式双向流
    /// 2. 停止连接池维护任务
    /// 3. 关闭所有连接
    pub async fn shutdown(&mut self) {
        info!("🛑 开始关闭 gRPC 客户端");

        // 关闭所有活跃的委托模式双向流
        if let Err(e) = self.delegated_manager.close_all_streams().await {
            warn!("⚠️ 关闭委托模式双向流失败: {}", e);
        }

        // 发送连接池关闭信号并等待处理
        self.connection_pool.send_shutdown_signal().await;
        
        info!("✅ gRPC 客户端已关闭");
    }
}

impl<S, R> GrpcBidirectionalStream<S, R> {
    /// 将双向流分解为发送端和接收端
    /// 
    /// 这个方法会消费 `GrpcBidirectionalStream` 并返回其组成部分，
    /// 允许用户独立使用发送端和接收端。
    pub fn into_parts(mut self) -> (GrpcStreamSender<S>, GrpcStreamReceiver<R>) {
        // 取出任务句柄，防止 Drop 时被 abort
        let _send_task = self.send_task.take();
        let _recv_task = self.recv_task.take();
        
        // 使用 ManuallyDrop 来避免 Drop 被调用
        let mut manual_drop = std::mem::ManuallyDrop::new(self);
        
        // 安全地移动出字段
        let sender = unsafe { std::ptr::read(&manual_drop.sender) };
        let receiver = unsafe { std::ptr::read(&manual_drop.receiver) };
        
        (sender, receiver)
    }

    /// 关闭流
    pub async fn close(&mut self) {
        // 等待任务完成
        if let Some(send_task) = self.send_task.take() {
            let _ = send_task.await;
        }
        if let Some(recv_task) = self.recv_task.take() {
            let _ = recv_task.await;
        }

        // 释放连接
        self.connection_pool.release_connection(&self.connection_id);
    }

    /// 获取连接统计信息
    pub fn get_connection_stats(&self) -> (usize, usize) {
        self.connection_pool.get_stats()
    }
}

impl<S, R> Drop for GrpcBidirectionalStream<S, R> {
    fn drop(&mut self) {
        // 确保连接被正确释放
        self.connection_pool.release_connection(&self.connection_id);
        
        // 取消任务
        if let Some(send_task) = self.send_task.take() {
            send_task.abort();
        }
        if let Some(recv_task) = self.recv_task.take() {
            recv_task.abort();
        }
    }
}