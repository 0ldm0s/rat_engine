//! RAT Engine Python API 客户端模块
//! 
//! 基于 rat_engine 客户端的 Python 绑定，使用委托模式和无锁队列

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyBytes};
use tokio::sync::oneshot;
use crossbeam::queue::SegQueue;
use serde::{Serialize, Deserialize};

use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use bytes::Bytes;

// 导入 rat_engine 客户端
use crate::client::{
    RatGrpcClient, RatHttpClient,
    builder::RatHttpClientBuilder,
    grpc_builder::RatGrpcClientBuilder,
    grpc_client_delegated::{ClientBidirectionalHandler, ClientStreamContext},
    http_client_delegated::{HttpRequestHandler, HttpRequestContext}
};
use crate::server::grpc_types::{GrpcRequest, GrpcResponse};
use crate::utils::logger::{info, warn, debug, error};
use hyper::{Method, Uri};
use crate::server::grpc_codec::GrpcCodec;
use uuid;
use crate::error::{RatError, RatResult};

/// gRPC 一元请求处理器特征（委托模式）
/// 
/// 类似双向流的 ClientBidirectionalHandler，但适用于一元请求场景
#[async_trait::async_trait]
pub trait GrpcUnaryHandler: Send + Sync {
    /// 请求数据类型
    type RequestData: Serialize + Send + Sync;
    /// 响应数据类型
    type ResponseData: for<'de> Deserialize<'de> + Send + Sync;

    /// 处理请求开始事件
    async fn on_request_start(&self, context: &GrpcUnaryContext) -> Result<(), String>;

    /// 处理响应接收事件
    async fn on_response_received(
        &self,
        response: Self::ResponseData,
        context: &GrpcUnaryContext,
    ) -> Result<(), String>;

    /// 处理请求错误事件
    async fn on_error(&self, context: &GrpcUnaryContext, error: String);

    /// 处理请求完成事件
    async fn on_completed(&self, context: &GrpcUnaryContext);
}

/// gRPC 一元请求上下文
/// 
/// 提供请求相关的元数据和状态信息
#[derive(Debug, Clone)]
pub struct GrpcUnaryContext {
    /// 请求ID
    pub request_id: u64,
    /// 服务名称
    pub service: String,
    /// 方法名称
    pub method: String,
    /// URI
    pub uri: String,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl GrpcUnaryContext {
    pub fn new(
        request_id: u64,
        service: String,
        method: String,
        uri: String,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            request_id,
            service,
            method,
            uri,
            metadata,
        }
    }

    /// 获取请求ID
    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    /// 获取完整方法路径
    pub fn full_method(&self) -> String {
        format!("{}/{}", self.service, self.method)
    }
}

/// Python gRPC 一元委托处理器
#[derive(Clone, Debug)]
pub struct PythonGrpcUnaryHandler {
    /// 请求ID
    request_id: String,
    /// 响应队列
    response_queue: Arc<SegQueue<Vec<u8>>>,
    /// 错误队列
    error_queue: Arc<SegQueue<String>>,
    /// 完成状态
    completed: Arc<std::sync::atomic::AtomicBool>,
}

impl PythonGrpcUnaryHandler {
    pub fn new(request_id: String) -> Self {
        Self {
            request_id,
            response_queue: Arc::new(SegQueue::new()),
            error_queue: Arc::new(SegQueue::new()),
            completed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn get_response(&self) -> Option<Vec<u8>> {
        self.response_queue.pop()
    }

    pub fn get_error(&self) -> Option<String> {
        self.error_queue.pop()
    }

    pub fn is_completed(&self) -> bool {
        self.completed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl GrpcUnaryHandler for PythonGrpcUnaryHandler {
    type RequestData = Vec<u8>;
    type ResponseData = Vec<u8>;

    async fn on_request_start(&self, context: &GrpcUnaryContext) -> Result<(), String> {
        info!("🔗 [gRPC一元委托] 请求开始: {}, 方法: {}", self.request_id, context.full_method());
        Ok(())
    }

    async fn on_response_received(
        &self,
        response: Self::ResponseData,
        context: &GrpcUnaryContext,
    ) -> Result<(), String> {
        info!("📥 [gRPC一元委托] 收到响应: {}, 数据大小: {} 字节", self.request_id, response.len());
        
        self.response_queue.push(response);
        Ok(())
    }

    async fn on_error(&self, context: &GrpcUnaryContext, error: String) {
        error!("❌ [gRPC一元委托] 请求错误: {}, 错误: {}", self.request_id, error);
        self.error_queue.push(error);
        self.completed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    async fn on_completed(&self, context: &GrpcUnaryContext) {
        info!("✅ [gRPC一元委托] 请求完成: {}", self.request_id);
        self.completed.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 客户端请求类型（使用无锁队列传递）
#[derive(Debug)]
pub enum ClientRequest {
    /// gRPC 一元请求
    GrpcUnary {
        uri: String,
        service: String,
        method: String,
        data: Vec<u8>,
        response_tx: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// gRPC 一元委托模式请求
    GrpcUnaryDelegated {
        uri: String,
        service: String,
        method: String,
        data: Vec<u8>,
        metadata: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<String, String>>, // 返回请求ID
    },
    /// gRPC 一元委托模式接收响应
    GrpcUnaryDelegatedReceive {
        request_id: String,
        response_tx: oneshot::Sender<Result<Option<Vec<u8>>, String>>,
    },
    /// gRPC 一元委托模式检查状态
    GrpcUnaryDelegatedStatus {
        request_id: String,
        response_tx: oneshot::Sender<Result<bool, String>>, // 是否完成
    },
    /// gRPC 双向流请求
    GrpcBidirectional {
        uri: String,
        service: String,
        method: String,
        metadata: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<String, String>>, // 返回流ID
    },
    /// gRPC 双向流发送消息
    GrpcBidirectionalSend {
        stream_id: String,
        data: Vec<u8>,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    /// gRPC 双向流关闭
    GrpcBidirectionalClose {
        stream_id: String,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    /// HTTP GET 请求
    HttpGet {
        url: String,
        headers: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<HttpResponse, String>>,
    },
    /// HTTP POST 请求
    HttpPost {
        url: String,
        body: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<HttpResponse, String>>,
    },
    /// HTTP POST JSON 请求
    HttpPostJson {
        url: String,
        json_data: String,
        headers: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<HttpResponse, String>>,
    },
    /// HTTP PUT 请求
    HttpPut {
        url: String,
        body: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<HttpResponse, String>>,
    },
    /// HTTP DELETE 请求
    HttpDelete {
        url: String,
        headers: Option<HashMap<String, String>>,
        response_tx: oneshot::Sender<Result<HttpResponse, String>>,
    },
    /// HTTP 委托模式请求
    HttpDelegated {
        method: String,
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<Vec<u8>>,
        response_tx: oneshot::Sender<Result<String, String>>, // 返回请求ID
    },
    /// HTTP 委托模式接收响应
    HttpDelegatedReceive {
        request_id: String,
        response_tx: oneshot::Sender<Result<Option<HttpResponse>, String>>,
    },
    /// HTTP 委托模式检查状态
    HttpDelegatedStatus {
        request_id: String,
        response_tx: oneshot::Sender<Result<bool, String>>, // 是否完成
    },
    /// 关闭客户端
    Shutdown,
}

/// HTTP 响应结构
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// Python HTTP 委托处理器
#[derive(Clone, Debug)]
pub struct PythonHttpDelegatedHandler {
    /// 请求ID
    request_id: String,
    /// 响应队列
    response_queue: Arc<SegQueue<HttpResponse>>,
    /// 错误队列
    error_queue: Arc<SegQueue<String>>,
    /// 完成状态
    completed: Arc<std::sync::atomic::AtomicBool>,
}

impl PythonHttpDelegatedHandler {
    pub fn new(request_id: String) -> Self {
        Self {
            request_id,
            response_queue: Arc::new(SegQueue::new()),
            error_queue: Arc::new(SegQueue::new()),
            completed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn get_response(&self) -> Option<HttpResponse> {
        self.response_queue.pop()
    }

    pub fn get_error(&self) -> Option<String> {
        self.error_queue.pop()
    }

    pub fn is_completed(&self) -> bool {
        self.completed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl HttpRequestHandler for PythonHttpDelegatedHandler {
    type RequestData = Vec<u8>;
    type ResponseData = Vec<u8>;

    async fn on_request_start(&self, _context: &HttpRequestContext) -> Result<(), String> {
        info!("🔗 [HTTP委托] 请求开始: {}", self.request_id);
        Ok(())
    }

    async fn on_response_received(
        &self,
        response: crate::client::http_client::RatHttpResponse,
        _context: &HttpRequestContext,
    ) -> Result<(), String> {
        info!("📥 [HTTP委托] 收到响应: {}, 状态: {}", self.request_id, response.status);
        
        // 转换响应格式
        let http_response = HttpResponse {
            status: response.status.as_u16(),
            headers: response.headers.iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect(),
            body: response.body.to_vec(),
        };
        
        self.response_queue.push(http_response);
        Ok(())
    }

    async fn on_error(&self, _context: &HttpRequestContext, error: String) {
        error!("❌ [HTTP委托] 请求错误: {}, 错误: {}", self.request_id, error);
        self.error_queue.push(error);
        self.completed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    async fn on_completed(&self, _context: &HttpRequestContext) {
        info!("✅ [HTTP委托] 请求完成: {}", self.request_id);
        self.completed.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 客户端配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_idle_connections: usize,
    pub enable_compression: bool,
    pub enable_retry: bool,
    pub max_retries: u32,
    pub user_agent: String,
    pub enable_http: bool,
    pub http_user_agent: Option<String>,
    pub enable_grpc: bool,
    pub development_mode: bool,
    pub http2_only: bool,
    pub http1_only: bool,  // 强制使用HTTP/1.1模式
    // mTLS 客户端配置
    pub mtls_client_cert_path: Option<String>,
    pub mtls_client_key_path: Option<String>,
    pub mtls_ca_cert_path: Option<String>,
    pub mtls_skip_server_verification: bool,
    pub mtls_server_name: Option<String>,
}

/// Python 双向流处理器
#[derive(Clone)]
pub struct PythonBidirectionalHandler {
    /// 流ID
    stream_id: String,
    /// 消息接收队列
    message_queue: Arc<SegQueue<Vec<u8>>>,
    /// 关闭信号
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl PythonBidirectionalHandler {
    pub fn new(stream_id: String) -> Self {
        Self {
            stream_id,
            message_queue: Arc::new(SegQueue::new()),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

#[async_trait::async_trait]
impl ClientBidirectionalHandler for PythonBidirectionalHandler {
    type SendData = Vec<u8>;
    type ReceiveData = Vec<u8>;

    async fn on_connected(&self, _context: &ClientStreamContext) -> Result<(), String> {
        info!("🔗 [Python双向流] 连接建立，流ID: {}", self.stream_id);
        Ok(())
    }

    async fn on_message_received(
        &self,
        message: Self::ReceiveData,
        _context: &ClientStreamContext,
    ) -> Result<(), String> {
        // 将消息放入队列供 Python 端消费
        self.message_queue.push(message);
        Ok(())
    }

    async fn on_send_task(&self, _context: &ClientStreamContext) -> Result<(), String> {
        // Python 端控制发送逻辑，这里不做任何操作
        Ok(())
    }

    async fn on_disconnected(&self, _context: &ClientStreamContext, reason: Option<String>) {
        self.closed.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(reason) = reason {
            info!("🔌 [Python双向流] 连接断开，流ID: {}，原因: {}", self.stream_id, reason);
        } else {
            info!("🔌 [Python双向流] 连接断开，流ID: {}", self.stream_id);
        }
    }

    async fn on_error(&self, _context: &ClientStreamContext, error: String) {
        error!("❌ [Python双向流] 错误，流ID: {}，错误: {}", self.stream_id, error);
    }
}

/// 客户端管理器（使用真实的 rat_engine 客户端）
pub struct ClientManager {
    /// gRPC 客户端
    grpc_client: Option<Arc<RatGrpcClient>>,
    /// HTTP 客户端
    http_client: Option<Arc<RatHttpClient>>,
    /// 请求队列（无锁）
    request_queue: Arc<SegQueue<ClientRequest>>,
    /// 关闭信号
    shutdown_signal: Arc<std::sync::atomic::AtomicBool>,
    /// 工作线程句柄
    worker_handle: Option<tokio::task::JoinHandle<()>>,
    /// 双向流处理器管理器
    bidirectional_handlers: Arc<RwLock<HashMap<String, PythonBidirectionalHandler>>>,
    /// HTTP 委托处理器管理器
    http_delegated_handlers: Arc<RwLock<HashMap<String, PythonHttpDelegatedHandler>>>,
    /// gRPC 一元委托处理器管理器
    grpc_unary_handlers: Arc<RwLock<HashMap<String, PythonGrpcUnaryHandler>>>,
    /// HTTP 委托管理器
    http_delegated_manager: Option<Arc<crate::client::http_client_delegated::HttpRequestManager>>,
}

impl ClientManager {
    /// 创建新的客户端管理器
    pub async fn new(config: ClientConfig) -> RatResult<Self> {
        // 初始化 Rustls 加密提供者（仅在首次调用时生效）
        crate::utils::crypto_provider::ensure_crypto_provider_installed();
        
        // 构建 gRPC 客户端（可选）
        let grpc_client = if config.enable_grpc {
            let mut grpc_builder = RatGrpcClientBuilder::new()
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs))?
                .request_timeout(Duration::from_secs(config.request_timeout_secs))?
                .max_idle_connections(config.max_idle_connections)?
                .user_agent(config.user_agent.clone())?
                .disable_compression();
        
        // 配置 mTLS 客户端证书认证
        if let (Some(cert_path), Some(key_path)) = (&config.mtls_client_cert_path, &config.mtls_client_key_path) {
            use std::fs;
            use openssl::x509::X509;
            use openssl::pkey::{PKey, Private};
            use openssl::stack::Stack;

            // 读取客户端证书和私钥
            let cert_pem = fs::read_to_string(cert_path)
                .map_err(|e| RatError::ConfigError(format!("无法读取客户端证书文件 {}: {}", cert_path, e)))?;
            let key_pem = fs::read_to_string(key_path)
                .map_err(|e| RatError::ConfigError(format!("无法读取客户端私钥文件 {}: {}", key_path, e)))?;

            // 解析客户端证书
            let cert = X509::from_pem(cert_pem.as_bytes())
                .map_err(|e| RatError::ConfigError(format!("解析客户端证书失败: {}", e)))?;

            // 解析客户端私钥
            let private_key = PKey::private_key_from_pem(key_pem.as_bytes())
                .map_err(|e| RatError::ConfigError(format!("解析客户端私钥失败: {}", e)))?;

            // 如果配置了跳过服务器验证，使用自签名 mTLS 配置
            if config.mtls_skip_server_verification {
                grpc_builder = grpc_builder.with_self_signed_mtls_openssl(
                    cert,
                    private_key,
                    config.mtls_server_name.clone(),
                    config.mtls_client_cert_path.clone(),
                    config.mtls_client_key_path.clone(),
                )?;
            } else {
                // 解析 CA 证书（如果提供）
                let ca_certs = if let Some(ca_path) = &config.mtls_ca_cert_path {
                    let ca_pem = fs::read_to_string(ca_path)
                        .map_err(|e| RatError::ConfigError(format!("无法读取 CA 证书文件 {}: {}", ca_path, e)))?;

                    let ca_cert = X509::from_pem(ca_pem.as_bytes())
                        .map_err(|e| RatError::ConfigError(format!("解析 CA 证书失败: {}", e)))?;

                    Some(vec![ca_cert])
                } else {
                    None
                };

                grpc_builder = grpc_builder.with_mtls_openssl(
                    cert,
                    private_key,
                    ca_certs,
                    config.mtls_skip_server_verification,
                    config.mtls_server_name.clone(),
                    config.mtls_client_cert_path.clone(),
                    config.mtls_client_key_path.clone(),
                    config.mtls_ca_cert_path.clone(),
                )?;
            }
        }
        
        // 根据配置设置 HTTP 协议模式和开发模式
        if config.http2_only {
            grpc_builder = grpc_builder.http2_only();
        } else {
            grpc_builder = grpc_builder.http_mixed();
        }
        
        grpc_builder = grpc_builder.with_development_mode(config.development_mode)?;
        
        Some(Arc::new(grpc_builder.build()?))
    } else {
        None
    };

        // 构建 HTTP 客户端（可选）
        let (http_client, http_delegated_manager) = if config.enable_http {
            let mut http_builder = RatHttpClientBuilder::new()
                .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
                .request_timeout(Duration::from_secs(config.request_timeout_secs))
                .max_idle_per_host(config.max_idle_connections)
                .user_agent(config.http_user_agent.clone().unwrap_or_else(|| config.user_agent.clone()))?
                .http2_only(config.http2_only);
            
            // 如果启用http1_only，则强制使用HTTP/1.1模式
            if config.http1_only {
                http_builder = http_builder.http1_only();
            }
            
            // 配置 mTLS 客户端证书认证（与 gRPC 客户端保持一致）
            if let (Some(cert_path), Some(key_path)) = (&config.mtls_client_cert_path, &config.mtls_client_key_path) {
                use std::fs;
                use openssl::x509::X509;
                use openssl::pkey::{PKey, Private};
                use openssl::stack::Stack;

                // 读取客户端证书和私钥
                let cert_pem = fs::read_to_string(cert_path)
                    .map_err(|e| RatError::ConfigError(format!("无法读取客户端证书文件 {}: {}", cert_path, e)))?;
                let key_pem = fs::read_to_string(key_path)
                    .map_err(|e| RatError::ConfigError(format!("无法读取客户端私钥文件 {}: {}", key_path, e)))?;

                // 解析客户端证书
                let cert = X509::from_pem(cert_pem.as_bytes())
                    .map_err(|e| RatError::ConfigError(format!("解析客户端证书失败: {}", e)))?;

                // 解析客户端私钥
                let private_key = PKey::private_key_from_pem(key_pem.as_bytes())
                    .map_err(|e| RatError::ConfigError(format!("解析客户端私钥失败: {}", e)))?;

                // 如果配置了跳过服务器验证，使用自签名 mTLS 配置
                if config.mtls_skip_server_verification {
                    http_builder = http_builder.with_self_signed_mtls_openssl(
                        cert,
                        private_key,
                        config.mtls_server_name.clone(),
                        config.mtls_client_cert_path.clone(),
                        config.mtls_client_key_path.clone(),
                    )?;
                } else {
                    // 使用标准 mTLS 配置
                    let ca_certs = if let Some(ca_path) = &config.mtls_ca_cert_path {
                        let ca_pem = fs::read_to_string(ca_path)
                            .map_err(|e| RatError::ConfigError(format!("无法读取 CA 证书文件 {}: {}", ca_path, e)))?;

                        let ca_cert = X509::from_pem(ca_pem.as_bytes())
                            .map_err(|e| RatError::ConfigError(format!("解析 CA 证书失败: {}", e)))?;

                        Some(vec![ca_cert])
                    } else {
                        None
                    };

                    http_builder = http_builder.with_mtls_openssl(
                        cert,
                        private_key,
                        ca_certs,
                        config.mtls_skip_server_verification,
                        config.mtls_server_name.clone(),
                        config.mtls_client_cert_path.clone(),
                        config.mtls_client_key_path.clone(),
                        config.mtls_ca_cert_path.clone(),
                    )?;
                }
            }
            
            // 设置压缩和开发模式
            if config.enable_compression {
                http_builder = http_builder.enable_compression();
            } else {
                http_builder = http_builder.disable_compression();
            }
            
            if config.development_mode {
                http_builder = http_builder.development_mode();
            }
            
            let http_client = Arc::new(http_builder.build()?);
            
            // 创建HTTP委托管理器，使用HTTP客户端的连接池
            let http_delegated_manager = Arc::new(crate::client::http_client_delegated::HttpRequestManager::new(
                Arc::downgrade(&http_client),
                http_client.connection_pool.clone(),
            ));
            
            // 更新HTTP客户端的委托管理器
            http_client.update_delegated_manager(http_delegated_manager.clone());
            
            (Some(http_client), Some(http_delegated_manager))
        } else {
            (None, None)
        };
        
        if config.enable_http {
            info!("✅ HTTP客户端初始化完成（支持委托模式）");
        } else {
            info!("ℹ️ HTTP客户端已禁用");
        }

        let request_queue = Arc::new(SegQueue::new());
        let shutdown_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let bidirectional_handlers = Arc::new(RwLock::new(HashMap::new()));
        let http_delegated_handlers = Arc::new(RwLock::new(HashMap::new()));
        let grpc_unary_handlers = Arc::new(RwLock::new(HashMap::new()));

        rat_logger::info!("🚀 [CLIENT_MANAGER] 启动工作线程...");
        // 启动工作线程
        let worker_handle = Self::start_worker(
            grpc_client.clone(),
            http_client.clone(),
            request_queue.clone(),
            shutdown_signal.clone(),
            config.clone(),
            bidirectional_handlers.clone(),
            http_delegated_handlers.clone(),
            grpc_unary_handlers.clone(),
        ).await;

        Ok(Self {
            grpc_client,
            http_client,
            request_queue,
            shutdown_signal,
            worker_handle: Some(worker_handle),
            bidirectional_handlers,
            http_delegated_handlers,
            grpc_unary_handlers,
            http_delegated_manager,
        })
    }

    /// 启动工作线程
    async fn start_worker(
        grpc_client: Option<Arc<RatGrpcClient>>,
        http_client: Option<Arc<RatHttpClient>>,
        request_queue: Arc<SegQueue<ClientRequest>>,
        shutdown_signal: Arc<std::sync::atomic::AtomicBool>,
        config: ClientConfig,
        bidirectional_handlers: Arc<RwLock<HashMap<String, PythonBidirectionalHandler>>>,
        http_delegated_handlers: Arc<RwLock<HashMap<String, PythonHttpDelegatedHandler>>>,
        grpc_unary_handlers: Arc<RwLock<HashMap<String, PythonGrpcUnaryHandler>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            rat_logger::info!("🚀 [WORKER_THREAD] PyO3客户端工作线程启动");
            rat_logger::info!("📋 [WORKER_THREAD] gRPC客户端: {}", if grpc_client.is_some() { "✅ 已启用" } else { "❌ 已禁用" });
            rat_logger::info!("📋 [WORKER_THREAD] HTTP客户端: {}", if http_client.is_some() { "✅ 已启用" } else { "❌ 已禁用" });
            
            while !shutdown_signal.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some(request) = request_queue.pop() {
                    rat_logger::debug!("🔄 [WORKER_THREAD] 收到请求类型: {:?}", std::mem::discriminant(&request));
                    Self::handle_request(
                        request,
                        &grpc_client,
                        &http_client,
                        &config,
                        &bidirectional_handlers,
                        &http_delegated_handlers,
                        &grpc_unary_handlers,
                    ).await;
                } else {
                    // 无任务时短暂休眠，避免 CPU 空转
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
            
            info!("🛑 [PyO3客户端] 工作线程关闭");
        })
    }

    /// 处理请求
    async fn handle_request(
        request: ClientRequest,
        grpc_client: &Option<Arc<RatGrpcClient>>,
        http_client: &Option<Arc<RatHttpClient>>,
        config: &ClientConfig,
        bidirectional_handlers: &Arc<RwLock<HashMap<String, PythonBidirectionalHandler>>>,
        http_delegated_handlers: &Arc<RwLock<HashMap<String, PythonHttpDelegatedHandler>>>,
        grpc_unary_handlers: &Arc<RwLock<HashMap<String, PythonGrpcUnaryHandler>>>,
    ) {
        rat_logger::debug!("🔧 [HANDLE_REQUEST] 开始处理请求类型: {:?}", std::mem::discriminant(&request));
        let start_time = std::time::Instant::now();
        
        match request {
            ClientRequest::GrpcUnary { uri, service, method, data, response_tx } => {
                let result = Self::handle_grpc_unary_request(
                    grpc_client, &uri, &service, &method, data
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::GrpcUnaryDelegated { uri, service, method, data, metadata, response_tx } => {
                let result = Self::handle_grpc_unary_delegated_request(
                    grpc_client, &uri, &service, &method, data, metadata, grpc_unary_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::GrpcUnaryDelegatedReceive { request_id, response_tx } => {
                let result = Self::handle_grpc_unary_delegated_receive(
                    &request_id, grpc_unary_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::GrpcUnaryDelegatedStatus { request_id, response_tx } => {
                let result = Self::handle_grpc_unary_delegated_status(
                    &request_id, grpc_unary_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpGet { url, headers, response_tx } => {
                rat_logger::info!("🌐 [HANDLE_REQUEST] 处理HTTP GET请求: {}", url);
                let result = Self::handle_http_get_request(
                    http_client, &url, headers
                ).await;
                let elapsed = start_time.elapsed();
                rat_logger::info!("⏱️ [HANDLE_REQUEST] HTTP GET请求处理完成，耗时: {:?}", elapsed);
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpPost { url, body, headers, response_tx } => {
                let result = Self::handle_http_post_request(
                    http_client, &url, body, headers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpPostJson { url, json_data, headers, response_tx } => {
                let result = Self::handle_http_post_json_request(
                    http_client, &url, json_data, headers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpPut { url, body, headers, response_tx } => {
                let result = Self::handle_http_put_request(
                    http_client, &url, body, headers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpDelete { url, headers, response_tx } => {
                let result = Self::handle_http_delete_request(
                    http_client, &url, headers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::GrpcBidirectional { uri, service, method, metadata, response_tx } => {
                let result = Self::handle_grpc_bidirectional_request(
                    grpc_client, &uri, &service, &method, metadata, bidirectional_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::GrpcBidirectionalSend { stream_id, data, response_tx } => {
                let result = Self::handle_grpc_bidirectional_send(
                    grpc_client, stream_id, data, bidirectional_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::GrpcBidirectionalClose { stream_id, response_tx } => {
                let result = Self::handle_grpc_bidirectional_close(
                    grpc_client, stream_id, bidirectional_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpDelegated { method, url, headers, body, response_tx } => {
                let result = Self::handle_http_delegated_request(
                    http_client, &method, &url, headers, body, http_delegated_handlers, config
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpDelegatedReceive { request_id, response_tx } => {
                let result = Self::handle_http_delegated_receive(
                    &request_id, http_delegated_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::HttpDelegatedStatus { request_id, response_tx } => {
                let result = Self::handle_http_delegated_status(
                    &request_id, http_delegated_handlers
                ).await;
                let _ = response_tx.send(result);
            },
            ClientRequest::Shutdown => {
                rat_logger::info!("📥 [HANDLE_REQUEST] 收到关闭指令");
            },
        }
        
        let total_elapsed = start_time.elapsed();
        rat_logger::info!("✅ [HANDLE_REQUEST] 请求处理完成，总耗时: {:?}", total_elapsed);
    }

    /// 处理 gRPC 一元请求
    async fn handle_grpc_unary_request(
        grpc_client: &Option<Arc<RatGrpcClient>>,
        uri: &str,
        service: &str,
        method: &str,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let full_method = format!("{}/{}", service, method);
        
        // 检查 gRPC 客户端是否可用
        match grpc_client {
            Some(client) => {
                // 使用 call_with_uri 方法替代已弃用的 call 方法
                match client.call_with_uri::<Vec<u8>, Vec<u8>>(uri, service, method, data, None).await {
                    Ok(response) => {
                        // 直接返回响应数据
                        Ok(response.data)
                    },
                    Err(e) => Err(format!("gRPC 请求失败: {}", e)),
                }
            },
            None => Err("gRPC 客户端未启用".to_string()),
        }
    }

    /// 处理 HTTP GET 请求
    async fn handle_http_get_request(
        http_client: &Option<Arc<RatHttpClient>>,
        url: &str,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, String> {
        println!("🔍 [HTTP_GET] 开始处理请求: {}", url);
        
        let req_headers = headers.map(|h| {
            let mut header_map = HeaderMap::new();
            for (k, v) in h {
                if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
                    header_map.insert(name, value);
                }
            }
            header_map
        });

        // 检查 HTTP 客户端是否可用
        let client = match http_client {
            Some(client) => {
                println!("✅ [HTTP_GET] HTTP 客户端可用");
                client
            },
            None => {
                println!("❌ [HTTP_GET] HTTP 客户端未启用");
                return Err("HTTP 客户端未启用".to_string());
            },
        };

        println!("🚀 [HTTP_GET] 发送请求到: {}", url);
        let start_time = std::time::Instant::now();
        
        match client.get(url, req_headers).await {
            Ok(response) => {
                let elapsed = start_time.elapsed();
                println!("✅ [HTTP_GET] 请求成功完成，耗时: {:?}", elapsed);
                println!("📊 [HTTP_GET] 响应状态: {}", response.status.as_u16());
                println!("📄 [HTTP_GET] 响应体大小: {} bytes", response.body.len());
                
                Ok(HttpResponse {
                    status: response.status.as_u16(),
                    headers: response.headers.into_iter()
                        .filter_map(|(k, v)| {
                            k.map(|key| (key.to_string(), v.to_str().unwrap_or("").to_string()))
                        })
                        .collect(),
                    body: response.body.to_vec(),
                })
            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                println!("❌ [HTTP_GET] 请求失败，耗时: {:?}, 错误: {}", elapsed, e);
                Err(format!("HTTP GET 请求失败: {}", e))
            },
        }
    }

    /// 处理 HTTP POST 请求
    async fn handle_http_post_request(
        http_client: &Option<Arc<RatHttpClient>>,
        url: &str,
        body: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, String> {
        println!("🔍 [HTTP_POST] 开始处理请求: {}", url);
        
        let req_headers = headers.map(|h| {
            let mut header_map = HeaderMap::new();
            for (k, v) in h {
                if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
                    header_map.insert(name, value);
                }
            }
            header_map
        });

        // 检查 HTTP 客户端是否可用
        let client = match http_client {
            Some(client) => {
                println!("✅ [HTTP_POST] HTTP 客户端可用");
                client
            },
            None => {
                println!("❌ [HTTP_POST] HTTP 客户端未启用");
                return Err("HTTP 客户端未启用".to_string());
            },
        };

        let body_bytes = body.map(|b| {
            println!("📄 [HTTP_POST] 请求体大小: {} bytes", b.len());
            Bytes::from(b)
        });
        
        println!("🚀 [HTTP_POST] 发送POST请求到: {}", url);
        let start_time = std::time::Instant::now();
        
        match client.post(url, body_bytes, req_headers).await {
            Ok(response) => {
                let elapsed = start_time.elapsed();
                println!("✅ [HTTP_POST] 请求成功完成，耗时: {:?}", elapsed);
                println!("📊 [HTTP_POST] 响应状态: {}", response.status.as_u16());
                println!("📄 [HTTP_POST] 响应体大小: {} bytes", response.body.len());
                
                Ok(HttpResponse {
                    status: response.status.as_u16(),
                    headers: response.headers.into_iter()
                        .filter_map(|(k, v)| {
                            k.map(|key| (key.to_string(), v.to_str().unwrap_or("").to_string()))
                        })
                        .collect(),
                    body: response.body.to_vec(),
                })
            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                println!("❌ [HTTP_POST] 请求失败，耗时: {:?}, 错误: {}", elapsed, e);
                Err(format!("HTTP POST 请求失败: {}", e))
            },
        }
    }

    /// 处理 HTTP POST JSON 请求
    async fn handle_http_post_json_request(
        http_client: &Option<Arc<RatHttpClient>>,
        url: &str,
        json_data: String,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, String> {
        let mut header_map = HeaderMap::new();
        header_map.insert(HeaderName::from_static("content-type"), HeaderValue::from_static("application/json"));
        
        if let Some(h) = headers {
            for (k, v) in h {
                if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
                    header_map.insert(name, value);
                }
            }
        }

        // 检查 HTTP 客户端是否可用
        let client = match http_client {
            Some(client) => client,
            None => return Err("HTTP 客户端未启用".to_string()),
        };

        match client.post(url, Some(Bytes::from(json_data.into_bytes())), Some(header_map)).await {
            Ok(response) => Ok(HttpResponse {
                status: response.status.as_u16(),
                headers: response.headers.into_iter()
                    .filter_map(|(k, v)| {
                        k.map(|key| (key.to_string(), v.to_str().unwrap_or("").to_string()))
                    })
                    .collect(),
                body: response.body.to_vec(),
            }),
            Err(e) => Err(format!("HTTP POST JSON 请求失败: {}", e)),
        }
    }

    /// 处理 HTTP PUT 请求
    async fn handle_http_put_request(
        http_client: &Option<Arc<RatHttpClient>>,
        url: &str,
        body: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, String> {
        let req_headers = headers.map(|h| {
            let mut header_map = HeaderMap::new();
            for (k, v) in h {
                if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
                    header_map.insert(name, value);
                }
            }
            header_map
        });

        // 检查 HTTP 客户端是否可用
        let client = match http_client {
            Some(client) => client,
            None => return Err("HTTP 客户端未启用".to_string()),
        };

        let body_bytes = body.map(|b| Bytes::from(b));
        match client.put(url, body_bytes, req_headers).await {
            Ok(response) => Ok(HttpResponse {
                status: response.status.as_u16(),
                headers: response.headers.into_iter()
                    .filter_map(|(k, v)| {
                        k.map(|key| (key.to_string(), v.to_str().unwrap_or("").to_string()))
                    })
                    .collect(),
                body: response.body.to_vec(),
            }),
            Err(e) => Err(format!("HTTP PUT 请求失败: {}", e)),
        }
    }

    /// 处理 HTTP DELETE 请求
    async fn handle_http_delete_request(
        http_client: &Option<Arc<RatHttpClient>>,
        url: &str,
        headers: Option<HashMap<String, String>>,
    ) -> Result<HttpResponse, String> {
        let req_headers = headers.map(|h| {
            let mut header_map = HeaderMap::new();
            for (k, v) in h {
                if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
                    header_map.insert(name, value);
                }
            }
            header_map
        });

        // 检查 HTTP 客户端是否可用
        let client = match http_client {
            Some(client) => client,
            None => return Err("HTTP 客户端未启用".to_string()),
        };

        match client.delete(url, req_headers).await {
            Ok(response) => Ok(HttpResponse {
                status: response.status.as_u16(),
                headers: response.headers.into_iter()
                    .filter_map(|(k, v)| {
                        k.map(|key| (key.to_string(), v.to_str().unwrap_or("").to_string()))
                    })
                    .collect(),
                body: response.body.to_vec(),
            }),
            Err(e) => Err(format!("HTTP DELETE 请求失败: {}", e)),
        }
    }

    /// 处理 gRPC 双向流请求
    async fn handle_grpc_bidirectional_request(
        grpc_client: &Option<Arc<RatGrpcClient>>,
        uri: &str,
        service: &str,
        method: &str,
        metadata: Option<HashMap<String, String>>,
        bidirectional_handlers: &Arc<RwLock<HashMap<String, PythonBidirectionalHandler>>>,
    ) -> Result<String, String> {
        // 生成流ID
        let stream_id = format!("{}-{}-{}", service, method, uuid::Uuid::new_v4());
        
        // 创建处理器
        let handler = PythonBidirectionalHandler::new(stream_id.clone());
        
        // 检查 gRPC 客户端是否可用
        match grpc_client {
            Some(client) => {
                // 使用委托模式创建双向流连接
                match client.create_bidirectional_stream_delegated_with_uri(
                    uri,
                    service,
                    method,
                    Arc::new(handler.clone()),
                    metadata,
                ).await {
                    Ok(actual_stream_id) => {
                        // 存储处理器，使用实际的流ID
                        let actual_stream_id_str = actual_stream_id.to_string();
                        {
                            let mut handlers = bidirectional_handlers.write().unwrap();
                            handlers.insert(actual_stream_id_str.clone(), handler);
                        }
                        info!("✅ [PyO3客户端] 双向流 {} 创建成功", actual_stream_id_str);
                        Ok(actual_stream_id_str)
                    },
                    Err(e) => {
                        error!("❌ [PyO3客户端] 双向流 {} 创建失败: {}", stream_id, e);
                        Err(format!("创建双向流失败: {}", e))
                    }
                }
            },
            None => Err("gRPC 客户端未启用".to_string()),
        }
    }

    /// 处理双向流发送消息
    async fn handle_grpc_bidirectional_send(
        grpc_client: &Option<Arc<RatGrpcClient>>,
        stream_id: String,
        data: Vec<u8>,
        bidirectional_handlers: &Arc<RwLock<HashMap<String, PythonBidirectionalHandler>>>,
    ) -> Result<(), String> {
        // 解析流ID为数字
        let numeric_stream_id: u64 = stream_id.parse()
            .map_err(|_| format!("无效的流ID格式: {}", stream_id))?;

        // 检查 gRPC 客户端是否可用
        match grpc_client {
            Some(client) => {
                // 通过委托管理器获取流上下文
                if let Some(context) = client.get_stream_context(numeric_stream_id).await {
                    // 使用流上下文发送数据
                    context.sender().send_raw(data).await
                        .map_err(|e| format!("发送消息失败: {}", e))?;
                    
                    debug!("📤 [PyO3客户端] 双向流 {} 发送消息成功", stream_id);
                    Ok(())
                } else {
                    Err(format!("未找到流 ID: {}", stream_id))
                }
            },
            None => Err("gRPC 客户端未启用".to_string()),
        }
    }

    /// 处理 HTTP 委托请求
    async fn handle_http_delegated_request(
        http_client: &Option<Arc<RatHttpClient>>,
        method: &str,
        url: &str,
        headers: Option<HashMap<String, String>>,
        body: Option<Vec<u8>>,
        http_delegated_handlers: &Arc<RwLock<HashMap<String, PythonHttpDelegatedHandler>>>,
        _config: &ClientConfig,
    ) -> Result<String, String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let handler = PythonHttpDelegatedHandler::new(request_id.clone());
        
        // 将处理器添加到管理器中
        {
            let mut handlers = http_delegated_handlers.write().unwrap();
            handlers.insert(request_id.clone(), handler.clone());
        }
        
        // 解析HTTP方法
        let http_method = match method.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            "PATCH" => Method::PATCH,
            _ => return Err(format!("不支持的 HTTP 方法: {}", method)),
        };
        
        // 解析URI
        let uri: Uri = url.parse().map_err(|e| format!("无效的URL: {}", e))?;
        
        // 转换请求头
        let mut header_map = HeaderMap::new();
        if let Some(headers) = headers {
            for (key, value) in headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| format!("无效的请求头名称 '{}': {}", key, e))?;
                let header_value = HeaderValue::from_str(&value)
                    .map_err(|e| format!("无效的请求头值 '{}': {}", value, e))?;
                header_map.insert(header_name, header_value);
            }
        }
        
        // 转换请求体
        let body_bytes = body.map(|b| Bytes::from(b));
        
        // 检查 HTTP 客户端是否可用
        let client = match http_client {
            Some(client) => client,
            None => {
                // 客户端不可用时清理处理器
                let mut handlers = http_delegated_handlers.write().unwrap();
                handlers.remove(&request_id);
                return Err("HTTP 客户端未启用".to_string());
            },
        };
        
        // 使用HTTP客户端的委托方法发送请求
        let handlers_clone = http_delegated_handlers.clone();
        let request_id_clone = request_id.clone();
        
        match client.send_request_delegated(
            http_method,
            uri,
            Some(header_map),
            body_bytes,
            handler,
        ).await {
            Ok(_) => {
                info!("🚀 [HTTP委托] 请求已发送: {}", request_id);
                Ok(request_id)
            }
            Err(e) => {
                // 请求失败时移除处理器
                {
                    let mut handlers = handlers_clone.write().unwrap();
                    handlers.remove(&request_id_clone);
                }
                Err(format!("委托请求发送失败: {}", e))
            }
        }
    }
    
    /// 处理 HTTP 委托响应接收
    async fn handle_http_delegated_receive(
        request_id: &str,
        http_delegated_handlers: &Arc<RwLock<HashMap<String, PythonHttpDelegatedHandler>>>,
    ) -> Result<Option<HttpResponse>, String> {
        let handlers = http_delegated_handlers.read().unwrap();
        if let Some(handler) = handlers.get(request_id) {
            Ok(handler.get_response())
        } else {
            Err(format!("未找到请求ID: {}", request_id))
        }
    }
    
    /// 处理 HTTP 委托状态查询
    async fn handle_http_delegated_status(
        request_id: &str,
        http_delegated_handlers: &Arc<RwLock<HashMap<String, PythonHttpDelegatedHandler>>>,
    ) -> Result<bool, String> {
        let handlers = http_delegated_handlers.read().unwrap();
        if let Some(handler) = handlers.get(request_id) {
            let is_completed = handler.is_completed();
            // 如果已完成，清理处理器
            if is_completed {
                drop(handlers);
                let mut handlers_mut = http_delegated_handlers.write().unwrap();
                handlers_mut.remove(request_id);
            }
            Ok(is_completed)
        } else {
            Err(format!("未找到请求ID: {}", request_id))
        }
    }

    /// 处理双向流关闭
    async fn handle_grpc_bidirectional_close(
        grpc_client: &Option<Arc<RatGrpcClient>>,
        stream_id: String,
        bidirectional_handlers: &Arc<RwLock<HashMap<String, PythonBidirectionalHandler>>>,
    ) -> Result<(), String> {
        // 解析流ID为数字
        let numeric_stream_id: u64 = stream_id.parse()
            .map_err(|_| format!("无效的流ID格式: {}", stream_id))?;

        // 检查 gRPC 客户端是否可用
        match grpc_client {
            Some(client) => {
                // 通过委托管理器关闭流
                client.close_bidirectional_stream_delegated(numeric_stream_id).await
                    .map_err(|e| format!("关闭流失败: {}", e))?;
            },
            None => return Err("gRPC 客户端未启用".to_string()),
        }

        // 从本地处理器映射中移除
        {
            let mut handlers = bidirectional_handlers.write().unwrap();
            handlers.remove(&stream_id);
        }

        info!("🔒 [PyO3客户端] 双向流 {} 已关闭", stream_id);
        Ok(())
    }

    /// 处理gRPC一元委托请求
    async fn handle_grpc_unary_delegated_request(
        grpc_client: &Option<Arc<RatGrpcClient>>,
        uri: &str,
        service: &str,
        method: &str,
        data: Vec<u8>,
        metadata: Option<HashMap<String, String>>,
        grpc_unary_handlers: &Arc<RwLock<HashMap<String, PythonGrpcUnaryHandler>>>,
    ) -> Result<String, String> {
        // 生成唯一的请求ID
        let request_id = uuid::Uuid::new_v4().to_string();
        
        // 创建处理器
        let handler = PythonGrpcUnaryHandler::new(request_id.clone());
        
        // 将处理器添加到映射中
        {
            let mut handlers = grpc_unary_handlers.write().unwrap();
            handlers.insert(request_id.clone(), handler.clone());
        }
        
        // 构建元数据
        let metadata_map = metadata.unwrap_or_default();
        
        // 检查 gRPC 客户端是否可用
        let result = match grpc_client {
            Some(client) => {
                client.call_unary_delegated_with_uri(
                    uri,
                    service,
                    method,
                    data,
                    Arc::new(handler),
                    Some(metadata_map),
                ).await
            },
            None => return Err("gRPC 客户端未启用".to_string()),
        };
        
        match result {
            Ok(_) => {
                info!("🚀 [PyO3客户端] gRPC一元委托请求已发送: {}", request_id);
                Ok(request_id)
            }
            Err(e) => {
                // 清理处理器
                let mut handlers = grpc_unary_handlers.write().unwrap();
                handlers.remove(&request_id);
                Err(format!("发送gRPC一元委托请求失败: {}", e))
            }
        }
    }

    /// 处理gRPC一元委托接收响应
    async fn handle_grpc_unary_delegated_receive(
        request_id: &str,
        grpc_unary_handlers: &Arc<RwLock<HashMap<String, PythonGrpcUnaryHandler>>>,
    ) -> Result<Option<Vec<u8>>, String> {
        let handlers = grpc_unary_handlers.read().unwrap();
        if let Some(handler) = handlers.get(request_id) {
            Ok(handler.get_response())
        } else {
            Err(format!("未找到请求ID: {}", request_id))
        }
    }

    /// 处理gRPC一元委托状态检查
    async fn handle_grpc_unary_delegated_status(
        request_id: &str,
        grpc_unary_handlers: &Arc<RwLock<HashMap<String, PythonGrpcUnaryHandler>>>,
    ) -> Result<bool, String> {
        let handlers = grpc_unary_handlers.read().unwrap();
        if let Some(handler) = handlers.get(request_id) {
            let is_completed = handler.is_completed();
            // 如果已完成，清理处理器
            if is_completed {
                drop(handlers);
                let mut handlers_mut = grpc_unary_handlers.write().unwrap();
                handlers_mut.remove(request_id);
            }
            Ok(is_completed)
        } else {
            Err(format!("未找到请求ID: {}", request_id))
        }
    }

    /// 提交请求到无锁队列
    pub fn submit_request(&self, request: ClientRequest) {
        self.request_queue.push(request);
    }

    /// 关闭客户端管理器
    pub async fn shutdown(&mut self) {
        info!("🛑 [PyO3客户端] 开始关闭");
        
        // 发送关闭信号
        self.shutdown_signal.store(true, std::sync::atomic::Ordering::Relaxed);
        
        // 发送关闭请求到队列
        self.request_queue.push(ClientRequest::Shutdown);
        
        // 等待工作线程结束
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.await;
        }
        
        info!("✅ [PyO3客户端] 已完全关闭");
    }
}

/// Python 客户端管理器
#[pyclass]
pub struct PyClientManager {
    manager: Arc<RwLock<Option<ClientManager>>>,
    runtime: Arc<RwLock<Option<tokio::runtime::Runtime>>>,
}

#[pymethods]
impl PyClientManager {
    #[new]
    pub fn new() -> Self {
        Self {
            manager: Arc::new(RwLock::new(None)),
            runtime: Arc::new(RwLock::new(None)),
        }
    }
    
    /// 初始化客户端
    /// 
    /// # 参数
    /// * `config_dict` - 配置字典
    pub fn initialize(&self, config_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let config = Self::parse_config(&config_dict)?;
        
        // 创建 Tokio 运行时
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("创建运行时失败: {}", e)
            ))?;
        
        // 在运行时中创建客户端管理器
        let client_manager = rt.block_on(async {
            ClientManager::new(config).await
        }).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            format!("初始化客户端失败: {}", e)
        ))?;
        
        // 存储运行时和管理器
        {
            let mut runtime_guard = self.runtime.write().unwrap();
            *runtime_guard = Some(rt);
        }
        
        {
            let mut manager_guard = self.manager.write().unwrap();
            *manager_guard = Some(client_manager);
        }
        
        info!("✅ [PyO3客户端] 初始化完成");
        Ok(())
    }

    /// 发送 gRPC 一元请求
    /// 
    /// # 参数
    /// * `service` - 服务名
    /// * `method` - 方法名
    /// * `data` - 请求数据
    pub fn grpc_unary_request(&self, uri: String, service: String, method: String, data: &Bound<'_, PyBytes>) -> PyResult<PyObject> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        let data = data.as_bytes().to_vec();
        
        let request = ClientRequest::GrpcUnary {
            uri,
            service,
            method,
            data,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let response = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("gRPC 请求失败: {}", e)))?;
        
        Python::with_gil(|py| {
            Ok(PyBytes::new(py, &response).into())
        })
    }

    /// HTTP GET 请求
    pub fn http_get(&self, url: String, headers: Option<HashMap<String, String>>) -> PyResult<PyObject> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::HttpGet {
            url,
            headers,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let response = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("HTTP GET 请求失败: {}", e)))?;
        
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("status", response.status)?;
            dict.set_item("headers", response.headers)?;
            dict.set_item("body", PyBytes::new(py, &response.body))?;
            Ok(dict.into())
        })
    }

    /// HTTP POST 请求
    pub fn http_post(&self, url: String, body: Option<Vec<u8>>, headers: Option<HashMap<String, String>>) -> PyResult<PyObject> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::HttpPost {
            url,
            body,
            headers,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let response = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("HTTP POST 请求失败: {}", e)))?;
        
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("status", response.status)?;
            dict.set_item("headers", response.headers)?;
            dict.set_item("body", PyBytes::new(py, &response.body))?;
            Ok(dict.into())
        })
    }

    /// HTTP POST JSON 请求
    pub fn http_post_json(&self, url: String, json_data: String, headers: Option<HashMap<String, String>>) -> PyResult<PyObject> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::HttpPostJson {
            url,
            json_data,
            headers,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let response = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("HTTP POST JSON 请求失败: {}", e)))?;
        
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("status", response.status)?;
            dict.set_item("headers", response.headers)?;
            dict.set_item("body", PyBytes::new(py, &response.body))?;
            Ok(dict.into())
        })
    }

    /// HTTP PUT 请求
    pub fn http_put(&self, url: String, body: Option<Vec<u8>>, headers: Option<HashMap<String, String>>) -> PyResult<PyObject> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::HttpPut {
            url,
            body,
            headers,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let response = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("HTTP PUT 请求失败: {}", e)))?;
        
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("status", response.status)?;
            dict.set_item("headers", response.headers)?;
            dict.set_item("body", PyBytes::new(py, &response.body))?;
            Ok(dict.into())
        })
    }

    /// HTTP DELETE 请求
    pub fn http_delete(&self, url: String, headers: Option<HashMap<String, String>>) -> PyResult<PyObject> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::HttpDelete {
            url,
            headers,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let response = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("HTTP DELETE 请求失败: {}", e)))?;
        
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("status", response.status)?;
            dict.set_item("headers", response.headers)?;
            dict.set_item("body", PyBytes::new(py, &response.body))?;
            Ok(dict.into())
        })
    }

    /// 创建 gRPC 双向流
    /// 
    /// # 参数
    /// * `service` - 服务名
    /// * `method` - 方法名
    pub fn grpc_bidirectional_stream(
        &self,
        uri: String,
        service: String,
        method: String,
    ) -> PyResult<String> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::GrpcBidirectional {
            uri,
            service,
            method,
            metadata: None,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        let stream_id = receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("创建双向流失败: {}", e)))?;
        
        Ok(stream_id)
    }

    /// 向双向流发送消息
    /// 
    /// # 参数
    /// * `stream_id` - 流 ID
    /// * `data` - 消息数据
    pub fn grpc_bidirectional_send(&self, stream_id: String, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        let data = data.as_bytes().to_vec();
        
        let request = ClientRequest::GrpcBidirectionalSend {
            stream_id,
            data,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("发送消息失败: {}", e)))?;
        
        Ok(())
    }

    /// 接收双向流消息
    /// 
    /// # 参数
    /// * `stream_id` - 流 ID
    /// 
    /// # 返回值
    /// * `Some(bytes)` - 如果有消息
    /// * `None` - 如果没有消息
    pub fn grpc_bidirectional_receive(&self, stream_id: String) -> PyResult<Option<PyObject>> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        // 获取双向流处理器
        let handlers = manager.bidirectional_handlers.read().unwrap();
        if let Some(handler) = handlers.get(&stream_id) {
            // 尝试从队列中获取消息
            if let Some(message) = handler.message_queue.pop() {
                Python::with_gil(|py| {
                    Ok(Some(PyBytes::new(py, &message).into()))
                })
            } else {
                Ok(None)
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("未找到流ID: {}", stream_id)
            ))
        }
    }

    /// 检查双向流是否已关闭
    /// 
    /// # 参数
    /// * `stream_id` - 流 ID
    pub fn grpc_bidirectional_is_closed(&self, stream_id: String) -> PyResult<bool> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        // 获取双向流处理器
        let handlers = manager.bidirectional_handlers.read().unwrap();
        if let Some(handler) = handlers.get(&stream_id) {
            Ok(handler.closed.load(std::sync::atomic::Ordering::Relaxed))
        } else {
            // 如果找不到处理器，认为已关闭
            Ok(true)
        }
    }

    /// 关闭双向流
    /// 
    /// # 参数
    /// * `stream_id` - 流 ID
    pub fn grpc_bidirectional_close(&self, stream_id: String) -> PyResult<()> {
        let manager_guard = self.manager.read().unwrap();
        let manager = manager_guard.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端未初始化"
            ))?;
        
        let (sender, receiver) = oneshot::channel();
        
        let request = ClientRequest::GrpcBidirectionalClose {
            stream_id,
            response_tx: sender,
        };
        
        manager.submit_request(request);
        
        // 同步等待响应
        receiver.blocking_recv()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("接收响应失败"))?
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("关闭双向流失败: {}", e)))?;
        
        Ok(())
    }

    /// HTTP 委托请求
    pub fn http_delegated(
        &self,
        method: String,
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<Vec<u8>>,
    ) -> PyResult<String> {
        let runtime_guard = self.runtime.read().unwrap();
        let manager_guard = self.manager.read().unwrap();
        
        if let (Some(runtime), Some(manager)) = (runtime_guard.as_ref(), manager_guard.as_ref()) {
            let (tx, rx) = oneshot::channel();
            let request = ClientRequest::HttpDelegated {
                method,
                url,
                headers,
                body,
                response_tx: tx,
            };
            
            manager.submit_request(request);
            
            match runtime.block_on(rx) {
                Ok(result) => match result {
                    Ok(request_id) => Ok(request_id),
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
                },
                Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "HTTP 委托请求通道错误"
                )),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端管理器未初始化"
            ))
        }
    }
    
    /// HTTP 委托响应接收
    pub fn http_delegated_receive(&self, request_id: String) -> PyResult<Option<PyObject>> {
        let runtime_guard = self.runtime.read().unwrap();
        let manager_guard = self.manager.read().unwrap();
        
        if let (Some(runtime), Some(manager)) = (runtime_guard.as_ref(), manager_guard.as_ref()) {
            let (tx, rx) = oneshot::channel();
            let request = ClientRequest::HttpDelegatedReceive {
                request_id,
                response_tx: tx,
            };
            
            manager.submit_request(request);
            
            match runtime.block_on(rx) {
                Ok(result) => match result {
                    Ok(Some(response)) => {
                        Python::with_gil(|py| {
                            let dict = PyDict::new_bound(py);
                            dict.set_item("status", response.status)?;
                            dict.set_item("headers", response.headers)?;
                            dict.set_item("body", PyBytes::new_bound(py, &response.body))?;
                            Ok(Some(dict.into()))
                        })
                    },
                    Ok(None) => Ok(None),
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
                },
                Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "HTTP 委托响应接收通道错误"
                )),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端管理器未初始化"
            ))
        }
    }
    
    /// HTTP 委托状态查询
    pub fn http_delegated_is_completed(&self, request_id: String) -> PyResult<bool> {
        let runtime_guard = self.runtime.read().unwrap();
        let manager_guard = self.manager.read().unwrap();
        
        if let (Some(runtime), Some(manager)) = (runtime_guard.as_ref(), manager_guard.as_ref()) {
            let (tx, rx) = oneshot::channel();
            let request = ClientRequest::HttpDelegatedStatus {
                request_id,
                response_tx: tx,
            };
            
            manager.submit_request(request);
            
            match runtime.block_on(rx) {
                Ok(result) => match result {
                    Ok(is_completed) => Ok(is_completed),
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
                },
                Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "HTTP 委托状态查询通道错误"
                )),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端管理器未初始化"
            ))
        }
    }

    /// gRPC 一元委托请求
    pub fn grpc_unary_delegated(
        &self,
        uri: String,
        service: String,
        method: String,
        data: &Bound<'_, PyBytes>,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<String> {
        let runtime_guard = self.runtime.read().unwrap();
        let manager_guard = self.manager.read().unwrap();
        
        if let (Some(runtime), Some(manager)) = (runtime_guard.as_ref(), manager_guard.as_ref()) {
            let (tx, rx) = oneshot::channel();
            let request = ClientRequest::GrpcUnaryDelegated {
                uri,
                service,
                method,
                data: data.as_bytes().to_vec(),
                metadata,
                response_tx: tx,
            };
            
            manager.submit_request(request);
            
            match runtime.block_on(rx) {
                Ok(result) => match result {
                    Ok(request_id) => Ok(request_id),
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
                },
                Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "gRPC 一元委托请求通道错误"
                )),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端管理器未初始化"
            ))
        }
    }

    /// gRPC 一元委托接收响应
    pub fn grpc_unary_delegated_receive(&self, request_id: String) -> PyResult<Option<PyObject>> {
        let runtime_guard = self.runtime.read().unwrap();
        let manager_guard = self.manager.read().unwrap();
        
        if let (Some(runtime), Some(manager)) = (runtime_guard.as_ref(), manager_guard.as_ref()) {
            let (tx, rx) = oneshot::channel();
            let request = ClientRequest::GrpcUnaryDelegatedReceive {
                request_id,
                response_tx: tx,
            };
            
            manager.submit_request(request);
            
            match runtime.block_on(rx) {
                Ok(result) => match result {
                    Ok(Some(data)) => {
                        Python::with_gil(|py| {
                            Ok(Some(PyBytes::new_bound(py, &data).into()))
                        })
                    },
                    Ok(None) => Ok(None),
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
                },
                Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "gRPC 一元委托接收通道错误"
                )),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端管理器未初始化"
            ))
        }
    }

    /// gRPC 一元委托检查完成状态
    pub fn grpc_unary_delegated_is_completed(&self, request_id: String) -> PyResult<bool> {
        let runtime_guard = self.runtime.read().unwrap();
        let manager_guard = self.manager.read().unwrap();
        
        if let (Some(runtime), Some(manager)) = (runtime_guard.as_ref(), manager_guard.as_ref()) {
            let (tx, rx) = oneshot::channel();
            let request = ClientRequest::GrpcUnaryDelegatedStatus {
                request_id,
                response_tx: tx,
            };
            
            manager.submit_request(request);
            
            match runtime.block_on(rx) {
                Ok(result) => match result {
                    Ok(is_completed) => Ok(is_completed),
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
                },
                Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "gRPC 一元委托状态检查通道错误"
                )),
            }
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "客户端管理器未初始化"
            ))
        }
    }

    /// 关闭客户端管理器
    pub fn close(&self) -> PyResult<()> {
        let mut manager_guard = self.manager.write().unwrap();
        
        if let Some(mut client_manager) = manager_guard.take() {
            // 使用 tokio 运行时同步等待
            let runtime_guard = self.runtime.read().unwrap();
            if let Some(rt) = runtime_guard.as_ref() {
                rt.block_on(async {
                    client_manager.shutdown().await;
                });
            }
        }
        
        // 关闭运行时
        let mut runtime_guard = self.runtime.write().unwrap();
        if let Some(rt) = runtime_guard.take() {
            rt.shutdown_background();
        }
        
        Ok(())
    }
}

impl PyClientManager {
    /// 解析配置
    fn parse_config(config_dict: &Bound<'_, PyDict>) -> PyResult<ClientConfig> {
        let connect_timeout = config_dict
            .get_item("connect_timeout")?
            .map(|item| item.extract::<u64>())
            .transpose()?
            .unwrap_or(5000);
        
        let request_timeout = config_dict
            .get_item("request_timeout")?
            .map(|item| item.extract::<u64>())
            .transpose()?
            .unwrap_or(30000);
        
        let max_idle_connections = config_dict
            .get_item("max_idle_connections")?
            .map(|item| item.extract::<usize>())
            .transpose()?
            .unwrap_or(10);
        
        let http2_only = config_dict
            .get_item("http2_only")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(false);
        
        let http1_only = config_dict
            .get_item("http1_only")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(false);
        
        let user_agent = config_dict
            .get_item("user_agent")?
            .map(|item| item.extract::<String>())
            .transpose()?
            .unwrap_or_else(|| "rat-engine-python/1.0".to_string());
        
        let http_user_agent = config_dict
            .get_item("http_user_agent")?
            .map(|item| item.extract::<String>())
            .transpose()?;
        
        let enable_compression = config_dict
            .get_item("enable_compression")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(false);
        
        let development_mode = config_dict
            .get_item("development_mode")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(false);
        
        let enable_http = config_dict
            .get_item("enable_http")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(true);
        
        let enable_grpc = config_dict
            .get_item("enable_grpc")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(false);
        
        // 解析 mTLS 配置
        let mtls_client_cert_path = config_dict
            .get_item("mtls_client_cert_path")?
            .map(|item| item.extract::<String>())
            .transpose()?;
        
        let mtls_client_key_path = config_dict
            .get_item("mtls_client_key_path")?
            .map(|item| item.extract::<String>())
            .transpose()?;
        
        let mtls_ca_cert_path = config_dict
            .get_item("mtls_ca_cert_path")?
            .map(|item| item.extract::<String>())
            .transpose()?;
        
        let mtls_skip_server_verification = config_dict
            .get_item("mtls_skip_server_verification")?
            .map(|item| item.extract::<bool>())
            .transpose()?
            .unwrap_or(false);
        
        let mtls_server_name = config_dict
            .get_item("mtls_server_name")?
            .map(|item| item.extract::<String>())
            .transpose()?;

        Ok(ClientConfig {
            connect_timeout_secs: connect_timeout / 1000, // 转换为秒
            request_timeout_secs: request_timeout / 1000, // 转换为秒
            max_idle_connections,
            enable_compression,
            enable_retry: true,
            max_retries: 3,
            user_agent,
            enable_http,
            http_user_agent,
            enable_grpc,
            development_mode,
            http2_only,
            http1_only,
            mtls_client_cert_path,
            mtls_client_key_path,
            mtls_ca_cert_path,
            mtls_skip_server_verification,
            mtls_server_name,
        })
    }
}

/// 注册客户端模块
pub fn register_client_module(_py: Python, parent_module: &PyModule) -> PyResult<()> {
    // 直接将 PyClientManager 添加到父模块，而不是创建子模块
    parent_module.add_class::<PyClientManager>()?;
    Ok(())
}