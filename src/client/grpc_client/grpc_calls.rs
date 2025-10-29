//! gRPC 一元调用模块

use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use bincode;

use hyper::{Request, Method, Uri};
use hyper::header::{HeaderMap, HeaderValue, CONTENT_TYPE, ACCEPT_ENCODING, USER_AGENT, CONTENT_ENCODING};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::{Full, BodyExt};
use hyper::body::Bytes;

use crate::error::{RatError, RatResult};
use crate::compression::{CompressionType, CompressionConfig};
use crate::server::grpc_types::{GrpcRequest, GrpcResponse};
use crate::server::grpc_codec::GrpcCodec;
use crate::client::connection_pool::{ClientConnectionPool, ConnectionPoolConfig};
use crate::client::grpc_builder::MtlsClientConfig;
use crate::client::grpc_client_delegated::ClientBidirectionalManager;
use crate::utils::logger::{info, warn, debug, error};
use super::GrpcCompressionMode;
use crate::client::grpc_client::RatGrpcClient;

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
    /// 获取压缩模式
    pub fn compression_mode(&self) -> GrpcCompressionMode {
        self.compression_mode
    }
}
