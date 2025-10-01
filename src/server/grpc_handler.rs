//! gRPC 处理器模块
//! 
//! 提供一元请求、服务端流和双向流的处理功能
//! 使用 bincode 2.x 进行序列化
//! 集成无锁队列和向下委托机制

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::future::Future;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use futures_util::{Stream, StreamExt, stream::StreamExt as _};
use h2::{server::SendResponse, RecvStream, Reason};
use hyper::http::{Request, Response, StatusCode, HeaderMap, HeaderValue};
use pin_project_lite::pin_project;
use tokio::sync::{mpsc, broadcast};
use bytes;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{info, warn, error, debug};
use crate::engine::work_stealing::WorkStealingQueue;
use serde::Serialize;

/// gRPC 任务类型，用于无锁队列处理
pub enum GrpcTask {
    /// 一元请求任务
    UnaryRequest {
        method: String,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
        respond: Option<SendResponse<bytes::Bytes>>,
    },
    /// 服务端流请求任务
    ServerStreamRequest {
        method: String,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
        respond: Option<SendResponse<bytes::Bytes>>,
    },
    /// 双向流数据任务
    BidirectionalData {
        method: String,
        request_stream: Option<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>>,
        context: GrpcContext,
        respond: Option<SendResponse<bytes::Bytes>>,
    },
}

impl std::fmt::Debug for GrpcTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrpcTask::UnaryRequest { method, .. } => {
                write!(f, "GrpcTask::UnaryRequest {{ method: {:?} }}", method)
            }
            GrpcTask::ServerStreamRequest { method, .. } => {
                write!(f, "GrpcTask::ServerStreamRequest {{ method: {:?} }}", method)
            }
            GrpcTask::BidirectionalData { method, .. } => {
                write!(f, "GrpcTask::BidirectionalData {{ method: {:?} }}", method)
            }
        }
    }
}

// 为 GrpcTask 实现 Send 和 Sync，确保可以在线程间安全传递
unsafe impl Send for GrpcTask {}
unsafe impl Sync for GrpcTask {}

/// gRPC 连接类型
#[derive(Debug, Clone, PartialEq)]
pub enum GrpcConnectionType {
    /// 客户端流连接（客户端向服务端流式发送数据）
    ClientStream,
    /// 服务端流连接（单向推送）
    ServerStream,
    /// 双向流连接（双向通信）
    BidirectionalStream,
}

/// gRPC 连接信息
#[derive(Debug, Clone)]
pub struct GrpcConnection {
    /// 连接ID
    pub connection_id: String,
    /// 用户ID
    pub user_id: String,
    /// 房间ID（可选）
    /// 
    /// 用于标识连接所属的逻辑房间或频道。当连接加入特定房间时，
    /// 该字段包含房间的唯一标识符，用于：
    /// - 房间内消息广播
    /// - 房间成员管理
    /// - 房间级别的权限控制
    /// 
    /// 如果连接未加入任何房间，则为 None
    pub room_id: Option<String>,
    /// 连接类型
    pub connection_type: GrpcConnectionType,
    /// 连接时间
    pub connected_at: Instant,
    /// 最后活跃时间
    pub last_active: Instant,
    /// 广播发送器
    pub broadcast_tx: broadcast::Sender<Vec<u8>>,
}

/// 无锁 gRPC 连接管理器
/// 集成到框架底层，提供连接池和保活机制
pub struct GrpcConnectionManager {
    /// 活跃连接（连接ID -> 连接信息）
    connections: Arc<DashMap<String, GrpcConnection>>,
    /// 用户连接映射（用户ID -> 连接ID列表）
    user_connections: Arc<DashMap<String, Vec<String>>>,
    /// 房间连接映射（房间ID -> 连接ID列表）
    room_connections: Arc<DashMap<String, Vec<String>>>,
    /// 连接ID生成器
    connection_id_counter: Arc<AtomicU64>,
    /// 消息历史（无锁队列）
    message_history: Arc<SegQueue<Vec<u8>>>,
    /// 保活间隔
    keepalive_interval: Duration,
    /// 连接超时时间
    connection_timeout: Duration,
}

impl GrpcConnectionManager {
    /// 创建新的连接管理器
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            user_connections: Arc::new(DashMap::new()),
            room_connections: Arc::new(DashMap::new()),
            connection_id_counter: Arc::new(AtomicU64::new(1)),
            message_history: Arc::new(SegQueue::new()),
            keepalive_interval: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(300), // 5分钟超时
        }
    }
    
    /// 添加新连接
    pub fn add_connection(&self, user_id: String, room_id: Option<String>, connection_type: GrpcConnectionType) -> (String, broadcast::Receiver<Vec<u8>>) {
        let connection_id = self.connection_id_counter.fetch_add(1, Ordering::Relaxed).to_string();
        let (tx, rx) = broadcast::channel(1000);
        let now = Instant::now();
        
        let connection = GrpcConnection {
            connection_id: connection_id.clone(),
            user_id: user_id.clone(),
            room_id: room_id.clone(),
            connection_type: connection_type.clone(),
            connected_at: now,
            last_active: now,
            broadcast_tx: tx,
        };
        
        // 添加到连接映射
        self.connections.insert(connection_id.clone(), connection);
        
        // 添加到用户连接映射
        self.user_connections.entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(connection_id.clone());
        
        // 添加到房间连接映射（如果有房间）
        if let Some(ref room_id) = room_id {
            self.room_connections.entry(room_id.clone())
                .or_insert_with(Vec::new)
                .push(connection_id.clone());
        }
        
        info!("🔗 新 gRPC 连接: {} (用户: {}, 房间: {:?}, 类型: {:?})", connection_id, user_id, room_id, connection_type);
        (connection_id, rx)
    }
    
    /// 移除连接
    pub fn remove_connection(&self, connection_id: &str) {
        if let Some((_, connection)) = self.connections.remove(connection_id) {
            // 从用户连接映射中移除
            if let Some(mut user_conns) = self.user_connections.get_mut(&connection.user_id) {
                user_conns.retain(|id| id != connection_id);
                if user_conns.is_empty() {
                    drop(user_conns);
                    self.user_connections.remove(&connection.user_id);
                }
            }
            
            // 从房间连接映射中移除
            if let Some(ref room_id) = connection.room_id {
                if let Some(mut room_conns) = self.room_connections.get_mut(room_id) {
                    room_conns.retain(|id| id != connection_id);
                    if room_conns.is_empty() {
                        drop(room_conns);
                        self.room_connections.remove(room_id);
                    }
                }
            }
            
            info!("🔌 移除 gRPC 连接: {} (用户: {})", connection_id, connection.user_id);
        }
    }
    
    /// 更新连接活跃时间
    pub fn update_activity(&self, connection_id: &str) {
        if let Some(mut connection) = self.connections.get_mut(connection_id) {
            connection.last_active = Instant::now();
        }
    }
    
    /// 广播消息到房间
    pub fn broadcast_to_room(&self, room_id: &str, message: Vec<u8>) {
        // 保存到历史记录
        self.message_history.push(message.clone());
        
        // 获取房间中的连接
        if let Some(connection_ids) = self.room_connections.get(room_id) {
            let mut sent_count = 0;
            for connection_id in connection_ids.iter() {
                if let Some(connection) = self.connections.get(connection_id) {
                    if let Err(_) = connection.broadcast_tx.send(message.clone()) {
                        warn!("⚠️ 向连接 {} 发送消息失败", connection_id);
                    } else {
                        sent_count += 1;
                    }
                }
            }
            debug!("📢 消息已广播到房间 {} 的 {} 个连接", room_id, sent_count);
        }
    }
    
    /// 发送消息给特定用户
    pub fn send_to_user(&self, user_id: &str, message: Vec<u8>) {
        if let Some(connection_ids) = self.user_connections.get(user_id) {
            for connection_id in connection_ids.iter() {
                if let Some(connection) = self.connections.get(connection_id) {
                    if let Err(_) = connection.broadcast_tx.send(message.clone()) {
                        warn!("⚠️ 向用户 {} 的连接 {} 发送消息失败", user_id, connection_id);
                    }
                }
            }
        }
    }
    
    /// 清理超时连接
    pub fn cleanup_expired_connections(&self) {
        let now = Instant::now();
        let mut expired_connections: Vec<String> = Vec::new();
        
        for entry in self.connections.iter() {
            let connection = entry.value();
            if now.duration_since(connection.last_active) > self.connection_timeout {
                expired_connections.push(connection.connection_id.clone());
            }
        }
        
        for connection_id in expired_connections {
            warn!("⏰ 清理超时连接: {}", connection_id);
            self.remove_connection(&connection_id);
        }
    }
    
    /// 获取连接统计信息
    pub fn get_stats(&self) -> (usize, usize, usize) {
        (
            self.connections.len(),
            self.user_connections.len(),
            self.room_connections.len(),
        )
    }
    
    /// 启动保活和清理任务
    pub fn start_maintenance_tasks(&self) -> tokio::task::JoinHandle<()> {
        let connections = self.connections.clone();
        let keepalive_interval = self.keepalive_interval;
        let connection_timeout = self.connection_timeout;
        
        tokio::spawn(async move {
            let mut cleanup_interval = tokio::time::interval(Duration::from_secs(60)); // 每分钟清理一次
            let mut keepalive_interval = tokio::time::interval(keepalive_interval);
            
            loop {
                tokio::select! {
                    _ = cleanup_interval.tick() => {
                        // 清理超时连接的逻辑已经在 cleanup_expired_connections 中实现
                        let now = Instant::now();
                        let mut expired_connections = Vec::new();
                        
                        for entry in connections.iter() {
                            let connection = entry.value();
                            if now.duration_since(connection.last_active) > connection_timeout {
                                expired_connections.push(connection.connection_id.clone());
                            }
                        }
                        
                        if !expired_connections.is_empty() {
                            info!("🧹 清理 {} 个超时连接", expired_connections.len());
                        }
                    }
                    _ = keepalive_interval.tick() => {
                        // 发送保活消息
                        let keepalive_message = b"keepalive".to_vec();
                        for entry in connections.iter() {
                            let connection = entry.value();
                            let _ = connection.broadcast_tx.send(keepalive_message.clone());
                        }
                        debug!("💓 发送保活消息到 {} 个连接", connections.len());
                    }
                }
            }
        })
    }
}

/// gRPC 服务注册表（集成无锁队列）
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
    lockfree_enabled: bool,
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
    fn delegate_task(&self, task: GrpcTask) -> bool {
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

/// 一元请求处理器特征
pub trait UnaryHandler: Send + Sync {
    /// 处理一元请求
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>>;
}

/// 服务端流处理器特征（原始版本，用于向后兼容）
pub trait ServerStreamHandler: Send + Sync {
    /// 处理服务端流请求
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>>;
}

/// 泛型服务端流处理器特征（支持框架层统一序列化）
pub trait TypedServerStreamHandler<T>: Send + Sync 
where
    T: Serialize + bincode::Encode + Send + Sync + 'static,
{
    /// 处理服务端流请求，返回强类型的流
    fn handle_typed(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<T>, GrpcError>> + Send>>, GrpcError>> + Send>>;
}

/// 泛型服务端流处理器适配器
pub struct TypedServerStreamAdapter<T, H> {
    handler: H,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, H> TypedServerStreamAdapter<T, H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// 为泛型处理器适配器实现原始处理器接口（自动序列化适配器）
impl<T, H> ServerStreamHandler for TypedServerStreamAdapter<T, H>
where
    T: Serialize + bincode::Encode + Send + Sync + 'static,
    H: TypedServerStreamHandler<T> + Clone + 'static,
{
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        // 克隆处理器以避免生命周期问题
        let handler = self.handler.clone();
        Box::pin(async move {
            // 调用强类型处理器
            let typed_stream = handler.handle_typed(request, context).await?;
            
            // 创建序列化适配器流
            let serialized_stream = typed_stream.map(|item| {
                match item {
                    Ok(typed_message) => {
                        // 序列化 data 字段
                        match GrpcCodec::encode(&typed_message.data) {
                            Ok(serialized_data) => Ok(GrpcStreamMessage {
                                id: typed_message.id,
                                stream_id: typed_message.stream_id,
                                sequence: typed_message.sequence,
                                end_of_stream: typed_message.end_of_stream,
                                data: serialized_data,
                                metadata: typed_message.metadata,
                            }),
                            Err(e) => Err(GrpcError::Internal(format!("序列化数据失败: {}", e))),
                        }
                    }
                    Err(e) => Err(e),
                }
            });
            
            Ok(Box::pin(serialized_stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>)
        })
    }
}

/// 客户端流处理器特征
pub trait ClientStreamHandler: Send + Sync {
    /// 处理客户端流请求
    fn handle(
        &self,
        request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>>;
}

/// 双向流处理器特征
pub trait BidirectionalHandler: Send + Sync {
    /// 处理双向流请求
    fn handle(
        &self,
        request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>>;
}

/// gRPC 请求处理器
pub struct GrpcRequestHandler {
    registry: Arc<RwLock<GrpcServiceRegistry>>,
}

impl GrpcRequestHandler {
    /// 创建新的请求处理器
    pub fn new(registry: Arc<RwLock<GrpcServiceRegistry>>) -> Self {
        Self { registry }
    }
    
    /// 处理 gRPC 请求（集成无锁队列和向下委托）
    pub async fn handle_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let method = self.extract_grpc_method(&request)?;
        let context = self.create_grpc_context(&request);
        
        debug!("🔄 处理 gRPC 请求: {}", method);
        
        // 检查是否启用无锁模式
        let lockfree_enabled = {
            let registry = self.registry.read().unwrap();
            registry.lockfree_enabled
        };
        
        if lockfree_enabled {
            // 无锁模式：向下委托任务
            debug!("🚀 使用无锁模式处理 gRPC 请求");
            self.handle_request_lockfree(request, respond, method, context).await
        } else {
            // 传统模式：直接处理
            debug!("🔄 使用传统模式处理 gRPC 请求");
            self.handle_request_traditional(request, respond, method, context).await
        }
    }
    
    /// 无锁模式处理请求
    async fn handle_request_lockfree(
        &self,
        request: Request<RecvStream>,
        respond: SendResponse<bytes::Bytes>,
        method: String,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 获取处理器类型，避免长时间持有锁
        let handler_type = {
            let registry = self.registry.read().unwrap();
            if registry.get_unary_handler(&method).is_some() {
                Some("unary")
            } else if registry.get_server_stream_handler(&method).is_some() {
                Some("server_stream")
            } else if registry.get_bidirectional_handler(&method).is_some() {
                Some("bidirectional")
            } else {
                None
            }
        };
        
        match handler_type {
            Some("unary") => {
                // 读取请求体
                let grpc_request = self.read_grpc_request(request).await?;
                
                // 创建任务并委托
                let task = GrpcTask::UnaryRequest {
                    method: method.clone(),
                    request: grpc_request,
                    context,
                    respond: Some(respond),
                };
                
                let registry = self.registry.read().unwrap();
                registry.delegate_task(task);
                debug!("📤 一元请求已委托到无锁队列: {}", method);
            },
            Some("server_stream") => {
                // 读取请求体
                let grpc_request = self.read_grpc_request(request).await?;
                
                // 创建任务并委托
                let task = GrpcTask::ServerStreamRequest {
                    method: method.clone(),
                    request: grpc_request,
                    context,
                    respond: Some(respond),
                };
                
                let registry = self.registry.read().unwrap();
                registry.delegate_task(task);
                debug!("📤 服务端流请求已委托到无锁队列: {}", method);
            },
            Some("bidirectional") => {
                // 创建请求流
                let request_stream = self.create_grpc_request_stream(request);
                
                // 创建任务并委托
                let task = GrpcTask::BidirectionalData {
                    method: method.clone(),
                    request_stream: Some(request_stream),
                    context,
                    respond: Some(respond),
                };
                
                let registry = self.registry.read().unwrap();
                registry.delegate_task(task);
                debug!("📤 双向流请求已委托到无锁队列: {}", method);
            },
            _ => {
                // 方法未找到
                warn!("❌ gRPC 方法未找到: {}", method);
                self.send_grpc_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await?;
            }
        }
        
        Ok(())
    }
    
    /// 传统模式处理请求
    async fn handle_request_traditional(
        &self,
        request: Request<RecvStream>,
        respond: SendResponse<bytes::Bytes>,
        method: String,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("🚨🚨🚨 [服务端] 传统模式处理请求，方法: {}", method);
        
        // 获取处理器类型，避免长时间持有锁
        let handler_type = {
            let registry = self.registry.read().unwrap();
            debug!("🚨🚨🚨 [服务端] 检查一元处理器: {}", registry.get_unary_handler(&method).is_some());
            debug!("🚨🚨🚨 [服务端] 检查服务端流处理器: {}", registry.get_server_stream_handler(&method).is_some());
            debug!("🚨🚨🚨 [服务端] 检查客户端流处理器: {}", registry.get_client_stream_handler(&method).is_some());
            debug!("🚨🚨🚨 [服务端] 检查双向流处理器: {}", registry.get_bidirectional_handler(&method).is_some());
            
            if registry.get_unary_handler(&method).is_some() {
                Some("unary")
            } else if registry.get_server_stream_handler(&method).is_some() {
                Some("server_stream")
            } else if registry.get_client_stream_handler(&method).is_some() {
                Some("client_stream")
            } else if registry.get_bidirectional_handler(&method).is_some() {
                Some("bidirectional")
            } else {
                None
            }
        };
        
        debug!("🚨🚨🚨 [服务端] 处理器类型: {:?}", handler_type);
        
        match handler_type {
            Some("unary") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_unary_handler(&method).unwrap()
                };
                self.handle_unary_request(request, respond, &*handler, context).await
            },
            Some("server_stream") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_server_stream_handler(&method).unwrap()
                };
                self.handle_server_stream_request(request, respond, &*handler, context).await
            },
            Some("client_stream") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_client_stream_handler(&method).unwrap()
                };
                self.handle_client_stream_request(request, respond, &*handler, context).await
            },
            Some("bidirectional") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_bidirectional_handler(&method).unwrap()
                };
                self.handle_bidirectional_request(request, respond, &*handler, context).await
            },
            _ => {
                // 方法未找到
                warn!("❌ gRPC 方法未找到: {}", method);
                self.send_grpc_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await
            }
        }
    }
    
    /// 处理一元请求
    async fn handle_unary_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn UnaryHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 读取请求体
        let grpc_request = self.read_grpc_request(request).await?;
        
        // 调用处理器
        match handler.handle(grpc_request, context).await {
            Ok(response) => {
                self.send_grpc_response(respond, response).await?;
            }
            Err(error) => {
                self.send_grpc_error(respond, error).await?;
            }
        }
        
        Ok(())
    }
    
    /// 处理服务端流请求
    async fn handle_server_stream_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn ServerStreamHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 读取请求体
        let grpc_request = self.read_grpc_request(request).await?;
        
        // 调用处理器
        match handler.handle(grpc_request, context).await {
            Ok(mut stream) => {
                // 发送响应头
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/grpc")
                    .header("grpc-encoding", "identity")
                    .body(())?;
                
                let mut send_stream = match respond.send_response(response, false) {
                    Ok(stream) => stream,
                    Err(e) => {
                        // 如果发送响应头失败，可能是连接已关闭
                        if e.to_string().contains("inactive stream") || e.to_string().contains("closed") {
                            info!("ℹ️ [服务端] 客户端连接已关闭，无法发送响应头");
                            return Ok(());
                        }
                        return Err(Box::new(e));
                    }
                };
                
                let mut stream_closed = false;
                let mut error_sent = false;
                
                // 发送流数据
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(message) => {
                            let data = match self.encode_grpc_message(&message) {
                                Ok(data) => data,
                                Err(e) => {
                                    error!("❌ 编码 gRPC 消息失败: {}", e);
                                    break;
                                }
                            };
                            
                            // 发送数据时检查连接状态
                            if let Err(e) = send_stream.send_data(data.into(), false) {
                                let error_msg = e.to_string();
                                if error_msg.contains("inactive stream") || 
                                   error_msg.contains("closed") || 
                                   error_msg.contains("broken pipe") ||
                                   error_msg.contains("connection reset") {
                                    info!("ℹ️ [服务端] 客户端连接已关闭，停止发送数据");
                                    stream_closed = true;
                                    break;
                                } else {
                                    error!("❌ 发送数据失败: {}", error_msg);
                                    break;
                                }
                            }
                        }
                        Err(error) => {
                            // 尝试发送错误，但如果连接已关闭则忽略
                            let _ = self.send_grpc_error_to_stream(&mut send_stream, error).await;
                            error_sent = true;
                            break;
                        }
                    }
                }
                
                // 只有在流未关闭且未发送错误时才发送正常的 gRPC 状态
                if !stream_closed && !error_sent {
                    let _ = self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await;
                }
            }
            Err(error) => {
                // 对于服务端流，当处理器返回错误时，先发送正常的响应头，然后通过 trailers 发送错误
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/grpc")
                    .header("grpc-encoding", "identity")
                    .body(())?;
                
                match respond.send_response(response, false) {
                    Ok(mut send_stream) => {
                        // 通过 trailers 发送错误状态
                        let _ = self.send_grpc_error_to_stream(&mut send_stream, error).await;
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("inactive stream") || 
                           error_msg.contains("closed") || 
                           error_msg.contains("broken pipe") ||
                           error_msg.contains("connection reset") {
                            info!("ℹ️ [服务端] 客户端连接已关闭，错误响应发送被忽略");
                        } else {
                            error!("❌ 发送服务端流响应头失败: {}", error_msg);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 处理客户端流请求
    async fn handle_client_stream_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn ClientStreamHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("DEBUG: 开始处理客户端流请求");
        
        // 对于客户端流，需要先发送响应头让客户端知道连接已建立
        println!("DEBUG: 发送客户端流响应头");
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "identity")
            .body(())?;
        
        let mut send_stream = respond.send_response(response, false)?;
        println!("DEBUG: 客户端流响应头发送成功");
        
        // 创建请求流
        let request_stream = self.create_grpc_request_stream(request);
        println!("DEBUG: 请求流创建完成，调用处理器");
        
        // 调用处理器
        match handler.handle(request_stream, context).await {
            Ok(response) => {
                println!("DEBUG: 处理器返回成功响应");
                // 直接发送 GrpcResponse 数据，不包装成 GrpcStreamMessage
                let data = GrpcCodec::encode_frame(&response)
                    .map_err(|e| GrpcError::Internal(format!("编码 gRPC 响应失败: {}", e)))?;
                send_stream.send_data(data.into(), false)?;
                // 发送 gRPC 状态
                self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await?;
            }
            Err(error) => {
                println!("DEBUG: 处理器返回错误: {:?}", error);
                self.send_grpc_error_to_stream(&mut send_stream, error).await?;
            }
        }
        
        Ok(())
    }
    
    /// 处理双向流请求
    async fn handle_bidirectional_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn BidirectionalHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("🔍 [DEBUG] handle_bidirectional_request 开始");
        
        // 创建请求流
        debug!("🔍 [DEBUG] 准备创建请求流");
        let request_stream = self.create_grpc_request_stream(request);
        debug!("🔍 [DEBUG] 请求流创建完成");
        
        // 调用处理器
        debug!("🔍 [DEBUG] 准备调用双向流处理器");
        match handler.handle(request_stream, context).await {
            Ok(mut response_stream) => {
                debug!("🔍 [DEBUG] 双向流处理器调用成功，准备发送响应头");
                
                // 发送响应头
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/grpc")
                    .header("grpc-encoding", "identity")
                    .body(())?;
                
                debug!("🔍 [DEBUG] 响应头构建完成，准备发送");
                let mut send_stream = respond.send_response(response, false)?;
                debug!("🔍 [DEBUG] 响应头发送成功，开始处理响应流");
                
                let mut stream_closed = false;
                
                // 发送流数据
                while let Some(result) = response_stream.next().await {
                    debug!("🔍 [DEBUG] 收到响应流数据");
                    match result {
                        Ok(message) => {
                            debug!("🔍 [DEBUG] 编码响应消息");
                            let data = self.encode_grpc_message(&message)?;
                            debug!("🔍 [DEBUG] 发送响应数据");
                            if let Err(e) = send_stream.send_data(data.into(), false) {
                                let error_msg = e.to_string();
                                if error_msg.contains("inactive stream") || 
                                   error_msg.contains("closed") || 
                                   error_msg.contains("broken pipe") ||
                                   error_msg.contains("connection reset") {
                                    info!("ℹ️ [服务端] 客户端连接已关闭，停止发送数据");
                                    stream_closed = true;
                                    break;
                                } else {
                                    return Err(e.into());
                                }
                            }
                        }
                        Err(error) => {
                            debug!("🔍 [DEBUG] 响应流出现错误: {:?}", error);
                            self.send_grpc_error_to_stream(&mut send_stream, error).await?;
                            break;
                        }
                    }
                }
                
                debug!("🔍 [DEBUG] 响应流处理完成");
                
                // 只有在流未关闭时才发送 gRPC 状态
                if !stream_closed {
                    debug!("🔍 [DEBUG] 发送 gRPC 状态");
                    self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await?;
                }
                
                debug!("🔍 [DEBUG] handle_bidirectional_request 成功完成");
            }
            Err(error) => {
                debug!("🔍 [DEBUG] 双向流处理器调用失败: {:?}", error);
                self.send_grpc_error(respond, error).await?;
            }
        }
        
        Ok(())
    }
    
    /// 提取 gRPC 方法名
    fn extract_grpc_method(&self, request: &Request<RecvStream>) -> Result<String, GrpcError> {
        let path = request.uri().path();
        debug!("🚨🚨🚨 [服务端] 提取 gRPC 方法路径: {}", path);
        if path.starts_with('/') {
            // 保留完整路径，包括前导斜杠，以匹配注册时的方法名
            debug!("🚨🚨🚨 [服务端] 返回方法名: {}", path);
            Ok(path.to_string())
        } else {
            Err(GrpcError::InvalidArgument("无效的 gRPC 方法路径".to_string()))
        }
    }
    
    /// 创建 gRPC 上下文
    fn create_grpc_context(&self, request: &Request<RecvStream>) -> GrpcContext {
        let mut metadata = HashMap::new();
        
        // 提取请求头作为元数据
        for (name, value) in request.headers() {
            if let Ok(value_str) = value.to_str() {
                metadata.insert(name.to_string(), value_str.to_string());
            }
        }
        
        // 从请求扩展中获取远程地址（如果可用）
        let remote_addr = request.extensions()
            .get::<std::net::SocketAddr>()
            .copied();
        
        GrpcContext {
            remote_addr,
            headers: metadata,
            method: GrpcMethodDescriptor::from_path(request.uri().path(), GrpcMethodType::Unary)
                .unwrap_or_else(|| GrpcMethodDescriptor::new("unknown", "unknown", GrpcMethodType::Unary)),
        }
    }
    
    /// 读取 gRPC 请求
    async fn read_grpc_request(&self, request: Request<RecvStream>) -> Result<GrpcRequest<Vec<u8>>, GrpcError> {
        // 先创建上下文以获取方法信息
        let context = self.create_grpc_context(&request);
        
        let mut body = request.into_body();
        let mut data = Vec::new();
        
        while let Some(chunk) = body.data().await {
            match chunk {
                Ok(bytes) => {
                    // 释放流控制容量
                    if let Err(e) = body.flow_control().release_capacity(bytes.len()) {
                        return Err(GrpcError::Internal(format!("释放流控制容量失败: {}", e)));
                    }
                    data.extend_from_slice(&bytes);
                }
                Err(e) => {
                    return Err(GrpcError::Internal(format!("读取请求体失败: {}", e)));
                }
            }
        }
        
        self.decode_grpc_request(&data, &context)
    }
    
    /// 解码 gRPC 请求
    fn decode_grpc_request(&self, data: &[u8], context: &GrpcContext) -> Result<GrpcRequest<Vec<u8>>, GrpcError> {
        // 使用统一的编解码器解析帧
        let payload = GrpcCodec::parse_frame(data)
            .map_err(|e| GrpcError::InvalidArgument(format!("解析 gRPC 帧失败: {}", e)))?;
        
        // 尝试反序列化为 GrpcRequest 结构体（客户端发送的是完整的 GrpcRequest）
        match GrpcCodec::decode::<GrpcRequest<Vec<u8>>>(&payload) {
            Ok(grpc_request) => {
                // 成功反序列化，直接返回
                Ok(grpc_request)
            },
            Err(_) => {
                // 反序列化失败，可能是原始数据，直接使用
                let request = GrpcRequest {
                    id: 0, // 默认 ID
                    method: context.method.method.clone(),
                    data: payload.to_vec(),
                    metadata: context.headers.clone(),
                };
                Ok(request)
            }
        }
    }
    
    /// 创建 gRPC 请求流
    fn create_grpc_request_stream(
        &self,
        request: Request<RecvStream>,
    ) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>> {
        debug!("🔍 [DEBUG] create_grpc_request_stream 开始");
        let body = request.into_body();
        debug!("🔍 [DEBUG] 获取请求体成功");
        let stream = GrpcRequestStream::new(body);
        debug!("🔍 [DEBUG] 创建 GrpcRequestStream 成功");
        let boxed_stream = Box::pin(stream);
        debug!("🔍 [DEBUG] 包装为 Pin<Box> 成功");
        boxed_stream
    }
    
    /// 编码 gRPC 消息
    fn encode_grpc_message(&self, message: &GrpcStreamMessage<Vec<u8>>) -> Result<Vec<u8>, GrpcError> {
        // 使用统一的编解码器编码并创建帧
        GrpcCodec::encode_frame(message)
            .map_err(|e| GrpcError::Internal(format!("编码 gRPC 流消息失败: {}", e)))
    }
    
    /// 发送 gRPC 响应
    async fn send_grpc_response(
        &self,
        mut respond: SendResponse<bytes::Bytes>,
        response: GrpcResponse<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 直接使用 response.data，不再序列化整个 GrpcResponse 结构体
        // 因为 response.data 已经包含了序列化后的实际响应数据
        let response_data = response.data;
        
        // 构建 gRPC 消息格式（5字节头部 + 数据）
        let mut data = Vec::new();
        
        // 压缩标志（0 = 不压缩）
        data.push(0);
        
        // 消息长度
        let length = response_data.len() as u32;
        data.extend_from_slice(&length.to_be_bytes());
        
        // 消息数据
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
    
    /// 发送 gRPC 错误
    async fn send_grpc_error(
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
        
        if let Err(e) = respond.send_response(http_response, true) {
            let error_msg = e.to_string();
            if error_msg.contains("inactive stream") || 
               error_msg.contains("closed") || 
               error_msg.contains("broken pipe") ||
               error_msg.contains("connection reset") {
                info!("ℹ️ [服务端] 客户端连接已关闭，gRPC 错误响应发送被忽略");
            } else {
                error!("❌ 发送 gRPC 错误响应失败: {}", error_msg);
                return Err(e.into());
            }
        }
        
        Ok(())
    }
    
    /// 发送 gRPC 错误到流
    async fn send_grpc_error_to_stream(
        &self,
        send_stream: &mut h2::SendStream<bytes::Bytes>,
        error: GrpcError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&error.status_code().as_u32().to_string())?);
        trailers.insert("grpc-message", HeaderValue::from_str(&error.message())?);
        
        match send_stream.send_trailers(trailers) {
            Ok(_) => Ok(()),
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("inactive stream") || 
                   error_msg.contains("closed") || 
                   error_msg.contains("broken pipe") ||
                   error_msg.contains("connection reset") {
                    info!("ℹ️ [服务端] 客户端连接已关闭，gRPC 错误发送被忽略");
                    Ok(())
                } else {
                    Err(Box::new(e))
                }
            }
        }
    }
    
    /// 发送 gRPC 状态
    async fn send_grpc_status(
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
        
        if let Err(e) = send_stream.send_trailers(trailers) {
            let error_msg = e.to_string();
            if error_msg.contains("inactive stream") || 
               error_msg.contains("closed") || 
               error_msg.contains("broken pipe") ||
               error_msg.contains("connection reset") {
                info!("ℹ️ [服务端] 客户端连接已关闭，gRPC 状态发送被忽略");
            } else {
                return Err(Box::new(e));
            }
        }
        
        Ok(())
    }
}

/// gRPC 请求流
pin_project! {
    struct GrpcRequestStream {
        #[pin]
        body: RecvStream,
        buffer: Vec<u8>,
        sequence: u64,
    }
}

impl GrpcRequestStream {
    fn new(body: RecvStream) -> Self {
        debug!("🔍 [DEBUG] GrpcRequestStream::new 开始");
        let stream = Self {
            body,
            buffer: Vec::new(),
            sequence: 0,
        };
        debug!("🔍 [DEBUG] GrpcRequestStream::new 完成");
        stream
    }
}

impl Stream for GrpcRequestStream {
    type Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>;
    
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        
        // 尝试从缓冲区解析完整的 gRPC 消息
        if this.buffer.len() >= 5 {
            let length = u32::from_be_bytes([
                this.buffer[1],
                this.buffer[2], 
                this.buffer[3],
                this.buffer[4]
            ]) as usize;
            
            // 添加合理的长度限制，防止容量溢出（最大 100MB）
            const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
            if length > MAX_MESSAGE_SIZE {
                return Poll::Ready(Some(Err(GrpcError::Internal(format!(
                    "gRPC 消息长度过大: {} 字节，最大允许: {} 字节", 
                    length, MAX_MESSAGE_SIZE
                )))));
            }
            
            if this.buffer.len() >= 5 + length {
                // 有完整的消息
                let compressed = this.buffer[0] != 0;
                if compressed {
                    return Poll::Ready(Some(Err(GrpcError::Unimplemented("不支持压缩的 gRPC 消息".to_string()))));
                }
                
                let data = this.buffer[5..5 + length].to_vec();
                this.buffer.drain(..5 + length);
                
                let current_sequence = *this.sequence;
                *this.sequence += 1;
                
                // 尝试解析为 GrpcStreamMessage<Vec<u8>>（关闭信号）
                if let Ok(stream_message) = GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(&data) {
                    // 这是一个关闭信号或其他流消息
                    let msg = stream_message;
                    println!("DEBUG: 收到流消息，end_of_stream: {}, 数据长度: {}", msg.end_of_stream, msg.data.len());
                    if msg.end_of_stream {
                        // 收到关闭信号，结束流
                        println!("DEBUG: 收到关闭信号，正常结束流");
                        return Poll::Ready(None);
                    } else {
                        return Poll::Ready(Some(Ok(msg)));
                    }
                } else {
                    // 这是普通数据（如序列化的 FileChunk）
                    println!("DEBUG: 收到普通数据块，大小: {} 字节", data.len());
                    return Poll::Ready(Some(Ok(GrpcStreamMessage { 
                        id: current_sequence,
                        stream_id: 1,
                        sequence: current_sequence,
                        data: data,
                        end_of_stream: false,
                        metadata: HashMap::new(),
                    })));
                }
            }
        }
        
        // 读取更多数据
        match this.body.poll_data(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                // 释放流控制容量
                if let Err(e) = this.body.flow_control().release_capacity(chunk.len()) {
                    println!("DEBUG: 释放流控制容量失败: {}", e);
                    return Poll::Ready(Some(Err(GrpcError::Internal(format!("释放流控制容量失败: {}", e)))));
                }
                
                this.buffer.extend_from_slice(&chunk);
                println!("DEBUG: 接收到 {} 字节数据，缓冲区总大小: {} 字节", chunk.len(), this.buffer.len());
                
                // 立即尝试解析消息，而不是返回 Pending
                // 这避免了无限循环问题
                if this.buffer.len() >= 5 {
                    let length = u32::from_be_bytes([
                        this.buffer[1],
                        this.buffer[2], 
                        this.buffer[3],
                        this.buffer[4]
                    ]) as usize;
                    
                    // 添加合理的长度限制，防止容量溢出（最大 100MB）
                    const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
                    if length > MAX_MESSAGE_SIZE {
                        return Poll::Ready(Some(Err(GrpcError::Internal(format!(
                            "gRPC 消息长度过大: {} 字节，最大允许: {} 字节", 
                            length, MAX_MESSAGE_SIZE
                        )))));
                    }
                    
                    if this.buffer.len() >= 5 + length {
                        // 有完整的消息，立即处理
                        let compressed = this.buffer[0] != 0;
                        if compressed {
                            return Poll::Ready(Some(Err(GrpcError::Unimplemented("不支持压缩的 gRPC 消息".to_string()))));
                        }
                        
                        let data = this.buffer[5..5 + length].to_vec();
                        this.buffer.drain(..5 + length);
                        
                        let current_sequence = *this.sequence;
                        *this.sequence += 1;
                        
                        // 尝试解析为 GrpcStreamMessage<Vec<u8>>（关闭信号）
                        if let Ok(stream_message) = GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(&data) {
                            // 这是一个关闭信号或其他流消息
                            let msg = stream_message;
                            println!("DEBUG: 收到流消息，end_of_stream: {}, 数据长度: {}", msg.end_of_stream, msg.data.len());
                            if msg.end_of_stream {
                                // 收到关闭信号，结束流
                                println!("DEBUG: 收到关闭信号，正常结束流");
                                return Poll::Ready(None);
                            } else {
                                return Poll::Ready(Some(Ok(msg)));
                            }
                        } else {
                            // 这是普通数据（如序列化的 FileChunk）
                            println!("DEBUG: 收到普通数据块，大小: {} 字节", data.len());
                            return Poll::Ready(Some(Ok(GrpcStreamMessage { 
                                id: current_sequence,
                                stream_id: 1,
                                sequence: current_sequence,
                                data: data,
                                end_of_stream: false,
                                metadata: HashMap::new(),
                            })));
                        }
                    }
                }
                
                // 数据不完整，继续等待
                Poll::Pending
            }
            Poll::Ready(Some(Err(e))) => {
                let error_msg = e.to_string();
                println!("DEBUG: 读取流数据失败: {}", error_msg);
                
                // 检查是否是客户端断开连接
                if error_msg.contains("stream no longer needed") || 
                   error_msg.contains("connection closed") ||
                   error_msg.contains("reset") ||
                   error_msg.contains("broken pipe") {
                    println!("DEBUG: 检测到客户端断开连接，正常结束流");
                    return Poll::Ready(None);
                }
                
                Poll::Ready(Some(Err(GrpcError::Internal(format!("读取流数据失败: {}", e)))))
            }
            Poll::Ready(None) => {
                println!("DEBUG: 流已结束（客户端断开连接）");
                if this.buffer.is_empty() {
                    println!("DEBUG: 缓冲区为空，正常结束流");
                    Poll::Ready(None)
                } else {
                    println!("DEBUG: 缓冲区中还有 {} 字节未处理数据，但客户端已断开", this.buffer.len());
                    // 客户端断开时，如果缓冲区中有数据，我们仍然正常结束流，而不是报错
                    // 这是一种容错处理，避免因网络问题导致的数据丢失被误判为错误
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Default for GrpcServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}