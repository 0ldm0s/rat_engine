//! SSE 连接池管理模块
//! 
//! 基于 gRPC 连接池架构，为 SSE 提供连接复用、负载均衡和资源管理功能
//! 支持 HTTP/1.1 和 HTTP/2（含 H2C）协议

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use dashmap::DashMap;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use hyper::{Response, StatusCode, HeaderMap};
use hyper::header::HeaderValue;
use hyper::body::{Bytes, Frame};
use http_body_util::StreamBody;
use futures_util::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use crossbeam::queue::SegQueue;
use tokio_stream::wrappers::UnboundedReceiverStream;
use crate::error::{RatError, RatResult};
use crate::server::streaming::StreamingBody;
use crate::utils::logger::{info, warn, debug, error};

/// SSE 连接信息
#[derive(Debug)]
pub struct SseConnection {
    /// 连接ID
    pub connection_id: String,
    /// 客户端地址
    pub client_addr: String,
    /// 协议版本（HTTP/1.1 或 HTTP/2）
    pub protocol_version: SseProtocolVersion,
    /// 连接类型（普通 TCP、TLS、H2C）
    pub connection_type: SseConnectionType,
    /// 数据发送器
    pub sender: mpsc::UnboundedSender<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
    /// 连接创建时间
    pub created_at: Instant,
    /// 最后活跃时间
    pub last_active: Instant,
    /// 连接状态
    pub is_active: bool,
    /// 发送消息计数
    pub message_count: AtomicU64,
    /// 连接任务句柄
    pub connection_handle: Option<tokio::task::JoinHandle<()>>,
    /// 用户ID（可选）
    pub user_id: Option<String>,
    /// 房间ID（可选）
    pub room_id: Option<String>,
    /// 连接元数据
    pub metadata: HashMap<String, String>,
}

/// SSE 协议版本
#[derive(Debug, Clone, PartialEq)]
pub enum SseProtocolVersion {
    /// HTTP/1.1
    Http1_1,
    /// HTTP/2
    Http2,
    /// HTTP/2 over cleartext (H2C)
    Http2Cleartext,
}

/// SSE 连接类型
#[derive(Debug, Clone, PartialEq)]
pub enum SseConnectionType {
    /// 普通 TCP 连接
    PlainTcp,
    /// TLS 加密连接
    Tls,
    /// H2C 连接
    H2c,
}

impl SseConnection {
    /// 创建新的 SSE 连接
    pub fn new(
        connection_id: String,
        client_addr: String,
        protocol_version: SseProtocolVersion,
        connection_type: SseConnectionType,
        sender: mpsc::UnboundedSender<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
    ) -> Self {
        let now = Instant::now();
        Self {
            connection_id,
            client_addr,
            protocol_version,
            connection_type,
            sender,
            created_at: now,
            last_active: now,
            is_active: true,
            message_count: AtomicU64::new(0),
            connection_handle: None,
            user_id: None,
            room_id: None,
            metadata: HashMap::new(),
        }
    }

    /// 发送 SSE 事件
    pub fn send_event(&self, event: &str, data: &str) -> Result<(), String> {
        if !self.is_active {
            return Err("连接已关闭".to_string());
        }

        let formatted = match self.protocol_version {
            SseProtocolVersion::Http1_1 => {
                format!("event: {}\ndata: {}\n\n", event, data)
            }
            SseProtocolVersion::Http2 | SseProtocolVersion::Http2Cleartext => {
                // HTTP/2 需要特殊处理，确保头部格式正确
                format!("event: {}\ndata: {}\n\n", event, data)
            }
        };

        self.sender
            .send(Ok(Frame::data(Bytes::from(formatted))))
            .map_err(|e| {
                error!("❌ SSE 事件发送失败: {:?}", e);
                "SSE 事件发送失败".to_string()
            })?;

        self.message_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 发送简单数据
    pub fn send_data(&self, data: &str) -> Result<(), String> {
        if !self.is_active {
            return Err("连接已关闭".to_string());
        }

        let formatted = format!("data: {}\n\n", data);
        self.sender
            .send(Ok(Frame::data(Bytes::from(formatted))))
            .map_err(|e| {
                error!("❌ SSE 数据发送失败: {:?}", e);
                "SSE 数据发送失败".to_string()
            })?;

        self.message_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 发送心跳
    pub fn send_heartbeat(&self) -> Result<(), String> {
        if !self.is_active {
            return Err("连接已关闭".to_string());
        }

        self.sender
            .send(Ok(Frame::data(Bytes::from(": heartbeat\n\n"))))
            .map_err(|e| {
                error!("❌ SSE 心跳发送失败: {:?}", e);
                "SSE 心跳发送失败".to_string()
            })
    }

    /// 更新最后活跃时间
    pub fn update_last_active(&mut self) {
        self.last_active = Instant::now();
    }

    /// 获取消息计数
    pub fn get_message_count(&self) -> u64 {
        self.message_count.load(Ordering::Relaxed)
    }

    /// 关闭连接
    pub fn close(&mut self) {
        self.is_active = false;
        // sender 会在 drop 时自动关闭
    }
}

/// SSE 连接池配置
#[derive(Debug, Clone)]
pub struct SseConnectionPoolConfig {
    /// 最大连接数
    pub max_connections: usize,
    /// 连接空闲超时时间
    pub idle_timeout: Duration,
    /// 心跳间隔
    pub heartbeat_interval: Duration,
    /// 清理间隔
    pub cleanup_interval: Duration,
    /// 每个用户的最大连接数
    pub max_connections_per_user: usize,
    /// 每个房间的最大连接数
    pub max_connections_per_room: usize,
    /// 是否启用心跳
    pub enable_heartbeat: bool,
    /// 是否启用连接统计
    pub enable_statistics: bool,
}

impl Default for SseConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            idle_timeout: Duration::from_secs(300), // 5分钟
            heartbeat_interval: Duration::from_secs(30), // 30秒
            cleanup_interval: Duration::from_secs(60), // 1分钟
            max_connections_per_user: 10,
            max_connections_per_room: 1000,
            enable_heartbeat: true,
            enable_statistics: true,
        }
    }
}

/// SSE 连接池管理器
/// 基于 gRPC 连接池架构，提供连接复用、负载均衡和资源管理
pub struct SseConnectionPool {
    /// 活跃连接（连接ID -> 连接信息）
    connections: Arc<DashMap<String, SseConnection>>,
    /// 用户连接映射（用户ID -> 连接ID列表）
    user_connections: Arc<DashMap<String, Vec<String>>>,
    /// 房间连接映射（房间ID -> 连接ID列表）
    room_connections: Arc<DashMap<String, Vec<String>>>,
    /// 连接ID生成器
    connection_id_counter: Arc<AtomicU64>,
    /// 消息历史（无锁队列）
    message_history: Arc<SegQueue<SseMessage>>,
    /// 配置
    config: SseConnectionPoolConfig,
    /// 维护任务句柄
    maintenance_handle: Option<tokio::task::JoinHandle<()>>,
    /// 关闭信号发送器
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// 连接统计
    statistics: Arc<SseConnectionStatistics>,
}

/// SSE 消息
#[derive(Debug, Clone)]
pub struct SseMessage {
    /// 消息ID
    pub message_id: String,
    /// 事件类型
    pub event_type: Option<String>,
    /// 数据内容
    pub data: String,
    /// 时间戳
    pub timestamp: Instant,
    /// 目标连接ID（可选）
    pub target_connection_id: Option<String>,
    /// 目标用户ID（可选）
    pub target_user_id: Option<String>,
    /// 目标房间ID（可选）
    pub target_room_id: Option<String>,
}

/// SSE 连接统计
#[derive(Debug, Default)]
pub struct SseConnectionStatistics {
    /// 总连接数
    pub total_connections: AtomicU64,
    /// 活跃连接数
    pub active_connections: AtomicU64,
    /// 总消息数
    pub total_messages: AtomicU64,
    /// HTTP/1.1 连接数
    pub http1_connections: AtomicU64,
    /// HTTP/2 连接数
    pub http2_connections: AtomicU64,
    /// H2C 连接数
    pub h2c_connections: AtomicU64,
}

impl SseConnectionPool {
    /// 创建新的 SSE 连接池
    pub fn new(config: SseConnectionPoolConfig) -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            user_connections: Arc::new(DashMap::new()),
            room_connections: Arc::new(DashMap::new()),
            connection_id_counter: Arc::new(AtomicU64::new(1)),
            message_history: Arc::new(SegQueue::new()),
            config,
            maintenance_handle: None,
            shutdown_tx: None,
            statistics: Arc::new(SseConnectionStatistics::default()),
        }
    }

    /// 启动连接池维护任务
    pub fn start_maintenance_tasks(&mut self) {
        if self.maintenance_handle.is_some() {
            return; // 已经启动
        }

        let connections = self.connections.clone();
        let user_connections = self.user_connections.clone();
        let room_connections = self.room_connections.clone();
        let config = self.config.clone();
        let statistics = self.statistics.clone();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            let mut cleanup_interval = interval(config.cleanup_interval);
            let mut heartbeat_interval = interval(config.heartbeat_interval);

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("🛑 SSE 连接池维护任务收到关闭信号");
                        break;
                    }
                    _ = cleanup_interval.tick() => {
                        Self::cleanup_expired_connections(&connections, &user_connections, &room_connections, &config, &statistics).await;
                    }
                    _ = heartbeat_interval.tick() => {
                        if config.enable_heartbeat {
                            Self::send_heartbeat_to_all(&connections).await;
                        }
                    }
                }
            }

            info!("✅ SSE 连接池维护任务已停止");
        });

        self.maintenance_handle = Some(handle);
    }

    /// 创建新的 SSE 连接
    pub async fn create_connection(
        &self,
        client_addr: String,
        protocol_version: SseProtocolVersion,
        connection_type: SseConnectionType,
        user_id: Option<String>,
        room_id: Option<String>,
    ) -> RatResult<(String, mpsc::UnboundedReceiver<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>)> {
        // 检查连接数限制
        if !self.can_create_new_connection(&user_id, &room_id) {
            return Err(RatError::NetworkError("连接数超限".to_string()));
        }

        // 生成连接ID
        let connection_id = format!("sse_{}", self.connection_id_counter.fetch_add(1, Ordering::Relaxed));

        // 创建通道
        let (sender, receiver) = mpsc::unbounded_channel();

        // 创建连接
        let mut connection = SseConnection::new(
            connection_id.clone(),
            client_addr,
            protocol_version.clone(),
            connection_type,
            sender,
        );

        connection.user_id = user_id.clone();
        connection.room_id = room_id.clone();

        // 添加到连接池
        self.connections.insert(connection_id.clone(), connection);

        // 更新用户连接映射
        if let Some(user_id) = &user_id {
            self.user_connections
                .entry(user_id.clone())
                .or_insert_with(Vec::new)
                .push(connection_id.clone());
        }

        // 更新房间连接映射
        if let Some(room_id) = &room_id {
            self.room_connections
                .entry(room_id.clone())
                .or_insert_with(Vec::new)
                .push(connection_id.clone());
        }

        // 更新统计
        self.statistics.total_connections.fetch_add(1, Ordering::Relaxed);
        self.statistics.active_connections.fetch_add(1, Ordering::Relaxed);
        
        match protocol_version {
            SseProtocolVersion::Http1_1 => {
                self.statistics.http1_connections.fetch_add(1, Ordering::Relaxed);
            }
            SseProtocolVersion::Http2 => {
                self.statistics.http2_connections.fetch_add(1, Ordering::Relaxed);
            }
            SseProtocolVersion::Http2Cleartext => {
                self.statistics.h2c_connections.fetch_add(1, Ordering::Relaxed);
            }
        }

        info!("✅ 创建 SSE 连接: {} (协议: {:?}, 用户: {:?}, 房间: {:?})", 
            connection_id, protocol_version, user_id, room_id);

        Ok((connection_id, receiver))
    }

    /// 构建协议特定的 SSE 响应
    pub fn build_sse_response(
        &self,
        protocol_version: &SseProtocolVersion,
        receiver: mpsc::UnboundedReceiver<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
    ) -> crate::error::RatResult<Response<StreamingBody>> {
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(receiver);
        
        let mut response = Response::builder().status(StatusCode::OK);

        // 根据协议版本设置不同的头部
        match protocol_version {
            SseProtocolVersion::Http1_1 => {
                response = response
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .header("connection", "keep-alive")
                    .header("access-control-allow-origin", "*");
            }
            SseProtocolVersion::Http2 | SseProtocolVersion::Http2Cleartext => {
                // HTTP/2 不支持 Connection 头部
                response = response
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .header("access-control-allow-origin", "*");
            }
        }

        let body = StreamBody::new(Box::pin(stream) as Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> + Send + Sync>>);
        response.body(body).map_err(|e| {
            error!("❌ 构建 SSE 响应失败: {}", e);
            crate::error::RatError::NetworkError(format!("构建 SSE 响应失败: {}", e))
        })
    }

    /// 检查是否可以创建新连接
    fn can_create_new_connection(&self, user_id: &Option<String>, room_id: &Option<String>) -> bool {
        // 检查总连接数
        if self.connections.len() >= self.config.max_connections {
            return false;
        }

        // 检查用户连接数
        if let Some(user_id) = user_id {
            if let Some(user_conns) = self.user_connections.get(user_id) {
                if user_conns.len() >= self.config.max_connections_per_user {
                    return false;
                }
            }
        }

        // 检查房间连接数
        if let Some(room_id) = room_id {
            if let Some(room_conns) = self.room_connections.get(room_id) {
                if room_conns.len() >= self.config.max_connections_per_room {
                    return false;
                }
            }
        }

        true
    }

    /// 清理过期连接
    async fn cleanup_expired_connections(
        connections: &Arc<DashMap<String, SseConnection>>,
        user_connections: &Arc<DashMap<String, Vec<String>>>,
        room_connections: &Arc<DashMap<String, Vec<String>>>,
        config: &SseConnectionPoolConfig,
        statistics: &Arc<SseConnectionStatistics>,
    ) {
        let now = Instant::now();
        let mut expired_connections = Vec::new();

        // 查找过期连接
        for entry in connections.iter() {
            let connection = entry.value();
            if !connection.is_active || now.duration_since(connection.last_active) > config.idle_timeout {
                expired_connections.push(connection.connection_id.clone());
            }
        }

        // 清理过期连接
        for connection_id in expired_connections {
            if let Some((_, mut connection)) = connections.remove(&connection_id) {
                connection.close();
                
                // 从用户连接映射中移除
                if let Some(user_id) = &connection.user_id {
                    if let Some(mut user_conns) = user_connections.get_mut(user_id) {
                        user_conns.retain(|id| id != &connection_id);
                        if user_conns.is_empty() {
                            drop(user_conns);
                            user_connections.remove(user_id);
                        }
                    }
                }

                // 从房间连接映射中移除
                if let Some(room_id) = &connection.room_id {
                    if let Some(mut room_conns) = room_connections.get_mut(room_id) {
                        room_conns.retain(|id| id != &connection_id);
                        if room_conns.is_empty() {
                            drop(room_conns);
                            room_connections.remove(room_id);
                        }
                    }
                }

                statistics.active_connections.fetch_sub(1, Ordering::Relaxed);
                debug!("🧹 清理过期 SSE 连接: {}", connection_id);
            }
        }
    }

    /// 向所有连接发送心跳
    async fn send_heartbeat_to_all(connections: &Arc<DashMap<String, SseConnection>>) {
        for entry in connections.iter() {
            let connection = entry.value();
            if connection.is_active {
                let _ = connection.send_heartbeat();
            }
        }
    }

    /// 获取连接统计
    pub fn get_statistics(&self) -> SseConnectionStatistics {
        SseConnectionStatistics {
            total_connections: AtomicU64::new(self.statistics.total_connections.load(Ordering::Relaxed)),
            active_connections: AtomicU64::new(self.statistics.active_connections.load(Ordering::Relaxed)),
            total_messages: AtomicU64::new(self.statistics.total_messages.load(Ordering::Relaxed)),
            http1_connections: AtomicU64::new(self.statistics.http1_connections.load(Ordering::Relaxed)),
            http2_connections: AtomicU64::new(self.statistics.http2_connections.load(Ordering::Relaxed)),
            h2c_connections: AtomicU64::new(self.statistics.h2c_connections.load(Ordering::Relaxed)),
        }
    }

    /// 广播消息到所有连接
    pub async fn broadcast_to_all(&self, event: &str, data: &str) -> usize {
        let mut sent_count = 0;
        for entry in self.connections.iter() {
            let connection = entry.value();
            if connection.is_active {
                if connection.send_event(event, data).is_ok() {
                    sent_count += 1;
                }
            }
        }
        sent_count
    }

    /// 向特定用户发送消息
    pub async fn send_to_user(&self, user_id: &str, event: &str, data: &str) -> usize {
        let mut sent_count = 0;
        if let Some(connection_ids) = self.user_connections.get(user_id) {
            for connection_id in connection_ids.iter() {
                if let Some(connection) = self.connections.get(connection_id) {
                    if connection.is_active {
                        if connection.send_event(event, data).is_ok() {
                            sent_count += 1;
                        }
                    }
                }
            }
        }
        sent_count
    }

    /// 向特定房间发送消息
    pub async fn send_to_room(&self, room_id: &str, event: &str, data: &str) -> usize {
        let mut sent_count = 0;
        if let Some(connection_ids) = self.room_connections.get(room_id) {
            for connection_id in connection_ids.iter() {
                if let Some(connection) = self.connections.get(connection_id) {
                    if connection.is_active {
                        if connection.send_event(event, data).is_ok() {
                            sent_count += 1;
                        }
                    }
                }
            }
        }
        sent_count
    }
}

impl Drop for SseConnectionPool {
    fn drop(&mut self) {
        // 关闭所有连接
        for mut entry in self.connections.iter_mut() {
            entry.value_mut().close();
        }
    }
}