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
    debug!("📋 [HTTP专用] 请求头:");
    for (name, value) in request.headers() {
        if let Ok(value_str) = value.to_str() {
            debug!("   {}: {}", name, value_str);
        }
    }

    info!("📡 [HTTP专用] 处理 HTTP/2 请求: {} {}",
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

    debug!("🔄 [HTTP专用] 已转换为通用 HttpRequest，调用 Router::handle_http");

    // 调用 Router 的通用 handle_http 方法
    match router.handle_http(http_request).await {
        Ok(response) => {
            debug!("✅ [HTTP专用] Router 处理成功");

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
                                            crate::utils::logger::debug!("ℹ️ [HTTP专用] 流已关闭");
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
            error!("❌ [HTTP专用] Router 处理失败: {}", e);
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

    Ok(())
}

