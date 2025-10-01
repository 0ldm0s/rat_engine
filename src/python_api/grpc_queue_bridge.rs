//! RAT Engine gRPC Python API 队列桥接模块
//! 
//! 基于真正的委托模式实现 Rust 和 Python 的彻底解耦
//! 
//! ## 架构设计
//! 1. **完全委托**：Python 层不直接处理 gRPC 流，通过队列消息通信
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
use rand;
use dashmap::DashMap;
use futures_util::{StreamExt, Stream};
use async_trait::async_trait;
use async_stream;
use crate::utils::logger::{info, warn, debug, error};

use crate::error::{RatResult, RatError};
use crate::server::grpc_handler::{
    UnaryHandler, ServerStreamHandler, ClientStreamHandler, BidirectionalHandler
};

// 导入 mammoth_transport 的消息类型
use crate::server::grpc_queue_bridge_adapter::{
    QueueBridgeAdapter, QueueBridgeConfig, TransportToEngineMessage, EngineToTransportMessage,
    RequestType, RequestData, ResponseData, GrpcStatusInfo, ConnectionId
};

/// 主 gRPC 线程管理器
/// 
/// 基于队列桥接适配器实现真正的委托模式
#[pyclass]
pub struct PyGrpcMainThread {
    /// 队列桥接适配器
    queue_bridge: Option<Arc<QueueBridgeAdapter>>,
    /// 运行状态
    running: Arc<std::sync::RwLock<bool>>,
    /// Python 回调函数
    callback: Option<PyObject>,
}

#[pymethods]
impl PyGrpcMainThread {
    #[new]
    pub fn new() -> Self {
        Self {
            queue_bridge: None,
            running: Arc::new(std::sync::RwLock::new(false)),
            callback: None,
        }
    }

    /// 设置 Python 回调函数
    pub fn set_callback(&mut self, callback: PyObject) {
        self.callback = Some(callback);
    }

    /// 初始化队列桥接适配器
    pub fn initialize_queue_bridge(&mut self, max_connections: Option<usize>) -> PyResult<()> {
        let config = QueueBridgeConfig {
            queue_name: "rat_engine_grpc_bridge".to_string(),
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
                    error!("❌ [PyGrpcMainThread] 启动队列桥接适配器失败: {}", e);
                } else {
                    info!("✅ [PyGrpcMainThread] 队列桥接适配器启动成功");
                }
            });
        });
        
        self.queue_bridge = Some(adapter);
        Ok(())
    }

    /// 启动主 gRPC 线程
    pub fn start(&self) -> PyResult<()> {
        let mut running = self.running.write().unwrap();
        if *running {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "主线程已经在运行中"
            ));
        }
        *running = true;
        
        // 启动消息处理循环
        self.start_message_loop();
        
        Ok(())
    }

    /// 停止主 gRPC 线程
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
                        error!("❌ [PyGrpcMainThread] 停止队列桥接适配器失败: {}", e);
                    } else {
                        info!("🛑 [PyGrpcMainThread] 队列桥接适配器停止成功");
                    }
                });
            });
        }
        
        Ok(())
    }

    /// 发送响应到传输层
    pub fn send_response(&self, connection_id: &str, request_id: &str, data: Vec<u8>, 
                        status_code: Option<u32>, status_message: Option<String>) -> PyResult<()> {
        let bridge = self.queue_bridge.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "队列桥接适配器未初始化"
            ))?;

        let response = EngineToTransportMessage::SendResponse {
            connection_id: ConnectionId::from_string(connection_id.to_string()),
            request_id: request_id.to_string(),
            response: ResponseData {
                status_code: 200,
                headers: HashMap::new(),
                body: data,
                grpc_status: Some(GrpcStatusInfo {
                    code: status_code.unwrap_or(0),
                    message: status_message.unwrap_or_default(),
                    details: Vec::new(),
                }),
            },
        };

        // 使用队列直接推送消息
        let queue = bridge.get_engine_to_transport_queue();
        queue.push(response);
        
        Ok(())
    }

    /// 发送流数据到传输层
    pub fn send_stream_data(&self, connection_id: &str, request_id: &str, 
                           data: Vec<u8>, is_end: bool) -> PyResult<()> {
        let bridge = self.queue_bridge.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "队列桥接适配器未初始化"
            ))?;

        let stream_msg = EngineToTransportMessage::SendStreamData {
            connection_id: ConnectionId::from_string(connection_id.to_string()),
            request_id: request_id.to_string(),
            data,
            is_end,
        };

        // 使用队列直接推送消息
        let queue = bridge.get_engine_to_transport_queue();
        queue.push(stream_msg);
        
        Ok(())
    }

    /// 从传输层接收消息
    pub fn receive_from_transport(&self) -> PyResult<Option<String>> {
        let bridge = self.queue_bridge.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "队列桥接适配器未初始化"
            ))?;

        // 使用队列直接拉取消息
        let queue = bridge.get_transport_to_engine_queue();
        if let Some(message) = queue.pop() {
            let json = serde_json::to_string(&message)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!("消息序列化失败: {}", e)
                ))?;
            return Ok(Some(json));
        }
        Ok(None)
    }

    /// 获取队列桥接适配器的统计信息
    pub fn get_bridge_stats(&self) -> PyResult<String> {
        let bridge = self.queue_bridge.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "队列桥接适配器未初始化"
            ))?;

        // 创建基本统计信息
        let mut stats = std::collections::HashMap::new();
        let transport_queue = bridge.get_transport_to_engine_queue();
        let engine_queue = bridge.get_engine_to_transport_queue();
        
        stats.insert("transport_to_engine_queue_size".to_string(), transport_queue.len() as u64);
        stats.insert("engine_to_transport_queue_size".to_string(), engine_queue.len() as u64);
        
        serde_json::to_string(&stats)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("统计信息序列化失败: {}", e)
            ))
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> PyResult<bool> {
        Ok(*self.running.read().unwrap())
    }

    /// 启动消息处理循环
    fn start_message_loop(&self) {
        let running = self.running.clone();
        let bridge = self.queue_bridge.clone();
        let callback = self.callback.clone();

        std::thread::spawn(move || {
            info!("🚀 [PyGrpcMainThread] 消息处理循环启动");
            
            while *running.read().unwrap() {
                if let Some(ref bridge) = bridge {
                    // 处理来自传输层的消息
                    let queue = bridge.get_transport_to_engine_queue();
                    if let Some(message) = queue.pop() {
                        info!("📥 [PyGrpcMainThread] 收到传输层消息");
                        
                        // 如果设置了回调函数，调用 Python 回调
                        if let Some(ref callback) = callback {
                            Python::with_gil(|py| {
                                if let Ok(json) = serde_json::to_string(&message) {
                                    let _ = callback.call1(py, (json,));
                                }
                            });
                        }
                    }
                }
                
                // 短暂休眠避免 CPU 空转
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            
            info!("🏁 [PyGrpcMainThread] 消息处理循环结束");
        });
    }

}

impl PyGrpcMainThread {
    /// 获取队列桥接适配器的引用（供内部使用）
    #[doc(hidden)]
    pub(crate) fn get_queue_bridge(&self) -> Option<Arc<QueueBridgeAdapter>> {
        self.queue_bridge.clone()
    }

    /// 为处理器创建一个克隆（供内部使用）
    #[doc(hidden)]
    pub(crate) fn clone_for_handler(&self) -> PyGrpcMainThread {
        PyGrpcMainThread {
            queue_bridge: self.queue_bridge.clone(),
            running: self.running.clone(),
            callback: self.callback.clone(),
        }
    }
}

// 旧的队列桥接类已移除，现在使用真正的队列桥接适配器

/// Python gRPC 一元处理器
pub struct PyGrpcUnaryHandler {
    handler: PyObject,
    main_thread: Arc<PyGrpcMainThread>,
}

impl PyGrpcUnaryHandler {
    pub fn new(handler: PyObject, main_thread: Arc<PyGrpcMainThread>) -> Self {
        Self { handler, main_thread }
    }
}

impl crate::server::grpc_handler::UnaryHandler for PyGrpcUnaryHandler {
    fn handle(
        &self,
        request: crate::server::grpc_types::GrpcRequest<Vec<u8>>,
        context: crate::server::grpc_types::GrpcContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<crate::server::grpc_types::GrpcResponse<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>> {
        let handler = self.handler.clone();
        let bridge = self.main_thread.queue_bridge.clone();
        
        Box::pin(async move {
            let bridge = bridge.ok_or_else(|| crate::server::grpc_types::GrpcError::Internal("队列桥接适配器未初始化".to_string()))?;
            
            // 生成连接ID和请求ID
            let connection_id = ConnectionId::new();
            let request_id = uuid::Uuid::new_v4().to_string();
            
            // 克隆请求数据以便后续使用
            let request_data_clone = request.data.clone();
            
            // 发送请求到引擎层
            let request_message = TransportToEngineMessage::RequestReceived {
                connection_id: connection_id.clone(),
                request_id: request_id.clone(),
                request_type: RequestType::GrpcUnary,
                request_data: RequestData {
                    method: None,
                    path: "/grpc".to_string(),
                    headers: HashMap::new(),
                    body: request.data,
                    service: None,
                    grpc_method: None,
                    query_params: HashMap::new(),
                },
            };
            
            // 使用队列直接推送消息
            let queue = bridge.get_transport_to_engine_queue();
            queue.push(request_message);
            
            // 调用 Python 处理器
            let response_data = Python::with_gil(|py| -> Result<Vec<u8>, crate::server::grpc_types::GrpcError> {
                // 准备参数
                let request_data = PyBytes::new(py, &request_data_clone);
                let metadata = PyDict::new(py);
                for (key, value) in &request.metadata {
                    metadata.set_item(key, value).map_err(|e| {
                        crate::server::grpc_types::GrpcError::Internal(format!("设置元数据失败: {}", e))
                    })?;
                }
                
                let context_dict = PyDict::new(py);
                context_dict.set_item("method", format!("{:?}", context.method)).map_err(|e| {
                    crate::server::grpc_types::GrpcError::Internal(format!("设置上下文失败: {}", e))
                })?;
                if let Some(addr) = context.remote_addr {
                    context_dict.set_item("peer_addr", addr.to_string()).map_err(|e| {
                        crate::server::grpc_types::GrpcError::Internal(format!("设置对端地址失败: {}", e))
                    })?;
                }
                
                // 调用 Python 处理器
                let args = pyo3::types::PyTuple::new(py, &[request_data.as_ref(), metadata.as_ref(), context_dict.as_ref()]);
                let result = handler.call(py, args, None).map_err(|e| {
                    crate::server::grpc_types::GrpcError::Internal(format!("Python 处理器调用失败: {}", e))
                })?;
                
                // 提取响应数据
                let response_bytes = result.downcast::<PyBytes>(py).map_err(|e| {
                    crate::server::grpc_types::GrpcError::Internal(format!("Python 处理器返回值不是 bytes 类型: {}", e))
                })?;
                
                Ok(response_bytes.as_bytes().to_vec())
            })?;
            
            // 将响应推送到队列中，供 DelegatedUnaryHandler 等待
            let response_message = EngineToTransportMessage::SendResponse {
                connection_id: connection_id.clone(),
                request_id: request_id.clone(),
                response: ResponseData {
                    status_code: 200,
                    headers: HashMap::new(),
                    body: response_data.clone(),
                    grpc_status: Some(GrpcStatusInfo {
                        code: 0, // OK
                        message: "Success".to_string(),
                        details: Vec::new(),
                    }),
                },
            };
            
            // 推送响应到队列
            let engine_queue = bridge.get_engine_to_transport_queue();
            engine_queue.push(response_message);
            
            // 直接返回响应（为了兼容性）
            return Ok(crate::server::grpc_types::GrpcResponse {
                status: 0, // OK
                message: "Success".to_string(),
                data: response_data,
                metadata: HashMap::new(),
            });
            
            // 注意：响应已经在上面直接返回了
        })
    }
}

/// Python gRPC 服务端流处理器
pub struct PyGrpcServerStreamHandler {
    handler: PyObject,
    main_thread: Arc<PyGrpcMainThread>,
}

impl PyGrpcServerStreamHandler {
    pub fn new(handler: PyObject, main_thread: Arc<PyGrpcMainThread>) -> Self {
        Self { handler, main_thread }
    }
}

impl crate::server::grpc_handler::ServerStreamHandler for PyGrpcServerStreamHandler {
    fn handle(
        &self,
        request: crate::server::grpc_types::GrpcRequest<Vec<u8>>,
        context: crate::server::grpc_types::GrpcContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<std::pin::Pin<Box<dyn Stream<Item = Result<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>>, crate::server::grpc_types::GrpcError>> + Send>> {
        let handler = self.handler.clone();
        let bridge = self.main_thread.queue_bridge.clone();
        
        Box::pin(async move {
            let bridge = bridge.ok_or_else(|| crate::server::grpc_types::GrpcError::Internal("队列桥接适配器未初始化".to_string()))?;
            
            // 生成连接ID和请求ID
            let connection_id = ConnectionId::new();
            let request_id = uuid::Uuid::new_v4().to_string();
            
            // 发送请求到引擎层
            let request_message = TransportToEngineMessage::RequestReceived {
                connection_id: connection_id.clone(),
                request_id: request_id.clone(),
                request_type: RequestType::GrpcServerStreaming,
                request_data: RequestData {
                    method: None,
                    path: "/grpc".to_string(),
                    headers: HashMap::new(),
                    body: request.data,
                    service: None,
                    grpc_method: None,
                    query_params: HashMap::new(),
                },
            };
            
            // 使用队列直接推送消息
            let queue = bridge.get_transport_to_engine_queue();
            queue.push(request_message);
            
            // 调用 Python 处理器
            Python::with_gil(|py| {
                let args = pyo3::types::PyTuple::new(py, &[connection_id.as_str().to_string(), request_id.clone()]);
                let _ = handler.call(py, args, None);
            });
            
            // 创建流，从队列中读取数据
            let conn_id = connection_id.clone();
            let req_id = request_id.clone();
            let engine_queue = bridge.get_engine_to_transport_queue();
            
            let stream = async_stream::stream! {
                let mut sequence = 0u64;
                loop {
                    if let Some(message) = engine_queue.pop() {
                        match message {
                            EngineToTransportMessage::SendStreamData { connection_id: msg_conn_id, request_id: msg_req_id, data, is_end } => {
                                if msg_conn_id == conn_id && msg_req_id == req_id {
                                    let stream_message = crate::server::grpc_types::GrpcStreamMessage {
                                        id: msg_req_id.parse::<u64>().unwrap_or(rand::random::<u64>()),
                                        stream_id: 1,
                                        sequence,
                                        end_of_stream: is_end,
                                        data,
                                        metadata: HashMap::new(),
                                    };
                                    sequence += 1;
                                    yield Ok(stream_message);
                                    
                                    if is_end {
                                        break;
                                    }
                                }
                            }
                            _ => continue,
                        }
                    } else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            };
            
            Ok(Box::pin(stream) as std::pin::Pin<Box<dyn Stream<Item = Result<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>>)
        })
    }
}

/// Python gRPC 客户端流处理器
pub struct PyGrpcClientStreamHandler {
    handler: PyObject,
    main_thread: Arc<PyGrpcMainThread>,
}

impl PyGrpcClientStreamHandler {
    pub fn new(handler: PyObject, main_thread: Arc<PyGrpcMainThread>) -> Self {
        Self { handler, main_thread }
    }
}

impl crate::server::grpc_handler::ClientStreamHandler for PyGrpcClientStreamHandler {
    fn handle(
        &self,
        mut request_stream: std::pin::Pin<Box<dyn Stream<Item = Result<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>>,
        context: crate::server::grpc_types::GrpcContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<crate::server::grpc_types::GrpcResponse<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>> {
        let handler = self.handler.clone();
        let bridge = self.main_thread.queue_bridge.clone();
        
        Box::pin(async move {
            let bridge = bridge.ok_or_else(|| crate::server::grpc_types::GrpcError::Internal("队列桥接适配器未初始化".to_string()))?;
            
            // 生成连接ID
            let connection_id = ConnectionId::new();
            let request_id = uuid::Uuid::new_v4().to_string();
            
            // 调用 Python 处理器
            Python::with_gil(|py| {
                let args = pyo3::types::PyTuple::new(py, &[connection_id.as_str().to_string(), request_id.clone()]);
                let _ = handler.call(py, args, None);
            });
            
            // 处理输入流，转发到引擎层
            let transport_queue = bridge.get_transport_to_engine_queue();
            while let Some(stream_item) = request_stream.next().await {
                match stream_item {
                    Ok(stream_message) => {
                        let message = TransportToEngineMessage::StreamDataReceived {
                            connection_id: connection_id.clone(),
                            request_id: request_id.clone(),
                            data: stream_message.data.into(),
                            is_end: stream_message.end_of_stream,
                        };
                        transport_queue.push(message);
                    }
                    Err(e) => return Err(e),
                }
            }
            
            // 等待最终响应
            let timeout = std::time::Duration::from_secs(30);
            let start = std::time::Instant::now();
            let engine_queue = bridge.get_engine_to_transport_queue();
            
            while start.elapsed() < timeout {
                if let Some(message) = engine_queue.pop() {
                    match message {
                        EngineToTransportMessage::SendResponse { connection_id: msg_conn_id, request_id: msg_req_id, response } => {
                            if msg_conn_id == connection_id && msg_req_id == request_id {
                                return Ok(crate::server::grpc_types::GrpcResponse {
                                    status: response.grpc_status.as_ref().map(|s| s.code).unwrap_or(0),
                                    message: response.grpc_status.as_ref().map(|s| s.message.clone()).unwrap_or_default(),
                                    data: response.body,
                                    metadata: HashMap::new(),
                                });
                            }
                        }
                        _ => continue,
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            Err(crate::server::grpc_types::GrpcError::Internal("Python 处理器超时".to_string()))
        })
    }
}

/// Python gRPC 双向流处理器
pub struct PyGrpcBidirectionalHandler {
    handler: PyObject,
    main_thread: Arc<PyGrpcMainThread>,
}

impl PyGrpcBidirectionalHandler {
    pub fn new(handler: PyObject, main_thread: Arc<PyGrpcMainThread>) -> Self {
        Self { handler, main_thread }
    }
}

impl crate::server::grpc_handler::BidirectionalHandler for PyGrpcBidirectionalHandler {
    fn handle(
        &self,
        mut request_stream: std::pin::Pin<Box<dyn Stream<Item = Result<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>>,
        context: crate::server::grpc_types::GrpcContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<std::pin::Pin<Box<dyn Stream<Item = Result<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>>, crate::server::grpc_types::GrpcError>> + Send>> {
        let handler = self.handler.clone();
        let bridge = self.main_thread.queue_bridge.clone();
        let main_thread = self.main_thread.clone();
        
        Box::pin(async move {
            let bridge = bridge.ok_or_else(|| crate::server::grpc_types::GrpcError::Internal("队列桥接适配器未初始化".to_string()))?;
            
            // 生成连接ID
            let connection_id = ConnectionId::new();
            let request_id = uuid::Uuid::new_v4().to_string();
            
            // 创建 sender 和 receiver 对象
            let sender = PyGrpcBidirectionalSender::new(
                connection_id.as_str().to_string(),
                request_id.clone(),
                main_thread.clone()
            );
            let receiver = PyGrpcBidirectionalReceiver::new(
                connection_id.as_str().to_string(),
                request_id.clone(),
                main_thread.clone()
            );
            
            // 在单独的线程中调用 Python 处理器，避免阻塞异步任务
            let handler_clone = handler.clone();
            let conn_id_for_handler = connection_id.as_str().to_string();
            let req_id_for_handler = request_id.clone();
            tokio::task::spawn_blocking(move || {
                Python::with_gil(|py| {
                    // 创建 context 字典
                    let context_dict = pyo3::types::PyDict::new(py);
                    context_dict.set_item("connection_id", conn_id_for_handler).unwrap();
                    context_dict.set_item("request_id", req_id_for_handler).unwrap();
                    
                    let sender_py = Py::new(py, sender).unwrap();
                    let receiver_py = Py::new(py, receiver).unwrap();
                    let _ = handler_clone.call1(py, (context_dict, sender_py, receiver_py));
                });
            });
            
            // 启动输入流处理任务
            let input_connection_id = connection_id.clone();
            let input_request_id = request_id.clone();
            let input_transport_queue = bridge.get_transport_to_engine_queue();
            tokio::spawn(async move {
                while let Some(stream_item) = request_stream.next().await {
                    match stream_item {
                        Ok(stream_message) => {
                            let message = TransportToEngineMessage::StreamDataReceived {
                            connection_id: input_connection_id.clone(),
                            request_id: input_request_id.clone(),
                            data: stream_message.data.into(),
                            is_end: stream_message.end_of_stream,
                        };
                            input_transport_queue.push(message);
                        }
                        Err(_) => break,
                    }
                }
            });
            
            // 创建输出流
            let conn_id = connection_id.clone();
            let req_id = request_id.clone();
            let engine_queue = bridge.get_engine_to_transport_queue();
            
            let stream = async_stream::stream! {
                let mut sequence = 0u64;
                loop {
                    if let Some(message) = engine_queue.pop() {
                        match message {
                            EngineToTransportMessage::SendStreamData { connection_id: msg_conn_id, request_id: msg_req_id, data, is_end } => {
                                if msg_conn_id == conn_id && msg_req_id == req_id {
                                    let stream_message = crate::server::grpc_types::GrpcStreamMessage {
                                        id: msg_req_id.parse::<u64>().unwrap_or(rand::random::<u64>()),
                                        stream_id: 1,
                                        sequence,
                                        end_of_stream: is_end,
                                        data,
                                        metadata: HashMap::new(),
                                    };
                                    sequence += 1;
                                    yield Ok(stream_message);
                                    
                                    if is_end {
                                        break;
                                    }
                                }
                            }
                            _ => continue,
                        }
                    } else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            };
            
            Ok(Box::pin(stream) as std::pin::Pin<Box<dyn Stream<Item = Result<crate::server::grpc_types::GrpcStreamMessage<Vec<u8>>, crate::server::grpc_types::GrpcError>> + Send>>)
        })
    }
}

/// gRPC 双向流发送器
#[pyclass(name = "GrpcBidirectionalSender")]
pub struct PyGrpcBidirectionalSender {
    connection_id: String,
    request_id: String,
    main_thread: Arc<PyGrpcMainThread>,
}

#[pymethods]
impl PyGrpcBidirectionalSender {
    /// 发送 JSON 数据
    pub fn send_json(&self, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            // 将 Python 对象转换为 JSON 字符串
            let json_module = py.import("json")?;
            let json_str: String = json_module
                .getattr("dumps")?
                .call1((data,))?
                .extract()?;
            
            // 发送数据
            self.main_thread.send_stream_data(
                &self.connection_id,
                &self.request_id,
                json_str.into_bytes(),
                false
            )
        })
    }
    
    /// 发送原始字节数据
    pub fn send_bytes(&self, data: &PyBytes) -> PyResult<()> {
        self.main_thread.send_stream_data(
            &self.connection_id,
            &self.request_id,
            data.as_bytes().to_vec(),
            false
        )
    }
    
    /// 发送字符串数据
    pub fn send_string(&self, data: String) -> PyResult<()> {
        self.main_thread.send_stream_data(
            &self.connection_id,
            &self.request_id,
            data.into_bytes(),
            false
        )
    }
    
    /// 结束流
    pub fn end_stream(&self) -> PyResult<()> {
        self.main_thread.send_stream_data(
            &self.connection_id,
            &self.request_id,
            Vec::new(),
            true
        )
    }
    
    /// 发送错误
    pub fn send_error(&self, error_message: String) -> PyResult<()> {
        self.main_thread.send_response(
            &self.connection_id,
            &self.request_id,
            error_message.clone().into_bytes(),
            Some(13), // gRPC INTERNAL 错误码
            Some(error_message)
        )
    }
}

impl PyGrpcBidirectionalSender {
    pub fn new(connection_id: String, request_id: String, main_thread: Arc<PyGrpcMainThread>) -> Self {
        Self {
            connection_id,
            request_id,
            main_thread,
        }
    }
}

/// gRPC 双向流接收器
#[pyclass(name = "GrpcBidirectionalReceiver")]
pub struct PyGrpcBidirectionalReceiver {
    connection_id: String,
    request_id: String,
    main_thread: Arc<PyGrpcMainThread>,
    message_callback: Option<PyObject>,
    error_callback: Option<PyObject>,
    end_callback: Option<PyObject>,
    receiving: Arc<std::sync::RwLock<bool>>,
}

#[pymethods]
impl PyGrpcBidirectionalReceiver {
    /// 设置消息回调
    pub fn set_message_callback(&mut self, callback: PyObject) {
        self.message_callback = Some(callback);
    }
    
    /// 设置错误回调
    pub fn set_error_callback(&mut self, callback: PyObject) {
        self.error_callback = Some(callback);
    }
    
    /// 设置结束回调
    pub fn set_end_callback(&mut self, callback: PyObject) {
        self.end_callback = Some(callback);
    }
    
    /// 启动接收循环
    pub fn start_receiving(&self) -> PyResult<()> {
        let mut receiving = self.receiving.write().unwrap();
        if *receiving {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "接收循环已经在运行中"
            ));
        }
        *receiving = true;
        
        // 启动接收线程
        let connection_id = self.connection_id.clone();
        let request_id = self.request_id.clone();
        let main_thread = self.main_thread.clone();
        let message_callback = self.message_callback.clone();
        let error_callback = self.error_callback.clone();
        let end_callback = self.end_callback.clone();
        let receiving_flag = self.receiving.clone();
        
        std::thread::spawn(move || {
            info!("🔄 [GrpcBidirectionalReceiver] 开始接收消息循环");
            
            while *receiving_flag.read().unwrap() {
                // 直接从队列桥接适配器获取消息
                if let Some(bridge) = main_thread.get_queue_bridge() {
                    let queue = bridge.get_transport_to_engine_queue();
                    if let Some(message) = queue.pop() {
                        match message {
                            TransportToEngineMessage::StreamDataReceived { 
                                connection_id: msg_conn_id, 
                                request_id: msg_req_id, 
                                data, 
                                is_end 
                            } => {
                                if msg_conn_id.as_str() == connection_id && msg_req_id == request_id {
                                    if is_end {
                                        // 调用结束回调
                                        if let Some(ref callback) = end_callback {
                                            Python::with_gil(|py| {
                                                let _ = callback.call0(py);
                                            });
                                        }
                                        break;
                                    } else {
                                        // 调用消息回调
                                        if let Some(ref callback) = message_callback {
                                            Python::with_gil(|py| {
                                                let py_bytes = PyBytes::new(py, &data);
                                                let _ = callback.call1(py, (py_bytes,));
                                            });
                                        }
                                    }
                                }
                            }
                            _ => continue,
                        }
                    }
                }
                
                // 短暂休眠避免 CPU 空转
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            
            info!("🏁 [GrpcBidirectionalReceiver] 接收消息循环结束");
        });
        
        Ok(())
    }
    
    /// 停止接收
    pub fn stop_receiving(&self) -> PyResult<()> {
        let mut receiving = self.receiving.write().unwrap();
        *receiving = false;
        Ok(())
    }
    
    /// 检查是否正在接收
    pub fn is_receiving(&self) -> PyResult<bool> {
        Ok(*self.receiving.read().unwrap())
    }
}

impl PyGrpcBidirectionalReceiver {
    pub fn new(connection_id: String, request_id: String, main_thread: Arc<PyGrpcMainThread>) -> Self {
        Self {
            connection_id,
            request_id,
            main_thread,
            message_callback: None,
            error_callback: None,
            end_callback: None,
            receiving: Arc::new(std::sync::RwLock::new(false)),
        }
    }
}

/// 注册 gRPC 队列桥接模块
pub fn register_grpc_queue_bridge_module(parent_module: &PyModule) -> PyResult<()> {
    parent_module.add_class::<PyGrpcMainThread>()?;
    parent_module.add_class::<PyGrpcBidirectionalSender>()?;
    parent_module.add_class::<PyGrpcBidirectionalReceiver>()?;
    Ok(())
}