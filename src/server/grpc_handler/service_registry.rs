use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use futures_util::{Stream, StreamExt};
use h2::server::SendResponse;
use tokio::sync::broadcast;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use hyper::http::{Request, Response, StatusCode, HeaderMap, HeaderValue};
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{info, warn, debug, error};
use crate::engine::work_stealing::WorkStealingQueue;
use super::types::*;
use super::handler_traits::*;
use super::connection_manager::GrpcConnectionManager;

pub struct GrpcServiceRegistry {
    /// 一元请求处理器
    unary_handlers: HashMap<String, Arc<dyn UnaryHandler>>,
    /// 服务端流处理器
    server_stream_handlers: HashMap<String, Arc<dyn ServerStreamHandler>>,
    /// 客户端流处理器
    client_stream_handlers: HashMap<String, Arc<dyn ClientStreamHandler>>,
    /// 双向流处理器
    bidirectional_handlers: HashMap<String, Arc<dyn BidirectionalHandler>>,
    /// 无锁任务队列（向下委托到工作窃取队列）
    task_queue: Arc<SegQueue<GrpcTask>>,
    /// 工作窃取队列（集成现有的引擎）
    work_stealing_queue: Option<Arc<WorkStealingQueue<GrpcTask>>>,
    /// 是否启用无锁处理
    pub(crate) lockfree_enabled: bool,
    /// 工作线程句柄
    worker_handles: Vec<tokio::task::JoinHandle<()>>,
    /// 关闭信号
    shutdown_tx: Option<tokio::sync::broadcast::Sender<()>>,
    /// gRPC 连接管理器（框架底层）
    connection_manager: Arc<GrpcConnectionManager>,
    /// 维护任务句柄
    maintenance_handle: Option<tokio::task::JoinHandle<()>>,
}

impl GrpcServiceRegistry {
    /// 创建新的服务注册表
    pub fn new() -> Self {
        let connection_manager = Arc::new(GrpcConnectionManager::new());
        // 不在构造时启动维护任务，避免 Tokio 运行时错误
        
        Self {
            unary_handlers: HashMap::new(),
            server_stream_handlers: HashMap::new(),
            client_stream_handlers: HashMap::new(),
            bidirectional_handlers: HashMap::new(),
            task_queue: Arc::new(SegQueue::new()),
            work_stealing_queue: None,
            lockfree_enabled: false,
            worker_handles: Vec::new(),
            shutdown_tx: None,
            connection_manager,
            maintenance_handle: None,
        }
    }
    
    /// 创建带无锁队列的服务注册表
    pub fn new_with_lockfree(work_stealing_queue: Arc<WorkStealingQueue<GrpcTask>>) -> Self {
        info!("🚀 创建无锁 gRPC 服务注册表，集成工作窃取队列");
        let connection_manager = Arc::new(GrpcConnectionManager::new());
        // 不在构造时启动维护任务，避免 Tokio 运行时错误
        
        Self {
            unary_handlers: HashMap::new(),
            server_stream_handlers: HashMap::new(),
            client_stream_handlers: HashMap::new(),
            bidirectional_handlers: HashMap::new(),
            task_queue: Arc::new(SegQueue::new()),
            work_stealing_queue: Some(work_stealing_queue),
            lockfree_enabled: true,
            worker_handles: Vec::new(),
            shutdown_tx: None,
            connection_manager,
            maintenance_handle: None,
        }
    }
    
    /// 获取连接管理器
    pub fn connection_manager(&self) -> Arc<GrpcConnectionManager> {
        self.connection_manager.clone()
    }
    
    /// 启动维护任务（需要在 Tokio 运行时上下文中调用）
    pub fn start_maintenance_tasks(&mut self) {
        if self.maintenance_handle.is_none() {
            info!("🚀 启动 gRPC 连接维护任务");
            self.maintenance_handle = Some(self.connection_manager.start_maintenance_tasks());
        } else {
            warn!("⚠️ gRPC 维护任务已经启动，跳过重复启动");
        }
    }
    
    /// 启用无锁处理模式
    pub fn enable_lockfree(&mut self, work_stealing_queue: Arc<WorkStealingQueue<GrpcTask>>) {
        info!("🔄 启用 gRPC 无锁处理模式");
        self.work_stealing_queue = Some(work_stealing_queue);
        self.lockfree_enabled = true;
        
        // 自动启动工作线程
        self.start_workers(4); // 默认 4 个工作线程
    }
    
    /// 禁用无锁处理模式
    pub fn disable_lockfree(&mut self) {
        info!("⏸️ 禁用 gRPC 无锁处理模式");
        self.work_stealing_queue = None;
        self.lockfree_enabled = false;
        
        // 停止工作线程
        self.stop_workers();
    }
    
    /// 启动工作线程
    pub fn start_workers(&mut self, worker_count: usize) {
        if !self.worker_handles.is_empty() {
            warn!("⚠️ gRPC 工作线程已经启动，跳过重复启动");
            return;
        }
        
        info!("🚀 启动 {} 个 gRPC 工作线程", worker_count);
        
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx.clone());
        
        for worker_id in 0..worker_count {
            let task_queue = self.task_queue.clone();
            let work_stealing_queue = self.work_stealing_queue.clone();
            let lockfree_enabled = self.lockfree_enabled;
            let mut shutdown_rx = shutdown_tx.subscribe();
            
            // 创建一个 Arc<Self> 来在工作线程中使用
            let registry_clone = Arc::new(GrpcServiceRegistry {
                unary_handlers: self.unary_handlers.clone(),
                server_stream_handlers: self.server_stream_handlers.clone(),
                client_stream_handlers: self.client_stream_handlers.clone(),
                bidirectional_handlers: self.bidirectional_handlers.clone(),
                task_queue: task_queue.clone(),
                work_stealing_queue: work_stealing_queue.clone(),
                lockfree_enabled,
                worker_handles: Vec::new(),
                shutdown_tx: None,
                connection_manager: self.connection_manager.clone(),
                maintenance_handle: None,
            });
            
            let handle = tokio::spawn(async move {
                info!("🔧 gRPC 工作线程 {} 已启动", worker_id);
                
                loop {
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            info!("🛑 gRPC 工作线程 {} 收到关闭信号", worker_id);
                            break;
                        }
                        _ = tokio::time::sleep(Duration::from_millis(10)) => {
                            // 从队列中获取任务
                            if let Some(task) = registry_clone.pop_task(worker_id) {
                                debug!("🔄 gRPC 工作线程 {} 处理任务", worker_id);
                                if let Err(e) = registry_clone.process_task(task).await {
                                    error!("❌ gRPC 工作线程 {} 处理任务失败: {}", worker_id, e);
                                }
                            }
                        }
                    }
                }
                
                info!("✅ gRPC 工作线程 {} 已停止", worker_id);
            });
            
            self.worker_handles.push(handle);
        }
        
        info!("✅ 已启动 {} 个 gRPC 工作线程", worker_count);
    }
    
    /// 停止工作线程
    pub fn stop_workers(&mut self) {
        if let Some(shutdown_tx) = &self.shutdown_tx {
            info!("🛑 正在停止 gRPC 工作线程...");
            let _ = shutdown_tx.send(());
        }
        
        // 停止维护任务
        if let Some(handle) = self.maintenance_handle.take() {
            handle.abort();
            info!("🛑 gRPC 连接维护任务已停止");
        }
        
        // 清空句柄（实际的 join 会在 Drop 时处理）
        self.worker_handles.clear();
        self.shutdown_tx = None;
        
        info!("✅ gRPC 工作线程已停止");
    }
    
    /// 向下委托任务到工作窃取队列
    pub(crate) fn delegate_task(&self, task: GrpcTask) -> bool {
        if self.lockfree_enabled {
            if let Some(ref work_queue) = self.work_stealing_queue {
                // 向下委托到工作窃取队列，使用轮询分配
                work_queue.push(task, None);
                debug!("📤 任务已委托到工作窃取队列");
                return true;
            }
        }
        
        // 回退到无锁队列
        self.task_queue.push(task);
        debug!("📤 任务已推送到无锁队列");
        false
    }
    
    /// 从队列中获取任务
    pub fn pop_task(&self, worker_id: usize) -> Option<GrpcTask> {
        if self.lockfree_enabled {
            if let Some(ref work_queue) = self.work_stealing_queue {
                // 优先从工作窃取队列获取
                if let Some(task) = work_queue.pop(worker_id) {
                    return Some(task);
                }
            }
        }
        
        // 从无锁队列获取
        self.task_queue.pop()
    }
    
    /// 处理从队列中获取的任务
    pub async fn process_task(&self, task: GrpcTask) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match task {
            GrpcTask::UnaryRequest { method, request, context, respond } => {
                if let Some(mut respond) = respond {
                    if let Some(handler) = self.get_unary_handler(&method) {
                        debug!("🔄 处理无锁队列中的一元请求: {}", method);
                        match handler.handle(request, context).await {
                            Ok(response) => {
                                // 直接发送响应，不创建临时处理器
                                self.send_unary_response(respond, response).await?;
                            }
                            Err(error) => {
                                self.send_unary_error(respond, error).await?;
                            }
                        }
                    } else {
                        warn!("❌ 无锁队列中的一元请求处理器未找到: {}", method);
                        self.send_unary_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await?;
                    }
                }
            }
            GrpcTask::ServerStreamRequest { method, request, context, respond } => {
                if let Some(mut respond) = respond {
                    if let Some(handler) = self.get_server_stream_handler(&method) {
                        debug!("🔄 处理无锁队列中的服务端流请求: {}", method);
                        match handler.handle(request, context).await {
                            Ok(mut stream) => {
                                // 发送响应头
                                let response = Response::builder()
                                    .status(StatusCode::OK)
                                    .header("content-type", "application/grpc")
                                    .header("grpc-encoding", "identity")
                                    .body(())?;
                                
                                let mut send_stream = respond.send_response(response, false)?;
                                
                                // 发送流数据
                                while let Some(result) = stream.next().await {
                                    match result {
                                        Ok(message) => {
                                            let data = self.encode_grpc_message(&message)?;
                                            if let Err(e) = send_stream.send_data(data.into(), false) {
                                                if e.to_string().contains("inactive stream") {
                                                    info!("ℹ️ [服务端] 流已关闭，数据发送被忽略");
                                                    break;
                                                } else {
                                                    return Err(Box::new(e));
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            self.send_grpc_error_to_stream(&mut send_stream, error).await?;
                                            break;
                                        }
                                    }
                                }
                                
                                // 发送 gRPC 状态
                                self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await?;
                            }
                            Err(error) => {
                                self.send_unary_error(respond, error).await?;
                            }
                        }
                    } else {
                        warn!("❌ 无锁队列中的服务端流请求处理器未找到: {}", method);
                        self.send_unary_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await?;
                    }
                }
            }
            GrpcTask::BidirectionalData { method, request_stream, context, respond } => {
                if let (Some(request_stream), Some(mut respond)) = (request_stream, respond) {
                    if let Some(handler) = self.get_bidirectional_handler(&method) {
                        debug!("🔄 处理无锁队列中的双向流请求: {}", method);
                        match handler.handle(request_stream, context).await {
                            Ok(mut response_stream) => {
                                // 发送响应头
                                let response = Response::builder()
                                    .status(StatusCode::OK)
                                    .header("content-type", "application/grpc")
                                    .header("grpc-encoding", "identity")
                                    .body(())?;
                                
                                let mut send_stream = respond.send_response(response, false)?;
                                
                                // 发送流数据
                                while let Some(result) = response_stream.next().await {
                                    match result {
                                        Ok(message) => {
                                            let data = self.encode_grpc_message(&message)?;
                                            if let Err(e) = send_stream.send_data(data.into(), false) {
                                                if e.to_string().contains("inactive stream") {
                                                    info!("ℹ️ [服务端] 流已关闭，数据发送被忽略");
                                                    break;
                                                } else {
                                                    return Err(Box::new(e));
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            self.send_grpc_error_to_stream(&mut send_stream, error).await?;
                                            break;
                                        }
                                    }
                                }
                                
                                // 发送 gRPC 状态
                                self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await?;
                            }
                            Err(error) => {
                                self.send_unary_error(respond, error).await?;
                            }
                        }
                    } else {
                        warn!("❌ 无锁队列中的双向流请求处理器未找到: {}", method);
                        self.send_unary_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 注册一元请求处理器
    pub fn register_unary<H>(&mut self, method: impl Into<String>, handler: H)
    where
        H: UnaryHandler + 'static,
    {
        let method = method.into();
        info!("📝 注册一元 gRPC 方法: {}", method);
        self.unary_handlers.insert(method, Arc::new(handler));
    }
    
    /// 注册服务端流处理器
    pub fn register_server_stream<H>(&mut self, method: impl Into<String>, handler: H)
    where
        H: ServerStreamHandler + 'static,
    {
        let method = method.into();
        info!("📝 注册服务端流 gRPC 方法: {}", method);
        self.server_stream_handlers.insert(method, Arc::new(handler));
    }
    
    /// 注册客户端流处理器
    pub fn register_client_stream<H>(&mut self, method: impl Into<String>, handler: H)
    where
        H: ClientStreamHandler + 'static,
    {
        let method = method.into();
        info!("📝 注册客户端流 gRPC 方法: {}", method);
        self.client_stream_handlers.insert(method, Arc::new(handler));
    }
    
    /// 注册双向流处理器
    pub fn register_bidirectional<H>(&mut self, method: impl Into<String>, handler: H)
    where
        H: BidirectionalHandler + 'static,
    {
        let method = method.into();
        info!("📝 注册双向流 gRPC 方法: {}", method);
        self.bidirectional_handlers.insert(method, Arc::new(handler));
    }
    
    /// 获取一元请求处理器
    pub fn get_unary_handler(&self, method: &str) -> Option<Arc<dyn UnaryHandler>> {
        self.unary_handlers.get(method).cloned()
    }
    
    /// 获取服务端流处理器
    pub fn get_server_stream_handler(&self, method: &str) -> Option<Arc<dyn ServerStreamHandler>> {
        self.server_stream_handlers.get(method).cloned()
    }
    
    /// 获取客户端流处理器
    pub fn get_client_stream_handler(&self, method: &str) -> Option<Arc<dyn ClientStreamHandler>> {
        self.client_stream_handlers.get(method).cloned()
    }
    
    /// 获取双向流处理器
    pub fn get_bidirectional_handler(&self, method: &str) -> Option<Arc<dyn BidirectionalHandler>> {
        self.bidirectional_handlers.get(method).cloned()
    }
    
    /// 列出所有注册的方法
    pub fn list_methods(&self) -> Vec<String> {
        let mut methods = Vec::new();
        methods.extend(self.unary_handlers.keys().cloned());
        methods.extend(self.server_stream_handlers.keys().cloned());
        methods.extend(self.client_stream_handlers.keys().cloned());
        methods.extend(self.bidirectional_handlers.keys().cloned());
        methods.sort();
        methods
    }
    
    /// 发送一元响应
    async fn send_unary_response(
        &self,
        mut respond: SendResponse<bytes::Bytes>,
        response: GrpcResponse<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 直接使用 response.data，不再序列化整个 GrpcResponse 结构体
        // 因为 response.data 已经包含了序列化后的实际响应数据
        let response_data = response.data;
        
        // 编码 gRPC 消息
        let mut data = Vec::new();
        data.push(0); // 压缩标志（0 = 不压缩）
        let length = response_data.len() as u32;
        data.extend_from_slice(&length.to_be_bytes());
        data.extend_from_slice(&response_data);
        
        let http_response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "identity")
            .header("grpc-status", response.status.to_string())
            .body(())?;
        
        let mut send_stream = respond.send_response(http_response, false)?;
        
        // 容错处理：如果流已经关闭，不记录为错误
        if let Err(e) = send_stream.send_data(data.into(), false) {
            if e.to_string().contains("inactive stream") {
                info!("ℹ️ [服务端] 流已关闭，一元响应数据发送被忽略");
                return Ok(());
            } else {
                return Err(Box::new(e));
            }
        }
        
        // 发送 gRPC 状态
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&response.status.to_string())?);
        if !response.message.is_empty() {
            trailers.insert("grpc-message", HeaderValue::from_str(&response.message)?);
        }
        
        // 容错处理：如果流已经关闭，不记录为错误
        if let Err(e) = send_stream.send_trailers(trailers) {
            if e.to_string().contains("inactive stream") {
                info!("ℹ️ [服务端] 流已关闭，一元响应状态发送被忽略");
            } else {
                return Err(Box::new(e));
            }
        }
        
        Ok(())
    }
    
    /// 发送一元错误
    async fn send_unary_error(
        &self,
        mut respond: SendResponse<bytes::Bytes>,
        error: GrpcError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let http_response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-status", error.status_code().as_u32().to_string())
            .header("grpc-message", error.message())
            .body(())?;
        
        respond.send_response(http_response, true)?;
        
        Ok(())
    }
    
    /// 编码 gRPC 消息
    pub fn encode_grpc_message(&self, message: &GrpcStreamMessage<Vec<u8>>) -> Result<Vec<u8>, GrpcError> {
        debug!("🚨🚨🚨 [服务端] encode_grpc_message 被调用！！！");
        debug!("🚨🚨🚨 [服务端] 输入消息 - ID: {}, 序列: {}, 数据长度: {}, 结束标志: {}", 
                message.id, message.sequence, message.data.len(), message.end_of_stream);
        debug!("🚨🚨🚨 [服务端] 输入数据前32字节: {:?}", 
                &message.data[..std::cmp::min(32, message.data.len())]);
        
        // 序列化整个 GrpcStreamMessage 结构体
        let serialized_message = GrpcCodec::encode(message)
            .map_err(|e| GrpcError::Internal(format!("编码 GrpcStreamMessage 失败: {}", e)))?;
        
        debug!("🚨🚨🚨 [服务端] GrpcStreamMessage 序列化成功，序列化后大小: {} bytes", serialized_message.len());
        debug!("🚨🚨🚨 [服务端] 序列化后前32字节: {:?}", 
                &serialized_message[..std::cmp::min(32, serialized_message.len())]);
        
        let mut result = Vec::new();
        
        // 压缩标志（0 = 不压缩）
        result.push(0);
        
        // 消息长度
        let length = serialized_message.len() as u32;
        result.extend_from_slice(&length.to_be_bytes());
        
        // 消息数据
        result.extend_from_slice(&serialized_message);
        
        debug!("🚨🚨🚨 [服务端] 最终编码结果大小: {} bytes (包含5字节头部)", result.len());
        debug!("🚨🚨🚨 [服务端] 最终编码前37字节: {:?}", 
                &result[..std::cmp::min(37, result.len())]);
        
        Ok(result)
    }
    
    /// 发送 gRPC 错误到流
    pub async fn send_grpc_error_to_stream(
        &self,
        send_stream: &mut h2::SendStream<bytes::Bytes>,
        error: GrpcError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&error.status_code().as_u32().to_string())?);
        trailers.insert("grpc-message", HeaderValue::from_str(&error.message())?);
        
        // 容错处理：如果流已经关闭，不记录为错误
        if let Err(e) = send_stream.send_trailers(trailers) {
            if e.to_string().contains("inactive stream") {
                info!("ℹ️ [服务端] 流已关闭，gRPC 错误发送被忽略");
            } else {
                return Err(Box::new(e));
            }
        }
        
        Ok(())
    }
    
    /// 发送 gRPC 状态
    pub async fn send_grpc_status(
        &self,
        send_stream: &mut h2::SendStream<bytes::Bytes>,
        status: GrpcStatusCode,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&status.as_u32().to_string())?);
        if !message.is_empty() {
            trailers.insert("grpc-message", HeaderValue::from_str(message)?);
        }
        
        // 容错处理：如果流已经关闭，不记录为错误
        if let Err(e) = send_stream.send_trailers(trailers) {
            if e.to_string().contains("inactive stream") {
                info!("ℹ️ [服务端] 流已关闭，gRPC 状态发送被忽略");
            } else {
                return Err(Box::new(e));
            }
        }
        
        Ok(())
    }
}
