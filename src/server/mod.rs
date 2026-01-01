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
use tokio::net::TcpListener;
use tokio::signal;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use h2::server::SendResponse;
use h2::RecvStream;
use bytes;
use http_body_util;
// 使用简化的协议枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolType {
    HTTP1_0,
    HTTP1_1,
    HTTP2,
    HTTP3,
    GRPC,
    TLS,
    Unknown,
    // 以下变体用于兼容性，但不再使用
    WebSocket,
    SSH,
    TCP,
    QUIC,
    MQTT,
    UDP,
    FTP,
    SMTP,
    DNS,
    Redis,
    MySQL,
    Custom,
}

use crate::utils::logger::{debug, info, warn, error};

use hyper_util::rt::TokioIo;
use tokio_rustls::server::TlsStream;

// ============ 已移除 H2C 支持 ============
// gRPC 服务端强制使用 TLS (HTTP/2-only)
// 不再支持 H2C (HTTP/2 over cleartext)

/// 重新构造的流，包含预读的数据
pub struct ReconstructedStream {
    inner: tokio::net::TcpStream,
    prefix: Vec<u8>,
    prefix_pos: usize,
}

impl ReconstructedStream {
    pub fn new(stream: tokio::net::TcpStream, prefix: &[u8]) -> Self {
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
pub mod cors;
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
pub mod protocol_detector;
pub mod grpc_types;
pub mod grpc_codec;
pub mod cert_manager;
pub mod grpc_handler;
pub mod grpc_queue_bridge_adapter;
pub mod grpc_delegated_handler;
pub mod http_request;
pub mod global_sse_manager;
pub mod proxy_protocol;

// 物理分离：HTTP 和 gRPC 独立服务器
pub mod http_server;
pub mod grpc_server;
pub mod grpc_h2c_server;
pub mod multi_protocol_adapter;

// 重新导出分离模块的函数
pub use http_server::handle_http_dedicated_connection;
pub use http_server::handle_tls_connection;
pub use http_server::handle_h2_tls_connection;
pub use grpc_server::handle_grpc_tls_connection;
pub use grpc_h2c_server::handle_grpc_h2c_over_tls_connection;

pub use config::ServerConfig;
pub use port_config::{PortConfig, PortConfigBuilder, PortMode, PortConfigError, HttpsConfig, CertificateConfig};
pub use router::Router;
pub use performance::{PerformanceManager, global_performance_manager, init_performance_optimization, set_thread_affinity, optimize_for_throughput};
pub use worker_pool::WorkerPool;
pub use hyper_adapter::HyperAdapter;
pub use streaming::{StreamingResponse, SseResponse, ChunkedResponse};


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
pub async fn run_separated_server(
    config: ServerConfig,
    router: Arc<Router>,
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> crate::error::RatResult<()> {
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

        // 注意：rustls 的 ALPN 在创建 ServerConfig 时已经设置（只支持 h2）
        // 不需要在这里配置 ALPN
    }

    // H2C 已移除，gRPC 强制使用 TLS
    // if router.is_h2c_enabled() {
    //     protocols.push("H2C");
    // }

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
        let cert_mgr = cert_manager.clone();
        async move {
            loop {
                let (stream, remote_addr) = http_listener.accept().await
                    .map_err(|e| crate::error::RatError::IoError(e))?;

                let router_clone = router.clone();
                let adapter_clone = adapter.clone();
                let cert_mgr_clone = cert_mgr.clone();

                tokio::task::spawn(async move {
                    if let Err(err) = handle_http_connection_with_cert(stream, remote_addr, router_clone, adapter_clone, cert_mgr_clone).await {
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
        let cert_mgr = cert_manager.clone();
        async move {
            loop {
                let (stream, remote_addr) = grpc_listener.accept().await
                    .map_err(|e| crate::error::RatError::IoError(e))?;

                let router_clone = router.clone();
                let adapter_clone = adapter.clone();
                let cert_mgr_clone = cert_mgr.clone();

                tokio::task::spawn(async move {
                    if let Err(err) = handle_grpc_connection_with_cert(stream, remote_addr, router_clone, adapter_clone, cert_mgr_clone).await {
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

/// 处理 HTTP 连接（分端口模式，带证书管理器）
async fn handle_http_connection_with_cert(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::utils::logger::debug!("🔗 [HTTP] 新连接: {}", remote_addr);

    // 在分端口模式下，传递证书管理器
    detect_and_handle_protocol_with_tls(stream, remote_addr, router, adapter, cert_manager).await
}

/// 处理 HTTP 连接（分端口模式，无证书管理器 - 兼容旧代码）
async fn handle_http_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    handle_http_connection_with_cert(stream, remote_addr, router, adapter, None).await
}

/// 处理 gRPC 连接（分端口模式，带证书管理器）
async fn handle_grpc_connection_with_cert(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::utils::logger::debug!("🔗 [gRPC] 新连接: {}", remote_addr);

    let cert_mgr = cert_manager.unwrap_or_else(|| {
        panic!("gRPC 服务必须配置 TLS 证书！请在启动前配置证书。");
    });

    debug!("🔐 [gRPC] 使用 TLS 处理连接: {}", remote_addr);
    crate::server::grpc_server::handle_grpc_tls_connection(stream, remote_addr, router, cert_mgr).await
}

/// 处理 gRPC 连接（分端口模式，无证书管理器 - 兼容旧代码）
async fn handle_grpc_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    handle_grpc_connection_with_cert(stream, remote_addr, router.clone(), adapter, router.get_cert_manager()).await
}


/// 处理单个连接，支持 HTTP/1.1、HTTP/2 和 gRPC
async fn handle_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("🔗 [服务端] 新连接: {}", remote_addr);

    // 始终进行协议检测，以支持 TLS、HTTP/2 等协议
    debug!("🔍 [服务端] 开始协议检测: {}", remote_addr);
    
    // 尝试检测协议类型并路由到相应的处理器
    match detect_and_handle_protocol(stream, remote_addr, router.clone(), adapter.clone()).await {
        Ok(_) => return Ok(()),
        Err(e) => {
            rat_logger::warn!("❌ [服务端] 协议检测失败: {}", e);
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
    // 从 router 获取证书管理器并传递给 TLS 版本
    let tls_cert_manager = router.get_cert_manager();
    detect_and_handle_protocol_with_tls(stream, remote_addr, router, adapter, tls_cert_manager).await
}

pub async fn detect_and_handle_protocol_with_tls(
    mut stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    tls_cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // 读取连接的前几个字节来检测协议
    let mut buffer = [0u8; 1024];
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
    
    // 首先检查是否是 PROXY protocol v2
    let mut detection_data = &buffer[..bytes_read];
    let mut actual_remote_addr = remote_addr;
    let mut proxy_header_len = 0;

    if crate::server::proxy_protocol::ProxyProtocolV2Parser::is_proxy_v2(detection_data) {
        println!("📡 [服务端] 检测到 PROXY protocol v2: {}", remote_addr);
        println!("🔍 [服务端] 原始代理地址: {}", remote_addr);
        println!("🔍 [服务端] PROXY头部数据: {:?}", &detection_data[..detection_data.len().min(50)]);

        // 解析 PROXY protocol v2
        if let Ok(proxy_info) = crate::server::proxy_protocol::ProxyProtocolV2Parser::parse(detection_data) {
            println!("✅ [服务端] PROXY protocol v2 解析成功");
            println!("🔍 [服务端] 命令类型: {:?}", proxy_info.command);
            println!("🔍 [服务端] 地址族: {:?}", proxy_info.address_family);
            println!("🔍 [服务端] 传输协议: {:?}", proxy_info.protocol);

            // 提取原始客户端地址
            if let Some(client_ip) = proxy_info.client_ip() {
                println!("📍 [服务端] PROXY protocol v2 - 原始客户端IP: {}", client_ip);

                // 如果有端口信息，尝试解析完整地址
                if let Some(client_port) = proxy_info.client_port() {
                    println!("📍 [服务端] PROXY protocol v2 - 原始客户端端口: {}", client_port);
                    if let Ok(parsed_addr) = format!("{}:{}", client_ip, client_port).parse::<SocketAddr>() {
                        actual_remote_addr = parsed_addr;
                        println!("✅ [服务端] 更新远程地址为原始客户端地址: {} (原来是: {})",
                            actual_remote_addr, remote_addr);
                    } else {
                        println!("⚠️ [服务端] 无法解析客户端地址: {}:{}", client_ip, client_port);
                    }
                } else {
                    println!("ℹ️ [服务端] PROXY protocol v2 - 只有客户端IP，无端口信息: {}", client_ip);
                }
            } else {
                println!("⚠️ [服务端] PROXY protocol v2 中没有客户端地址信息");
            }

            // 检查ALPN协议
            if let Some(ref alpn) = proxy_info.alpn {
                println!("🔐 [服务端] PROXY ALPN: {}", alpn);
                if alpn.to_lowercase() == "h2" {
                    println!("🚀 [服务端] ALPN指示为HTTP/2");
                }
            } else {
                println!("ℹ️ [服务端] PROXY protocol v2 中没有ALPN信息");
            }

            // 显示TLV信息
            if !proxy_info.tlvs.is_empty() {
                println!("🔍 [服务端] PROXY TLV数量: {}", proxy_info.tlvs.len());
                for (i, tlv) in proxy_info.tlvs.iter().enumerate() {
                    println!("🔍 [服务端] TLV[{}]: Type=0x{:02x}, Length={}", i, tlv.tpe, tlv.value.len());
                }
            }
        } else {
            println!("❌ [服务端] PROXY protocol v2 解析失败");
        }

        // 计算并跳过PROXY头部
        proxy_header_len = 16 + u16::from_be_bytes([detection_data[14], detection_data[15]]) as usize;
        detection_data = &detection_data[proxy_header_len..];

        println!("🔄 [服务端] 跳过 PROXY protocol v2 头部 ({} 字节)，剩余应用数据: {} 字节",
            proxy_header_len, detection_data.len());
        println!("🔍 [服务端] 跳过后应用数据预览: {:?}", &detection_data[..detection_data.len().min(50)]);
    } else {
        println!("ℹ️ [服务端] 未检测到 PROXY protocol v2，使用普通协议检测");
    }

    // ============ 简化协议检测逻辑 ============
    // 根据 Router 模式决定如何处理请求
    // 如果检测到 PROXY protocol v2，已在上面的代码中提取真实客户端 IP
    // 并跳过 PPv2 头部，detection_data 现在指向应用层数据

    // 打印调试信息（安全地处理二进制数据）
    let data_str = String::from_utf8_lossy(detection_data);
    let safe_preview: String = data_str.chars().take(100).collect();
    println!("🔍 [服务端] 协议检测数据 (前100字符): {:?}", safe_preview);

    // 情况1: HTTP 专用模式 - 支持 HTTP 和 HTTPS（自动升级到 TLS）
    if router.is_http_only() {
        // 检测是否为 TLS 连接
        let is_tls = detection_data.len() > 0 && detection_data[0] == 0x16;

        if is_tls && tls_cert_manager.is_some() {
            // HTTP 专用模式 + TLS 连接 + 有证书 → 使用 HTTPS
            println!("✅ [服务端] HTTP 专用模式，检测到 TLS 连接，使用 HTTPS");
            route_by_detected_protocol(stream, detection_data, ProtocolType::TLS, actual_remote_addr, router, adapter, tls_cert_manager.clone()).await;
            return Ok(());
        } else {
            // HTTP 专用模式 + 明文连接 → 使用 HTTP
            println!("✅ [服务端] HTTP 专用模式，使用 HTTP 处理器");
            route_by_detected_protocol(stream, detection_data, ProtocolType::HTTP1_1, actual_remote_addr, router, adapter, tls_cert_manager.clone()).await;
            return Ok(());
        }
    }

    // 情况2: gRPC 专用模式 - 直接走 gRPC 处理（需要 TLS）
    if router.is_grpc_only() {
        let cert_manager = tls_cert_manager
            .as_ref()
            .unwrap_or_else(|| {
                panic!("gRPC 专用模式必须配置 TLS 证书！请在启动前配置证书。");
            });
        println!("✅ [服务端] gRPC 专用模式，使用 TLS 处理连接");
        route_by_detected_protocol(stream, detection_data, ProtocolType::HTTP2, actual_remote_addr, router, adapter, Some(cert_manager.clone())).await;
        return Ok(());
    }

    // 情况3: 混合模式 - 检测是 gRPC 还是 HTTP
    // 单端口混合模式：检查是否为 gRPC，不是则默认为 HTTP
    println!("🔍 [服务端] 混合模式 - 检测请求类型");

    // 检查是否为 TLS 连接
    let is_tls = detection_data.len() > 0 && detection_data[0] == 0x16;

    // 检查是否为 gRPC 请求
    let is_grpc = crate::server::protocol_detector::is_grpc_request(detection_data);

    if is_tls {
        // TLS 连接 - 先进行 TLS 握手，然后根据内容路由
        println!("✅ [服务端] 检测到 TLS 连接，进行 TLS 握手");
        route_by_detected_protocol(stream, detection_data, ProtocolType::TLS, actual_remote_addr, router, adapter, tls_cert_manager.clone()).await;
        return Ok(());
    } else if is_grpc {
        // gRPC 请求（需要 TLS 证书）
        if tls_cert_manager.is_some() {
            println!("✅ [服务端] 检测到 gRPC 请求，使用 TLS 处理");
            route_by_detected_protocol(stream, detection_data, ProtocolType::HTTP2, actual_remote_addr, router, adapter, tls_cert_manager.clone()).await;
            return Ok(());
        } else {
            println!("❌ [服务端] gRPC 请求需要 TLS 证书，但未配置");
            return Err("gRPC 请求需要 TLS 证书".into());
        }
    } else {
        // 默认为 HTTP 请求
        println!("✅ [服务端] 默认路由到 HTTP 处理器");
        route_by_detected_protocol(stream, detection_data, ProtocolType::HTTP1_1, actual_remote_addr, router, adapter, tls_cert_manager.clone()).await;
        return Ok(());
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
            rat_logger::debug!("🌐 [服务端] 路由到 HTTP/1.1 处理器: {}", remote_addr);
            let reconstructed_stream = ReconstructedStream::new(stream, buffer);
            handle_http1_connection_with_stream(reconstructed_stream, remote_addr, adapter).await
        }
        ProtocolType::TLS => {
            info!("🔐 [服务端] 检测到 TLS 连接，进行 TLS 握手: {}", remote_addr);
            println!("🔍 [DEBUG] TLS 分支: tls_cert_manager.is_some()={}", tls_cert_manager.is_some());

            // 判断使用哪种处理器
            if router.is_grpc_only() {
                // gRPC 专用模式 - 使用 grpc_server
                info!("🔧 [服务端] gRPC 专用模式，路由到 gRPC 处理器: {}", remote_addr);
                let cert_manager = tls_cert_manager
                    .unwrap_or_else(|| panic!("gRPC 专用模式必须配置 TLS 证书"));
                let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                crate::server::grpc_server::handle_grpc_tls_connection(reconstructed_stream, remote_addr, router, cert_manager).await
            } else if router.is_http_only() {
                // HTTP 专用模式 - 使用 http_server
                info!("🌐 [服务端] HTTP 专用模式，路由到 HTTP 处理器: {}", remote_addr);
                let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                handle_tls_connection(reconstructed_stream, remote_addr, router, adapter, tls_cert_manager.clone()).await
            } else {
                // 单端口多协议模式 - 转发到 HTTP 服务器处理
                // http_server 使用 hyper auto builder，支持 HTTP/1.1 和 HTTP/2
                // 能正确处理 SSE 流式响应
                info!("🌐 [服务端] 单端口多协议模式，路由到 HTTP 处理器: {}", remote_addr);
                let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                handle_tls_connection(reconstructed_stream, remote_addr, router, adapter, tls_cert_manager.clone()).await
            }
        }
        ProtocolType::HTTP2 => {
            // 处理 HTTP/2 请求
            // 检查是否是 TLS 连接上的 HTTP/2
            println!("🔍 [DEBUG] HTTP2 分支: buffer.len()={}, buffer[0]={:02x}", buffer.len(), if !buffer.is_empty() { buffer[0] } else { 0 });
            if !buffer.is_empty() && buffer[0] == 0x16 {
                // TLS 上的 HTTP/2，根据模式选择处理器
                if router.is_grpc_only() {
                    // gRPC 专用模式 - 使用 grpc_server
                    info!("🔧 [服务端] gRPC 专用模式，路由到 gRPC 处理器: {}", remote_addr);
                    let cert_manager = tls_cert_manager
                        .unwrap_or_else(|| panic!("gRPC 专用模式必须配置 TLS 证书"));
                    let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                    crate::server::grpc_server::handle_grpc_tls_connection(reconstructed_stream, remote_addr, router, cert_manager).await
                } else {
                    // HTTP 模式或混合模式 - 使用 http_server
                    info!("🌐 [服务端] HTTP 模式，路由到 HTTP 处理器: {}", remote_addr);
                    let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                    crate::server::http_server::handle_tls_connection(reconstructed_stream, remote_addr, router, adapter, tls_cert_manager.clone()).await
                }
            } else {
                // 拒绝 cleartext HTTP/2 (H2C)，强制要求 TLS
                warn!("🚫 [服务端] 拒绝 cleartext HTTP/2 (H2C) 连接，必须使用 TLS: {}", remote_addr);
                Err("HTTP/2 over cleartext (H2C) 不再支持，请使用 TLS".into())
            }
        }
        ProtocolType::GRPC => {
            // 处理 gRPC 请求
            info!("🚀 [服务端] 路由到 gRPC 处理器: {}", remote_addr);

            // 检查数据格式以决定使用哪种处理器
            let data_str = String::from_utf8_lossy(buffer);

            if !buffer.is_empty() && buffer[0] == 0x16 {
                // TLS 上的 gRPC
                info!("🔐 [服务端] 检测到 TLS 上的 gRPC，进行 TLS 握手: {}", remote_addr);
                let reconstructed_stream = ReconstructedStream::new(stream, buffer);
                handle_tls_connection(reconstructed_stream, remote_addr, router, adapter, tls_cert_manager.clone()).await
            } else if data_str.contains("HTTP/1.") {
                // 普通的 HTTP/1.x 请求 - 使用 HTTP 处理器
                info!("🚀 [服务端] 检测到普通 HTTP/1.x 请求，使用 HTTP 处理器");

                // 创建 ReconstructedStream
                let reconstructed_stream = ReconstructedStream::new(stream, buffer);

                // 直接调用 HTTP 处理器
                handle_http1_connection_with_stream(reconstructed_stream, remote_addr, adapter).await
            } else {
                // gRPC 必须使用 TLS，拒绝 cleartext 连接
                warn!("🚫 [服务端] 拒绝 cleartext gRPC 连接，gRPC 必须使用 TLS: {}", remote_addr);
                Err("gRPC 必须使用 TLS，不再支持 H2C".into())
            }
        }
        ProtocolType::WebSocket => {
            warn!("🚫 [服务端] WebSocket 协议不支持，拒绝连接: {}", remote_addr);
            Err("WebSocket 协议不支持".into())
        }
        ProtocolType::Unknown => {
            rat_logger::debug!("🤔 [服务端] 未知协议类型，尝试按HTTP/1.1处理: {} (协议: {:?})", remote_addr, protocol_type);
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

// 严禁创建空路由器启动服务器！！！

// HTTP/2 请求处理模块
mod h2_request_handler;
pub use h2_request_handler::handle_h2_request;

// HTTP 连接处理模块（委托到分离的 http_server）
mod http_connection;
pub use http_connection::{
    handle_http1_connection,
    handle_http1_connection_with_stream,
};

// gRPC 连接处理模块（委托到分离的 grpc_server）
mod grpc_connection;
