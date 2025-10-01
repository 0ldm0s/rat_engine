//! RAT Engine 服务器模块
//! 
//! 提供高性能的 HTTP 服务器实现

use hyper::{Request, Response};
use hyper::body::Incoming;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use hyper::service::service_fn;
use hyper_util::server::conn::auto::Builder;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use h2::server::SendResponse;
use h2::RecvStream;
use bytes;
use http_body_util;
use psi_detector::core::protocol::ProtocolType;
use crate::utils::logger::{debug, info, warn, error};

/// 重新构造的流，包含预读的数据
struct ReconstructedStream {
    inner: tokio::net::TcpStream,
    prefix: Vec<u8>,
    prefix_pos: usize,
}

impl ReconstructedStream {
    fn new(stream: tokio::net::TcpStream, prefix: &[u8]) -> Self {
        Self {
            inner: stream,
            prefix: prefix.to_vec(),
            prefix_pos: 0,
        }
    }
}

impl AsyncRead for ReconstructedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // 首先读取预读的数据
        if self.prefix_pos < self.prefix.len() {
            let remaining_prefix = &self.prefix[self.prefix_pos..];
            let to_copy = std::cmp::min(remaining_prefix.len(), buf.remaining());
            buf.put_slice(&remaining_prefix[..to_copy]);
            self.prefix_pos += to_copy;
            return Poll::Ready(Ok(()));
        }
        
        // 预读数据已经读完，从原始流读取
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ReconstructedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
pub mod config;
pub mod port_config;
pub mod router;
pub mod trie_router;
pub mod worker_pool;
pub mod hyper_adapter;
pub mod performance;
pub mod file_handler;
pub mod streaming;
#[cfg(feature = "compression")]
pub mod compression_middleware;
#[cfg(feature = "compression")]
pub mod compression_middleware_impl;
#[cfg(feature = "cache")]
pub mod cache_middleware;
#[cfg(feature = "cache")]
pub mod cache_middleware_impl;
#[cfg(feature = "cache")]
pub mod cache_version_manager;
pub mod protocol_detection_middleware;
pub mod grpc_types;
pub mod grpc_codec;
pub mod cert_manager;
pub mod grpc_handler;
pub mod grpc_queue_bridge_adapter;
pub mod grpc_delegated_handler;
pub mod http_request;
pub mod sse_connection_pool;
pub mod sse_adapter;

pub use config::ServerConfig;
pub use port_config::{PortConfig, PortConfigBuilder, PortMode, PortConfigError, HttpsConfig, CertificateConfig};
pub use router::Router;
pub use performance::{PerformanceManager, global_performance_manager, init_performance_optimization, set_thread_affinity, optimize_for_throughput};
pub use worker_pool::WorkerPool;
pub use hyper_adapter::HyperAdapter;
pub use streaming::{SseManager, StreamingResponse, SseResponse, ChunkedResponse};
pub use sse_adapter::{SseAdapter, SseAdapterBuilder};
pub use sse_connection_pool::{SseConnectionPool, SseConnectionPoolConfig, SseProtocolVersion, SseConnectionType};


/// 使用自定义路由器启动服务器（已弃用 - 请使用 RatEngineBuilder）
/// 
/// # ⚠️ 重要提醒
/// 此函数已被弃用，因为它绕过了 RatEngine 架构。
/// 请使用 `RatEngine::builder()` 来创建和配置引擎。
#[deprecated(since = "1.0.0", note = "请使用 `RatEngine::builder()` 来创建和配置引擎")]
pub async fn run_server_with_router(config: ServerConfig, router: Router) -> crate::error::RatResult<()> {
    crate::utils::logger::error!("🚫 run_server_with_router 已被弃用！请使用 RatEngine::builder() 来创建和配置引擎。");
    panic!("run_server_with_router 已被弃用！请使用 RatEngine::builder() 来创建和配置引擎。");
}

/// 创建 RAT 引擎构建器（推荐使用的服务器启动方式）
/// 
/// 这是创建和配置 RAT 引擎的唯一入口点。
/// 
/// # 示例
/// 
/// ```rust
/// use rat_engine::RatEngine;
/// 
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let engine = RatEngine::builder()
///         .worker_threads(4)
///         .max_connections(10000)
///         .route("/".to_string(), |data| async move {
///             b"Hello World".to_vec()
///         })
///         .build_and_start("127.0.0.1".to_string(), 8080).await?;
///     
///     // 服务器正在运行...
///     
///     Ok(())
/// }
/// ```
pub fn create_engine_builder() -> crate::engine::RatEngineBuilder {
    crate::engine::RatEngineBuilder::new()
}

/// 分端口模式启动服务器
async fn run_separated_server(config: ServerConfig, router: Router) -> crate::error::RatResult<()> {
    let router = Arc::new(router);
    let adapter = Arc::new(HyperAdapter::new(router.clone()));

    // 获取 HTTP 和 gRPC 地址
    let http_addr = config.addr();
    let grpc_addr = config.grpc_addr().ok_or_else(|| {
        format!("分端口模式下必须配置 gRPC 端口，当前配置: {:?}", config.port_config.mode)
    })?;

    // 绑定 HTTP 监听器
    let http_listener = TcpListener::bind(&http_addr).await
        .map_err(|e| crate::error::RatError::IoError(e))?;

    // 绑定 gRPC 监听器
    let grpc_listener = TcpListener::bind(&grpc_addr).await
        .map_err(|e| crate::error::RatError::IoError(e))?;

    // 统一配置 ALPN 协议支持
    let mut protocols = Vec::new();
    let has_tls = router.get_cert_manager().is_some();
    
    if has_tls {
        let mut alpn_protocols = Vec::new();
        let grpc_methods = router.list_grpc_methods();
        let has_grpc_methods = !grpc_methods.is_empty();
        
        if router.is_h2_enabled() {
            alpn_protocols.push(b"h2".to_vec());
            protocols.push("HTTP/2 (TLS)");
        }
        
        // 只有在没有 gRPC 方法且未启用 H2 或同时启用了 H2C 时才添加 HTTP/1.1 作为回退
        // gRPC 强制要求 HTTP/2，所以不能回退到 HTTP/1.1
        if !has_grpc_methods && (!router.is_h2_enabled() || router.is_h2c_enabled()) {
            alpn_protocols.push(b"http/1.1".to_vec());
            protocols.push("HTTPS/1.1");
        }
        
        if let Some(cert_manager) = router.get_cert_manager() {
            if let Ok(mut cert_manager_guard) = cert_manager.write() {
                if let Err(e) = cert_manager_guard.configure_alpn_protocols(alpn_protocols) {
                    crate::utils::logger::error!("配置 ALPN 协议失败: {}", e);
                    return Err(crate::error::RatError::ConfigError(format!("ALPN 配置失败: {}", e)));
                }
                crate::utils::logger::info!("✅ ALPN 协议配置成功");
            }
        }
    }
    
    if router.is_h2c_enabled() {
        protocols.push("H2C");
    }
    
    if protocols.is_empty() {
        protocols.push("HTTP/1.1");
    }
    
    let protocol_str = protocols.join(", ");
    let scheme = if has_tls { "https" } else { "http" };
    
    crate::utils::logger::info!("🚀 RAT Engine server running in separated mode:");
    crate::utils::logger::info!("   📡 HTTP server: {}://{} (支持: {})", scheme, http_addr, protocol_str);
    crate::utils::logger::info!("   🔧 gRPC server: {}://{}", scheme, grpc_addr);

    // 显示已注册的路由和 gRPC 方法
    let routes = router.list_routes();
    let grpc_methods = router.list_grpc_methods();
    let has_http_routes = !routes.is_empty();
    let has_grpc_methods = !grpc_methods.is_empty();
    
    if has_http_routes {
        crate::utils::logger::info!("📋 已注册的 HTTP 路由:");
        for (method, path) in routes {
            crate::utils::logger::info!("   {} {}", method, path);
        }
    }
    
    if has_grpc_methods {
        crate::utils::logger::info!("🔧 已注册的 gRPC 方法:");
        for method in grpc_methods {
            crate::utils::logger::info!("   {}", method);
        }
    }

    // 创建信号处理器
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    // HTTP 服务器循环
    let http_server_loop = {
        let router = router.clone();
        let adapter = adapter.clone();
        async move {
            loop {
                let (stream, remote_addr) = http_listener.accept().await
                    .map_err(|e| crate::error::RatError::IoError(e))?;
                
                let router_clone = router.clone();
                let adapter_clone = adapter.clone();
                
                tokio::task::spawn(async move {
                    if let Err(err) = handle_http_connection(stream, remote_addr, router_clone, adapter_clone).await {
                        let err_str = err.to_string();
                        if err_str.contains("IncompleteMessage") || err_str.contains("connection closed") {
                            crate::utils::logger::debug!("HTTP client disconnected: {:?}", err);
                        } else {
                            crate::utils::logger::error!("Error serving HTTP connection: {:?}", err);
                        }
                    }
                });
            }
        }
    };

    // gRPC 服务器循环
    let grpc_server_loop = {
        let router = router.clone();
        let adapter = adapter.clone();
        async move {
            loop {
                let (stream, remote_addr) = grpc_listener.accept().await
                    .map_err(|e| crate::error::RatError::IoError(e))?;
                
                let router_clone = router.clone();
                let adapter_clone = adapter.clone();
                
                tokio::task::spawn(async move {
                    if let Err(err) = handle_grpc_connection(stream, remote_addr, router_clone, adapter_clone).await {
                        let err_str = err.to_string();
                        if err_str.contains("IncompleteMessage") || err_str.contains("connection closed") {
                            crate::utils::logger::debug!("gRPC client disconnected: {:?}", err);
                        } else {
                            crate::utils::logger::error!("Error serving gRPC connection: {:?}", err);
                        }
                    }
                });
            }
        }
    };

    // 等待任一服务器循环或 Ctrl+C 信号
    tokio::select! {
        result = http_server_loop => {
            result
        }
        result = grpc_server_loop => {
            result
        }
        _ = ctrl_c => {
            println!("\n🛑 收到 Ctrl+C 信号，正在优雅关闭服务器...");
            Ok(())
        }
    }
}

/// 处理 HTTP 连接（分端口模式）
async fn handle_http_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::utils::logger::debug!("🔗 [HTTP] 新连接: {}", remote_addr);
    
    // 在分端口模式下，HTTP 端口只处理 HTTP 协议
    // 直接复用现有的协议检测逻辑，但只允许 HTTP 协议
    detect_and_handle_protocol(stream, remote_addr, router, adapter).await
}

/// 处理 gRPC 连接（分端口模式）
async fn handle_grpc_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::utils::logger::debug!("🔗 [gRPC] 新连接: {}", remote_addr);
    
    // 在分端口模式下，gRPC 端口只处理 gRPC 协议
    // 直接复用现有的协议检测逻辑，但只允许 gRPC 协议
    detect_and_handle_protocol(stream, remote_addr, router, adapter).await
}

/// 处理单个连接，支持 HTTP/1.1、HTTP/2 和 gRPC
async fn handle_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("🔗 [服务端] 新连接: {}", remote_addr);
    debug!("🔍 [服务端] H2C 启用状态: {}", router.is_h2c_enabled());
    
    // 始终进行协议检测，以支持 TLS、HTTP/2 等协议
    debug!("🔍 [服务端] 开始协议检测: {}", remote_addr);
    
    // 尝试检测协议类型并路由到相应的处理器
    match detect_and_handle_protocol(stream, remote_addr, router.clone(), adapter.clone()).await {
        Ok(_) => return Ok(()),
        Err(e) => {
            println!("❌ [服务端] 协议检测失败: {}", e);
            return Err(e);
        }
    }
}

/// 检测协议类型并处理连接
pub async fn detect_and_handle_protocol(
    mut stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 调用带有TLS支持的版本，但不传递证书管理器
    detect_and_handle_protocol_with_tls(stream, remote_addr, router, adapter, None).await
}

pub async fn detect_and_handle_protocol_with_tls(
    mut stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    tls_cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use psi_detector::{
        builder::DetectorBuilder,
        core::{
            detector::{DefaultProtocolDetector, DetectionConfig, ProtocolDetector},
            protocol::ProtocolType,
            probe::ProbeStrategy,
        },
    };
    
    // 读取连接的前几个字节来检测协议
    let mut buffer = [0u8; 1024]; // 增加缓冲区大小以便更好地进行协议检测
    let mut total_read = 0;
    
    // 尝试读取数据，但设置超时
    let read_result = tokio::time::timeout(
        std::time::Duration::from_millis(1000), // 增加超时时间到1秒，给正常客户端足够时间
        async {
            while total_read < buffer.len() {
                match stream.read(&mut buffer[total_read..]).await {
                    Ok(0) => break, // 连接关闭
                    Ok(n) => total_read += n,
                    Err(e) => return Err(e),
                }
                
                // 如果已经读取到足够的数据来判断协议，就提前退出
                if total_read >= 64 { // 增加最小读取量到64字节，确保能检测到HTTP/2前言
                    break;
                }
            }
            Ok(total_read)
        }
    ).await;
    
    let bytes_read = match read_result {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            debug!("🚫 [服务端] 读取协议检测数据失败，疑似慢速攻击，直接丢弃连接: {} (错误: {})", remote_addr, e);
            crate::utils::logger::warn!("🚫 读取协议检测数据失败，疑似慢速攻击，丢弃连接: {} (错误: {})", remote_addr, e);
            // 直接关闭连接，不进行任何响应，避免背压
            drop(stream);
            return Ok(());
        }
        Err(_) => {
            // 超时，疑似慢速攻击，直接丢弃连接
            debug!("🚫 [服务端] 协议检测超时，疑似慢速攻击，直接丢弃连接: {}", remote_addr);
            crate::utils::logger::warn!("🚫 协议检测超时，疑似慢速攻击，丢弃连接: {}", remote_addr);
            // 直接关闭连接，不进行任何响应，避免背压
            drop(stream);
            return Ok(());
        }
    };
    
    if bytes_read == 0 {
        debug!("🔌 [服务端] 连接立即关闭: {}", remote_addr);
        return Ok(());
    }
    
    // 使用 psi_detector 进行协议检测
    let detection_data = &buffer[..bytes_read];
    println!("🔍 [服务端] 开始 psi_detector 协议检测: {} (数据长度: {})", remote_addr, bytes_read);
    
    // 添加调试信息：打印接收到的数据
    let data_preview = String::from_utf8_lossy(&buffer[..bytes_read.min(50)]);
    println!("🔍 [服务端] 接收到的数据预览: {}", data_preview);
    
    // 创建协议检测器
    let detector = match DetectorBuilder::new()
        .enable_http()
        .enable_http2()
        .enable_grpc()
        .enable_tls()  // 添加 TLS 检测支持
        .balanced()
        .build()
    {
        Ok(detector) => detector,
        Err(e) => {
            debug!("🚫 [服务端] 创建协议检测器失败，疑似扫描器攻击，直接丢弃连接: {} (错误: {})", remote_addr, e);
            crate::utils::logger::warn!("🚫 协议检测器创建失败，疑似扫描器攻击，丢弃连接: {} (错误: {})", remote_addr, e);
            // 直接关闭连接，不进行任何响应
            drop(stream);
            return Ok(());
        }
    };
    
    // 执行协议检测
    let detection_result = detector.detect(detection_data);
    
    match detection_result {
        Ok(result) => {
            let protocol_type = result.protocol_type();
            let confidence = result.confidence();
            
            println!("🔍 [服务端] psi_detector 检测结果: {} (置信度: {:.1}%, 协议: {:?})", 
                remote_addr, confidence * 100.0, protocol_type);
            
            // 检查是否需要拦截
            if should_block_protocol(&protocol_type, confidence) {
                debug!("🚫 [服务端] 拦截恶意或未知协议: {} (协议: {:?}, 置信度: {:.1}%)", 
                    remote_addr, protocol_type, confidence * 100.0);
                
                // 发送拦截响应并关闭连接
                let block_response = b"HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 47\r\n\r\n{\"error\":\"Forbidden\",\"message\":\"Protocol blocked\"}";
                let _ = stream.write_all(block_response).await;
                let _ = stream.shutdown().await;
                return Ok(());
            }
            
            // 根据检测结果路由到相应的处理器
            route_by_detected_protocol(stream, &buffer[..bytes_read], protocol_type, remote_addr, router, adapter, tls_cert_manager.clone()).await
        }
        Err(e) => {
            debug!("🚫 [服务端] psi_detector 检测失败，疑似恶意探测，直接丢弃连接: {} (错误: {})", remote_addr, e);
            crate::utils::logger::warn!("🚫 协议检测失败，疑似恶意探测，丢弃连接: {} (错误: {})", remote_addr, e);
            // 直接关闭连接，不进行任何响应
            drop(stream);
            Ok(())
        }
    }
}

/// 判断是否应该拦截协议
/// 作为纯 HTTP + gRPC 服务器库，只允许以下协议：
/// - HTTP/1.0, HTTP/1.1, HTTP/2, HTTP/3 (HTTP 协议族)
/// - gRPC (基于 HTTP/2)
/// - TLS (用于 HTTPS)
/// - 低置信度的未知协议（可能是 HTTP 变种）
fn should_block_protocol(protocol_type: &ProtocolType, confidence: f32) -> bool {
    match protocol_type {
        // 允许的协议
        ProtocolType::HTTP1_0 => false,  // HTTP/1.0 协议允许
        ProtocolType::HTTP1_1 => false,  // HTTP/1.1 协议允许
        ProtocolType::HTTP2 => false,    // HTTP/2 协议允许
        ProtocolType::HTTP3 => false,    // HTTP/3 协议允许
        ProtocolType::GRPC => false,     // gRPC 协议允许
        ProtocolType::TLS => false,      // TLS 协议允许（用于 HTTPS）
        ProtocolType::Unknown => {
            // 对于未知协议，如果置信度很低（<0.5），可能是HTTP变种，允许尝试
            // 如果置信度较高（>=0.5），说明确实是其他协议，应该拦截
            confidence >= 0.5
        }
        
        // 拦截的协议 - 所有非 HTTP/gRPC 协议
        ProtocolType::WebSocket => true, // WebSocket 协议拦截
        ProtocolType::SSH => true,       // SSH 协议拦截
        ProtocolType::TCP => true,       // 原始 TCP 协议拦截
        ProtocolType::QUIC => true,      // QUIC 协议拦截（除非是 HTTP/3）
        ProtocolType::MQTT => true,      // MQTT 协议拦截
        ProtocolType::UDP => true,       // UDP 协议拦截
        
        // 其他协议默认拦截
        ProtocolType::FTP => true,       // FTP 协议拦截
        ProtocolType::SMTP => true,      // SMTP 协议拦截
        ProtocolType::DNS => true,       // DNS 协议拦截
        ProtocolType::Redis => true,     // Redis 协议拦截
        ProtocolType::MySQL => true,     // MySQL 协议拦截
        ProtocolType::Custom => true,    // 自定义协议拦截
    }
}

/// 根据检测到的协议类型路由到相应的处理器
async fn route_by_detected_protocol(
    stream: tokio::net::TcpStream,
    buffer: &[u8],
    protocol_type: ProtocolType,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    tls_cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match protocol_type {
        ProtocolType::HTTP1_0 | ProtocolType::HTTP1_1 => {
            println!("🌐 [服务端] 路由到 HTTP/1.1 处理器: {}", remote_addr);
            let reconstructed_stream = ReconstructedStream::new(stream, buffer);
            handle_http1_connection_with_stream(reconstructed_stream, remote_addr, adapter).await
        }
        ProtocolType::TLS => {
            info!("🔐 [服务端] 检测到 TLS 连接，进行 TLS 握手: {}", remote_addr);
            let reconstructed_stream = ReconstructedStream::new(stream, buffer);
            handle_tls_connection(reconstructed_stream, remote_addr, router, adapter, tls_cert_manager.clone()).await
        }
        ProtocolType::HTTP2 | ProtocolType::GRPC => {
            // 检查是否是 TLS 连接上的 HTTP/2
            // 通过检查数据开头是否是 TLS 记录类型 (0x16) 来判断
            if !buffer.is_empty() && buffer[0] == 0x16 {
                // 这是 TLS 连接上的 HTTP/2，需要先进行 TLS 握手
                info!("🔐 [服务端] 检测到 TLS 上的 HTTP/2 连接，进行 TLS 握手: {}", remote_addr);
                let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                handle_tls_connection(reconstructed_stream, remote_addr, router, adapter, tls_cert_manager.clone()).await
            } else {
                // 这是 cleartext HTTP/2 (H2C)
                if router.is_h2c_enabled() {
                    debug!("🚀 [服务端] 路由到 HTTP/2 (H2C) 处理器: {}", remote_addr);
                    let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                    handle_h2c_connection_with_stream(reconstructed_stream, remote_addr, router).await
                } else {
                    warn!("🚫 [服务端] 检测到 HTTP/2 连接但 H2C 未启用，拒绝连接: {}", remote_addr);
                    Err("HTTP/2 over cleartext (H2C) 未启用".into())
                }
            }
        }
        ProtocolType::WebSocket => {
            warn!("🚫 [服务端] WebSocket 协议不支持，拒绝连接: {}", remote_addr);
            Err("WebSocket 协议不支持".into())
        }
        ProtocolType::Unknown => {
            println!("🤔 [服务端] 未知协议类型，尝试按HTTP/1.1处理: {} (协议: {:?})", remote_addr, protocol_type);
            // 对于未知协议，尝试按HTTP/1.1处理，可能是HTTP变种或者检测不准确
            let reconstructed_stream = ReconstructedStream::new(stream, buffer);
            handle_http1_connection_with_stream(reconstructed_stream, remote_addr, adapter).await
        }
        _ => {
            warn!("🚫 [服务端] 不支持的协议类型，拒绝连接: {} (协议: {:?})", remote_addr, protocol_type);
            Err("不支持的协议类型".into())
        }
    }
}



/// 处理 TLS 连接
async fn handle_tls_connection<S>(
    stream: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use tokio_rustls::{TlsAcceptor, rustls::ServerConfig as RustlsServerConfig};
    
    // 获取证书管理器
    let cert_manager = cert_manager
        .ok_or("TLS 连接需要证书管理器，但未配置")?;
    
    // 获取服务器配置
    let server_config = {
        let cert_manager_guard = cert_manager.read()
            .map_err(|e| format!("无法获取证书管理器读锁: {}", e))?;
        cert_manager_guard.get_server_config()
            .ok_or("证书管理器未初始化服务器配置")?
    };
    
    // 创建 TLS 接受器
    let acceptor = TlsAcceptor::from(server_config);
    
    info!("🔐 [服务端] 开始 TLS 握手: {}", remote_addr);
    
    // 进行 TLS 握手
    let tls_stream = acceptor.accept(stream).await
        .map_err(|e| {
            error!("❌ [服务端] TLS 握手失败: {}", e);
            format!("TLS 握手失败: {}", e)
        })?;
    
    info!("✅ [服务端] TLS 握手成功: {}", remote_addr);
    
    // 直接使用 ALPN 协商结果进行路由，无需重复协议检测
    let negotiated_protocol = tls_stream.get_ref().1.alpn_protocol();
    let grpc_methods = router.list_grpc_methods();
    let has_grpc_methods = !grpc_methods.is_empty();
    
    // 调试信息：打印 ALPN 协商结果
    println!("🔍 [服务端] ALPN 协商结果: {:?}, gRPC 方法存在: {}", negotiated_protocol, has_grpc_methods);
    
    match negotiated_protocol {
        Some(protocol) if protocol == b"h2" => {
            debug!("🚀 [服务端] ALPN 协商: HTTP/2，直接路由到 HTTP/2 处理器: {}", remote_addr);
            handle_h2_tls_connection(tls_stream, remote_addr, router).await
        }
        Some(protocol) if protocol == b"http/1.1" => {
            // 如果有 gRPC 方法，HTTP/1.1 是不被接受的
            if has_grpc_methods {
                error!("❌ [服务端] gRPC 服务器需要 HTTP/2，但 ALPN 协商为 HTTP/1.1: {}", remote_addr);
                return Err("gRPC 服务器需要 HTTP/2，但客户端仅支持 HTTP/1.1".into());
            }
            info!("🌐 [服务端] ALPN 协商: HTTP/1.1，直接路由到 HTTP/1.1 处理器: {}", remote_addr);
            handle_http1_tls_connection(tls_stream, remote_addr, adapter).await
        }
        Some(protocol) => {
            let protocol_str = String::from_utf8_lossy(protocol);
            // 如果有 gRPC 方法，未知协议是不被接受的
            if has_grpc_methods {
                error!("❌ [服务端] gRPC 服务器需要 HTTP/2，但 ALPN 协商为未知协议 {}: {}", protocol_str, remote_addr);
                return Err(format!("gRPC 服务器需要 HTTP/2，但客户端协商了未知协议: {}", protocol_str).into());
            }
            warn!("⚠️  [服务端] 未知 ALPN 协议: {}，回退到 HTTP/1.1: {}", protocol_str, remote_addr);
            crate::utils::logger::warn!("未知 ALPN 协议: {}，回退到 HTTP/1.1", protocol_str);
            handle_http1_tls_connection(tls_stream, remote_addr, adapter).await
        }
        None => {
            // 如果有 gRPC 方法，无 ALPN 协商是不被接受的
            if has_grpc_methods {
                error!("❌ [服务端] gRPC 服务器需要 HTTP/2 ALPN 协商，但客户端未提供 ALPN: {}", remote_addr);
                return Err("gRPC 服务器需要 HTTP/2 ALPN 协商，但客户端未提供 ALPN".into());
            }
            warn!("⚠️  [服务端] 无 ALPN 协商，回退到 HTTP/1.1: {}", remote_addr);
            crate::utils::logger::warn!("TLS 连接无 ALPN 协商，回退到 HTTP/1.1");
            handle_http1_tls_connection(tls_stream, remote_addr, adapter).await
        }
    }
}

/// 处理 HTTP/2 over TLS 连接
async fn handle_h2_tls_connection(
    tls_stream: tokio_rustls::server::TlsStream<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use h2::server;
    
    debug!("🔍 [服务端] 开始处理 HTTP/2 over TLS 连接: {}", remote_addr);
    
    // 配置 HTTP/2 服务器，设置与客户端匹配的帧大小
    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024); // 设置最大帧大小为 1MB，与客户端保持一致
    
    // 创建 HTTP/2 服务器连接
    let mut connection = h2_builder.handshake(tls_stream).await
        .map_err(|e| {
            error!("❌ [服务端] HTTP/2 over TLS 握手失败: {}", e);
            format!("HTTP/2 over TLS 握手失败: {}", e)
        })?;
    
    info!("✅ [服务端] HTTP/2 over TLS 连接已建立: {}", remote_addr);
    crate::utils::logger::debug!("✅ HTTP/2 over TLS 连接已建立: {}", remote_addr);
    
    // 处理 HTTP/2 请求
    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                debug!("📥 [服务端] 接收到 HTTP/2 over TLS 请求: {} {}", 
                    request.method(), request.uri().path());
                
                let router_clone = router.clone();
                
                // 为每个请求启动处理任务
                tokio::spawn(async move {
                    if let Err(e) = handle_h2_request(request, respond, remote_addr, router_clone).await {
                        error!("❌ [服务端] 处理 HTTP/2 over TLS 请求失败: {}", e);
                        crate::utils::logger::error!("处理 HTTP/2 over TLS 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
            error!("❌ [服务端] 接受 HTTP/2 over TLS 请求失败: {}", e);
            crate::utils::logger::error!("接受 HTTP/2 over TLS 请求失败: {}", e);
            break;
        }
        }
    }
    
    Ok(())
}

/// 处理 HTTP/1.1 over TLS 连接
async fn handle_http1_tls_connection(
    tls_stream: tokio_rustls::server::TlsStream<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>,
    remote_addr: SocketAddr,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let io = TokioIo::new(tls_stream);
    let service = hyper::service::service_fn(move |req| {
        let adapter = adapter.clone();
        async move {
            adapter.handle_request(req, Some(remote_addr)).await
        }
    });
    
    if let Err(e) = Builder::new(hyper_util::rt::TokioExecutor::new())
        .serve_connection(io, service)
        .await
    {
        println!("❌ [服务端] HTTP/1.1 over TLS 连接处理失败: {}", e);
        return Err(format!("HTTP/1.1 over TLS 连接处理失败: {}", e).into());
    }
    
    Ok(())
}

/// 处理 HTTP/1.1 连接
async fn handle_http1_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let io = TokioIo::new(stream);
    let service = hyper::service::service_fn(move |req| {
        let adapter = adapter.clone();
        async move {
            adapter.handle_request(req, Some(remote_addr)).await
        }
    });
    
    if let Err(e) = Builder::new(hyper_util::rt::TokioExecutor::new())
        .serve_connection(io, service)
        .await
    {
        // 区分正常的客户端断开连接和真正的服务器错误
        let error_msg = e.to_string();
        if error_msg.contains("connection closed before message completed") ||
           error_msg.contains("broken pipe") ||
           error_msg.contains("connection reset by peer") ||
           error_msg.contains("unexpected end of file") {
            // 这些是正常的客户端断开连接，只记录调试信息
            debug!("🔌 [服务端] 客户端断开连接: {} ({})", remote_addr, error_msg);
        } else {
            // 真正的服务器错误，需要记录警告
            error!("❌ [服务端] HTTP/1.1 连接处理失败: {}", e);
            warn!("HTTP/1.1 连接处理失败: {} ({})", remote_addr, e);
            return Err(format!("HTTP/1.1 连接处理失败: {}", e).into());
        }
    }
    
    Ok(())
}

/// 处理带有预读数据的 HTTP/1.1 连接
async fn handle_http1_connection_with_stream<S>(
    stream: S,
    remote_addr: SocketAddr,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(stream);
    let service = hyper::service::service_fn(move |req| {
        let adapter = adapter.clone();
        async move {
            adapter.handle_request(req, Some(remote_addr)).await
        }
    });
    
    if let Err(e) = Builder::new(hyper_util::rt::TokioExecutor::new())
        .serve_connection(io, service)
        .await
    {
        // 区分正常的客户端断开连接和真正的服务器错误
        let error_msg = e.to_string();
        if error_msg.contains("connection closed before message completed") ||
           error_msg.contains("broken pipe") ||
           error_msg.contains("connection reset by peer") ||
           error_msg.contains("unexpected end of file") {
            // 这些是正常的客户端断开连接，只记录调试信息
            debug!("🔌 [服务端] 客户端断开连接: {} ({})", remote_addr, error_msg);
        } else {
            // 真正的服务器错误，需要记录警告
            error!("❌ [服务端] HTTP/1.1 连接处理失败: {}", e);
            warn!("HTTP/1.1 连接处理失败: {} ({})", remote_addr, e);
            return Err(format!("HTTP/1.1 连接处理失败: {}", e).into());
        }
    }
    
    Ok(())
}

/// 处理 H2C（HTTP/2 over cleartext）连接
async fn handle_h2c_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    handle_h2c_connection_with_stream(stream, remote_addr, router).await
}

/// 处理带有预读数据的 H2C 连接
async fn handle_h2c_connection_with_stream<S>(
    stream: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use h2::server;
    
    debug!("🔍 [服务端] 开始处理 H2C 连接（带预读数据）: {}", remote_addr);
    
    // 配置 HTTP/2 服务器，设置与客户端匹配的帧大小
    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024); // 设置最大帧大小为 1MB，与客户端保持一致
    
    // 创建 HTTP/2 服务器连接
    let mut connection = h2_builder.handshake(stream).await
        .map_err(|e| {
            error!("❌ [服务端] HTTP/2 握手失败: {}", e);
            format!("HTTP/2 握手失败: {}", e)
        })?;
    
    info!("✅ [服务端] HTTP/2 连接已建立: {}", remote_addr);
    crate::utils::logger::debug!("✅ HTTP/2 连接已建立: {}", remote_addr);
    
    // 处理 HTTP/2 请求
    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                debug!("📥 [服务端] 接收到 HTTP/2 请求: {} {}", 
                    request.method(), request.uri().path());
                
                let router_clone = router.clone();
                
                // 为每个请求启动处理任务
                tokio::spawn(async move {
                    if let Err(e) = handle_h2_request(request, respond, remote_addr, router_clone).await {
                        error!("❌ [服务端] 处理 HTTP/2 请求失败: {}", e);
                        crate::utils::logger::error!("处理 HTTP/2 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
            error!("❌ [服务端] 接受 HTTP/2 请求失败: {}", e);
            crate::utils::logger::error!("接受 HTTP/2 请求失败: {}", e);
            break;
        }
        }
    }
    
    Ok(())
}

/// 处理单个 HTTP/2 请求
async fn handle_h2_request(
    request: hyper::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<bytes::Bytes>,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("🔍 [服务端] 开始处理 HTTP/2 请求: {} {} from {}", 
        request.method(), request.uri().path(), remote_addr);
    
    // 打印请求头信息
    debug!("📋 [服务端] 请求头:");
    for (name, value) in request.headers() {
        if let Ok(value_str) = value.to_str() {
            debug!("   {}: {}", name, value_str);
        }
    }
    
    // 检查是否为 gRPC 请求
    let is_grpc = request.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.starts_with("application/grpc"))
        .unwrap_or(false);
    
    debug!("🔍 [服务端] 请求类型判断: is_grpc = {}", is_grpc);
    
    if is_grpc {
        debug!("🔧 [服务端] 处理 gRPC 请求: {} {}", 
            request.method(), request.uri().path());
        crate::utils::logger::debug!("🔧 处理 gRPC 请求: {} {}", 
            request.method(), request.uri().path());
        
        // 将 remote_addr 添加到 request 的 extensions 中
        let (mut parts, body) = request.into_parts();
        parts.extensions.insert(remote_addr);
        let request_with_addr = hyper::Request::from_parts(parts, body);
        
        debug!("🔍 [服务端] 已将 remote_addr {} 添加到 gRPC 请求扩展中", remote_addr);
        
        // 处理 gRPC 请求
        router.handle_grpc_request(request_with_addr, respond).await
            .map_err(|e| {
                println!("❌ [服务端] gRPC 请求处理失败: {}", e);
                format!("gRPC 请求处理失败: {}", e)
            })?;
    } else {
        info!("📡 [服务端] 处理普通 HTTP/2 请求: {} {}", 
            request.method(), request.uri().path());
        crate::utils::logger::debug!("📡 处理 HTTP/2 请求: {} {}", 
            request.method(), request.uri().path());
        
        // 读取 RecvStream 数据
        let (parts, mut recv_stream) = request.into_parts();
        let mut body_data = Vec::new();
        
        while let Some(chunk) = recv_stream.data().await {
            let chunk = chunk.map_err(|e| format!("读取 HTTP/2 请求体失败: {}", e))?;
            body_data.extend_from_slice(&chunk);
            recv_stream.flow_control().release_capacity(chunk.len())
                .map_err(|e| format!("HTTP/2 流量控制失败: {}", e))?;
        }
        
        // 使用通用的 HttpRequest 结构体
        let http_request = crate::server::http_request::HttpRequest::from_h2_request(
            parts.method,
            parts.uri,
            parts.headers,
            bytes::Bytes::from(body_data),
            Some(remote_addr),
        );
        
        debug!("🔄 [HTTP/2] 已转换为通用 HttpRequest，调用 Router::handle_http");
        
        // 调用 Router 的通用 handle_http 方法
        match router.handle_http(http_request).await {
            Ok(response) => {
                debug!("✅ [HTTP/2] Router 处理成功");
                
                // 将 BoxBody 响应转换为 H2 响应
                let (parts, mut body) = response.into_parts();
                
                // 构建 H2 响应头
                let mut h2_response = hyper::Response::builder()
                    .status(parts.status);
                
                // 复制响应头
                for (name, value) in parts.headers {
                    if let Some(name) = name {
                        h2_response = h2_response.header(name, value);
                    }
                }
                
                let h2_response = h2_response.body(()).unwrap();
                
                // 发送响应头
                match respond.send_response(h2_response, false) {
                    Ok(mut send_stream) => {
                        // 读取并发送响应体
                        use http_body_util::BodyExt;
                        
                        let mut body_stream = std::pin::Pin::new(&mut body);
                        while let Some(frame_result) = body_stream.frame().await {
                            match frame_result {
                                Ok(frame) => {
                                    if let Some(data) = frame.data_ref() {
                                        if let Err(e) = send_stream.send_data(data.clone(), false) {
                                            if e.to_string().contains("inactive stream") {
                                                crate::utils::logger::debug!("ℹ️ [服务端] 流已关闭，HTTP/2 响应发送被忽略");
                                                break;
                                            } else {
                                                crate::utils::logger::error!("发送 HTTP/2 响应数据失败: {}", e);
                                                break;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    crate::utils::logger::error!("读取响应体帧失败: {}", e);
                                    break;
                                }
                            }
                        }
                        
                        // 发送结束标志
                        if let Err(e) = send_stream.send_data(bytes::Bytes::new(), true) {
                            if !e.to_string().contains("inactive stream") {
                                crate::utils::logger::error!("发送 HTTP/2 响应结束标志失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        crate::utils::logger::error!("发送 HTTP/2 响应头失败: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("❌ [HTTP/2] Router 处理失败: {}", e);
                crate::utils::logger::error!("Router 处理 HTTP/2 请求失败: {}", e);
                
                // 发送错误响应
                let error_response = hyper::Response::builder()
                    .status(500)
                    .header("content-type", "application/json")
                    .header("server", format!("RAT-Engine/{}", env!("CARGO_PKG_VERSION")))
                    .body(())
                    .unwrap();
                
                match respond.send_response(error_response, false) {
                    Ok(mut send_stream) => {
                        let error_body = format!(r#"{{"error":"Internal server error","message":"{}"}}"#, e);
                        let body_bytes = bytes::Bytes::from(error_body);
                        if let Err(e) = send_stream.send_data(body_bytes, true) {
                            if !e.to_string().contains("inactive stream") {
                                crate::utils::logger::error!("发送 HTTP/2 错误响应失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        crate::utils::logger::error!("发送 HTTP/2 错误响应头失败: {}", e);
                    }
                }
            }
        }
    }
    
    Ok(())
}

// 严禁创建空路由器启动服务器！！！