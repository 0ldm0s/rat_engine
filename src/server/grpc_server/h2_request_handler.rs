//! HTTP/2 请求处理模块
//!
//! 处理 HTTP/2 请求，包含 gRPC 检测和路由逻辑

use hyper::Request;
use h2::{RecvStream, server::SendResponse};
use h2::server;
use crate::server::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::utils::logger::{debug, info, error};
use crate::server::http_request::HttpRequest;
use bytes::Bytes;
use http_body_util::{Full, combinators::BoxBody};
use hyper::Response;
use hyper::body::Incoming;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use std::pin::Pin;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

pub async fn handle_h2_request(
    request: hyper::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<bytes::Bytes>,
    remote_addr: SocketAddr,
    router: Arc<Router>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("🔍 [服务端] 开始处理 HTTP/2 请求: {} {} from {}", 
        request.method(), request.uri().path(), remote_addr);
    
    // 打印请求头信息
    debug!("📋 [gRPC专用] 请求头:");
    for (name, value) in request.headers() {
        if let Ok(value_str) = value.to_str() {
            debug!("   {}: {}", name, value_str);
        }
    }

    info!("🔧 [gRPC专用] 处理 gRPC 请求: {} {}",
        request.method(), request.uri().path());

    // 将 remote_addr 添加到 request 的 extensions 中
    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(remote_addr);
    let request_with_addr = hyper::Request::from_parts(parts, body);

    debug!("🔍 [gRPC专用] 已将 remote_addr {} 添加到请求扩展中", remote_addr);

    // 处理 gRPC 请求
    router.handle_grpc_request(request_with_addr, respond).await
        .map_err(|e| {
            rat_logger::error!("❌ [gRPC专用] gRPC 请求处理失败: {}", e);
            format!("gRPC 请求处理失败: {}", e)
        })?;

    Ok(())
}

