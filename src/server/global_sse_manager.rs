//! 全局 SSE 管理器
//!
//! 基于无锁 DashMap 的高性能 SSE 连接管理

use dashmap::DashMap;
use std::sync::Arc;
use crate::server::streaming::SseResponse;
use crate::utils::logger::{info, debug, warn};

/// 全局 SSE 管理器
///
/// 提供无锁的 SSE 连接注册和消息发送功能
pub struct GlobalSseManager {
    /// 连接映射表：connection_id -> SseResponse
    connections: Arc<DashMap<String, SseResponse>>,
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
    /// # 参数
    /// * `connection_id` - 连接ID，由调用者自定义
    /// * `sse_response` - SSE响应实例
    pub fn register_connection(&self, connection_id: String, sse_response: SseResponse) {
        self.connections.insert(connection_id.clone(), sse_response);
        info!("🔗 [全局SSE管理器] 注册连接: {}", connection_id);
    }

    /// 发送消息到指定连接（带事件类型）
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    /// * `event_type` - 事件类型，允许为空
    /// * `data` - 消息数据
    ///
    /// # 返回值
    /// * `Ok(())` - 发送成功
    /// * `Err(String)` - 发送失败（连接不存在或发送失败）
    pub fn send_to_connection_with_type(
        &self,
        connection_id: &str,
        event_type: &str,
        data: &str,
    ) -> Result<(), String> {
        if let Some(sse) = self.connections.get(connection_id) {
            let result = if event_type.is_empty() {
                sse.send_data(data)
            } else {
                sse.send_event(event_type, data)
            };

            match &result {
                Ok(()) => {
                    debug!("📤 [全局SSE管理器] 发送消息到连接 {}: event='{}', data='{}'",
                           connection_id, event_type, data);
                }
                Err(e) => {
                    warn!("❌ [全局SSE管理器] 发送消息失败到连接 {}: {}", connection_id, e);
                    // 如果发送失败，可能是连接已断开，移除连接
                    self.remove_connection(connection_id);
                }
            }

            result
        } else {
            warn!("🔍 [全局SSE管理器] 连接不存在: {}", connection_id);
            Err("Connection not found".to_string())
        }
    }

    /// 发送消息到指定连接（简化接口，无事件类型）
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    /// * `data` - 消息数据
    ///
    /// # 返回值
    /// * `Ok(())` - 发送成功
    /// * `Err(String)` - 发送失败
    pub fn send_to_connection(&self, connection_id: &str, data: &str) -> Result<(), String> {
        self.send_to_connection_with_type(connection_id, "", data)
    }

    /// 广播消息到所有连接
    ///
    /// # 参数
    /// * `event_type` - 事件类型，允许为空
    /// * `data` - 消息数据
    ///
    /// # 返回值
    /// 返回成功发送的连接数量
    pub fn broadcast_with_type(&self, event_type: &str, data: &str) -> usize {
        let mut success_count = 0;
        let mut failed_connections = Vec::new();

        for entry in self.connections.iter() {
            let connection_id = entry.key();
            let sse = entry.value();

            let result = if event_type.is_empty() {
                sse.send_data(data)
            } else {
                sse.send_event(event_type, data)
            };

            match result {
                Ok(()) => {
                    success_count += 1;
                }
                Err(_) => {
                    failed_connections.push(connection_id.clone());
                }
            }
        }

        // 移除失败的连接
        for failed_id in failed_connections {
            self.remove_connection(&failed_id);
            warn!("❌ [全局SSE管理器] 移除失效连接: {}", failed_id);
        }

        info!("📡 [全局SSE管理器] 广播消息: event='{}', 成功连接数={}", event_type, success_count);
        success_count
    }

    /// 广播消息到所有连接（简化接口，无事件类型）
    ///
    /// # 参数
    /// * `data` - 消息数据
    ///
    /// # 返回值
    /// 返回成功发送的连接数量
    pub fn broadcast(&self, data: &str) -> usize {
        self.broadcast_with_type("", data)
    }

    /// 主动断开 SSE 连接
    ///
    /// 发送断开事件后移除连接，不管发送成功或失败都会移除
    ///
    /// # 参数
    /// * `connection_id` - 连接ID
    ///
    /// # 返回值
    /// * `true` - 连接存在并已断开
    /// * `false` - 连接不存在
    pub fn disconnect_connection(&self, connection_id: &str) -> bool {
        if let Some((_, sse)) = self.connections.remove(connection_id) {
            // 发送断开事件（不管成功失败）
            let _ = sse.send_event("disconnect", "Server is disconnecting connection");
            let _ = sse.send_data("DISCONNECT_EVENT");

            // 关闭发送器
            drop(sse);

            info!("🔌 [全局SSE管理器] 主动断开连接: {}", connection_id);
            true
        } else {
            warn!("🔍 [全局SSE管理器] 尝试断开不存在的连接: {}", connection_id);
            false
        }
    }

    /// 移除连接（内部方法）
    ///
    /// 仅用于内部清理，外部请使用 disconnect_connection
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
    manager.send_to_connection(connection_id, data)
}

/// 便捷函数：向特定连接发送消息（带事件类型）
pub fn send_sse_message_with_type(connection_id: &str, event_type: &str, data: &str) -> Result<(), String> {
    let manager = get_global_sse_manager();
    manager.send_to_connection_with_type(connection_id, event_type, data)
}

/// 便捷函数：广播 SSE 消息（无事件类型）
pub fn broadcast_sse_message(data: &str) -> usize {
    let manager = get_global_sse_manager();
    manager.broadcast(data)
}

/// 便捷函数：广播 SSE 消息（带事件类型）
pub fn broadcast_sse_message_with_type(event_type: &str, data: &str) -> usize {
    let manager = get_global_sse_manager();
    manager.broadcast_with_type(event_type, data)
}

/// 便捷函数：注册 SSE 连接
pub fn register_sse_connection(connection_id: String, sse_response: SseResponse) {
    let manager = get_global_sse_manager();
    manager.register_connection(connection_id, sse_response);
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