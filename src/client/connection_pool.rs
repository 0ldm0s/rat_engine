//! RAT Engine 客户端连接池实现
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
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use tokio_openssl::SslStream;
use x509_parser::prelude::FromDer;
use crate::error::{RatError, RatResult};
use crate::utils::logger::{info, warn, debug, error};

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
    pub development_mode: bool,
    /// mTLS 客户端配置
    pub mtls_config: Option<crate::client::grpc_builder::MtlsClientConfig>,
  }

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            idle_timeout: Duration::from_secs(300), // 5分钟
            keepalive_interval: Duration::from_secs(30), // 30秒
            connect_timeout: Duration::from_secs(10),
            cleanup_interval: Duration::from_secs(60), // 1分钟
            max_connections_per_target: 10,
            development_mode: false, // 默认不启用开发模式
            mtls_config: None,
          }
    }
}

/// 客户端连接池管理器
/// 复用服务器端的连接管理架构，提供连接复用和保活功能
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
            return; // 已经启动
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

    /// 发送关闭信号（可以从共享引用调用）
    pub async fn send_shutdown_signal(&self) {
        // 这个方法只发送关闭信号，不等待任务完成
        // 适用于从 Arc<ClientConnectionPool> 调用的场景
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
            info!("🛑 已发送客户端连接池关闭信号");
            
            // 给维护任务一点时间来处理关闭信号
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    /// 获取或创建连接
    pub async fn get_connection(&self, target_uri: &Uri) -> RatResult<Arc<ClientConnection>> {
        let authority = target_uri.authority()
            .ok_or_else(|| RatError::InvalidArgument("URI 必须包含 authority 部分".to_string()))?;
        let target_key = format!("{}://{}", target_uri.scheme_str().unwrap_or("http"), authority);

        // 首先尝试获取现有连接
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
                        connection_handle: None, // 不复制句柄
                    }));
                }
            }
        }

        // 检查连接数限制
        if !self.can_create_new_connection(&target_key) {
            return Err(RatError::NetworkError("连接池已满或目标连接数超限".to_string()));
        }

        // 创建新连接
        self.create_new_connection(target_uri.clone()).await
    }

    /// 查找可用连接
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

    /// 检查是否可以创建新连接
    fn can_create_new_connection(&self, target_key: &str) -> bool {
        // 检查总连接数
        if self.connections.len() >= self.config.max_connections {
            return false;
        }

        // 检查目标连接数
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
        use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
        use tokio_openssl::SslStream;

        let connection_id = self.connection_id_counter.fetch_add(1, Ordering::Relaxed).to_string();
        let target_key = format!("{}://{}", target_uri.scheme_str().unwrap_or("http"), target_uri.authority().unwrap());

        // 建立 TCP 连接
        let host = target_uri.host().ok_or_else(|| RatError::NetworkError("无效的主机地址".to_string()))?;
        let is_https = target_uri.scheme_str() == Some("https");
        let port = target_uri.port_u16().unwrap_or(if is_https { 443 } else { 80 });
        let addr = format!("{}:{}", host, port);

        let tcp_stream = tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(&addr)
        ).await
        .map_err(|_| RatError::NetworkError("连接超时".to_string()))?
            .map_err(|e| RatError::NetworkError(format!("TCP 连接失败: {}", e)))?;

        // 配置 TCP 选项
        tcp_stream.set_nodelay(true)
            .map_err(|e| RatError::NetworkError(format!("设置 TCP_NODELAY 失败: {}", e)))?;

        // 根据协议执行握手
        let send_request;
        let connection_handle;
        
        if is_https {
            // HTTPS: 先进行 TLS 握手，再进行 H2 握手
            debug!("[客户端] 🔐 建立 TLS 连接到 {}:{} (开发模式: {})", host, port, self.config.development_mode);
            
            // 根据开发模式和 mTLS 配置创建 TLS 配置
            let ssl_connector = if let Some(mtls_config) = &self.config.mtls_config {
                // mTLS 模式：启用客户端证书认证
                info!("🔐 连接池启用 mTLS 客户端证书认证");

                let mut ssl_builder = SslConnector::builder(SslMethod::tls())
                    .map_err(|e| RatError::TlsError(format!("创建 SSL 连接器失败: {}", e)))?;
                // 配置证书验证模式
                if mtls_config.skip_server_verification || self.config.development_mode {
                    // 开发模式：跳过证书验证
                    warn!("⚠️  警告：连接池启用开发模式或跳过服务器验证，将跳过服务器证书验证！仅用于开发环境！");
                    ssl_builder.set_verify(SslVerifyMode::NONE);
                } else {
                    // 严格证书验证
                    ssl_builder.set_verify(SslVerifyMode::PEER);

                    // 添加自定义 CA 证书（如果有）
                    if let Some(ca_certs) = &mtls_config.ca_certs {
                        for ca_cert in ca_certs {
                            let cert = openssl::x509::X509::from_der(ca_cert)
                                .map_err(|e| RatError::TlsError(format!("解析 CA 证书失败: {}", e)))?;
                            ssl_builder.cert_store_mut()
                                .add_cert(cert)
                                .map_err(|e| RatError::TlsError(format!("添加 CA 证书失败: {}", e)))?;
                        }
                        info!("✅ 连接池已加载 {} 个自定义 CA 证书", ca_certs.len());
                    }
                }

                // 配置客户端证书
                for cert_data in &mtls_config.client_cert_chain {
                    let cert = openssl::x509::X509::from_pem(cert_data)
                        .map_err(|e| RatError::TlsError(format!("解析客户端证书失败: {}", e)))?;
                    ssl_builder.set_certificate(&cert)
                        .map_err(|e| RatError::TlsError(format!("设置客户端证书失败: {}", e)))?;
                }

                let key = openssl::pkey::PKey::private_key_from_pem(&mtls_config.client_private_key)
                    .map_err(|e| RatError::TlsError(format!("解析客户端私钥失败: {}", e)))?;
                ssl_builder.set_private_key(&key)
                    .map_err(|e| RatError::TlsError(format!("设置客户端私钥失败: {}", e)))?;

                // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
                ssl_builder.set_alpn_protos(b"\x02h2")?;

    
                ssl_builder.build()
            } else if self.config.development_mode {
                // 开发模式：跳过证书验证，无客户端证书
                warn!("⚠️  警告：连接池已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");

                let mut ssl_builder = SslConnector::builder(SslMethod::tls())
                    .map_err(|e| RatError::TlsError(format!("创建 SSL 连接器失败: {}", e)))?;

                ssl_builder.set_verify(SslVerifyMode::NONE);
                ssl_builder.set_alpn_protos(b"\x02h2")?;

                // 开发模式下保持标准协议版本，仅跳过证书验证

                ssl_builder.build()
            } else {
                // 非开发模式：严格证书验证，无客户端证书
                let mut ssl_builder = SslConnector::builder(SslMethod::tls())
                    .map_err(|e| RatError::TlsError(format!("创建 SSL 连接器失败: {}", e)))?;

                ssl_builder.set_verify(SslVerifyMode::PEER);
                ssl_builder.set_alpn_protos(b"\x02h2")?;

                ssl_builder.build()
            };

            // 建立 TLS 连接
            let mut ssl = openssl::ssl::Ssl::new(&ssl_connector.context())
                .map_err(|e| RatError::NetworkError(format!("创建 SSL 失败: {}", e)))?;

            println!("[客户端调试] SSL 对象创建成功");
            println!("[客户端调试] SSL 版本: {:?}", ssl.version_str());

            // 调试：无法直接获取 SSL 上下文的 ALPN 配置，但可以通过其他方式验证
            println!("[客户端调试] SSL 上下文创建完成，ALPN 配置已在 Builder 中设置");

            // 调试：检查当前 SSL 对象的 ALPN 协议（握手前）
            println!("[客户端调试] 握手前 SSL 对象的 ALPN 协议: {:?}", ssl.selected_alpn_protocol());

            // 尝试直接在 SSL 对象上设置 ALPN 协议
            // 首先尝试使用 set_alpn_protos 方法
            match ssl.set_alpn_protos(b"\x02h2") {
                Ok(_) => {
                    println!("[客户端调试] SSL 对象 ALPN 协议已显式设置");
                    println!("[客户端调试] 设置后 SSL 对象的 ALPN 协议: {:?}", ssl.selected_alpn_protocol());
                }
                Err(e) => {
                    println!("[客户端调试] SSL 对象 set_alpn_protos 失败: {}, 尝试其他方法", e);

                    // 如果直接设置失败，尝试使用 SslConnector 重新创建
                    println!("[客户端调试] 尝试重新创建带有 ALPN 的 SSL 对象");

                    // 在 SslConnector Builder 中再次确认 ALPN 设置
                    drop(ssl); // 丢弃当前的 SSL 对象

                    let mut ssl_builder = SslConnector::builder(SslMethod::tls())
                        .map_err(|e| RatError::NetworkError(format!("重新创建 SSL Builder 失败: {}", e)))?;

                    ssl_builder.set_verify(SslVerifyMode::NONE);
                    ssl_builder.set_alpn_protos(b"\x02h2")?;

                    let ssl_connector_new = ssl_builder.build();
                    ssl = openssl::ssl::Ssl::new(&ssl_connector_new.context())
                        .map_err(|e| RatError::NetworkError(format!("重新创建 SSL 失败: {}", e)))?;

                    println!("[客户端调试] 重新创建的 SSL 对象 ALPN 协议: {:?}", ssl.selected_alpn_protocol());
                }
            }

            // 设置 SNI (Server Name Indication)
            ssl.set_hostname(host)
                .map_err(|e| RatError::NetworkError(format!("设置 SNI 主机名失败: {}", e)))?;
            println!("[客户端调试] SNI 主机名设置: {}", host);

            // 设置连接类型为客户端
            ssl.set_connect_state();
            println!("[客户端调试] SSL 连接类型设置为客户端");

            let mut tls_stream = SslStream::new(ssl, tcp_stream)
                .map_err(|e| RatError::NetworkError(format!("创建 TLS 流失败: {}", e)))?;
            println!("[客户端调试] TLS 流创建成功");

            // 使用异步方式完成 TLS 握手
            debug!("[客户端] 🔐 开始 TLS 握手...");
            println!("[客户端调试] 开始 TLS 握手过程...");
            use futures_util::future::poll_fn;
            poll_fn(|cx| {
                match std::pin::Pin::new(&mut tls_stream).poll_do_handshake(cx) {
                    std::task::Poll::Ready(Ok(())) => {
                        debug!("[客户端] ✅ TLS 握手成功");
                        println!("[客户端调试] ✅ TLS 握手成功！");

                        // 打印握手后的详细信息
                        let ssl = tls_stream.ssl();
                        println!("[客户端调试] 握手后 SSL 版本: {:?}", ssl.version_str());
                        println!("[客户端调试] 握手后 ALPN 协议: {:?}", ssl.selected_alpn_protocol());
                        println!("[客户端调试] 握手后 服务器证书: {:?}", ssl.peer_certificate());

                        std::task::Poll::Ready(Ok(()))
                    },
                    std::task::Poll::Ready(Err(e)) => {
                        error!("[客户端] ❌ TLS 握手失败: {}", e);
                        println!("[客户端调试] ❌ TLS 握手失败: {}", e);
                        std::task::Poll::Ready(Err(e))
                    },
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }).await.map_err(|e| RatError::NetworkError(format!("TLS 握手失败: {}", e)))?;

            // 调试 ALPN 协商结果
            let selected_protocol = tls_stream.ssl().selected_alpn_protocol();
            debug!("[客户端] 🔐 TLS 连接建立成功，ALPN 协商结果: {:?}", selected_protocol);
            debug!("[客户端] 🔐 TLS 连接建立成功，开始 HTTP/2 握手");
            
            // 配置 HTTP/2 客户端，设置合适的帧大小
            let mut h2_builder = h2::client::Builder::default();
            h2_builder.max_frame_size(1024 * 1024); // 设置最大帧大小为 1MB
            
            // 在 TLS 连接上进行 HTTP/2 握手
            let (send_req, h2_conn) = h2_builder.handshake(tls_stream).await
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 over TLS 握手失败: {}", e)))?;
            
            send_request = send_req;
            
            // 启动 H2 连接任务
            connection_handle = tokio::spawn(async move {
                if let Err(e) = h2_conn.await {
                    error!("[客户端] H2 TLS 连接错误: {}", e);
                }
            });
        } else {
            // HTTP: 使用 H2C (HTTP/2 Cleartext)
            debug!("[客户端] 🌐 建立 HTTP/2 Cleartext 连接到 {}:{}", host, port);

            // 配置 HTTP/2 客户端，设置合适的帧大小
            let mut h2_builder = h2::client::Builder::default();
            h2_builder.max_frame_size(1024 * 1024); // 设置最大帧大小为 1MB

            let (send_req, h2_conn) = h2_builder.handshake(tcp_stream).await
                .map_err(|e| RatError::NetworkError(format!("H2 握手失败: {}", e)))?;

            send_request = send_req;

            // 启动 H2 连接任务
            connection_handle = tokio::spawn(async move {
                if let Err(e) = h2_conn.await {
                    error!("[客户端] H2 连接错误: {}", e);
                }
            });
        }

        // 创建连接对象
        let client_connection = ClientConnection::new(
            connection_id.clone(),
            target_uri,
            send_request,
            Some(connection_handle),
        );

        // 添加到连接池
        self.connections.insert(connection_id.clone(), client_connection);

        // 更新目标连接映射
        self.target_connections.entry(target_key)
            .or_insert_with(Vec::new)
            .push(connection_id.clone());

        info!("[客户端] 🔗 创建新的客户端连接: {}", connection_id);

        // 返回连接的 Arc 包装
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
                connection_handle: None, // 不复制句柄
            }))
        } else {
            Err(RatError::NetworkError("连接创建后立即丢失".to_string()))
        }
    }

    /// 释放连接
    pub fn release_connection(&self, connection_id: &str) {
        if let Some(mut connection) = self.connections.get_mut(connection_id) {
            connection.update_last_active();
        }
    }

    /// 移除连接
    pub fn remove_connection(&self, connection_id: &str) {
        if let Some((_, connection)) = self.connections.remove(connection_id) {
            let target_key = format!("{}://{}", 
                connection.target_uri.scheme_str().unwrap_or("http"), 
                connection.target_uri.authority().unwrap()
            );

            // 从目标连接映射中移除
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

    /// 清理过期连接
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
                        connection.target_uri.authority().unwrap()
                    );

                    // 从目标连接映射中移除
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

    /// 发送保活消息
    async fn send_keepalive_messages(connections: &Arc<DashMap<String, ClientConnection>>) {
        let active_count = connections.len();
        if active_count > 0 {
            crate::utils::logger::debug!("💓 客户端连接池保活检查: {} 个活跃连接", active_count);
            
            // 对于 H2 连接，保活是通过底层协议自动处理的
            // 这里主要是更新连接状态和统计信息
            for mut entry in connections.iter_mut() {
                let connection = entry.value_mut();
                if connection.is_ready() {
                    connection.update_last_active();
                }
            }
        }
    }

    /// 获取连接池统计信息
    pub fn get_stats(&self) -> (usize, usize) {
        (
            self.connections.len(),
            self.target_connections.len(),
        )
    }

    /// 获取连接池配置
    pub fn get_config(&self) -> &ConnectionPoolConfig {
        &self.config
    }

    /// 关闭连接池
    pub async fn shutdown(&mut self) {
        crate::utils::logger::info!("🛑 关闭客户端连接池");

        // 停止维护任务
        self.stop_maintenance_tasks().await;

        // 关闭所有连接
        let connection_ids: Vec<String> = self.connections.iter().map(|entry| entry.key().clone()).collect();
        for connection_id in connection_ids {
            self.remove_connection(&connection_id);
        }

        crate::utils::logger::info!("✅ 客户端连接池已关闭");
    }
}

impl Drop for ClientConnectionPool {
    fn drop(&mut self) {
        // 在析构时尝试清理资源
        if self.maintenance_handle.is_some() {
            // 检查维护任务是否已经完成
            if let Some(handle) = &self.maintenance_handle {
                if !handle.is_finished() {
                    crate::utils::logger::warn!("⚠️ 客户端连接池在析构时仍有活跃的维护任务");
                    
                    // 尝试发送关闭信号
                    if let Some(shutdown_tx) = &self.shutdown_tx {
                        let _ = shutdown_tx.try_send(());
                    }
                    
                    // 取消维护任务
                    if let Some(handle) = self.maintenance_handle.take() {
                        handle.abort();
                        // 注意：在 Drop 中不能使用 block_on，因为可能在异步运行时中
                        // 任务会被异步取消，无需等待
                        
                        crate::utils::logger::info!("🛑 强制终止客户端连接池维护任务");
                    }
                } else {
                    // 维护任务已经完成，只需要清理句柄
                    self.maintenance_handle.take();
                    crate::utils::logger::debug!("✅ 客户端连接池维护任务已正常完成");
                }
            }
        }
        
        crate::utils::logger::debug!("✅ 客户端连接池已完成清理");
    }
}