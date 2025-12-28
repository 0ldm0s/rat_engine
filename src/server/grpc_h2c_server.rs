//! gRPC h2c-over-TLS 服务端模块
//!
//! Xray-core 风格：接受无 ALPN 的 TLS 连接，在 TLS 通道内传输 h2c 帧

use crate::server::Router;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use h2::server;
use tokio_rustls::server::TlsStream;
use crate::utils::logger::{debug, info, error};

/// 处理 h2c-over-TLS 模式的 gRPC 连接
///
/// 与标准 gRPC 服务端的区别：
/// - 不检查 ALPN（因为客户端使用 insecure credentials，不会发送 ALPN）
/// - TLS 解密后直接进行 h2c handshake（客户端发送的是 h2c 格式）
///
/// # 参数
/// * `stream` - TCP 流
/// * `remote_addr` - 远程地址
/// * `router` - 路由器
/// * `cert_manager` - 证书管理器
pub async fn handle_grpc_h2c_over_tls_connection<S>(
    stream: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    cert_manager: Arc<std::sync::RwLock<CertificateManager>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    info!("🔐 [gRPC h2c-over-TLS] 开始 TLS 握手: {}", remote_addr);

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
            error!("❌ [gRPC h2c-over-TLS] TLS 握手失败: {}", e);
            format!("TLS 握手失败: {}", e)
        })?;

    info!("✅ [gRPC h2c-over-TLS] TLS 握手成功（无 ALPN 检查）: {}", remote_addr);

    // 获取 ALPN 协议（仅用于日志）
    let (_tcp_stream, conn) = tls_stream.get_ref();
    let alpn_protocol = conn.alpn_protocol().map(|p| p.to_vec());
    info!("🔐 [gRPC h2c-over-TLS] ALPN 协议: {:?}（忽略，接受任何值）", alpn_protocol);

    // 委托给内部处理函数
    handle_grpc_h2c_over_tls_internal(tls_stream, remote_addr, router).await
}

/// 内部函数：处理已建立的 TLS 连接上的 h2c-over-TLS
pub async fn handle_grpc_h2c_over_tls_internal<S>(
    tls_stream: TlsStream<S>,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    debug!("🔧 [gRPC h2c-over-TLS] 开始处理 h2c over TLS: {}", remote_addr);

    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024);

    // h2c handshake：客户端发送的是 h2c 格式（PRI * HTTP/2.0...）
    let mut connection = h2_builder.handshake(tls_stream).await
        .map_err(|e| {
            error!("❌ [gRPC h2c-over-TLS] h2c 握手失败: {}", e);
            format!("h2c 握手失败: {}", e)
        })?;

    debug!("✅ [gRPC h2c-over-TLS] h2c 握手成功: {}", remote_addr);

    // 处理连接上的流
    while let Some(result) = connection.accept().await {
        match result {
            Ok((request, mut respond)) => {
                debug!("📨 [gRPC h2c-over-TLS] 收到请求: {} {}", request.method(), request.uri());

                // 处理 gRPC 请求
                let router_clone = router.clone();
                let remote_addr_clone = remote_addr;

                tokio::spawn(async move {
                    if let Err(e) = crate::server::h2_request_handler::handle_h2_request(
                        request,
                        respond,
                        remote_addr_clone,
                        router_clone,
                    ).await {
                        error!("❌ [服务端] 处理 h2c-over-TLS 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("❌ [gRPC h2c-over-TLS] 接受流失败: {}", e);
                break;
            }
        }
    }

    debug!("🔚 [gRPC h2c-over-TLS] 连接关闭: {}", remote_addr);
    Ok(())
}
