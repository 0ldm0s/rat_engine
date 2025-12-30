//! RAT Engine 客户端连接池实现（rustls）
//!
//! 基于服务器端连接管理架构，为客户端提供连接复用、保活和资源管理功能

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use dashmap::DashMap;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use hyper::Uri;
use h2::{client::SendRequest, RecvStream};
use hyper::body::Bytes;
use rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;
use crate::error::{RatError, RatResult};
use crate::utils::logger::{info, warn, debug, error};
use crate::client::grpc_builder::MtlsClientConfig;

/// 客户端连接信息
#[derive(Debug)]
pub struct ClientConnection {
    /// 连接ID
    pub connection_id: String,
    /// 目标URI
    pub target_uri: Uri,
    /// H2 发送请求句柄
    pub send_request: SendRequest<Bytes>,
    /// 连接创建时间
    pub created_at: Instant,
    /// 最后活跃时间
    pub last_active: Instant,
    /// 连接状态
    pub is_active: bool,
    /// 使用计数
    pub usage_count: AtomicU64,
    /// 连接任务句柄
    pub connection_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ClientConnection {
    /// 创建新的客户端连接
    pub fn new(
        connection_id: String,
        target_uri: Uri,
        send_request: SendRequest<Bytes>,
        connection_handle: Option<tokio::task::JoinHandle<()>>,
    ) -> Self {
        let now = Instant::now();
        Self {
            connection_id,
            target_uri,
            send_request,
            created_at: now,
            last_active: now,
            is_active: true,
            usage_count: AtomicU64::new(0),
            connection_handle,
        }
    }

    /// 更新最后活跃时间
    pub fn update_last_active(&mut self) {
        self.last_active = Instant::now();
    }

    /// 增加使用计数
    pub fn increment_usage(&self) {
        self.usage_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取使用计数
    pub fn get_usage_count(&self) -> u64 {
        self.usage_count.load(Ordering::Relaxed)
    }

    /// 检查连接是否可用
    pub fn is_ready(&self) -> bool {
        self.is_active
    }
}

/// 客户端连接池配置
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// 最大连接数
    pub max_connections: usize,
    /// 空闲连接超时时间
    pub idle_timeout: Duration,
    /// 保活间隔
    pub keepalive_interval: Duration,
    /// 连接超时时间
    pub connect_timeout: Duration,
    /// 清理间隔
    pub cleanup_interval: Duration,
    /// 每个目标的最大连接数
    pub max_connections_per_target: usize,
    /// 开发模式（跳过 TLS 证书验证）
    pub h2c_mode: bool,
    /// mTLS 客户端配置
    pub mtls_config: Option<MtlsClientConfig>,
    /// TLS 配置（rustls）
    pub tls_config: Option<Arc<rustls::ClientConfig>>,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            idle_timeout: Duration::from_secs(300),
            keepalive_interval: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            cleanup_interval: Duration::from_secs(60),
            max_connections_per_target: 10,
            h2c_mode: false,
            mtls_config: None,
            tls_config: None,
        }
    }
}

/// 客户端连接池管理器
#[derive(Debug)]
pub struct ClientConnectionPool {
    /// 活跃连接（连接ID -> 连接信息）
    connections: Arc<DashMap<String, ClientConnection>>,
    /// 目标连接映射（目标URI -> 连接ID列表）
    target_connections: Arc<DashMap<String, Vec<String>>>,
    /// 连接ID生成器
    connection_id_counter: Arc<AtomicU64>,
    /// 配置
    config: ConnectionPoolConfig,
    /// 维护任务句柄
    maintenance_handle: Option<tokio::task::JoinHandle<()>>,
    /// 关闭信号发送器
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl ClientConnectionPool {
    /// 创建新的客户端连接池
    pub fn new(config: ConnectionPoolConfig) -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
            target_connections: Arc::new(DashMap::new()),
            connection_id_counter: Arc::new(AtomicU64::new(1)),
            config,
            maintenance_handle: None,
            shutdown_tx: None,
        }
    }

    /// 启动连接池维护任务
    pub fn start_maintenance_tasks(&mut self) {
        if self.maintenance_handle.is_some() {
            return;
        }

        let connections = self.connections.clone();
        let target_connections = self.target_connections.clone();
        let config = self.config.clone();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            let mut cleanup_interval = interval(config.cleanup_interval);
            let mut keepalive_interval = interval(config.keepalive_interval);

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("🛑 客户端连接池维护任务收到关闭信号");
                        break;
                    }
                    _ = cleanup_interval.tick() => {
                        Self::cleanup_expired_connections(&connections, &target_connections, &config).await;
                    }
                    _ = keepalive_interval.tick() => {
                        Self::send_keepalive_messages(&connections).await;
                    }
                }
            }

            info!("✅ 客户端连接池维护任务已停止");
        });

        self.maintenance_handle = Some(handle);
    }

    /// 停止维护任务
    pub async fn stop_maintenance_tasks(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(()).await;
        }

        if let Some(handle) = self.maintenance_handle.take() {
            let _ = handle.await;
        }
    }

    /// 发送关闭信号
    pub async fn send_shutdown_signal(&self) {
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
            info!("🛑 已发送客户端连接池关闭信号");
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    /// 获取或创建连接
    pub async fn get_connection(&self, target_uri: &Uri) -> RatResult<Arc<ClientConnection>> {
        let authority = target_uri.authority()
            .ok_or_else(|| RatError::InvalidArgument("URI 必须包含 authority 部分".to_string()))?;
        let target_key = format!("{}://{}", target_uri.scheme_str().unwrap_or("http"), authority);

        if let Some(connection_id) = self.find_available_connection(&target_key) {
            if let Some(connection) = self.connections.get(&connection_id) {
                if connection.is_ready() {
                    connection.increment_usage();
                    return Ok(Arc::new(ClientConnection {
                        connection_id: connection.connection_id.clone(),
                        target_uri: connection.target_uri.clone(),
                        send_request: connection.send_request.clone(),
                        created_at: connection.created_at,
                        last_active: connection.last_active,
                        is_active: connection.is_active,
                        usage_count: AtomicU64::new(connection.get_usage_count()),
                        connection_handle: None,
                    }));
                }
            }
        }

        if !self.can_create_new_connection(&target_key) {
            return Err(RatError::NetworkError("连接池已满或目标连接数超限".to_string()));
        }

        self.create_new_connection(target_uri.clone()).await
    }

    fn find_available_connection(&self, target_key: &str) -> Option<String> {
        if let Some(connection_ids) = self.target_connections.get(target_key) {
            for connection_id in connection_ids.iter() {
                if let Some(connection) = self.connections.get(connection_id) {
                    if connection.is_ready() {
                        return Some(connection_id.clone());
                    }
                }
            }
        }
        None
    }

    fn can_create_new_connection(&self, target_key: &str) -> bool {
        if self.connections.len() >= self.config.max_connections {
            return false;
        }

        if let Some(connection_ids) = self.target_connections.get(target_key) {
            if connection_ids.len() >= self.config.max_connections_per_target {
                return false;
            }
        }

        true
    }

    /// 创建新连接
    async fn create_new_connection(&self, target_uri: Uri) -> RatResult<Arc<ClientConnection>> {
        use tokio::net::TcpStream;

        let connection_id = self.connection_id_counter.fetch_add(1, Ordering::Relaxed).to_string();
        let target_key = format!("{}://{}",
            target_uri.scheme_str().unwrap_or("http"),
            target_uri.authority().ok_or_else(|| RatError::NetworkError("缺少目标URI的authority".to_string()))?
        );

        let host = target_uri.host().ok_or_else(|| RatError::NetworkError("无效的主机地址".to_string()))?;
        let is_https = target_uri.scheme_str() == Some("https");
        let port = target_uri.port_u16().unwrap_or(if is_https { 443 } else { 80 });
        let addr = format!("{}:{}", host, port);

        let tcp_stream = tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(&addr)
        ).await
        .map_err(|_| RatError::NetworkError(rat_embed_lang::t("tcp_timeout")))?
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("tcp_connection_failed", &[("msg", &e.to_string())])))?;

        tcp_stream.set_nodelay(true)
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("set_tcp_nodelay_failed", &[("msg", &e.to_string())])))?;

        let send_request;
        let connection_handle;

        if is_https {
            debug!("[客户端] 🔐 建立 TLS 连接到 {}:{} (开发模式: {})", host, port, self.config.h2c_mode);

            // 获取 TLS 配置
            let tls_config = self.config.tls_config.as_ref()
                .ok_or_else(|| RatError::TlsError("TLS 配置未设置".to_string()))?;

            // 创建 SNI
            let server_name = ServerName::try_from(host)
                .map_err(|e| RatError::TlsError(format!("无效的服务器名称: {}", e)))?
                .to_owned();

            // 创建 TLS 连接器
            let connector = TlsConnector::from(tls_config.clone());

            let tls_stream = connector.connect(server_name, tcp_stream).await
                .map_err(|e| RatError::NetworkError(format!("TLS 握手失败: {}", e)))?;

            debug!("[客户端] ✅ TLS 握手成功，开始 HTTP/2 握手");

            let mut h2_builder = h2::client::Builder::default();
            h2_builder.max_frame_size(1024 * 1024);

            let (send_req, h2_conn) = h2_builder.handshake(tls_stream).await
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 握手失败: {}", e)))?;

            send_request = send_req;

            connection_handle = tokio::spawn(async move {
                if let Err(e) = h2_conn.await {
                    error!("[客户端] H2 TLS 连接错误: {}", e);
                }
            });
        } else {
            debug!("[客户端] 🌐 建立 HTTP/2 Cleartext 连接到 {}:{}", host, port);

            let mut h2_builder = h2::client::Builder::default();
            h2_builder.max_frame_size(1024 * 1024);

            let (send_req, h2_conn) = h2_builder.handshake(tcp_stream).await
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 握手失败: {}", e)))?;

            send_request = send_req;

            connection_handle = tokio::spawn(async move {
                if let Err(e) = h2_conn.await {
                    error!("[客户端] H2 连接错误: {}", e);
                }
            });
        }

        let client_connection = ClientConnection::new(
            connection_id.clone(),
            target_uri,
            send_request,
            Some(connection_handle),
        );

        self.connections.insert(connection_id.clone(), client_connection);

        self.target_connections.entry(target_key)
            .or_insert_with(Vec::new)
            .push(connection_id.clone());

        info!("[客户端] 🔗 创建新的客户端连接: {}", connection_id);

        if let Some(connection) = self.connections.get(&connection_id) {
            connection.increment_usage();
            Ok(Arc::new(ClientConnection {
                connection_id: connection.connection_id.clone(),
                target_uri: connection.target_uri.clone(),
                send_request: connection.send_request.clone(),
                created_at: connection.created_at,
                last_active: connection.last_active,
                is_active: connection.is_active,
                usage_count: AtomicU64::new(connection.get_usage_count()),
                connection_handle: None,
            }))
        } else {
            Err(RatError::NetworkError("连接创建后立即丢失".to_string()))
        }
    }

    pub fn release_connection(&self, connection_id: &str) {
        if let Some(mut connection) = self.connections.get_mut(connection_id) {
            connection.update_last_active();
        }
    }

    pub fn remove_connection(&self, connection_id: &str) {
        if let Some((_, connection)) = self.connections.remove(connection_id) {
            let target_key = format!("{}://{}",
                connection.target_uri.scheme_str().unwrap_or("http"),
                connection.target_uri.authority().map(|a| a.as_str()).unwrap_or("<missing-authority>")
            );

            if let Some(mut connection_ids) = self.target_connections.get_mut(&target_key) {
                connection_ids.retain(|id| id != connection_id);
                if connection_ids.is_empty() {
                    drop(connection_ids);
                    self.target_connections.remove(&target_key);
                }
            }

            crate::utils::logger::info!("[客户端] 🗑️ 移除客户端连接: {}", connection_id);
        }
    }

    async fn cleanup_expired_connections(
        connections: &Arc<DashMap<String, ClientConnection>>,
        target_connections: &Arc<DashMap<String, Vec<String>>>,
        config: &ConnectionPoolConfig,
    ) {
        let now = Instant::now();
        let mut expired_connections = Vec::new();

        for entry in connections.iter() {
            let connection = entry.value();
            if now.duration_since(connection.last_active) > config.idle_timeout || !connection.is_ready() {
                expired_connections.push(connection.connection_id.clone());
            }
        }

        if !expired_connections.is_empty() {
            crate::utils::logger::info!("🧹 清理 {} 个过期的客户端连接", expired_connections.len());

            for connection_id in expired_connections {
                if let Some((_, connection)) = connections.remove(&connection_id) {
                    let target_key = format!("{}://{}",
                        connection.target_uri.scheme_str().unwrap_or("http"),
                        connection.target_uri.authority().map(|a| a.as_str()).unwrap_or("<missing-authority>")
                    );

                    if let Some(mut connection_ids) = target_connections.get_mut(&target_key) {
                        connection_ids.retain(|id| id != &connection_id);
                        if connection_ids.is_empty() {
                            drop(connection_ids);
                            target_connections.remove(&target_key);
                        }
                    }
                }
            }
        }
    }

    async fn send_keepalive_messages(connections: &Arc<DashMap<String, ClientConnection>>) {
        let active_count = connections.len();
        if active_count > 0 {
            crate::utils::logger::debug!("💓 客户端连接池保活检查: {} 个活跃连接", active_count);

            for mut entry in connections.iter_mut() {
                let connection = entry.value_mut();
                if connection.is_ready() {
                    connection.update_last_active();
                }
            }
        }
    }

    pub fn get_stats(&self) -> (usize, usize) {
        (
            self.connections.len(),
            self.target_connections.len(),
        )
    }

    pub fn get_config(&self) -> &ConnectionPoolConfig {
        &self.config
    }

    pub async fn shutdown(&mut self) {
        crate::utils::logger::info!("🛑 关闭客户端连接池");

        self.stop_maintenance_tasks().await;

        let connection_ids: Vec<String> = self.connections.iter().map(|entry| entry.key().clone()).collect();
        for connection_id in connection_ids {
            self.remove_connection(&connection_id);
        }

        crate::utils::logger::info!("✅ 客户端连接池已关闭");
    }
}

impl Drop for ClientConnectionPool {
    fn drop(&mut self) {
        if self.maintenance_handle.is_some() {
            if let Some(handle) = &self.maintenance_handle {
                if !handle.is_finished() {
                    crate::utils::logger::warn!("⚠️ 客户端连接池在析构时仍有活跃的维护任务");

                    if let Some(shutdown_tx) = &self.shutdown_tx {
                        let _ = shutdown_tx.try_send(());
                    }

                    if let Some(handle) = self.maintenance_handle.take() {
                        handle.abort();
                        crate::utils::logger::info!("🛑 强制终止客户端连接池维护任务");
                    }
                } else {
                    self.maintenance_handle.take();
                    crate::utils::logger::debug!("✅ 客户端连接池维护任务已正常完成");
                }
            }
        }

        crate::utils::logger::debug!("✅ 客户端连接池已完成清理");
    }
}
