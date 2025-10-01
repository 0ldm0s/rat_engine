//! 委托模式的 HTTP 客户端实现
//! 
//! 采用类似 gRPC 双向流的委托架构，让 HTTP 客户端也能使用连接池和无锁模式

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use serde::{Serialize, Deserialize};
use bytes::Bytes;
use hyper::{Request, Response, Method, Uri, StatusCode};
use hyper::header::HeaderMap;
use http_body_util::Full;
use crate::error::{RatError, RatResult};
use crate::client::connection_pool::ClientConnectionPool;
use crate::client::http_client::{RatHttpClient, RatHttpResponse};
use crate::utils::logger::{info, warn, error};

/// HTTP 请求处理器特征（委托模式）
/// 
/// 类似 gRPC 的 ClientBidirectionalHandler，但适用于 HTTP 请求场景
#[async_trait::async_trait]
pub trait HttpRequestHandler: Send + Sync {
    /// 请求数据类型
    type RequestData: Serialize + Send + Sync;
    /// 响应数据类型
    type ResponseData: for<'de> Deserialize<'de> + Send + Sync;

    /// 处理请求开始事件
    async fn on_request_start(&self, context: &HttpRequestContext) -> Result<(), String>;

    /// 处理响应接收事件
    async fn on_response_received(
        &self,
        response: RatHttpResponse,
        context: &HttpRequestContext,
    ) -> Result<(), String>;

    /// 处理请求错误事件
    async fn on_error(&self, context: &HttpRequestContext, error: String);

    /// 处理请求完成事件
    async fn on_completed(&self, context: &HttpRequestContext);
}

/// HTTP 请求上下文
#[derive(Debug, Clone)]
pub struct HttpRequestContext {
    /// 请求ID
    request_id: u64,
    /// 请求方法
    method: Method,
    /// 请求URI
    uri: Uri,
    /// 请求头
    headers: HeaderMap,
    /// 发送端
    sender: HttpRequestSender,
}

impl HttpRequestContext {
    /// 创建新的请求上下文
    pub fn new(
        request_id: u64,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        sender: HttpRequestSender,
    ) -> Self {
        Self {
            request_id,
            method,
            uri,
            headers,
            sender,
        }
    }

    /// 获取请求ID
    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    /// 获取请求方法
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// 获取请求URI
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// 获取请求头
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// 获取发送端
    pub fn sender(&self) -> &HttpRequestSender {
        &self.sender
    }
}

/// HTTP 请求发送端委托接口
/// 
/// 通过委托模式，用户不需要直接持有发送端，而是通过这个接口发送数据
#[derive(Debug, Clone)]
pub struct HttpRequestSender {
    /// 内部发送通道
    inner: mpsc::UnboundedSender<Bytes>,
}

impl HttpRequestSender {
    /// 创建新的发送端
    pub fn new(inner: mpsc::UnboundedSender<Bytes>) -> Self {
        Self { inner }
    }

    /// 发送原始字节数据
    pub async fn send_raw(&self, data: Vec<u8>) -> Result<(), String> {
        self.inner.send(Bytes::from(data))
            .map_err(|e| format!("发送失败: {}", e))
    }
    
    /// 发送JSON数据
    pub async fn send_json<T>(&self, data: &T) -> Result<(), String>
    where
        T: Serialize,
    {
        let json_bytes = serde_json::to_vec(data)
            .map_err(|e| format!("JSON序列化失败: {}", e))?;
        
        self.send_raw(json_bytes).await
    }
}

/// 委托模式的 HTTP 请求管理器
/// 
/// 负责管理所有 HTTP 请求，类似 gRPC 的委托管理器
#[derive(Debug)]
pub struct HttpRequestManager {
    /// HTTP 客户端弱引用（避免循环依赖）
    http_client: Weak<RatHttpClient>,
    /// 连接池引用
    connection_pool: Arc<ClientConnectionPool>,
    /// 活跃请求映射
    active_requests: Arc<RwLock<HashMap<u64, HttpRequestInfo>>>,
    /// 请求ID计数器
    request_id_counter: std::sync::atomic::AtomicU64,
}

/// HTTP 请求信息
#[derive(Debug)]
pub struct HttpRequestInfo {
    /// 请求ID
    pub request_id: u64,
    /// 连接ID
    pub connection_id: String,
    /// 请求任务句柄
    pub request_task: Option<tokio::task::JoinHandle<()>>,
    /// 发送端通道
    pub sender_tx: mpsc::UnboundedSender<Bytes>,
}

impl HttpRequestManager {
    /// 创建新的HTTP请求管理器
    pub fn new(
        http_client: Weak<RatHttpClient>,
        connection_pool: Arc<ClientConnectionPool>,
    ) -> Self {
        Self {
            http_client,
            connection_pool,
            active_requests: Arc::new(RwLock::new(HashMap::new())),
            request_id_counter: std::sync::atomic::AtomicU64::new(1),
        }
    }
    
    /// 更新HTTP客户端的弱引用
    pub fn update_http_client(&mut self, http_client: Weak<RatHttpClient>) {
        self.http_client = http_client;
    }
    
    /// 创建占位符实例
    pub fn placeholder() -> Self {
        use std::sync::Arc;
        use crate::client::connection_pool::ClientConnectionPool;
        
        let pool_config = crate::client::connection_pool::ConnectionPoolConfig::default();
        let connection_pool = Arc::new(ClientConnectionPool::new(pool_config));
        
        Self {
            http_client: Weak::new(), // 空的弱引用
            connection_pool,
            active_requests: Arc::new(RwLock::new(HashMap::new())),
            request_id_counter: std::sync::atomic::AtomicU64::new(1),
        }
    }


    /// 发送委托模式的 HTTP 请求
    pub async fn send_request_delegated<H>(
        &self,
        method: Method,
        uri: Uri,
        headers: Option<HeaderMap>,
        body: Option<Bytes>,
        handler: Arc<H>,
    ) -> RatResult<u64>
    where
        H: HttpRequestHandler + 'static,
    {
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        info!("🔗 创建委托模式HTTP请求: {} {}, 请求ID: {}", method, uri, request_id);
        
        // 1. 从连接池获取连接（复用 gRPC 的连接池）
        let connection = self.connection_pool.get_connection(&uri).await
            .map_err(|e| RatError::NetworkError(format!("获取连接失败: {}", e)))?;

        // 2. 创建发送/接收通道
        let (send_tx, send_rx) = mpsc::unbounded_channel::<Bytes>();

        // 创建请求上下文
        let context = HttpRequestContext::new(
            request_id,
            method.clone(),
            uri.clone(),
            headers.clone().unwrap_or_default(),
            HttpRequestSender::new(send_tx.clone()),
        );

        // 3. 启动请求处理任务
        let connection_id = connection.connection_id.clone();
        let connection_pool = self.connection_pool.clone();
        
        // 尝试升级弱引用为强引用
        let http_client = match self.http_client.upgrade() {
            Some(client) => client,
            None => {
                return Err(RatError::NetworkError("HTTP客户端已被释放".to_string()));
            }
        };
        
        let request_task = {
            let handler_clone = handler.clone();
            let context_clone = context.clone();
            tokio::spawn(async move {
                // 通知处理器请求开始
                if let Err(e) = handler_clone.on_request_start(&context_clone).await {
                    error!("❌ [委托模式] 请求开始处理失败: {}", e);
                    handler_clone.on_error(&context_clone, e).await;
                    return;
                }
                
                // 发送 HTTP 请求
                match http_client.send_request_with_protocol(
                    method,
                    uri,
                    body,
                    headers,
                    None, // 使用默认协议
                ).await {
                    Ok(response) => {
                        info!("✅ [委托模式] HTTP请求成功，状态码: {}", response.status);
                        
                        // 通知处理器响应接收
                        if let Err(e) = handler_clone.on_response_received(response, &context_clone).await {
                            error!("❌ [委托模式] 响应处理失败: {}", e);
                            handler_clone.on_error(&context_clone, e).await;
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("HTTP请求失败: {}", e);
                        error!("❌ [委托模式] {}", error_msg);
                        handler_clone.on_error(&context_clone, error_msg).await;
                    }
                }
                
                // 通知处理器请求完成
                handler_clone.on_completed(&context_clone).await;
                
                // 释放连接回连接池
                connection_pool.release_connection(&connection_id);
                info!("HTTP请求完成，连接已释放");
            })
        };

        // 存储请求信息
        let request_info = HttpRequestInfo {
            request_id,
            connection_id: connection.connection_id.clone(),
            request_task: Some(request_task),
            sender_tx: send_tx,
        };
        
        self.store_request_info(request_info).await;
        
        info!("✅ 委托模式HTTP请求 {} 创建完成", request_id);
        
        Ok(request_id)
    }

    /// 存储请求信息
    async fn store_request_info(&self, request_info: HttpRequestInfo) {
        let mut requests = self.active_requests.write().await;
        requests.insert(request_info.request_id, request_info);
    }

    /// 获取请求上下文
    pub async fn get_request_context(&self, request_id: u64) -> Option<HttpRequestContext> {
        let requests = self.active_requests.read().await;
        if let Some(request_info) = requests.get(&request_id) {
            // 注意：这里需要重新构建上下文，因为我们只存储了基本信息
            // 在实际使用中，可能需要调整存储策略
            None // 暂时返回 None，需要根据实际需求调整
        } else {
            None
        }
    }

    /// 取消请求
    pub async fn cancel_request(&self, request_id: u64) -> RatResult<()> {
        info!("🛑 取消委托模式HTTP请求: {}", request_id);
        
        let mut requests = self.active_requests.write().await;
        if let Some(request_info) = requests.remove(&request_id) {
            // 取消请求任务
            if let Some(task) = request_info.request_task {
                task.abort();
            }
            
            info!("✅ 委托模式HTTP请求 {} 已取消", request_id);
        } else {
            warn!("⚠️ 请求 {} 不存在或已完成", request_id);
        }
        
        Ok(())
    }

    /// 获取活跃请求数量
    pub async fn active_request_count(&self) -> usize {
        let requests = self.active_requests.read().await;
        requests.len()
    }

    /// 获取活跃请求ID列表
    pub async fn get_active_request_ids(&self) -> Vec<u64> {
        let requests = self.active_requests.read().await;
        requests.keys().copied().collect()
    }

    /// 关闭所有请求
    pub async fn close_all_requests(&self) -> RatResult<()> {
        info!("🛑 关闭所有委托模式HTTP请求");
        
        let mut requests = self.active_requests.write().await;
        for (request_id, request_info) in requests.drain() {
            if let Some(task) = request_info.request_task {
                task.abort();
            }
            info!("✅ 请求 {} 已关闭", request_id);
        }
        
        Ok(())
    }
}