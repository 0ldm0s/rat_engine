//! RAT Engine gRPC 队列桥接适配器
//! 
//! 基于 mammoth_transport 的设计理念，实现真正的委托模式
//! 
//! ## 架构设计
//! 1. **完全委托**：不持有连接状态，完全委托给队列桥接适配器
//! 2. **消息驱动**：通过队列消息进行通信，而不是直接调用
//! 3. **连接管理**：由 Rust 层负责连接生命周期管理
//! 4. **业务分离**：Python 层只处理业务逻辑，不涉及传输层

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crossbeam_queue::SegQueue;
use tokio::sync::{RwLock, mpsc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use bytes::Bytes;

use crate::error::{RatResult, RatError};
use crate::server::grpc_types::GrpcContext;
use crate::utils::logger::{info, warn, error, debug};

/// 连接 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(String);

impl ConnectionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 请求类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestType {
    /// HTTP 请求
    Http,
    /// gRPC 一元调用
    GrpcUnary,
    /// gRPC 服务端流
    GrpcServerStreaming,
    /// gRPC 客户端流
    GrpcClientStreaming,
    /// gRPC 双向流
    GrpcBidirectionalStreaming,
}

/// 请求数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestData {
    /// HTTP 方法
    pub method: Option<String>,
    /// 请求路径
    pub path: String,
    /// 请求头
    pub headers: HashMap<String, String>,
    /// 请求体
    pub body: Vec<u8>,
    /// gRPC 服务名
    pub service: Option<String>,
    /// gRPC 方法名
    pub grpc_method: Option<String>,
    /// 查询参数
    pub query_params: HashMap<String, String>,
}

/// 响应数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseData {
    /// 状态码
    pub status_code: u16,
    /// 响应头
    pub headers: HashMap<String, String>,
    /// 响应体
    pub body: Vec<u8>,
    /// gRPC 状态信息
    pub grpc_status: Option<GrpcStatusInfo>,
}

/// gRPC 状态信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcStatusInfo {
    /// 状态码
    pub code: u32,
    /// 状态消息
    pub message: String,
    /// 详细信息
    pub details: Vec<u8>,
}

/// 从 Rust 传输层发送到 Python 引擎的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportToEngineMessage {
    /// 新连接建立
    ConnectionEstablished {
        connection_id: ConnectionId,
        protocol: String,
        remote_addr: String,
        local_addr: String,
    },
    
    /// 连接断开
    ConnectionClosed {
        connection_id: ConnectionId,
        reason: String,
    },
    
    /// 收到请求
    RequestReceived {
        connection_id: ConnectionId,
        request_id: String,
        request_type: RequestType,
        request_data: RequestData,
    },
    
    /// 收到流数据
    StreamDataReceived {
        connection_id: ConnectionId,
        request_id: String,
        data: Vec<u8>,
        is_end: bool,
    },
}

/// 从 Python 引擎发送到 Rust 传输层的消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineToTransportMessage {
    /// 发送响应
    SendResponse {
        connection_id: ConnectionId,
        request_id: String,
        response: ResponseData,
    },
    
    /// 发送流数据
    SendStreamData {
        connection_id: ConnectionId,
        request_id: String,
        data: Vec<u8>,
        is_end: bool,
    },
    
    /// 关闭连接
    CloseConnection {
        connection_id: ConnectionId,
        reason: String,
    },
}

/// 连接信息
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: ConnectionId,
    pub protocol: String,
    pub remote_addr: String,
    pub local_addr: String,
    pub created_at: Instant,
    pub last_activity: Instant,
}

/// 请求信息
#[derive(Debug, Clone)]
pub struct RequestInfo {
    pub request_id: String,
    pub connection_id: ConnectionId,
    pub request_type: RequestType,
    pub created_at: Instant,
    pub timeout: Duration,
}

/// 队列桥接适配器配置
#[derive(Debug, Clone)]
pub struct QueueBridgeConfig {
    /// 队列名称
    pub queue_name: String,
    /// 最大队列大小
    pub max_queue_size: usize,
    /// 消息超时时间
    pub message_timeout: Duration,
    /// 连接超时时间
    pub connection_timeout: Duration,
    /// 清理间隔
    pub cleanup_interval: Duration,
}

impl Default for QueueBridgeConfig {
    fn default() -> Self {
        Self {
            queue_name: "rat_engine_grpc_bridge".to_string(),
            max_queue_size: 10000,
            message_timeout: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(300),
            cleanup_interval: Duration::from_secs(60),
        }
    }
}

/// 队列桥接适配器
/// 
/// 实现 Rust 传输层和 Python 引擎的完全解耦
pub struct QueueBridgeAdapter {
    /// 适配器配置
    config: QueueBridgeConfig,
    /// 从传输层到引擎的消息队列
    transport_to_engine: Arc<SegQueue<TransportToEngineMessage>>,
    /// 从引擎到传输层的消息队列
    engine_to_transport: Arc<SegQueue<EngineToTransportMessage>>,
    /// 活跃连接映射
    active_connections: Arc<RwLock<HashMap<ConnectionId, ConnectionInfo>>>,
    /// 待处理请求映射
    pending_requests: Arc<RwLock<HashMap<String, RequestInfo>>>,
    /// 运行状态
    running: Arc<RwLock<bool>>,
}

impl QueueBridgeAdapter {
    /// 创建新的队列桥接适配器
    pub fn new(config: QueueBridgeConfig) -> Self {
        Self {
            config,
            transport_to_engine: Arc::new(SegQueue::new()),
            engine_to_transport: Arc::new(SegQueue::new()),
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动队列桥接适配器
    pub async fn start(&self) -> RatResult<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(RatError::ConfigError("队列桥接适配器已经在运行".to_string()));
        }
        
        *running = true;
        
        // 启动清理任务
        let active_connections = self.active_connections.clone();
        let pending_requests = self.pending_requests.clone();
        let running_clone = self.running.clone();
        let cleanup_interval = self.config.cleanup_interval;
        let connection_timeout = self.config.connection_timeout;
        let message_timeout = self.config.message_timeout;
        
        tokio::spawn(async move {
            while *running_clone.read().await {
                tokio::time::sleep(cleanup_interval).await;
                
                let now = Instant::now();
                
                // 清理超时连接
                {
                    let mut connections = active_connections.write().await;
                    connections.retain(|_, conn| {
                        now.duration_since(conn.last_activity) < connection_timeout
                    });
                }
                
                // 清理超时请求
                {
                    let mut requests = pending_requests.write().await;
                    requests.retain(|_, req| {
                        now.duration_since(req.created_at) < message_timeout
                    });
                }
            }
        });
        
        info!("🌉 [QueueBridgeAdapter] 队列桥接适配器已启动");
        Ok(())
    }

    /// 停止队列桥接适配器
    pub async fn stop(&self) -> RatResult<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        
        *running = false;
        info!("🛑 [QueueBridgeAdapter] 队列桥接适配器已停止");
        Ok(())
    }

    /// 通知连接建立
    pub async fn notify_connection_established(
        &self,
        connection_id: ConnectionId,
        protocol: String,
        remote_addr: String,
        local_addr: String,
    ) -> RatResult<()> {
        // 记录连接信息
        {
            let mut connections = self.active_connections.write().await;
            connections.insert(connection_id.clone(), ConnectionInfo {
                connection_id: connection_id.clone(),
                protocol: protocol.clone(),
                remote_addr: remote_addr.clone(),
                local_addr: local_addr.clone(),
                created_at: Instant::now(),
                last_activity: Instant::now(),
            });
        }
        
        // 发送连接建立消息
        let message = TransportToEngineMessage::ConnectionEstablished {
            connection_id,
            protocol,
            remote_addr,
            local_addr,
        };
        
        self.transport_to_engine.push(message);
        Ok(())
    }

    /// 通知连接关闭
    pub async fn notify_connection_closed(
        &self,
        connection_id: ConnectionId,
        reason: String,
    ) -> RatResult<()> {
        // 移除连接信息
        {
            let mut connections = self.active_connections.write().await;
            connections.remove(&connection_id);
        }
        
        // 发送连接关闭消息
        let message = TransportToEngineMessage::ConnectionClosed {
            connection_id,
            reason,
        };
        
        self.transport_to_engine.push(message);
        Ok(())
    }

    /// 通知收到请求
    pub async fn notify_request_received(
        &self,
        connection_id: ConnectionId,
        request_id: String,
        request_type: RequestType,
        request_data: RequestData,
    ) -> RatResult<()> {
        // 记录请求信息
        {
            let mut requests = self.pending_requests.write().await;
            requests.insert(request_id.clone(), RequestInfo {
                request_id: request_id.clone(),
                connection_id: connection_id.clone(),
                request_type: request_type.clone(),
                created_at: Instant::now(),
                timeout: self.config.message_timeout,
            });
        }
        
        // 更新连接活动时间
        {
            let mut connections = self.active_connections.write().await;
            if let Some(conn) = connections.get_mut(&connection_id) {
                conn.last_activity = Instant::now();
            }
        }
        
        // 发送请求接收消息
        let message = TransportToEngineMessage::RequestReceived {
            connection_id,
            request_id,
            request_type,
            request_data,
        };
        
        self.transport_to_engine.push(message);
        Ok(())
    }

    /// 通知收到流数据
    pub async fn notify_stream_data_received(
        &self,
        connection_id: ConnectionId,
        request_id: String,
        data: Vec<u8>,
        is_end: bool,
    ) -> RatResult<()> {
        // 更新连接活动时间
        {
            let mut connections = self.active_connections.write().await;
            if let Some(conn) = connections.get_mut(&connection_id) {
                conn.last_activity = Instant::now();
            }
        }
        
        // 如果是流结束，清理请求信息
        if is_end {
            let mut requests = self.pending_requests.write().await;
            requests.remove(&request_id);
        }
        
        // 发送流数据消息
        let message = TransportToEngineMessage::StreamDataReceived {
            connection_id,
            request_id,
            data,
            is_end,
        };
        
        self.transport_to_engine.push(message);
        Ok(())
    }

    /// 获取从传输层到引擎的队列引用
    /// 供 Python 引擎拉取消息使用
    pub fn get_transport_to_engine_queue(&self) -> Arc<SegQueue<TransportToEngineMessage>> {
        self.transport_to_engine.clone()
    }

    /// 获取从引擎到传输层的队列引用
    /// 供 Python 引擎推送响应使用
    pub fn get_engine_to_transport_queue(&self) -> Arc<SegQueue<EngineToTransportMessage>> {
        self.engine_to_transport.clone()
    }

    /// 处理来自引擎的消息
    pub async fn process_engine_messages(&self) -> RatResult<Vec<EngineToTransportMessage>> {
        let mut messages = Vec::new();
        
        while let Some(message) = self.engine_to_transport.pop() {
            messages.push(message);
        }
        
        Ok(messages)
    }

    /// 获取统计信息
    pub async fn get_stats(&self) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        
        let connections = self.active_connections.read().await;
        let requests = self.pending_requests.read().await;
        
        stats.insert("active_connections".to_string(), connections.len() as u64);
        stats.insert("pending_requests".to_string(), requests.len() as u64);
        stats.insert("transport_to_engine_queue_size".to_string(), self.transport_to_engine.len() as u64);
        stats.insert("engine_to_transport_queue_size".to_string(), self.engine_to_transport.len() as u64);
        
        stats
    }
}