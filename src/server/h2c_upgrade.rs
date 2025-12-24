//! H2C (HTTP/2 over Cleartext) 升级处理器
//!
//! 参考 tonic 的 h2c 实现，处理 HTTP/1.1 → HTTP/2 升级
//! 当检测到 upgrade: h2c 头部时，进行协议升级

use hyper::{Request, Response};
use hyper::body::Incoming;
use std::net::SocketAddr;
use std::sync::Arc;
use std::pin::Pin;
use std::task::{Context, Poll};
use hyper::service::Service;
use hyper_util::rt::TokioIo;

use crate::server::router::Router;
use crate::utils::logger::{debug, info, warn, error};

// 使用 hyper::body::Body 替代 tonic::body::Body
type BoxBody = hyper::body::Body;

/// H2C 升级处理器
#[derive(Clone)]
pub struct H2cUpgradeHandler {
    router: Arc<Router>,
    remote_addr: SocketAddr,
}

impl H2cUpgradeHandler {
    /// 创建新的 H2C 升级处理器
    pub fn new(router: Arc<Router>, remote_addr: SocketAddr) -> Self {
        Self { router, remote_addr }
    }
}

impl Service<Request<Incoming>> for H2cUpgradeHandler {
    type Response = hyper::Response<BoxBody>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: hyper::Request<Incoming>) -> Self::Future {
        let router = self.router.clone();
        let remote_addr = self.remote_addr;

        Box::pin(async move {
            // 打印请求信息用于调试
            debug!("🔄 [H2C Handler] 处理升级请求: {} {}", req.method(), req.uri().path());

            // 启动异步任务处理协议升级
            tokio::spawn(async move {
                match hyper::upgrade::on(&mut req).await {
                    Ok(upgraded_io) => {
                        info!("✅ [H2C Handler] 协议升级成功，建立 HTTP/2 连接: {}", remote_addr);

                        // 使用 TokioIo 包装 Upgraded 连接
                        let io = TokioIo::new(upgraded_io);

                        // 在升级后的连接上建立 HTTP/2 服务器
                        if let Err(e) = handle_h2_upgraded_connection(io, remote_addr, router).await {
                            error!("❌ [H2C Handler] 升级后处理失败: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("❌ [H2C Handler] 协议升级失败: {}", e);
                    }
                }
            });

            // 返回 101 Switching Protocols 响应
            let mut response = hyper::Response::new(BoxBody::default());
            *response.status_mut() = hyper::StatusCode::SWITCHING_PROTOCOLS;
            response.headers_mut().insert(
                hyper::header::UPGRADE,
                hyper::header::HeaderValue::from_static("h2c"),
            );

            Ok(response)
        })
    }
}

/// 处理升级后的 HTTP/2 连接
pub async fn handle_h2_upgraded_connection<S>(
    upgraded_io: S,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use h2::server;

    info!("🔍 [服务端] 开始处理升级后的 H2C 连接: {}", remote_addr);

    let mut h2_builder = h2::server::Builder::default();
    h2_builder.max_frame_size(1024 * 1024);

    let mut connection = h2_builder.handshake(upgraded_io).await
        .map_err(|e| format!("HTTP/2 握手失败: {}", e))?;

    info!("✅ [服务端] 升级后 HTTP/2 连接已建立: {}", remote_addr);

    while let Some(request_result) = connection.accept().await {
        match request_result {
            Ok((request, respond)) => {
                let router_clone = router.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::server::connection_handler::handle_h2_request(request, respond, remote_addr, router_clone).await {
                        error!("❌ [服务端] 处理 HTTP/2 请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("❌ [服务端] 接受 HTTP/2 请求失败: {}", e);
                break;
            }
        }
    }

    Ok(())
}
