//! RAT Engine HTTP Python API 队列桥接模块
//! 
//! 基于真正的委托模式实现 Rust 和 Python 的彻底解耦
//! 
//! ## 架构设计
//! 1. **完全委托**：Python 层不直接处理 HTTP 请求，通过队列消息通信
//! 2. **消息驱动**：使用队列桥接适配器进行消息传递
//! 3. **连接管理**：由 Rust 层负责连接生命周期管理
//! 4. **业务分离**：Python 层只处理业务逻辑，不涉及传输层

use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyBytes};
use bytes::Bytes;
use crossbeam_queue::SegQueue;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use hyper::{Request, Response, Method, StatusCode};
use hyper::body::Incoming;
use http_body_util::{Full, BodyExt};
use std::pin::Pin;
use std::future::Future;
use crate::utils::logger::{info, warn, debug, error};

use crate::error::{RatResult, RatError};
use crate::server::http_request::HttpRequest;
use crate::python_api::codec::PyQuickCodec;

// 导入队列桥接适配器的消息类型
use crate::server::grpc_queue_bridge_adapter::{
    QueueBridgeAdapter, QueueBridgeConfig, TransportToEngineMessage, EngineToTransportMessage,
    RequestType, RequestData, ResponseData, ConnectionId
};

/// HTTP 主线程管理器
/// 
/// 基于队列桥接适配器实现真正的委托模式
#[pyclass]
pub struct PyHttpMainThread {
    /// 队列桥接适配器
    queue_bridge: Option<Arc<QueueBridgeAdapter>>,
    /// 运行状态
    running: Arc<std::sync::RwLock<bool>>,
    /// Python 回调函数
    callback: Option<PyObject>,
}

#[pymethods]
impl PyHttpMainThread {
    #[new]
    pub fn new() -> Self {
        Self {
            queue_bridge: None,
            running: Arc::new(std::sync::RwLock::new(false)),
            callback: None,
        }
    }

    /// 初始化队列桥接适配器
    pub fn initialize_queue_bridge(&mut self, max_connections: Option<usize>) -> PyResult<()> {
        let config = QueueBridgeConfig {
            queue_name: "rat_engine_http_bridge".to_string(),
            max_queue_size: 10000,
            message_timeout: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(300),
            cleanup_interval: Duration::from_secs(60),
        };
        
        let adapter = Arc::new(QueueBridgeAdapter::new(config));
        
        // 启动队列桥接适配器
        let adapter_clone = adapter.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Err(e) = adapter_clone.start().await {
                    error!("❌ [PyHttpMainThread] 启动队列桥接适配器失败: {}", e);
                } else {
                    info!("✅ [PyHttpMainThread] 队列桥接适配器启动成功");
                }
            });
        });
        
        self.queue_bridge = Some(adapter);
        Ok(())
    }

    /// 设置 Python 回调函数
    pub fn set_callback(&mut self, callback: PyObject) -> PyResult<()> {
        self.callback = Some(callback);
        Ok(())
    }

    /// 启动主 HTTP 线程
    pub fn start(&self) -> PyResult<()> {
        let mut running = self.running.write().unwrap();
        if *running {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "HTTP 主线程已经在运行"
            ));
        }
        
        *running = true;
        
        // 启动消息处理循环
        if let Some(bridge) = &self.queue_bridge {
            let bridge_clone = bridge.clone();
            let callback_clone = self.callback.clone();
            let running_clone = self.running.clone();
            
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    while *running_clone.read().unwrap() {
                        // 处理来自传输层的消息
                        let transport_queue = bridge_clone.get_transport_to_engine_queue();
                        while let Some(message) = transport_queue.pop() {
                            if let Some(ref callback) = callback_clone {
                                // 在单独的线程中调用 Python 回调，避免阻塞消息循环
                                let callback_clone = callback.clone();
                                let message_clone = message.clone();
                                tokio::task::spawn_blocking(move || {
                                    Python::with_gil(|py| {
                                        if let Err(e) = Self::handle_transport_message(py, &callback_clone, message_clone) {
                                            error!("❌ [PyHttpMainThread] 处理传输消息失败: {}", e);
                                        }
                                    });
                                });
                            }
                        }
                        
                        // 短暂等待，避免忙等待
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                });
            });
        }
        
        info!("🚀 [PyHttpMainThread] HTTP 主线程启动成功");
        Ok(())
    }

    /// 停止主 HTTP 线程
    pub fn stop(&self) -> PyResult<()> {
        let mut running = self.running.write().unwrap();
        *running = false;
        
        // 停止队列桥接适配器
        if let Some(bridge) = &self.queue_bridge {
            let bridge_clone = bridge.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = bridge_clone.stop().await {
                        error!("❌ [PyHttpMainThread] 停止队列桥接适配器失败: {}", e);
                    } else {
                        info!("🛑 [PyHttpMainThread] 队列桥接适配器停止成功");
                    }
                });
            });
        }
        
        Ok(())
    }

    /// 发送响应到传输层
    pub fn send_response(&self, connection_id: &str, request_id: &str, 
                        status_code: u16, headers: HashMap<String, String>, 
                        body: Vec<u8>) -> PyResult<()> {
        let bridge = self.queue_bridge.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "队列桥接适配器未初始化"
            ))?;

        let response = EngineToTransportMessage::SendResponse {
            connection_id: ConnectionId::from_string(connection_id.to_string()),
            request_id: request_id.to_string(),
            response: ResponseData {
                status_code,
                headers,
                body,
                grpc_status: None,
            },
        };

        // 使用队列直接推送消息
        let queue = bridge.get_engine_to_transport_queue();
        queue.push(response);
        
        Ok(())
    }

    /// 获取队列统计信息
    pub fn get_queue_stats(&self) -> PyResult<HashMap<String, u64>> {
        if let Some(bridge) = &self.queue_bridge {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let stats = rt.block_on(async {
                bridge.get_stats().await
            });
            Ok(stats)
        } else {
            Ok(HashMap::new())
        }
    }
}

impl PyHttpMainThread {
    /// 处理来自传输层的消息
    fn handle_transport_message(py: Python, callback: &PyObject, message: TransportToEngineMessage) -> PyResult<()> {
        match message {
            TransportToEngineMessage::ConnectionEstablished { connection_id, protocol, remote_addr, local_addr } => {
                // 创建连接信息字典
                let conn_info = PyDict::new(py);
                conn_info.set_item("type", "connection_established")?;
                conn_info.set_item("connection_id", connection_id.as_str())?;
                conn_info.set_item("protocol", protocol)?;
                conn_info.set_item("remote_addr", remote_addr)?;
                conn_info.set_item("local_addr", local_addr)?;
                
                callback.call1(py, (conn_info,))?;
            }
            TransportToEngineMessage::ConnectionClosed { connection_id, reason } => {
                // 创建连接关闭信息字典
                let conn_info = PyDict::new(py);
                conn_info.set_item("type", "connection_closed")?;
                conn_info.set_item("connection_id", connection_id.as_str())?;
                conn_info.set_item("reason", reason)?;
                
                callback.call1(py, (conn_info,))?;
            }
            TransportToEngineMessage::RequestReceived { connection_id, request_id, request_type, request_data } => {
                // 创建请求信息字典
                let req_info = PyDict::new(py);
                req_info.set_item("type", "request_received")?;
                req_info.set_item("connection_id", connection_id.as_str())?;
                req_info.set_item("request_id", request_id)?;
                req_info.set_item("request_type", format!("{:?}", request_type))?;
                
                // 添加请求数据
                if let Some(method) = &request_data.method {
                    req_info.set_item("method", method)?;
                }
                req_info.set_item("path", request_data.path)?;
                req_info.set_item("headers", request_data.headers)?;
                req_info.set_item("query_params", request_data.query_params)?;
                req_info.set_item("body", PyBytes::new(py, &request_data.body))?;
                
                callback.call1(py, (req_info,))?;
            }
            TransportToEngineMessage::StreamDataReceived { connection_id, request_id, data, is_end } => {
                // 创建流数据信息字典
                let stream_info = PyDict::new(py);
                stream_info.set_item("type", "stream_data_received")?;
                stream_info.set_item("connection_id", connection_id.as_str())?;
                stream_info.set_item("request_id", request_id)?;
                stream_info.set_item("data", PyBytes::new(py, &data))?;
                stream_info.set_item("is_end", is_end)?;
                
                callback.call1(py, (stream_info,))?;
            }
        }
        
        Ok(())
    }

    /// 获取队列桥接适配器的引用（供内部使用）
    #[doc(hidden)]
    pub(crate) fn get_queue_bridge(&self) -> Option<Arc<QueueBridgeAdapter>> {
        self.queue_bridge.clone()
    }

    /// 为处理器创建一个克隆（供内部使用）
    #[doc(hidden)]
    pub(crate) fn clone_for_handler(&self) -> PyHttpMainThread {
        PyHttpMainThread {
            queue_bridge: self.queue_bridge.clone(),
            running: self.running.clone(),
            callback: self.callback.clone(),
        }
    }
}

/// Python HTTP 处理器
pub struct PyHttpHandler {
    handler: PyObject,
    main_thread: Arc<PyHttpMainThread>,
    codec: PyQuickCodec,
    cache_middleware: Option<Arc<crate::server::cache_middleware_impl::CacheMiddlewareImpl>>,
}

impl PyHttpHandler {
    pub fn new(handler: PyObject, main_thread: Arc<PyHttpMainThread>, codec: PyQuickCodec) -> Self {
        Self { handler, main_thread, codec, cache_middleware: None }
    }
    
    /// 设置缓存中间件
    pub fn with_cache_middleware(mut self, cache_middleware: Arc<crate::server::cache_middleware_impl::CacheMiddlewareImpl>) -> Self {
        self.cache_middleware = Some(cache_middleware);
        self
    }

    /// 处理 HTTP 请求（队列桥接模式）
    pub fn handle_request(
        &self,
        req: HttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> {
        let handler = self.handler.clone();
        let bridge = self.main_thread.queue_bridge.clone();
        let codec = self.codec.clone();
        
        let cache_middleware = self.cache_middleware.clone();
        
        Box::pin(async move {
            let bridge = match bridge {
                Some(b) => b,
                None => {
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Full::new(Bytes::from("队列桥接适配器未初始化")))
                        .unwrap());
                }
            };
            
            
            // 生成连接ID和请求ID
            let connection_id = ConnectionId::new();
            let request_id = uuid::Uuid::new_v4().to_string();
            
            // 准备请求数据
            let request_data = RequestData {
                method: Some(req.method.to_string()),
                path: req.uri.path().to_string(),
                headers: req.headers.iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                body: req.body.to_vec(),
                query_params: req.query_params(),
                service: None,
                grpc_method: None,
            };
            
            // 发送请求到引擎层
            let request_message = TransportToEngineMessage::RequestReceived {
                connection_id: connection_id.clone(),
                request_id: request_id.clone(),
                request_type: RequestType::Http,
                request_data,
            };
            
            // 使用队列直接推送消息
            let transport_queue = bridge.get_transport_to_engine_queue();
            transport_queue.push(request_message);
            
            // 创建响应通道
            let (response_tx, response_rx) = tokio::sync::oneshot::channel::<ResponseData>();
            
            // 调用 Python 处理器（在单独线程中，避免阻塞）
            let handler_clone = handler.clone();
            let conn_id_str = connection_id.as_str().to_string();
            let req_id_str = request_id.clone();
            let req_clone = req.clone();
            let bridge_clone = bridge.clone();
            
            tokio::task::spawn_blocking(move || {
                Python::with_gil(|py| {
                    // 创建 HttpRequest Python 对象
                    // 获取真实的客户端地址信息
                    let remote_addr = req_clone.remote_addr
                        .map(|addr| addr.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    
                    let real_ip = req_clone.client_ip()
                        .map(|ip| ip.to_string())
                        .unwrap_or_else(|| {
                            // 如果无法获取真实IP，尝试从remote_addr提取IP部分
                            req_clone.remote_addr
                                .map(|addr| addr.ip().to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        });
                    
                    let py_request = crate::python_api::HttpRequest::new(
                            Some(req_clone.method.to_string()),
                            Some(req_clone.uri.path().to_string()),
                            Some(req_clone.uri.query().unwrap_or("").to_string()),
                            Some(req_clone.headers.iter().map(|(k, v)| {
                                (k.to_string(), v.to_str().unwrap_or("").to_string())
                            }).collect()),
                            Some(req_clone.body.to_vec()),
                            Some(remote_addr),
                            Some(real_ip),
                            Some(std::collections::HashMap::new())
                        );
                    
                    // 调用装饰器函数
                    let response_data = match handler_clone.call1(py, (py_request,)) {
                        Ok(result) => {
                            // 使用响应转换模块处理返回值
                            let (status_code, headers, body) = match crate::python_api::response_converter::convert_python_response(py, result) {
                                Ok((status, headers, body)) => (status, headers, body),
                                Err(_) => (500, std::collections::HashMap::new(), b"Internal Server Error".to_vec()),
                            };
                            
                            ResponseData {
                                status_code,
                                headers,
                                body,
                                grpc_status: None,
                            }
                        }
                        Err(e) => {
                            // 处理 Python 异常
                            let error_body = format!("Python handler error: {}", e).into_bytes();
                            ResponseData {
                                status_code: 500,
                                headers: std::collections::HashMap::new(),
                                body: error_body,
                                grpc_status: None,
                            }
                        }
                    };
                    
                    // 直接通过通道发送响应
                    if let Err(_) = response_tx.send(response_data) {
                        error!("🎯 [PyHttpHandler] 响应通道发送失败");
                    } else {
                        info!("🎯 [PyHttpHandler] 响应已通过通道发送");
                    }
                });
            });
            
            // 等待响应或超时（35秒，比客户端超时稍长）
            let response_data = match tokio::time::timeout(Duration::from_secs(35), response_rx).await {
                Ok(Ok(response)) => {
                    info!("🎯 [PyHttpHandler] 成功接收到响应，状态码: {}", response.status_code);
                    response
                },
                Ok(Err(_)) => {
                    error!("🎯 [PyHttpHandler] 响应通道接收失败");
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Full::new(Bytes::from("Response channel error")))
                        .unwrap());
                },
                Err(_) => {
                    warn!("🎯 [PyHttpHandler] 请求超时，返回408响应");
                    return Ok(Response::builder()
                        .status(StatusCode::REQUEST_TIMEOUT)
                        .body(Full::new(Bytes::from("Request timeout")))
                        .unwrap());
                }
            };
            
            // 构建 HTTP 响应
            let mut builder = Response::builder()
                .status(response_data.status_code);
            
            // 添加响应头
            for (key, value) in &response_data.headers {
                if let (Ok(header_name), Ok(header_value)) = (
                    hyper::header::HeaderName::from_bytes(key.as_bytes()),
                    hyper::header::HeaderValue::from_str(value)
                ) {
                    builder = builder.header(key, value);
                }
            }
            
            let http_response = builder
                .body(Full::new(Bytes::from(response_data.body.clone())))
                .unwrap();
            
            // 存储到缓存（如果启用）
            if let Some(cache_middleware) = &cache_middleware {
                // 创建用于缓存的请求对象，包含所有必要的头部信息
                let mut cache_request_builder = hyper::Request::builder()
                    .method(req.method.clone())
                    .uri(req.uri.clone());
                
                // 添加原始请求的头部信息
                for (key, value) in &req.headers {
                    cache_request_builder = cache_request_builder.header(key, value);
                }
                
                let cache_request = cache_request_builder
                    .body(())
                    .unwrap();
                
                // 创建用于缓存的响应对象，使用正确的BoxBody类型
                let mut cache_response_builder = hyper::Response::builder()
                    .status(response_data.status_code);
                
                // 添加响应头
                for (key, value) in &response_data.headers {
                    if let Ok(header_value) = hyper::header::HeaderValue::from_str(value) {
                        cache_response_builder = cache_response_builder.header(key, header_value);
                    }
                }
                
                let cache_response = cache_response_builder
                    .body(http_body_util::combinators::BoxBody::new(
                        Full::new(Bytes::from(response_data.body.clone()))
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                    ))
                    .unwrap();
                
                // 异步存储到缓存
                let cache_middleware_clone = cache_middleware.clone();
                tokio::spawn(async move {
                    if let Err(e) = cache_middleware_clone.process(&cache_request, cache_response).await {
                        error!("🎯 [PyHttpHandler] 缓存存储失败: {:?}", e);
                    } else {
                        info!("🎯 [PyHttpHandler] 响应已存储到缓存");
                    }
                });
            }
            
            Ok(http_response)
        })
    }
}