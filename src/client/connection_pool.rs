//! RAT Engine 客户端连接池实现
//! 
//! 基于服务器端连接管理架构，为客户端提供连接复用、保活和资源管理功能

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use crate::client::builder::ClientProtocolMode;
use dashmap::DashMap;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use hyper::Uri;
use h2::{client::SendRequest, RecvStream};
use hyper::body::Bytes;
use rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;
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
    /// 协议模式
    pub protocol_mode: ClientProtocolMode,
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
            protocol_mode: ClientProtocolMode::Auto, // 默认自动模式
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
        use rustls::pki_types::ServerName;
        use tokio_rustls::TlsConnector;

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
            let tls_config = if let Some(mtls_config) = &self.config.mtls_config {
                // mTLS 模式：启用客户端证书认证
                info!("🔐 连接池启用 mTLS 客户端证书认证");
                
                // 构建根证书存储
                let mut root_store = rustls::RootCertStore::empty();
                
                if let Some(ca_certs) = &mtls_config.ca_certs {
                    // 使用自定义 CA 证书
                    for ca_cert in ca_certs {
                        root_store.add(ca_cert.clone())
                            .map_err(|e| RatError::NetworkError(format!("添加 CA 证书失败: {}", e)))?;
                    }
                    info!("✅ 连接池已加载 {} 个自定义 CA 证书", ca_certs.len());
                } else {
                    // 使用系统默认根证书
                    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                    info!("✅ 连接池已加载系统默认根证书");
                }
                
                // 创建客户端证书链
                let client_cert_chain = mtls_config.client_cert_chain.clone();
                let client_private_key = mtls_config.client_private_key.clone_key();
                
                let mut tls_config = if mtls_config.skip_server_verification || self.config.development_mode {
                    // 开发模式或跳过服务器验证：使用危险的证书验证器
                    warn!("⚠️  警告：连接池启用开发模式或跳过服务器验证，将跳过服务器证书验证！仅用于开发环境！");
                    
                    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
                    use rustls::{pki_types, Error as RustlsError};
                    
                    #[derive(Debug)]
                    struct DangerousClientCertVerifier {
                        mtls_config: Option<crate::client::grpc_builder::MtlsClientConfig>,
                    }
                    
                    impl ServerCertVerifier for DangerousClientCertVerifier {
                        fn verify_server_cert(
                            &self,
                            end_entity: &pki_types::CertificateDer<'_>,
                            intermediates: &[pki_types::CertificateDer<'_>],
                            server_name: &pki_types::ServerName<'_>,
                            ocsp_response: &[u8],
                            now: pki_types::UnixTime,
                        ) -> Result<ServerCertVerified, RustlsError> {
                            debug!("🔍 [客户端-开发模式] 服务器证书验证开始");
                            debug!("   服务器名称: {:?}", server_name);
                            debug!("   证书验证模式: 开发模式（跳过验证）");
                            debug!("   中间证书数量: {}", intermediates.len());
                            debug!("   OCSP 响应: {}", if ocsp_response.is_empty() { "无" } else { "有" });
                            debug!("   验证时间: {:?}", now);
                            
                            // 显示配置的证书路径信息
                            if let Some(config) = &self.mtls_config {
                                debug!("   mTLS 配置信息:");
                                debug!("     - 客户端证书数量: {}", config.client_cert_chain.len());
                                if let Some(client_cert_path) = &config.client_cert_path {
                                    debug!("     - 客户端证书路径: {}", client_cert_path);
                                }
                                if let Some(client_key_path) = &config.client_key_path {
                                    debug!("     - 客户端私钥路径: {}", client_key_path);
                                }
                                if let Some(ca_certs) = &config.ca_certs {
                                    debug!("     - CA 证书数量: {}", ca_certs.len());
                                }
                                if let Some(ca_cert_path) = &config.ca_cert_path {
                                    debug!("     - CA 证书路径: {}", ca_cert_path);
                                }
                                if let Some(server_name) = &config.server_name {
                                    debug!("     - 配置的服务器名称: {}", server_name);
                                }
                                debug!("     - 跳过服务器验证: {}", config.skip_server_verification);
                            }
                            
                            // 尝试解析证书信息以获取更多调试信息
                            if let Ok((_, cert)) = x509_parser::certificate::X509Certificate::from_der(end_entity) {
                                debug!("   证书主题: {}", cert.subject());
                                debug!("   证书颁发者: {}", cert.issuer());
                                debug!("   证书有效期: {} - {}", 
                                    cert.validity().not_before.to_datetime(),
                                    cert.validity().not_after.to_datetime());
                            }
                            
                            debug!("✅ [客户端-开发模式] 服务器证书验证跳过（开发模式）");
                            Ok(ServerCertVerified::assertion())
                        }
                        
                        fn verify_tls12_signature(
                            &self,
                            message: &[u8],
                            cert: &pki_types::CertificateDer<'_>,
                            dss: &rustls::DigitallySignedStruct,
                        ) -> Result<HandshakeSignatureValid, RustlsError> {
                            debug!("🔍 [客户端-开发模式] TLS 1.2 签名验证开始");
                            debug!("   签名验证模式: 开发模式（跳过验证）");
                            debug!("   签名算法: {:?}", dss.scheme);
                            debug!("   消息哈希: 已计算");
                            
                            // 尝试解析证书信息
                            if let Ok((_, cert_info)) = x509_parser::certificate::X509Certificate::from_der(cert) {
                                debug!("   证书主题: {}", cert_info.subject());
                                debug!("   公钥算法: {}", cert_info.public_key().algorithm.algorithm);
                            }
                            
                            debug!("✅ [客户端-开发模式] TLS 1.2 签名验证跳过（开发模式）");
                            Ok(HandshakeSignatureValid::assertion())
                        }
                        
                        fn verify_tls13_signature(
                            &self,
                            message: &[u8],
                            cert: &pki_types::CertificateDer<'_>,
                            dss: &rustls::DigitallySignedStruct,
                        ) -> Result<HandshakeSignatureValid, RustlsError> {
                            debug!("🔍 [客户端-开发模式] TLS 1.3 签名验证开始");
                            debug!("   签名验证模式: 开发模式（跳过验证）");
                            debug!("   签名算法: {:?}", dss.scheme);
                            debug!("   消息哈希: 已计算");
                            
                            // 尝试解析证书信息
                            if let Ok((_, cert_info)) = x509_parser::certificate::X509Certificate::from_der(cert) {
                                debug!("   证书主题: {}", cert_info.subject());
                                debug!("   公钥算法: {}", cert_info.public_key().algorithm.algorithm);
                            }
                            
                            debug!("✅ [客户端-开发模式] TLS 1.3 签名验证跳过（开发模式）");
                            Ok(HandshakeSignatureValid::assertion())
                        }
                        
                        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                            vec![
                                rustls::SignatureScheme::RSA_PKCS1_SHA1,
                                rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                                rustls::SignatureScheme::RSA_PKCS1_SHA384,
                                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                                rustls::SignatureScheme::RSA_PKCS1_SHA512,
                                rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                                rustls::SignatureScheme::RSA_PSS_SHA256,
                                rustls::SignatureScheme::RSA_PSS_SHA384,
                                rustls::SignatureScheme::RSA_PSS_SHA512,
                                rustls::SignatureScheme::ED25519,
                                rustls::SignatureScheme::ED448,
                            ]
                        }
                    }
                    
                    rustls::ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(std::sync::Arc::new(DangerousClientCertVerifier {
                            mtls_config: Some(mtls_config.clone()),
                        }))
                        .with_client_auth_cert(
                            client_cert_chain,
                            client_private_key,
                        ).map_err(|e| RatError::TlsError(format!("配置客户端证书失败: {}", e)))?
                } else {
                    // 非开发模式：严格证书验证
                    rustls::ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_client_auth_cert(
                            client_cert_chain,
                            client_private_key,
                        ).map_err(|e| RatError::TlsError(format!("配置客户端证书失败: {}", e)))?
                };
                
                // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
                tls_config.alpn_protocols = vec![b"h2".to_vec()];
                println!("🔍 [客户端-mTLS] 配置的 ALPN 协议: {:?}", tls_config.alpn_protocols);
                
                tls_config
            } else if self.config.development_mode {
                // 开发模式：跳过证书验证，无客户端证书
                warn!("⚠️  警告：连接池已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");
                
                use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
                use rustls::{pki_types, Error as RustlsError};
                
                #[derive(Debug)]
                struct DangerousClientCertVerifier {
                    development_mode: bool,
                }
                
                impl ServerCertVerifier for DangerousClientCertVerifier {
                    fn verify_server_cert(
                        &self,
                        end_entity: &pki_types::CertificateDer<'_>,
                        intermediates: &[pki_types::CertificateDer<'_>],
                        server_name: &pki_types::ServerName<'_>,
                        ocsp_response: &[u8],
                        now: pki_types::UnixTime,
                    ) -> Result<ServerCertVerified, RustlsError> {
                        debug!("🔍 [客户端-开发模式-无mTLS] 服务器证书验证开始");
                        debug!("   服务器名称: {:?}", server_name);
                        debug!("   证书验证模式: 开发模式（跳过验证）");
                        debug!("   中间证书数量: {}", intermediates.len());
                        debug!("   OCSP 响应: {}", if ocsp_response.is_empty() { "无" } else { "有" });
                        debug!("   验证时间: {:?}", now);
                        debug!("   开发模式状态: {}", self.development_mode);
                        
                        // 尝试解析证书信息以获取更多调试信息
                        if let Ok((_, cert)) = x509_parser::certificate::X509Certificate::from_der(end_entity) {
                            debug!("   证书主题: {}", cert.subject());
                            debug!("   证书颁发者: {}", cert.issuer());
                            debug!("   证书有效期: {} - {}", 
                                cert.validity().not_before.to_datetime(),
                                cert.validity().not_after.to_datetime());
                        }
                        
                        debug!("✅ [客户端-开发模式-无mTLS] 服务器证书验证跳过（开发模式）");
                        Ok(ServerCertVerified::assertion())
                    }
                    
                    fn verify_tls12_signature(
                        &self,
                        message: &[u8],
                        cert: &pki_types::CertificateDer<'_>,
                        dss: &rustls::DigitallySignedStruct,
                    ) -> Result<HandshakeSignatureValid, RustlsError> {
                        debug!("🔍 [客户端-开发模式-无mTLS] TLS 1.2 签名验证开始");
                        debug!("   签名验证模式: 开发模式（跳过验证）");
                        debug!("   签名算法: {:?}", dss.scheme);
                        debug!("   消息哈希: 已计算");
                        
                        // 尝试解析证书信息
                        if let Ok((_, cert_info)) = x509_parser::certificate::X509Certificate::from_der(cert) {
                            debug!("   证书主题: {}", cert_info.subject());
                            debug!("   公钥算法: {}", cert_info.public_key().algorithm.algorithm);
                        }
                        
                        debug!("✅ [客户端-开发模式-无mTLS] TLS 1.2 签名验证跳过（开发模式）");
                        Ok(HandshakeSignatureValid::assertion())
                    }
                    
                    fn verify_tls13_signature(
                        &self,
                        message: &[u8],
                        cert: &pki_types::CertificateDer<'_>,
                        dss: &rustls::DigitallySignedStruct,
                    ) -> Result<HandshakeSignatureValid, RustlsError> {
                        debug!("🔍 [客户端-开发模式-无mTLS] TLS 1.3 签名验证开始");
                        debug!("   签名验证模式: 开发模式（跳过验证）");
                        debug!("   签名算法: {:?}", dss.scheme);
                        debug!("   消息哈希: 已计算");
                        
                        // 尝试解析证书信息
                        if let Ok((_, cert_info)) = x509_parser::certificate::X509Certificate::from_der(cert) {
                            debug!("   证书主题: {}", cert_info.subject());
                            debug!("   公钥算法: {}", cert_info.public_key().algorithm.algorithm);
                        }
                        
                        debug!("✅ [客户端-开发模式-无mTLS] TLS 1.3 签名验证跳过（开发模式）");
                        Ok(HandshakeSignatureValid::assertion())
                    }
                    
                    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                        vec![
                            rustls::SignatureScheme::RSA_PKCS1_SHA1,
                            rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                            rustls::SignatureScheme::RSA_PKCS1_SHA256,
                            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                            rustls::SignatureScheme::RSA_PKCS1_SHA384,
                            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                            rustls::SignatureScheme::RSA_PKCS1_SHA512,
                            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                            rustls::SignatureScheme::RSA_PSS_SHA256,
                            rustls::SignatureScheme::RSA_PSS_SHA384,
                            rustls::SignatureScheme::RSA_PSS_SHA512,
                            rustls::SignatureScheme::ED25519,
                            rustls::SignatureScheme::ED448,
                        ]
                    }
                }
                
                let mut tls_config = rustls::ClientConfig::builder()
                    .dangerous()
                    .with_custom_certificate_verifier(std::sync::Arc::new(DangerousClientCertVerifier {
                        development_mode: self.config.development_mode,
                    }))
                    .with_no_client_auth();
                
                // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
                tls_config.alpn_protocols = vec![b"h2".to_vec()];
                
                tls_config
            } else {
                // 非开发模式：严格证书验证，无客户端证书
                let mut root_store = rustls::RootCertStore::empty();
                root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                
                let mut tls_config = rustls::ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth();
                
                // 配置 ALPN 协议协商，gRPC 只支持 HTTP/2
                tls_config.alpn_protocols = vec![b"h2".to_vec()];
                
                tls_config
            };
            
            let tls_connector = TlsConnector::from(std::sync::Arc::new(tls_config));
            
            // 解析服务器名称
            let server_name = ServerName::try_from(host.to_string())
                .map_err(|e| RatError::RequestError(format!("无效的服务器名称 '{}': {}", host, e)))?;
            
            // 建立 TLS 连接
            let tls_stream = tls_connector.connect(server_name, tcp_stream).await
                .map_err(|e| RatError::NetworkError(format!("TLS 连接失败: {}", e)))?;
            
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
            // HTTP: 根据协议模式选择 HTTP/1.1 或 HTTP/2
            match self.config.protocol_mode {
                ClientProtocolMode::Http1Only => {
                    // HTTP/1.1 模式：返回错误，暂不支持 HTTP/1.1 连接池
                    return Err(RatError::NetworkError(
                        "HTTP/1.1 连接池模式暂未实现，请使用 HTTP/2 或启用 HTTP/2 支持".to_string()
                    ));
                }
                _ => {
                    // HTTP/2 模式：进行 H2 握手
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
            }
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