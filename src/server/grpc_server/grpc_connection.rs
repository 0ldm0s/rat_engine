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
use tokio_rustls::server::TlsStream;
use crate::utils::logger::{debug, info, error};

pub async fn handle_grpc_tls_connection<S>(
    stream: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    cert_manager: Arc<std::sync::RwLock<crate::server::cert_manager::CertificateManager>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use h2::server;

    info!("🔐 [gRPC] 开始 TLS 握手: {}", remote_addr);

    // 获取 gRPC 专用的 ServerConfig
    let server_config = {
        let cert_manager_guard = cert_manager.read()
            .map_err(|e| format!("无法获取证书管理器读锁: {}", e))?;
        cert_manager_guard.get_grpc_server_config()
    };

    // 使用 tokio-rustls 进行 TLS 握手
    println!("🔍 [DEBUG] [gRPC] 开始 TLS 握手，remote_addr={}", remote_addr);

    let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
    let tls_stream = acceptor.accept(stream).await
        .map_err(|e| {
            println!("❌ [DEBUG] [gRPC] TLS 握手失败，错误类型: {:?}", std::error::Error::source(&e));
            println!("❌ [DEBUG] [gRPC] 完整错误: {:?}", e);
            error!("❌ [gRPC] TLS 握手失败: {}", e);
            format!("TLS 握手失败: {}", e)
        })?;

    info!("✅ [gRPC] TLS 握手成功: {}", remote_addr);

    // 获取 ALPN 协议
    let (_tcp_stream, conn) = tls_stream.get_ref();
    let alpn_protocol = conn.alpn_protocol().map(|p| p.to_vec());
    info!("🔐 [gRPC] ALPN 协议: {:?}", alpn_protocol);

    // 检查 ALPN 是否为 h2，gRPC 强制要求 HTTP/2
    // 注意：如果客户端使用 h2c-over-TLS 模式（Xray-core 风格），ALPN 可能为 None
    // 我们仍然接受这种连接，因为客户端会在 TLS 通道内发送 h2c 帧
    if alpn_protocol.is_some() && !crate::server::cert_manager::rustls_cert::AlpnProtocol::is_http2(&alpn_protocol) {
        error!("❌ [gRPC] 拒绝非 HTTP/2 连接: ALPN={:?}, 客户端={}", alpn_protocol, remote_addr);
        return Err(format!("gRPC 只支持 HTTP/2，客户端协商的 ALPN 协议: {:?}", alpn_protocol).into());
    }

    if alpn_protocol.is_none() {
        info!("⚠️  [gRPC] 无 ALPN 协商，可能是 h2c-over-TLS 模式，继续处理");
    }

    info!("✅ [gRPC] HTTP/2 连接验证通过: {}", remote_addr);

    // 委托给内部处理函数
    handle_grpc_h2_connection_internal(tls_stream, remote_addr, router).await
}

/// 内部函数：处理已建立的 TLS 连接上的 gRPC over HTTP/2
pub async fn handle_grpc_h2_connection_internal<S>(
    tls_stream: TlsStream<S>,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    debug!("🔧 [gRPC专用] 开始处理 gRPC over HTTP/2: {}", remote_addr);

    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024);

    let mut connection = h2_builder.handshake(tls_stream).await
        .map_err(|e| {
            error!("❌ [gRPC专用] HTTP/2 握手失败: {}", e);
            format!("HTTP/2 握手失败: {}", e)
        })?;

    info!("✅ [gRPC专用] HTTP/2 连接已建立: {}", remote_addr);

    // 处理 gRPC 请求
    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                debug!("📥 [gRPC专用] 接收到 gRPC 请求: {} {}",
                    request.method(), request.uri().path());

                let router_clone = router.clone();

                tokio::spawn(async move {
                    if let Err(e) = super::h2_request_handler::handle_h2_request(request, respond, remote_addr, router_clone).await {
                        error!("❌ [gRPC专用] 处理 gRPC 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("❌ [gRPC专用] 接受请求失败: {}", e);
                break;
            }
        }
    }

    debug!("🔌 [gRPC专用] 连接关闭: {}", remote_addr);
    Ok(())
}

