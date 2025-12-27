//! HTTP 连接处理模块
//!
//! 专门处理 HTTP/1.1 和 HTTP/2 连接

use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsStream;
use h2::server;
use hyper::{Request, Response};
use hyper::body::Incoming;
use hyper::body::Bytes;
use http_body_util::{Full, combinators::BoxBody};
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto::Builder as AutoBuilder;
use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::StreamExt;
use bytes;
use crate::utils::logger::{debug, info, warn, error};
use tokio::io::AsyncReadExt;

pub async fn handle_tls_connection<S>(
    stream: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    cert_manager: Option<Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    println!("🔐 [服务端] handle_tls_connection 开始: {}", remote_addr);
    info!("🔐 [服务端] 开始 TLS 握手: {}", remote_addr);

    // 获取证书管理器
    let cert_manager = match cert_manager {
        Some(m) => {
            println!("✅ [服务端] 证书管理器存在");
            m
        }
        None => {
            println!("❌ [服务端] 证书管理器不存在");
            return Err("TLS 连接需要证书管理器，但未配置".into());
        }
    };

    // 获取 HTTP 的 ServerConfig（如果没有证书，返回 None，允许降级到 HTTP/1.1）
    println!("🔍 [服务端] 尝试获取 ServerConfig...");
    let server_config = {
        println!("🔍 [服务端] 尝试获取读锁...");
        let cert_manager_guard = cert_manager.read()
            .map_err(|e| format!("无法获取证书管理器读锁: {}", e))?;
        println!("✅ [服务端] 读锁获取成功");
        let config = cert_manager_guard.get_http_server_config();
        println!("🔍 [服务端] ServerConfig 结果: {:?}", config.is_some());
        config
    };

    debug!("🔍 [服务端] ServerConfig 获取结果: {:?}", server_config.is_some());

    if let Some(server_config) = server_config {
        // 有证书，使用 TLS
        println!("✅ [服务端] ServerConfig 存在，开始 TLS accept...");
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);

        debug!("🔍 [服务端] 开始 TLS accept...");
        // 将泛型 stream 转换为 TcpStream（这里需要一些技巧）
        // 简化处理：假设 S 是 TcpStream
        println!("🔍 [服务端] 调用 acceptor.accept()...");
        let mut tls_stream = acceptor.accept(stream).await
            .map_err(|e| {
                error!("❌ [服务端] TLS 握手失败: {}", e);
                format!("TLS 握手失败: {}", e)
            })?;
        println!("✅ [服务端] TLS accept 成功!");

        info!("✅ [服务端] TLS 握手成功: {}", remote_addr);

        // 获取 ALPN 协议
        println!("🔍 [服务端] 获取 ALPN 协议...");
        let (_tcp_stream, conn) = tls_stream.get_ref();
        let alpn_protocol = conn.alpn_protocol().map(|p| p.to_vec());
        println!("🔐 [服务端] ALPN 协议: {:?}", alpn_protocol);
        info!("🔐 [服务端] ALPN 协议: {:?}", alpn_protocol);

        // TLS 模式强制要求 HTTP/2
        let is_http2 = alpn_protocol.as_ref().map(|p| p == b"h2").unwrap_or(false);
        println!("🔍 [服务端] is_http2 = {}", is_http2);

        if !is_http2 {
            // 拒绝非 HTTP/2 的 TLS 连接
            warn!("🚫 [服务端] 拒绝非 HTTP/2 的 TLS 连接: ALPN={:?}, 客户端={}", alpn_protocol, remote_addr);
            warn!("🚫 [服务端] TLS 模式强制要求 HTTP/2，请使用支持 HTTP/2 的客户端");

            // 尝试发送 HTTP 426 响应（方便调试）
            let response = b"HTTP/1.1 426 Upgrade Required\r\n\
                Upgrade: HTTP/2.0\r\n\
                Connection: Upgrade\r\n\
                Content-Type: text/plain; charset=utf-8\r\n\
                Content-Length: 76\r\n\
                \r\n\
                426 Upgrade Required: TLS mode requires HTTP/2 protocol\r\n\
                Please use a client that supports HTTP/2 over TLS.\r\n";

            use tokio::io::AsyncWriteExt;
            let _ = tls_stream.write_all(response).await;
            let _ = tls_stream.flush().await;
            let _ = tls_stream.shutdown().await;

            return Err("TLS 模式只支持 HTTP/2，已发送 426 响应".into());
        }

        // HTTP/2 连接
        println!("🚀 [服务端] HTTP/2 连接: {}", remote_addr);
        info!("🚀 [服务端] HTTP/2 连接: {}", remote_addr);

        // 使用 hyper auto builder 处理 HTTP/2，通过 HyperAdapter 使用服务端连接池
        println!("🔍 [服务端] 使用 hyper auto builder 处理 HTTP/2...");
        use hyper_util::server::conn::auto::Builder as AutoBuilder;

        let io = TokioIo::new(tls_stream);
        let service = hyper::service::service_fn(move |req| {
            let adapter = adapter.clone();
            async move {
                adapter.handle_request(req, Some(remote_addr)).await
            }
        });

        if let Err(e) = AutoBuilder::new(hyper_util::rt::TokioExecutor::new())
            .http2()
            .enable_connect_protocol()
            .serve_connection_with_upgrades(io, service)
            .await
        {
            // 区分正常的客户端断开连接和真正的服务器错误
            let error_msg = e.to_string();
            if error_msg.contains("connection closed") ||
               error_msg.contains("broken pipe") ||
               error_msg.contains("connection reset") ||
               error_msg.contains("unexpected end of file") ||
               error_msg.contains("CANCELED") {
                // 正常的客户端断开，只记录调试信息
                debug!("🔌 [服务端] 客户端断开 TLS 连接: {} ({})", remote_addr, error_msg);
            } else {
                // 真正的服务器错误
                error!("❌ [服务端] HTTP/2 over TLS 连接处理失败: {}", e);
                return Err(format!("HTTP/2 over TLS 连接处理失败: {}", e).into());
            }
        }

        Ok(())
    } else {
        // 没有证书，返回错误（调用者应该降级到 HTTP/1.1）
        Err("HTTP 未配置证书，请降级到 HTTP/1.1".into())
    }
}


/// HTTP/2 over TLS 连接处理（使用本模块的 h2_request_handler）
pub async fn handle_h2_tls_connection(
    tls_stream: TlsStream<tokio::net::TcpStream>,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use h2::server;

    debug!("🔍 [HTTP专用] 开始处理 HTTP/2 over TLS 连接: {}", remote_addr);

    // 配置 HTTP/2 服务器
    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024);

    // 创建 HTTP/2 服务器连接
    let mut connection = h2_builder.handshake(tls_stream).await
        .map_err(|e| {
            error!("❌ [HTTP专用] HTTP/2 握手失败: {}", e);
            format!("HTTP/2 握手失败: {}", e)
        })?;

    info!("✅ [HTTP专用] HTTP/2 连接已建立: {}", remote_addr);

    // 处理 HTTP 请求
    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                debug!("📥 [HTTP专用] 接收到 HTTP 请求: {} {}",
                    request.method(), request.uri().path());

                let router_clone = router.clone();

                tokio::spawn(async move {
                    if let Err(e) = super::h2_request_handler::handle_h2_request(request, respond, remote_addr, router_clone).await {
                        error!("❌ [HTTP专用] 处理 HTTP 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("❌ [HTTP专用] 接受请求失败: {}", e);
                break;
            }
        }
    }

    debug!("🔌 [HTTP专用] 连接关闭: {}", remote_addr);
    Ok(())
}

/// 处理 HTTP/1.1 连接

pub async fn handle_http1_connection(
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

    if let Err(e) = AutoBuilder::new(hyper_util::rt::TokioExecutor::new())
        .http2()
        .enable_connect_protocol()
        .serve_connection_with_upgrades(io, service)
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

pub async fn handle_http1_connection_with_stream<S>(
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

    if let Err(e) = AutoBuilder::new(hyper_util::rt::TokioExecutor::new())
        .http2()
        .enable_connect_protocol()
        .serve_connection_with_upgrades(io, service)
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

// ============ 已移除 H2C 支持 ============
// handle_h2c_connection 和 handle_h2c_connection_with_stream 已移除
// gRPC 服务端强制使用 TLS (HTTP/2-only)

/// 处理 HTTP 专用端口的连接（简化版，跳过 gRPC 检测）
///
/// 此函数专为 HTTP 专用端口设计，会：
/// 1. 检测 PROXY protocol（如果有）
/// 2. 检测并处理 TLS（如果配置了证书）
/// 3. 使用 hyper auto builder 处理 HTTP/1.1 和 HTTP/2
/// 4. 跳过 gRPC 检测，提高性能
pub async fn handle_http_dedicated_connection(
    mut stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncWriteExt;

    info!("🔗 [HTTP专用] 新连接: {}", remote_addr);

    // 读取连接的前几个字节来检测 PROXY protocol 和 TLS
    let mut buffer = [0u8; 1024];
    let mut total_read = 0;

    // 尝试读取数据
    let read_result = tokio::time::timeout(
        std::time::Duration::from_millis(1000),
        async {
            while total_read < buffer.len() {
                match stream.read(&mut buffer[total_read..]).await {
                    Ok(0) => break,
                    Ok(n) => total_read += n,
                    Err(e) => return Err(e),
                }

                if total_read >= 64 {
                    break;
                }
            }
            Ok(total_read)
        }
    ).await;

    let bytes_read = match read_result {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            debug!("🚫 [HTTP专用] 读取数据失败: {} (错误: {})", remote_addr, e);
            return Err(format!("读取数据失败: {}", e).into());
        }
        Err(_) => {
            debug!("🚫 [HTTP专用] 读取超时: {}", remote_addr);
            return Err("读取超时".into());
        }
    };

    if bytes_read == 0 {
        debug!("🔌 [HTTP专用] 连接立即关闭: {}", remote_addr);
        return Ok(());
    }

    // 检查 PROXY protocol
    let mut detection_data = &buffer[..bytes_read];
    let mut actual_remote_addr = remote_addr;
    let mut proxy_header_len = 0;

    if crate::server::proxy_protocol::ProxyProtocolV2Parser::is_proxy_v2(detection_data) {
        info!("📡 [HTTP专用] 检测到 PROXY protocol v2: {}", remote_addr);

        // 计算并跳过 PROXY 头部
        proxy_header_len = 16 + u16::from_be_bytes([detection_data[14], detection_data[15]]) as usize;

        if let Ok(proxy_info) = crate::server::proxy_protocol::ProxyProtocolV2Parser::parse(detection_data) {
            if let Some(client_ip) = proxy_info.client_ip() {
                if let Some(client_port) = proxy_info.client_port() {
                    actual_remote_addr = format!("{}:{}", client_ip, client_port).parse()?;
                    info!("📍 [HTTP专用] PROXY protocol - 原始客户端地址: {}", actual_remote_addr);
                }
            }
        }
    }

    // 检查是否为 TLS
    let data_start = if proxy_header_len > 0 { proxy_header_len } else { 0 };
    let is_tls = detection_data.len() > data_start && detection_data[data_start] == 0x16;

    if is_tls {
        // TLS 连接
        info!("🔐 [HTTP专用] 检测到 TLS 连接: {}", actual_remote_addr);

        let cert_manager = router.get_cert_manager()
            .ok_or("HTTP专用端口检测到 TLS，但未配置证书")?;

        let reconstructed_stream = crate::server::ReconstructedStream::new(stream, &buffer[..bytes_read]);
        handle_tls_connection(reconstructed_stream, actual_remote_addr, router, adapter, Some(cert_manager)).await
    } else {
        // 直接 HTTP 连接
        info!("🌐 [HTTP专用] 检测到 HTTP 连接: {}", actual_remote_addr);

        let reconstructed_stream = crate::server::ReconstructedStream::new(stream, &buffer[..bytes_read]);
        handle_http1_connection_with_stream(reconstructed_stream, actual_remote_addr, adapter).await
    }
}


