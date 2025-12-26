//! gRPC 连接处理模块
//!
//! 专门处理 gRPC over TLS 连接，使用 h2 server builder

use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use h2::server;
use hyper::Request;
use crate::utils::logger::{debug, info, error};

pub async fn handle_grpc_tls_connection(
    stream: tokio::net::TcpStream,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    cert_manager: Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use h2::server;

    info!("🔐 [gRPC] 开始 TLS 握手: {}", remote_addr);

    // 获取 gRPC 专用的 ServerConfig
    let server_config = {
        let cert_manager_guard = cert_manager.read()
            .map_err(|e| format!("无法获取证书管理器读锁: {}", e))?;
        cert_manager_guard.get_grpc_server_config()
    };

    // 使用 tokio-rustls 进行 TLS 握手
    let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
    let tls_stream = acceptor.accept(stream).await
        .map_err(|e| {
            error!("❌ [gRPC] TLS 握手失败: {}", e);
            format!("TLS 握手失败: {}", e)
        })?;

    info!("✅ [gRPC] TLS 握手成功: {}", remote_addr);

    // 获取 ALPN 协议
    let (_tcp_stream, conn) = tls_stream.get_ref();
    let alpn_protocol = conn.alpn_protocol().map(|p| p.to_vec());
    info!("🔐 [gRPC] ALPN 协议: {:?}", alpn_protocol);

    // 检查 ALPN 是否为 h2，gRPC 强制要求 HTTP/2
    if !crate::server::cert_manager::rustls_cert::AlpnProtocol::is_http2(&alpn_protocol) {
        error!("❌ [gRPC] 拒绝非 HTTP/2 连接: ALPN={:?}, 客户端={}", alpn_protocol, remote_addr);
        return Err(format!("gRPC 只支持 HTTP/2，客户端协商的 ALPN 协议: {:?}", alpn_protocol).into());
    }

    info!("✅ [gRPC] HTTP/2 连接验证通过: {}", remote_addr);

    // 在 TLS 连接上建立 HTTP/2（内联处理）
    debug!("🔍 [gRPC] 开始处理 HTTP/2 连接: {}", remote_addr);

    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024);

    let mut connection = h2_builder.handshake(tls_stream).await
        .map_err(|e| {
            error!("❌ [gRPC] HTTP/2 握手失败: {}", e);
            format!("HTTP/2 握手失败: {}", e)
        })?;

    info!("✅ [gRPC] HTTP/2 连接已建立: {}", remote_addr);

    // 处理 HTTP/2 请求
    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                debug!("📥 [gRPC] 接收到 HTTP/2 请求: {} {}",
                    request.method(), request.uri().path());

                let router_clone = router.clone();

                tokio::spawn(async move {
                    if let Err(e) = crate::server::h2_request_handler::handle_h2_request(request, respond, remote_addr, router_clone).await {
                        error!("❌ [gRPC] 处理 HTTP/2 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("❌ [gRPC] 接受 HTTP/2 请求失败: {}", e);
                break;
            }
        }
    }

    Ok(())
}

