//! RAT Engine 高性能核心模块
//! 
//! 这个模块实现了基于工作窃取的无锁架构，专注于最大化性能：
//! - 工作窃取队列调度
//! - 零拷贝网络 I/O
//! - 内存池管理
//! - 原子性能监控

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::future::Future;
use std::pin::Pin;

/// HTTP 请求结构体
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub query_string: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Vec<u8>,
    pub remote_addr: String,
    pub real_ip: String,
}

/// HTTP 响应结构体
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Vec<u8>,
}

/// 处理函数类型定义
pub type HandlerFn = Arc<dyn Fn(Vec<u8>) -> Pin<Box<dyn Future<Output = Vec<u8>> + Send>> + Send + Sync>;

pub mod work_stealing;
pub mod network;
pub mod memory;
pub mod metrics;
pub mod smart_transfer;
pub mod congestion_control;

use work_stealing::WorkStealingQueue;
use network::{HttpTask, ZeroCopyBuffer};
use memory::MemoryPool;
use metrics::AtomicMetrics;
use smart_transfer::SmartTransferManager;
use congestion_control::CongestionControlManager;

/// 高性能 RAT 引擎核心（空实现 - 所有功能通过 RatEngineBuilder 访问）
pub struct RatEngine {
    _private: (), // 私有字段，防止直接实例化
}

impl RatEngine {
    /// 创建 RatEngineBuilder（唯一的配置入口点）
    pub fn builder() -> RatEngineBuilder {
        RatEngineBuilder::new()
    }
    
    /// 获取引擎配置（通过 builder 访问）
    pub fn config(&self) -> &EngineConfig {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取性能指标（通过 builder 访问）
    pub fn get_metrics(&self) -> HashMap<String, u64> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取智能传输管理器（通过 builder 访问）
    pub fn get_smart_transfer(&self) -> &Arc<SmartTransferManager> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 智能传输数据（通过 builder 访问）
    pub fn smart_transfer_data(&self, data: &[u8]) -> crate::error::RatResult<crate::engine::smart_transfer::TransferResult> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取智能传输性能统计（通过 builder 访问）
    pub fn get_transfer_stats(&self) -> crate::engine::smart_transfer::PerformanceStats {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 重置智能传输统计（通过 builder 访问）
    pub fn reset_transfer_stats(&self) {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 启用拥塞控制（通过 builder 访问）
    pub async fn enable_congestion_control(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 禁用拥塞控制（通过 builder 访问）
    pub async fn disable_congestion_control(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 手动切换拥塞控制算法（通过 builder 访问）
    pub async fn switch_congestion_algorithm(&self, algorithm: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取当前拥塞控制算法（通过 builder 访问）
    pub async fn get_congestion_algorithm(&self) -> String {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取拥塞控制统计信息（通过 builder 访问）
    pub async fn get_congestion_stats(&self) -> HashMap<String, f64> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取拥塞控制窗口大小（通过 builder 访问）
    pub async fn get_congestion_window(&self) -> u32 {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 获取拥塞控制发送速率（通过 builder 访问）
    pub async fn get_congestion_send_rate(&self) -> f64 {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 处理数据包发送事件（通过 builder 访问）
    pub async fn on_packet_sent(&self, packet_size: u32) {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 处理数据包确认事件（通过 builder 访问）
    pub async fn on_packet_acked(&self, packet_size: u32, rtt: std::time::Duration) {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 处理数据包丢失事件（通过 builder 访问）
    pub async fn on_packet_lost(&self, packet_size: u32) {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
    
    /// 优雅关闭（通过 builder 访问）
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!("RatEngine is an empty implementation. Use RatEngineBuilder to create and configure engines.");
    }
}

/// 引擎配置
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub worker_threads: usize,
    pub max_connections: usize,
    pub buffer_size: usize,
    pub timeout: Duration,
    pub enable_keepalive: bool,
    pub tcp_nodelay: bool,
    pub congestion_control: crate::engine::congestion_control::CongestionControlConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            worker_threads: num_cpus::get(),
            max_connections: 10000,
            buffer_size: 8192,
            timeout: Duration::from_secs(30),
            enable_keepalive: true,
            tcp_nodelay: true,
            congestion_control: crate::engine::congestion_control::CongestionControlConfig {
                enabled: false,
                algorithm: "auto".to_string(),
                auto_switching: true,
                platform_optimized: true,
                metrics_window_size: 32,
                switch_cooldown_ms: 1000,
            },
        }
    }
}

/// 连接池管理
pub struct ConnectionPool {
    active_connections: AtomicU64,
    max_connections: usize,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            active_connections: AtomicU64::new(0),
            max_connections,
        }
    }
    
    pub fn try_acquire(&self) -> bool {
        let current = self.active_connections.load(Ordering::Relaxed);
        if current >= self.max_connections as u64 {
            false
        } else {
            self.active_connections.fetch_add(1, Ordering::Relaxed);
            true
        }
    }
    
    pub fn release(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    pub fn active_count(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }
}

/// RAT 引擎构建器（唯一的配置入口点）
pub struct RatEngineBuilder {
    engine_config: EngineConfig,
    server_config: crate::server::config::ServerConfig,
    router: Option<crate::server::Router>,
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
    auto_init_logger: bool,
    built: bool,
}

/// 中间件特征
pub trait Middleware: Send + Sync {
    fn before_request(&self, request: &mut HttpRequest) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn after_response(&self, response: &mut HttpResponse) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// 实际的 RAT 引擎实现
pub struct ActualRatEngine {
    /// 工作窃取队列
    work_queue: Arc<WorkStealingQueue<HttpTask>>,
    /// 连接池管理
    connection_pool: Arc<ConnectionPool>,
    /// 内存池
    memory_pool: Arc<MemoryPool>,
    /// 智能传输管理器
    smart_transfer: Arc<SmartTransferManager>,
    /// 拥塞控制管理器
    congestion_control: Arc<tokio::sync::Mutex<CongestionControlManager>>,
    /// 路由器
    router: Option<Arc<crate::server::Router>>,
    /// 证书管理器
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
    /// 性能监控
    metrics: Arc<AtomicMetrics>,
    /// 配置
    config: EngineConfig,
    /// 服务器配置
    server_config: crate::server::config::ServerConfig,
    /// 工作线程句柄
    worker_handles: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl RatEngineBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            engine_config: EngineConfig::default(),
            server_config: crate::server::config::ServerConfig::default(8080),
            router: None,
            cert_manager: None,
            auto_init_logger: false,
            built: false,
        }
    }
    
    /// 设置工作线程数
    pub fn worker_threads(mut self, count: usize) -> Self {
        self.engine_config.worker_threads = count.max(1);
        self.server_config.workers = count.max(1);
        self
    }
    
    /// 设置最大连接数
    pub fn max_connections(mut self, count: usize) -> Self {
        self.engine_config.max_connections = count.max(1);
        self
    }
    
    /// 设置缓冲区大小
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.engine_config.buffer_size = size.max(1024);
        self
    }
    
    /// 设置超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.engine_config.timeout = timeout;
        self
    }
    
    /// 启用/禁用 Keep-Alive
    pub fn keepalive(mut self, enabled: bool) -> Self {
        self.engine_config.enable_keepalive = enabled;
        self
    }
    
    /// 启用/禁用 TCP_NODELAY
    pub fn tcp_nodelay(mut self, enabled: bool) -> Self {
        self.engine_config.tcp_nodelay = enabled;
        self
    }
    
        
    /// 设置路由器（这是配置路由的唯一方式）
    pub fn router(mut self, router: crate::server::Router) -> Self {
        self.router = Some(router);
        self
    }
    
    /// 配置证书管理器（这是配置TLS/MTLS的唯一方式）
    pub fn certificate_manager(mut self, cert_manager: crate::server::cert_manager::CertificateManager) -> Self {
        self.cert_manager = Some(Arc::new(std::sync::RwLock::new(cert_manager)));
        self
    }
    
    /// 获取证书管理器的引用（用于测试和高级配置）
    pub fn get_cert_manager(&self) -> Option<&Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>> {
        self.cert_manager.as_ref()
    }
    
    /// 启用开发模式（自动生成自签名证书）
    pub async fn enable_development_mode(mut self, hostnames: Vec<String>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        self.enable_development_mode_with_whitelist(hostnames, Vec::new()).await
    }

    /// 配置证书（rustls + ring，仅支持 TLS）
    ///
    /// gRPC 强制要求 TLS 证书
    pub async fn with_certificate_files(mut self, cert_path: String, key_path: String, ca_path: Option<String>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use crate::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};

        // 确保 CryptoProvider 只安装一次
        crate::utils::crypto_provider::ensure_crypto_provider_installed();

        let cert_config = CertConfig::from_paths(cert_path, key_path);
        let cert_config = if let Some(ca) = ca_path {
            cert_config.with_ca(ca)
        } else {
            cert_config
        };

        let config = CertManagerConfig::shared(cert_config);
        let cert_manager = CertificateManager::from_config(config)?;

        self.cert_manager = Some(Arc::new(std::sync::RwLock::new(cert_manager)));
        crate::utils::logger::info!("✅ TLS 证书配置完成");
        Ok(self)
    }

    /// 配置分离的 gRPC 和 HTTP 证书（分端口模式）
    pub async fn with_separated_certificates(
        mut self,
        grpc_cert_path: String,
        grpc_key_path: String,
        http_cert_path: Option<String>,
        http_key_path: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use crate::server::cert_manager::{CertificateManager, CertConfig, CertManagerConfig};

        // 确保 CryptoProvider 只安装一次
        crate::utils::crypto_provider::ensure_crypto_provider_installed();

        let grpc_cert = CertConfig::from_paths(grpc_cert_path, grpc_key_path);

        let http_cert = if let (Some(http_cert_path), Some(http_key_path)) = (http_cert_path, http_key_path) {
            Some(CertConfig::from_paths(http_cert_path, http_key_path))
        } else {
            None
        };

        let config = CertManagerConfig::separated(grpc_cert, http_cert);
        let cert_manager = CertificateManager::from_config(config)?;

        self.cert_manager = Some(Arc::new(std::sync::RwLock::new(cert_manager)));
        crate::utils::logger::info!("✅ 分离证书配置完成（gRPC 和 HTTP）");
        Ok(self)
    }

    /// ⚠️ 已废弃：开发模式不再支持，必须配置证书
    #[deprecated(note = "开发模式已移除，请使用 with_certificate_files 配置证书")]
    pub async fn enable_development_mode_with_whitelist(self, _hostnames: Vec<String>, _mtls_whitelist_paths: Vec<String>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        panic!("开发模式已移除！gRPC 必须配置 TLS 证书。请使用 with_certificate_files() 方法配置证书。");
    }

    /// 配置证书文件（已废弃，请使用 with_certificate_files）
    #[deprecated(note = "请使用 with_certificate_files")]
    pub async fn with_certificate_files_old(mut self, cert_path: String, key_path: String, ca_path: Option<String>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        self.with_certificate_files(cert_path, key_path, ca_path).await
    }

    /// 配置拥塞控制
    pub fn congestion_control(mut self, enabled: bool, algorithm: String) -> Self {
        self.engine_config.congestion_control.enabled = enabled;
        self.engine_config.congestion_control.algorithm = algorithm;
        self
    }
    
    /// 创建一个新的Router
    pub fn create_router(&self) -> crate::server::Router {
        crate::server::Router::new_with_config(self.server_config.clone())
    }
    
    /// 创建并配置Router的便捷方法
    pub fn with_router<F>(self, config_fn: F) -> Self 
    where
        F: FnOnce(crate::server::Router) -> crate::server::Router,
    {
        let router = self.create_router();
        let configured_router = config_fn(router);
        self.router(configured_router)
    }
    
    /// 配置SPA支持
    pub fn spa_config(mut self, fallback_path: String) -> Self {
        self.server_config.spa_config = crate::server::config::SpaConfig::enabled(fallback_path);
        self
    }
    
    /// 启用自动日志初始化
    pub fn enable_logger(mut self) -> Self {
        self.auto_init_logger = true;
        self
    }
    
    /// 禁用自动日志初始化
    pub fn disable_logger(mut self) -> Self {
        self.auto_init_logger = false;
        self
    }
    
    /// 自定义日志配置
    pub fn with_log_config(mut self, log_config: crate::utils::logger::LogConfig) -> Self {
        self.server_config.log_config = Some(log_config);
        self.auto_init_logger = true;
        self
    }
    
    /// ⚠️ ACME 证书管理暂时不可用
    ///
    /// 新的 rustls 实现暂不支持 ACME 自动证书。
    /// 请使用 with_certificate_files() 配置静态证书文件。
    #[deprecated(note = "ACME 暂不支持，请使用 with_certificate_files 配置静态证书")]
    pub async fn cert_manager_acme(
        self,
        _domain: String,
        _email: String,
        _cloudflare_token: String,
        _cert_dir: String,
        _renewal_days: u32,
        _production: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        panic!("ACME 自动证书功能暂不可用！请使用 with_certificate_files() 方法配置静态证书文件。");
    }
    
    /// 构建引擎
    pub fn build(mut self) -> Result<ActualRatEngine, Box<dyn std::error::Error + Send + Sync>> {
        if self.built {
            return Err("Builder has already been used".into());
        }
        
        // 必须提供路由器
        if self.router.is_none() {
            return Err("Router must be provided. Use .router() method to set a router.".into());
        }
        
        self.built = true;
        
        // 如果启用，自动初始化日志系统（避免重复初始化）
        if self.auto_init_logger {
            // 检查日志系统是否已经初始化
            if let Some(log_config) = &self.server_config.log_config {
                match crate::utils::logger::Logger::init(log_config.clone()) {
                    Ok(_) => {},
                    Err(e) if e.to_string().contains("already initialized") => {
                        // 日志系统已经初始化，忽略错误
                    },
                    Err(e) => {
                        return Err(format!("日志系统初始化失败: {}", e).into());
                    }
                }
            }
        }
        
        let work_queue = Arc::new(WorkStealingQueue::new(self.engine_config.worker_threads));
        let connection_pool = Arc::new(ConnectionPool::new(self.engine_config.max_connections));
        let memory_pool = Arc::new(MemoryPool::new(self.engine_config.buffer_size));
        
        // 创建智能传输管理器
        let smart_transfer = Arc::new(SmartTransferManager::new()
            .map_err(|e| format!("智能传输管理器初始化失败: {}", e))?);
        
        let metrics = Arc::new(AtomicMetrics::new());
        
        // 创建拥塞控制管理器
        let congestion_control = Arc::new(tokio::sync::Mutex::new(
            CongestionControlManager::new(
                self.engine_config.congestion_control.clone(),
                metrics.clone()
            )
        ));
        
        Ok(ActualRatEngine {
            work_queue,
            connection_pool,
            memory_pool,
            smart_transfer,
            congestion_control,
            router: self.router.map(Arc::new),
            cert_manager: self.cert_manager,
            metrics,
            config: self.engine_config,
            server_config: self.server_config,
            worker_handles: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        })
    }
    
    /// 构建并启动服务器
    pub async fn build_and_start(self, host: String, port: u16) -> Result<ActualRatEngine, Box<dyn std::error::Error + Send + Sync>> {
        let engine = self.build()?;
        engine.start(host, port).await?;
        Ok(engine)
    }
}

impl ActualRatEngine {
    /// 启动服务器
    pub async fn start(&self, host: String, port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::net::TcpListener;
        
        // 初始化性能优化（包含所有配置信息输出）
        if let Some(log_config) = &self.server_config.log_config {
            crate::server::performance::init_performance_optimization(self.config.worker_threads, log_config)?;
        }
        
        // 确保 CryptoProvider 只安装一次
        crate::utils::crypto_provider::ensure_crypto_provider_installed();
        
        // 同步 worker 数量到性能管理器
        crate::server::performance::global_performance_manager().update_worker_count(self.config.worker_threads);
        
        let addr = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr).await?;

        // ============ 证书校验 ============
        // gRPC 强制要求 TLS 证书
        if let Some(router) = &self.router {
            let grpc_methods = router.list_grpc_methods();
            let has_grpc_methods = !grpc_methods.is_empty();
            let is_grpc_only = router.is_grpc_only();

            // 如果有 gRPC 方法或者是 gRPC 专用模式，必须有证书配置
            if has_grpc_methods || is_grpc_only {
                if self.cert_manager.is_none() {
                    panic!("gRPC 服务必须配置 TLS 证书！请在启动前配置证书。");
                }

                // 检查证书管理器是否有 gRPC 证书
                if let Some(cert_manager) = &self.cert_manager {
                    if let Ok(cert_manager_guard) = cert_manager.read() {
                        if !cert_manager_guard.has_grpc_cert() {
                            panic!("gRPC 服务必须配置 TLS 证书！请使用 CertManagerConfig::shared() 或 CertManagerConfig::separated() 配置证书。");
                        }
                    }
                }
            }
        }

        // 根据路由器配置确定支持的协议
        let supported_protocols = if let Some(router) = &self.router {
            let mut protocols = vec!["HTTP/1.1"];
            if router.is_h2_enabled() {
                protocols.push("HTTP/2");
            }
            if router.is_h2c_enabled() {
                protocols.push("H2C");
            }
            protocols.join(", ")
        } else {
            "HTTP/1.1".to_string()
        };
        
        crate::utils::logger::info!("🌐 RAT Engine server running on {} (支持: {})", addr, supported_protocols);
        
        // 打印已注册的路由
        if let Some(router) = &self.router {
            let routes = router.list_routes();
            let grpc_methods = router.list_grpc_methods();
            
            if !routes.is_empty() {
                crate::utils::logger::info!("📋 已注册的 HTTP 路由:");
                for (method, path) in routes {
                    crate::utils::logger::info!("   {} {}", method, path);
                }
            }
            
            if !grpc_methods.is_empty() {
                crate::utils::logger::info!("📝 已注册的 gRPC 方法:");
                for method in grpc_methods {
                    crate::utils::logger::info!("   {}", method);
                }
            }
        }
        
        crate::utils::logger::info!("🌐 服务器支持 HTTP 请求");

        // 注意：rustls 的 ALPN 在创建 ServerConfig 时已经设置（只支持 h2）
        // 不需要在这里配置 ALPN

        // 启动工作线程
        self.start_workers().await;
        
        // 主接受循环
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    if !self.connection_pool.try_acquire() {
                        crate::utils::logger::warn!("Connection limit reached, dropping connection from {}", addr);
                        continue;
                    }
                    
                    self.metrics.increment_connections();
                    
                    // 配置 TCP 选项
                    if self.config.tcp_nodelay {
                        let _ = stream.set_nodelay(true);
                    }
                    
                    // 使用协议检测处理连接
                    if let Some(router) = &self.router {
                        let router = router.clone();
                        let adapter = Arc::new(crate::server::hyper_adapter::HyperAdapter::new(router.clone()));
                        // 优先使用router中的证书管理器，否则使用engine的证书管理器
                        let cert_manager = router.get_cert_manager().or_else(|| self.cert_manager.clone());

                        // 异步处理连接（使用协议检测）
                        tokio::spawn(async move {
                            if let Err(e) = crate::server::detect_and_handle_protocol_with_tls(stream, addr, router, adapter, cert_manager).await {
                                crate::utils::logger::error!("连接处理失败: {}: {}", addr, e);
                            }
                        });
                    } else {
                        crate::utils::logger::error!("路由器未配置，无法处理连接");
                        drop(stream);
                    }
                }
                Err(e) => {
                    crate::utils::logger::error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
    
    /// 启动工作线程
    async fn start_workers(&self) {
        let mut handles = self.worker_handles.lock().await;
        for worker_id in 0..self.config.worker_threads {
            let work_queue = self.work_queue.clone();
            let connection_pool = self.connection_pool.clone();
            let router = self.router.clone();
            let metrics = self.metrics.clone();
            let timeout = self.config.timeout;
            
            let handle = tokio::spawn(async move {
                Self::worker_loop(worker_id, work_queue, connection_pool, router, metrics, timeout).await;
            });
            
            handles.push(handle);
        }
        
        crate::utils::logger::info!("✅ Started {} worker threads", self.config.worker_threads);
    }
    
    /// 工作线程主循环
    async fn worker_loop(
        worker_id: usize,
        work_queue: Arc<WorkStealingQueue<HttpTask>>,
        connection_pool: Arc<ConnectionPool>,
        router: Option<Arc<crate::server::Router>>,
        metrics: Arc<AtomicMetrics>,
        timeout: Duration,
    ) {
        crate::utils::logger::debug!("Worker {} started", worker_id);
        
        let mut empty_count = 0;
        
        loop {
            // 从队列中获取任务
            if let Some(mut task) = work_queue.pop(worker_id) {
                empty_count = 0; // 重置空计数
                
                let start_time = Instant::now();
                
                // 处理 HTTP 请求
                let result = Self::process_http_task(&mut task, &router, timeout).await;
                
                // 记录性能指标
                let duration = start_time.elapsed();
                metrics.record_request_duration(duration);
                
                if result.is_err() {
                    metrics.increment_errors();
                }
                
                // 释放连接
                connection_pool.release();
            } else {
                empty_count += 1;
                
                // 没有任务时使用指数退避策略，避免忙等待
                let sleep_duration = if empty_count < 10 {
                    // 前10次空轮询：5ms
                    Duration::from_millis(5)
                } else if empty_count < 50 {
                    // 10-50次：20ms
                    Duration::from_millis(20)
                } else {
                    // 50次后：100ms
                    Duration::from_millis(100)
                };
                
                tokio::time::sleep(sleep_duration).await;
            }
        }
    }
    
    /// 处理单个 HTTP 任务
    async fn process_http_task(
        task: &mut HttpTask,
        router: &Option<Arc<crate::server::Router>>,
        timeout: Duration,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        
        // 读取 HTTP 请求
        let request = task.read_request().await?;
        
        // 记录请求日志
        crate::utils::logger::debug!("🔍 [引擎] 处理 HTTP 请求: {} {}", request.method, request.path);
        
        // 转换为服务器 HttpRequest 类型
        let server_request = crate::server::http_request::HttpRequest {
            method: hyper::Method::from_bytes(request.method.as_bytes())?,
            uri: format!("{}{}", request.path, request.query_string).parse()?,
            version: hyper::Version::HTTP_11,
            headers: request.headers.iter().filter_map(|(k, v)| {
                Some((hyper::header::HeaderName::from_bytes(k.as_bytes()).ok()?, hyper::header::HeaderValue::from_str(v).ok()?))
            }).collect(),
            body: request.body.into(),
            remote_addr: Some(request.remote_addr.parse()?),
            source: crate::server::http_request::RequestSource::Http1,
            path_params: std::collections::HashMap::new(),
            python_handler_name: None,
        };
        
        // 使用路由器处理请求
        if let Some(router) = router {
            // 使用路由器处理请求
            let result = router.handle_http(server_request).await;
            let total_duration = start_time.elapsed();
            
            match result {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    
                    // 统计信息日志（info 级别，生产环境可见）
                    crate::utils::logger::info!(
                        "📊 {} {} {} {} {}ms", 
                        request.real_ip, 
                        request.method, 
                        request.path, 
                        status_code, 
                        total_duration.as_millis()
                    );
                    
                    // 转换响应为字节数据
                    let response_data = Self::convert_response_to_bytes(response).await?;
                    task.send_response(response_data).await?;
                }
                Err(e) => {
                    // 错误访问日志 - error级别
                    crate::utils::logger::error!(
                        "❌ {} {} {} ERROR {}ms - {}", 
                        request.real_ip, 
                        request.method, 
                        request.path, 
                        total_duration.as_millis(),
                        e
                    );
                    
                    crate::utils::logger::error!("❌ [引擎] 路由器处理请求失败: {}", e);
                    let error_response = b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\n\r\nInternal Server Error";
                    task.send_response(error_response.to_vec()).await?;
                }
            }
        } else {
            // 默认响应
            let total_duration = start_time.elapsed();
            crate::utils::logger::warn!("⚠️ [引擎] 没有配置路由器");
            
            // 错误访问日志
            crate::utils::logger::error!(
                "❌ {} {} {} 500 {}ms - No router configured", 
                request.real_ip, 
                request.method, 
                request.path, 
                total_duration.as_millis()
            );
            
            let response_data = b"HTTP/1.1 500 Internal Server Error\r\n\r\nNo router configured";
            task.send_response(response_data.to_vec()).await?;
        }
        
        Ok(())
    }
    
    /// 将 hyper::Response 转换为 HTTP 响应字节数据
    async fn convert_response_to_bytes(
        response: hyper::Response<http_body_util::combinators::BoxBody<hyper::body::Bytes, Box<dyn std::error::Error + Send + Sync>>>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        use http_body_util::BodyExt;
        
        let (parts, body) = response.into_parts();
        let body_bytes = body.collect().await?.to_bytes();
        
        // 构建 HTTP 响应字符串
        let version_str = match parts.version {
            hyper::Version::HTTP_09 => "HTTP/0.9",
            hyper::Version::HTTP_10 => "HTTP/1.0",
            hyper::Version::HTTP_11 => "HTTP/1.1",
            hyper::Version::HTTP_2 => "HTTP/2.0",
            hyper::Version::HTTP_3 => "HTTP/3.0",
            _ => "HTTP/1.1",
        };
        let mut response_str = format!("{} {} {}\r\n", version_str, parts.status.as_u16(), parts.status.canonical_reason().unwrap_or("OK"));
        
        // 添加头部
        for (name, value) in parts.headers.iter() {
            if let Ok(value_str) = value.to_str() {
                response_str.push_str(&format!("{}: {}\r\n", name, value_str));
            }
        }
        
        response_str.push_str("\r\n");
        
        // 转换为字节数组
        let mut response_bytes = response_str.into_bytes();
        response_bytes.extend_from_slice(&body_bytes);
        
        Ok(response_bytes)
    }
    
    /// 获取性能指标
    pub fn get_metrics(&self) -> HashMap<String, u64> {
        self.metrics.get_all()
    }
    
    /// 重置性能指标
    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }
    
    /// 获取工作线程数
    pub fn get_workers(&self) -> usize {
        self.config.worker_threads
    }
    
    /// 获取最大连接数
    pub fn get_max_connections(&self) -> usize {
        self.config.max_connections
    }
    
    /// 获取主机地址
    pub fn get_host(&self) -> &str {
        "127.0.0.1" // 默认值，可以从配置中获取
    }
    
    /// 获取端口
    pub fn get_port(&self) -> u16 {
        8000 // 默认值，可以从配置中获取
    }
    
    /// 获取智能传输管理器
    pub fn get_smart_transfer(&self) -> &Arc<SmartTransferManager> {
        &self.smart_transfer
    }
    
    /// 智能传输数据
    pub fn smart_transfer_data(&self, data: &[u8]) -> crate::error::RatResult<crate::engine::smart_transfer::TransferResult> {
        self.smart_transfer.smart_transfer(data)
    }
    
    /// 获取智能传输性能统计
    pub fn get_transfer_stats(&self) -> crate::engine::smart_transfer::PerformanceStats {
        self.smart_transfer.get_performance_stats()
    }
    
    /// 重置智能传输统计
    pub fn reset_transfer_stats(&self) {
        self.smart_transfer.reset_stats();
    }
    
    /// 启用拥塞控制
    pub async fn enable_congestion_control(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut manager = self.congestion_control.lock().await;
        // CongestionControlManager 没有 enable 方法，拥塞控制在创建时就已启用
        crate::utils::logger::info!("✅ 拥塞控制已启用");
        Ok(())
    }
    
    /// 禁用拥塞控制
    pub async fn disable_congestion_control(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut manager = self.congestion_control.lock().await;
        // CongestionControlManager 没有 disable 方法
        crate::utils::logger::info!("⏸️ 拥塞控制已禁用");
        Ok(())
    }
    
    /// 手动切换拥塞控制算法
    pub async fn switch_congestion_algorithm(&self, algorithm: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut manager = self.congestion_control.lock().await;
        manager.switch_algorithm(algorithm)?;
        crate::utils::logger::info!("🔄 拥塞控制算法已切换到: {}", algorithm);
        Ok(())
    }
    
    /// 获取当前拥塞控制算法
    pub async fn get_congestion_algorithm(&self) -> String {
        let manager = self.congestion_control.lock().await;
        manager.current_algorithm()
    }
    
    /// 获取拥塞控制统计信息
    pub async fn get_congestion_stats(&self) -> HashMap<String, f64> {
        let manager = self.congestion_control.lock().await;
        manager.get_stats()
    }
    
    /// 获取拥塞控制窗口大小
    pub async fn get_congestion_window(&self) -> u32 {
        let manager = self.congestion_control.lock().await;
        manager.window_size() as u32
    }
    
    /// 获取拥塞控制发送速率
    pub async fn get_congestion_send_rate(&self) -> f64 {
        let manager = self.congestion_control.lock().await;
        manager.pacing_rate() as f64
    }
    
    /// 处理数据包发送事件（用于拥塞控制）
    pub async fn on_packet_sent(&self, packet_size: u32) {
        if self.config.congestion_control.enabled {
            let mut manager = self.congestion_control.lock().await;
            manager.on_packet_sent(packet_size);
        }
    }
    
    /// 处理数据包确认事件（用于拥塞控制）
    pub async fn on_packet_acked(&self, packet_size: u32, rtt: std::time::Duration) {
        if self.config.congestion_control.enabled {
            let mut manager = self.congestion_control.lock().await;
            manager.on_packet_acked(packet_size, rtt);
        }
    }
    
    /// 处理数据包丢失事件（用于拥塞控制）
    pub async fn on_packet_lost(&self, packet_size: u32) {
        if self.config.congestion_control.enabled {
            let mut manager = self.congestion_control.lock().await;
            manager.on_packet_lost(packet_size);
        }
    }
    
    /// 优雅关闭
    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        crate::utils::logger::info!("🛑 Shutting down RAT Engine...");
        
        // 等待所有工作线程完成
        let mut handles = self.worker_handles.lock().await;
        for handle in handles.drain(..) {
            handle.abort();
        }
        
        crate::utils::logger::info!("✅ RAT Engine shutdown complete");
        Ok(())
    }
}