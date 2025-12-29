//! 单端口多协议适配器
//!
//! 在同一端口同时处理 HTTP 和 gRPC 请求
//! 使用 h2 server，在请求层根据路径路由

use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsStream;
use h2::server;
use hyper::{Request, Response};
use hyper::body::Incoming;
use http_body_util::{Full, combinators::BoxBody};
use hyper::body::Bytes;
use crate::utils::logger::{debug, info, error};

/// 处理单端口多协议的 TLS 连接
/// 同时支持 HTTP 和 gRPC 请求
///
/// 接收原始 stream 和证书管理器，进行 TLS 握手后根据路径路由请求
pub async fn handle_multi_protocol_tls_connection<S>(
    stream: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
    adapter: Arc<HyperAdapter>,
    cert_manager: Arc<std::sync::RwLock<CertificateManager>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    info!("🔀 [多协议] 开始 TLS 握手: {}", remote_addr);

    // 获取 HTTP 的 ServerConfig（单端口多协议模式使用 HTTP 证书）
    let server_config = {
        let cert_manager_guard = cert_manager.read()
            .map_err(|e| format!("无法获取证书管理器读锁: {}", e))?;
        cert_manager_guard.get_http_server_config()
            .ok_or("单端口多协议模式需要配置 TLS 证书")?
    };

    // 进行 TLS 握手
    let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
    let tls_stream = acceptor.accept(stream).await
        .map_err(|e| {
            error!("❌ [多协议] TLS 握手失败: {}", e);
            format!("TLS 握手失败: {}", e)
        })?;

    info!("✅ [多协议] TLS 握手成功: {}", remote_addr);

    // 获取 ALPN 协议验证
    let (_tcp_stream, conn) = tls_stream.get_ref();
    let alpn_protocol = conn.alpn_protocol().map(|p| p.to_vec());
    info!("🔐 [多协议] ALPN 协议: {:?}", alpn_protocol);

    let is_http2 = alpn_protocol.as_ref().map(|p| p == b"h2").unwrap_or(false);
    if !is_http2 {
        error!("❌ [多协议] 单端口模式强制要求 HTTP/2，ALPN={:?}", alpn_protocol);
        return Err("单端口多协议模式强制要求 HTTP/2".into());
    }

    // 使用 h2 server 处理 HTTP/2
    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024);

    let mut connection = h2_builder.handshake(tls_stream).await
        .map_err(|e| {
            error!("❌ [多协议] HTTP/2 握手失败: {}", e);
            format!("HTTP/2 握手失败: {}", e)
        })?;

    info!("✅ [多协议] HTTP/2 连接已建立: {}", remote_addr);

    // 处理请求
    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                let path = request.uri().path().to_string();
                let method = request.method().clone();
                debug!("📥 [多协议] 接收到请求: {} {}", method, path);

                let router_clone = router.clone();
                let adapter_clone = adapter.clone();

                tokio::spawn(async move {
                    // 检测是否为 gRPC 请求
                    let grpc_methods = router_clone.list_grpc_methods();
                    let is_grpc_request = grpc_methods.iter().any(|m| m == &path);

                    if is_grpc_request {
                        // gRPC 请求 - 直接使用 h2 Request<RecvStream>
                        debug!("🔀 [多协议] 路由到 gRPC 处理器: {}", path);
                        if let Err(e) = handle_grpc_request(request, respond, router_clone, remote_addr).await {
                            error!("❌ [多协议] gRPC 请求处理失败: {}", e);
                        }
                    } else {
                        // HTTP 请求 - 传递 router 而不是 adapter
                        debug!("🔀 [多协议] 路由到 HTTP 处理器: {}", path);
                        if let Err(e) = handle_http_request(request, respond, router_clone, remote_addr).await {
                            error!("❌ [多协议] HTTP 请求处理失败: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                error!("❌ [多协议] 接收请求失败: {}", e);
                break;
            }
        }
    }

    info!("🔌 [多协议] 连接关闭: {}", remote_addr);
    Ok(())
}

/// 处理 gRPC 请求 - 复用现有的 gRPC 处理逻辑
async fn handle_grpc_request(
    request: Request<h2::RecvStream>,
    respond: h2::server::SendResponse<bytes::Bytes>,
    router: Arc<Router>,
    remote_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("📡 [多协议-gRPC] 处理请求: {}", request.uri().path());

    // 直接复用现有的 gRPC h2 请求处理器
    crate::server::grpc_server::h2_request_handler::handle_h2_request(request, respond, remote_addr, router).await
}

/// 处理 HTTP 请求 - 简化版本，直接构造 HttpRequest
async fn handle_http_request(
    request: Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<bytes::Bytes>,
    router: Arc<Router>,
    remote_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use http_body_util::BodyExt;
    use crate::server::http_request::HttpRequest;

    debug!("🌐 [多协议-HTTP] 处理请求: {}", request.uri().path());

    // 直接构造 HttpRequest（跳过 Incoming 转换）
    let (parts, _recv_stream) = request.into_parts();

    let http_request = HttpRequest {
        method: parts.method,
        uri: parts.uri,
        version: parts.version,
        headers: parts.headers,
        body: vec![].into(),
        remote_addr: Some(remote_addr),
        source: crate::server::http_request::RequestSource::Http2,
        path_params: std::collections::HashMap::new(),
        python_handler_name: None,
    };

    // 调用 HTTP 处理器
    let response = router.handle_http(http_request).await
        .map_err(|e| format!("HTTP 请求处理失败: {}", e))?;

    // 收集响应体
    let (parts, body) = response.into_parts();
    let body_bytes = body.collect().await
        .map_err(|e| format!("收集响应体失败: {}", e))?
        .to_bytes();

    // 构建 h2 响应（Response<()>）
    let mut h2_response = hyper::Response::new(());

    // 设置状态码
    *h2_response.status_mut() = parts.status;

    // 设置响应头
    for (name, value) in parts.headers.iter() {
        let _ = h2_response.headers_mut().append(name, value.clone());
    }

    // 发送响应（使用 h2 API）
    let mut send_stream = respond.send_response(h2_response, false)?;

    // 发送响应体数据
    send_stream.send_data(body_bytes, true)?;

    debug!("✅ [多协议-HTTP] 响应已发送");
    Ok(())
}
