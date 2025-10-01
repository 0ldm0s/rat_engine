//! SSE 适配器模块
//! 
//! 将 SSE 连接池集成到现有的 HTTP 处理流程中，提供统一的 SSE 服务接口

use std::sync::Arc;
use hyper::{Request, Response, HeaderMap, Method, Version};
use hyper::body::Incoming;
use crate::error::{RatError, RatResult};
use crate::server::streaming::StreamingBody;
use crate::server::sse_connection_pool::{
    SseConnectionPool, SseConnectionPoolConfig, SseProtocolVersion, SseConnectionType
};
use crate::utils::logger::{info, warn, debug, error};

/// SSE 适配器
/// 负责将 HTTP 请求适配为 SSE 连接，并管理连接生命周期
pub struct SseAdapter {
    /// SSE 连接池
    connection_pool: Arc<SseConnectionPool>,
}

impl SseAdapter {
    /// 创建新的 SSE 适配器
    pub fn new(config: SseConnectionPoolConfig) -> Self {
        let mut pool = SseConnectionPool::new(config);
        pool.start_maintenance_tasks();
        
        Self {
            connection_pool: Arc::new(pool),
        }
    }

    /// 处理 SSE 连接请求
    pub async fn handle_sse_request(
        &self,
        req: Request<Incoming>,
        client_addr: String,
    ) -> RatResult<Response<StreamingBody>> {
        // 验证请求
        self.validate_sse_request(&req)?;

        // 解析协议版本和连接类型
        let (protocol_version, connection_type) = self.parse_connection_info(&req)?;

        // 提取用户信息（从查询参数或头部）
        let (user_id, room_id) = self.extract_user_info(&req);

        // 创建 SSE 连接
        let (connection_id, receiver) = self.connection_pool
            .create_connection(
                client_addr,
                protocol_version.clone(),
                connection_type,
                user_id,
                room_id,
            )
            .await?;

        // 构建 SSE 响应
        let response = self.connection_pool
            .build_sse_response(&protocol_version, receiver)
            .map_err(|e| RatError::NetworkError(format!("构建 SSE 响应失败: {}", e)))?;

        info!("✅ SSE 连接已建立: {} (协议: {:?})", connection_id, protocol_version);

        Ok(response)
    }

    /// 验证 SSE 请求
    fn validate_sse_request(&self, req: &Request<Incoming>) -> RatResult<()> {
        // 检查请求方法
        if req.method() != Method::GET {
            return Err(RatError::NetworkError("SSE 只支持 GET 请求".to_string()));
        }

        // 检查 Accept 头部
        if let Some(accept) = req.headers().get("accept") {
            let accept_str = accept.to_str().map_err(|_| {
                RatError::NetworkError("无效的 Accept 头部".to_string())
            })?;
            
            if !accept_str.contains("text/event-stream") && !accept_str.contains("*/*") {
                return Err(RatError::NetworkError("不支持的 Accept 类型".to_string()));
            }
        }

        // 检查 Cache-Control 头部（可选）
        if let Some(cache_control) = req.headers().get("cache-control") {
            let cache_str = cache_control.to_str().map_err(|_| {
                RatError::NetworkError("无效的 Cache-Control 头部".to_string())
            })?;
            
            if cache_str.contains("no-cache") {
                debug!("🔄 客户端请求禁用缓存");
            }
        }

        Ok(())
    }

    /// 解析连接信息
    fn parse_connection_info(&self, req: &Request<Incoming>) -> RatResult<(SseProtocolVersion, SseConnectionType)> {
        let protocol_version = match req.version() {
            Version::HTTP_11 => SseProtocolVersion::Http1_1,
            Version::HTTP_2 => {
                // 检查是否是 H2C
                if req.uri().scheme_str() == Some("http") {
                    SseProtocolVersion::Http2Cleartext
                } else {
                    SseProtocolVersion::Http2
                }
            }
            _ => {
                return Err(RatError::NetworkError("不支持的 HTTP 版本".to_string()));
            }
        };

        let connection_type = match req.uri().scheme_str() {
            Some("https") => SseConnectionType::Tls,
            Some("http") => {
                if matches!(protocol_version, SseProtocolVersion::Http2Cleartext) {
                    SseConnectionType::H2c
                } else {
                    SseConnectionType::PlainTcp
                }
            }
            _ => SseConnectionType::PlainTcp,
        };

        Ok((protocol_version, connection_type))
    }

    /// 提取用户信息
    fn extract_user_info(&self, req: &Request<Incoming>) -> (Option<String>, Option<String>) {
        let query = req.uri().query().unwrap_or("");
        let mut user_id = None;
        let mut room_id = None;

        // 解析查询参数
        for param in query.split('&') {
            if let Some((key, value)) = param.split_once('=') {
                match key {
                    "user_id" => user_id = Some(value.to_string()),
                    "room_id" => room_id = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        // 也可以从头部获取
        if user_id.is_none() {
            if let Some(header_user_id) = req.headers().get("x-user-id") {
                if let Ok(user_str) = header_user_id.to_str() {
                    user_id = Some(user_str.to_string());
                }
            }
        }

        if room_id.is_none() {
            if let Some(header_room_id) = req.headers().get("x-room-id") {
                if let Ok(room_str) = header_room_id.to_str() {
                    room_id = Some(room_str.to_string());
                }
            }
        }

        (user_id, room_id)
    }

    /// 获取连接池引用
    pub fn get_connection_pool(&self) -> Arc<SseConnectionPool> {
        self.connection_pool.clone()
    }

    /// 广播消息到所有连接
    pub async fn broadcast(&self, event: &str, data: &str) -> usize {
        self.connection_pool.broadcast_to_all(event, data).await
    }

    /// 向特定用户发送消息
    pub async fn send_to_user(&self, user_id: &str, event: &str, data: &str) -> usize {
        self.connection_pool.send_to_user(user_id, event, data).await
    }

    /// 向特定房间发送消息
    pub async fn send_to_room(&self, room_id: &str, event: &str, data: &str) -> usize {
        self.connection_pool.send_to_room(room_id, event, data).await
    }

    /// 获取连接统计
    pub fn get_statistics(&self) -> crate::server::sse_connection_pool::SseConnectionStatistics {
        self.connection_pool.get_statistics()
    }
}

/// SSE 适配器构建器
/// 使用构建器模式配置 SSE 适配器
pub struct SseAdapterBuilder {
    config: SseConnectionPoolConfig,
}

impl SseAdapterBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            config: SseConnectionPoolConfig::default(),
        }
    }

    /// 设置最大连接数
    pub fn max_connections(mut self, max_connections: usize) -> Self {
        self.config.max_connections = max_connections;
        self
    }

    /// 设置连接空闲超时时间
    pub fn idle_timeout(mut self, idle_timeout: std::time::Duration) -> Self {
        self.config.idle_timeout = idle_timeout;
        self
    }

    /// 设置心跳间隔
    pub fn heartbeat_interval(mut self, heartbeat_interval: std::time::Duration) -> Self {
        self.config.heartbeat_interval = heartbeat_interval;
        self
    }

    /// 设置清理间隔
    pub fn cleanup_interval(mut self, cleanup_interval: std::time::Duration) -> Self {
        self.config.cleanup_interval = cleanup_interval;
        self
    }

    /// 设置每个用户的最大连接数
    pub fn max_connections_per_user(mut self, max_connections_per_user: usize) -> Self {
        self.config.max_connections_per_user = max_connections_per_user;
        self
    }

    /// 设置每个房间的最大连接数
    pub fn max_connections_per_room(mut self, max_connections_per_room: usize) -> Self {
        self.config.max_connections_per_room = max_connections_per_room;
        self
    }

    /// 启用或禁用心跳
    pub fn enable_heartbeat(mut self, enable_heartbeat: bool) -> Self {
        self.config.enable_heartbeat = enable_heartbeat;
        self
    }

    /// 启用或禁用连接统计
    pub fn enable_statistics(mut self, enable_statistics: bool) -> Self {
        self.config.enable_statistics = enable_statistics;
        self
    }

    /// 构建 SSE 适配器
    pub fn build(self) -> SseAdapter {
        SseAdapter::new(self.config)
    }
}

impl Default for SseAdapterBuilder {
    fn default() -> Self {
        Self::new()
    }
}
