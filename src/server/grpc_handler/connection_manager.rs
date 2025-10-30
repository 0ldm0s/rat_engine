use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use dashmap::DashMap;
use crossbeam_queue::SegQueue;
use h2::server::SendResponse;
use tokio::sync::broadcast;
use bytes;
use crate::server::grpc_types::*;
use crate::utils::logger::{info, warn, debug};
use super::types::{GrpcConnection, GrpcConnectionType};

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
