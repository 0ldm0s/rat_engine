//! 全局 SSE 管理器
//!
//! 独立的 SSE 连接管理，不依赖 SseResponse 的存储逻辑

use dashmap::DashMap;
use std::sync::Arc;
use hyper::{Response, StatusCode};
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use crate::server::streaming::{StreamingResponse, StreamingBody};
use crate::utils::logger::{info, debug, warn};

/// 全局 SSE 管理器
///
/// 只存储 sender，避免 receiver 的所有权问题
pub struct GlobalSseManager {
    /// 连接映射表：connection_id -> Arc<sender>
    connections: Arc<DashMap<String, Arc<mpsc::UnboundedSender<Result<hyper::body::Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>>>>,
}

impl GlobalSseManager {
    /// 创建新的全局 SSE 管理器
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }

    /// 注册 SSE 连接
    ///
    /// 在管理器内部创建通道，构建响应并存储 sender
    ///
    /// # 参数
    /// * `connection_id` - 连接ID，由调用者自定义
    ///
    /// # 返回值
    /// 构建好的 SSE 响应
    pub fn register_connection(&self, connection_id: String) -> Result<Response<StreamingBody>, hyper::Error> {
        // 创建通道
        let (sender, receiver) = mpsc::unbounded_channel();

        // 存储sender
        self.connections.insert(connection_id.clone(), Arc::new(sender));

        // 构建响应流
        let stream = UnboundedReceiverStream::new(receiver);

        let response = StreamingResponse::new()
            .status(StatusCode::OK)
            .with_header("Content-Type", "text/event-stream")
            .with_header("Cache-Control", "no-cache")
            .with_header("Connection", "keep-alive")
            .with_header("Access-Control-Allow-Origin", "*")
            .stream(stream)
            .build();

        info!("🔗 [全局SSE管理器] 创建并注册连接: {}", connection_id);
        response
    }

    /// 发送 SSE 事件
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    /// * `event` - 事件类型
    /// * `data` - 事件数据
    ///
    /// # 返回值
    /// * `Ok(())` - 发送成功
    /// * `Err(String)` - 发送失败
    pub fn send_event(&self, connection_id: &str, event: &str, data: &str) -> Result<(), String> {
        if let Some(sender) = self.connections.get(connection_id) {
            let formatted = format!("event: {}\ndata: {}\n\n", event, data);
            sender
                .send(Ok(hyper::body::Frame::data(Bytes::from(formatted))))
                .map_err(|e| format!("发送SSE事件失败: {:?}", e))
        } else {
            Err("连接不存在".to_string())
        }
    }

    /// 发送简单数据
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    /// * `data` - 数据
    ///
    /// # 返回值
    /// * `Ok(())` - 发送成功
    /// * `Err(String)` - 发送失败
    pub fn send_data(&self, connection_id: &str, data: &str) -> Result<(), String> {
        if let Some(sender) = self.connections.get(connection_id) {
            let formatted = format!("data: {}\n\n", data);
            sender
                .send(Ok(hyper::body::Frame::data(Bytes::from(formatted))))
                .map_err(|e| format!("发送SSE数据失败: {:?}", e))
        } else {
            Err("连接不存在".to_string())
        }
    }

    /// 发送心跳
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    ///
    /// # 返回值
    /// * `Ok(())` - 发送成功
    /// * `Err(String)` - 发送失败
    pub fn send_heartbeat(&self, connection_id: &str) -> Result<(), String> {
        if let Some(sender) = self.connections.get(connection_id) {
            sender
                .send(Ok(hyper::body::Frame::data(Bytes::from(": heartbeat\n\n"))))
                .map_err(|e| format!("发送心跳失败: {:?}", e))
        } else {
            Err("连接不存在".to_string())
        }
    }

    /// 主动断开 SSE 连接
    ///
    /// 发送断开事件后移除连接，不管发送成功或失败都会移除
    /// ⚠️ **重要：这是在SSE发送失败时应该调用的完整方法！**
    /// 包含发送断开事件、关闭发送器等完整清理流程
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    ///
    /// # 返回值
    /// * `true` - 连接存在并已断开
    /// * `false` - 连接不存在
    pub fn disconnect_connection(&self, connection_id: &str) -> bool {
        if let Some((_, sender)) = self.connections.remove(connection_id) {
            // 发送断开事件（不管成功失败）
            let _ = sender.send(Ok(hyper::body::Frame::data(Bytes::from("event: disconnect\ndata: 服务器断开连接\n\n"))));
            let _ = sender.send(Ok(hyper::body::Frame::data(Bytes::from("DISCONNECT_EVENT"))));

            // 关闭发送器
            drop(sender);

            info!("🔌 [全局SSE管理器] 主动断开连接: {}", connection_id);
            true
        } else {
            warn!("🔍 [全局SSE管理器] 尝试断开不存在的连接: {}", connection_id);
            false
        }
    }

    /// 移除连接（内部方法）
    ///
    /// ⚠️ **注意：这只是移除映射关系，不包含完整清理流程！**
    /// 仅用于内部清理，外部请使用 disconnect_connection
    /// 发送失败时应该使用 disconnect_connection 而不是这个方法！
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    ///
    /// # 返回值
    /// * `true` - 连接存在并已移除
    /// * `false` - 连接不存在
    pub(crate) fn remove_connection(&self, connection_id: &str) -> bool {
        let removed = self.connections.remove(connection_id).is_some();
        if removed {
            info!("🗑️ [全局SSE管理器] 移除连接: {}", connection_id);
        }
        removed
    }

    /// 广播消息到所有连接
    ///
    /// # 参数
    /// * `event` - 事件类型，允许为空
    /// * `data` - 消息数据
    ///
    /// # 返回值
    /// 返回成功发送的连接数量
    pub fn broadcast(&self, event: &str, data: &str) -> usize {
        let mut success_count = 0;
        let mut failed_connections = Vec::new();

        for entry in self.connections.iter() {
            let connection_id = entry.key();
            let sender = entry.value();

            let result = if event.is_empty() {
                self.send_data(connection_id, data)
            } else {
                self.send_event(connection_id, event, data)
            };

            match result {
                Ok(()) => {
                    success_count += 1;
                }
                Err(_) => {
                    failed_connections.push(connection_id.to_string());
                }
            }
        }

        // 移除失败的连接
        for failed_id in failed_connections {
            self.remove_connection(&failed_id);
            warn!("❌ [全局SSE管理器] 移除失效连接: {}", failed_id);
        }

        info!("📡 [全局SSE管理器] 广播消息: event='{}', 成功连接数={}", event, success_count);
        success_count
    }

    /// 获取连接统计
    ///
    /// # 返回值
    /// 返回当前活跃连接数量
    pub fn get_connection_count(&self) -> usize {
        self.connections.len()
    }

    /// 检查连接是否存在
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    ///
    /// # 返回值
    /// * `true` - 连接存在
    /// * `false` - 连接不存在
    pub fn has_connection(&self, connection_id: &str) -> bool {
        self.connections.contains_key(connection_id)
    }

    /// 清空所有连接
    pub fn clear(&self) {
        let count = self.connections.len();
        self.connections.clear();
        info!("🧹 [全局SSE管理器] 清空所有连接，清理了 {} 个连接", count);
    }
}

impl Default for GlobalSseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for GlobalSseManager {
    fn drop(&mut self) {
        info!("🛑 [全局SSE管理器] 已关闭");
    }
}

/// 全局 SSE 管理器实例
static GLOBAL_SSE_MANAGER: std::sync::OnceLock<Arc<GlobalSseManager>> = std::sync::OnceLock::new();

/// 获取全局 SSE 管理器实例
pub fn get_global_sse_manager() -> Arc<GlobalSseManager> {
    GLOBAL_SSE_MANAGER.get_or_init(|| {
        Arc::new(GlobalSseManager::new())
    }).clone()
}

/// 便捷函数：向特定连接发送消息（无事件类型）
pub fn send_sse_message(connection_id: &str, data: &str) -> Result<(), String> {
    let manager = get_global_sse_manager();
    manager.send_data(connection_id, data)
}

/// 便捷函数：向特定连接发送消息（带事件类型）
pub fn send_sse_message_with_type(connection_id: &str, event_type: &str, data: &str) -> Result<(), String> {
    let manager = get_global_sse_manager();
    manager.send_event(connection_id, event_type, data)
}

/// 便捷函数：广播 SSE 消息（无事件类型）
pub fn broadcast_sse_message(data: &str) -> usize {
    let manager = get_global_sse_manager();
    manager.broadcast("", data)
}

/// 便捷函数：广播 SSE 消息（带事件类型）
pub fn broadcast_sse_message_with_type(event_type: &str, data: &str) -> usize {
    let manager = get_global_sse_manager();
    manager.broadcast(event_type, data)
}

/// 便捷函数：注册 SSE 连接
pub fn register_sse_connection(connection_id: String) -> Result<Response<StreamingBody>, hyper::Error> {
    let manager = get_global_sse_manager();
    manager.register_connection(connection_id)
}

/// 便捷函数：主动断开 SSE 连接
pub fn disconnect_sse_connection(connection_id: &str) -> bool {
    let manager = get_global_sse_manager();
    manager.disconnect_connection(connection_id)
}

/// 便捷函数：获取连接数量
pub fn get_sse_connection_count() -> usize {
    let manager = get_global_sse_manager();
    manager.get_connection_count()
}