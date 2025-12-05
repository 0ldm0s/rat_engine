    //! gRPC HTTP 连接模块

use std::time::Duration;
use std::pin::Pin;
use std::net::{TcpStream, ToSocketAddrs};
use std::future::Future;
use std::task::{Context, Poll};

use hyper::{Request, Response, Uri};
use hyper::header::{HeaderMap, HeaderValue, CONTENT_TYPE, ACCEPT_ENCODING};
use hyper_util::client::legacy::connect::HttpConnector;
use http_body_util::{Full, BodyExt};
use hyper::body::Bytes;
use h2::{client, RecvStream};
use tokio::net::TcpStream as TokioTcpStream;
use tokio::time::timeout;
use futures_util::future;

use crate::error::{RatError, RatResult};
#[cfg(feature = "compression")]
use crate::compression::{CompressionType, CompressionConfig};
#[cfg(not(feature = "compression"))]
use crate::client::grpc_builder::CompressionConfig;

#[cfg(not(feature = "compression"))]
type CompressionType = (); // 空类型，在未启用compression时不会被实际使用
use crate::utils::logger::{info, warn, debug, error};
use openssl::ssl::{SslConnector, SslMethod};
use tokio_openssl::SslStream;
use crate::client::grpc_client::RatGrpcClient;

impl RatGrpcClient {
    async fn establish_h2_connection(&self, uri: &Uri) -> RatResult<h2::client::SendRequest<bytes::Bytes>> {
        let is_https = uri.scheme_str() == Some("https");
        let host = uri.host().ok_or_else(|| RatError::RequestError("URI 缺少主机".to_string()))?;
        let port = uri.port_u16().unwrap_or(if is_https { 443 } else { 80 });
        // 检查是否需要使用预解析IP
        let resolved_addr = if let Some(ref dns_mapping) = self.dns_mapping {
            if let Some(resolved_ip) = dns_mapping.get(host) {
                let addr = format!("{}:{}", resolved_ip, port);
                debug!("🔗 建立 H2 连接: {} (使用预解析IP: {} -> {}) ({})",
                    addr, host, resolved_ip, if is_https { "HTTPS" } else { "H2C" });
                addr
            } else {
                let addr = format!("{}:{}", host, port);
                debug!("🔗 建立 H2 连接: {} ({})", addr, if is_https { "HTTPS" } else { "H2C" });
                addr
            }
        } else {
            let addr = format!("{}:{}", host, port);
            debug!("🔗 建立 H2 连接: {} ({})", addr, if is_https { "HTTPS" } else { "H2C" });
            addr
        };
        
        // 建立 TCP 连接
        let tcp_stream = timeout(self.connect_timeout, tokio::net::TcpStream::connect(&resolved_addr))
            .await
            .map_err(|_| RatError::TimeoutError(rat_embed_lang::tf("h2_tcp_connection_timeout", &[("msg", &resolved_addr.to_string())])))?
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2_tcp_connection_failed", &[("msg", &e.to_string())])))?;
        
        debug!("✅ H2 TCP 连接已建立: {}", resolved_addr);
        
        // 根据协议类型进行握手
        let client = if is_https {
            // HTTPS: 先进行 TLS 握手，再进行 H2 握手
            let ssl_connector = self.create_tls_config()?;

            // 使用异步 TLS 连接
            let mut ssl = openssl::ssl::Ssl::new(&ssl_connector.context())
                .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("create_ssl_failed_http", &[("msg", &e.to_string())])))?;

            // 配置服务器名称验证（SNI）- 必须在创建 SSL 对象后设置
            if let Some(ref mtls_config) = self.mtls_config {
                if let Some(ref server_name) = mtls_config.server_name {
                    ssl.set_hostname(server_name)
                        .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("set_sni_hostname_failed", &[("msg", &e.to_string())])))?;
                } else {
                    ssl.set_hostname(host)
                        .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("set_default_hostname_failed", &[("msg", &e.to_string())])))?;
                }
            } else {
                ssl.set_hostname(host)
                    .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("set_hostname_failed", &[("msg", &e.to_string())])))?;
            }

            // 设置连接类型为客户端
            ssl.set_connect_state();
        let mut ssl_stream = SslStream::new(ssl, tcp_stream)
                .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("create_tls_stream_failed", &[("msg", &e.to_string())])))?;

        // 使用异步方式完成 TLS 握手
        use futures_util::future::poll_fn;
        poll_fn(|cx| {
            match std::pin::Pin::new(&mut ssl_stream).poll_do_handshake(cx) {
                std::task::Poll::Ready(Ok(())) => {
                         std::task::Poll::Ready(Ok(()))
                },
                std::task::Poll::Ready(Err(e)) => {
                           std::task::Poll::Ready(Err(e))
                },
                std::task::Poll::Pending => std::task::Poll::Pending,
            }
        }).await.map_err(|e| RatError::NetworkError(rat_embed_lang::tf("tls_handshake_failed_http", &[("msg", &e.to_string())])))?;

            debug!("🔐 TLS 连接建立成功，开始 HTTP/2 握手");

            let (client, h2_connection) = h2::client::handshake(ssl_stream)
                .await
                .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("http2_over_tls_handshake_failed", &[("msg", &e.to_string())])))?;

            // 在后台运行 H2 连接
            tokio::spawn(async move {
                if let Err(e) = h2_connection.await {
                    error!("❌ H2 连接错误: {}", e);
                }
            });

            client
        } else {
            // H2C: 直接进行 H2 握手
            let (client, h2_connection) = h2::client::handshake(tcp_stream)
                .await
                .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2c_handshake_failed", &[("msg", &e.to_string())])))?;
            
            // 在后台运行 H2 连接
            tokio::spawn(async move {
                if let Err(e) = h2_connection.await {
                    error!("❌ H2 连接错误: {}", e);
                }
            });
            
            client
        };
        
        debug!("🚀 H2 连接建立完成: {}", resolved_addr);
        Ok(client)
    }

    /// 发送 H2 请求（一元调用版本 - 读取完整响应体）
    pub async fn send_h2_request(&self, request: Request<Full<Bytes>>) -> RatResult<Response<Full<Bytes>>> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        debug!("🔗 使用 H2 发送 gRPC 请求: {} {}", method, uri);
        
        // 建立 H2 连接
        let client = self.establish_h2_connection(&uri).await?;
        
        // 发送请求并获取响应
        let h2_response = self.send_h2_request_internal(client, request).await?;
        
        debug!("📥 收到 H2 响应: {} {} - 状态码: {}", method, uri, h2_response.status());
        
        // 提取状态码和头部信息
        let status = h2_response.status();
        let headers = h2_response.headers().clone();
        
        // 读取响应体
        let mut body_stream = h2_response.into_body();
        let mut body_data = Vec::new();
        
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2_read_response_body_failed", &[("msg", &e.to_string())])))?;
            body_data.extend_from_slice(&chunk);
            // 释放流控制窗口
            let _ = body_stream.flow_control().release_capacity(chunk.len());
        }
        
        // 构建 Hyper 兼容的响应
        let mut response_builder = Response::builder()
            .status(status);
        
        // 复制响应头
        for (name, value) in &headers {
            response_builder = response_builder.header(name, value);
        }
        
        // 创建响应体
        let body = http_body_util::Full::new(Bytes::from(body_data));
        
        // 构建最终响应
        let response = response_builder
            .body(body)
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("build_response_failed", &[("msg", &e.to_string())])))?;
        
        Ok(response)
    }

    /// 内部方法：发送 H2 请求的通用逻辑
    async fn send_h2_request_internal(&self, mut client: h2::client::SendRequest<bytes::Bytes>, request: Request<Full<Bytes>>) -> RatResult<hyper::Response<h2::RecvStream>> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        // 构建 H2 请求
        let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
        let mut h2_request = hyper::Request::builder()
            .method(method.clone())
            .uri(path);
        
        // 复制头部
        for (name, value) in request.headers() {
            h2_request = h2_request.header(name, value);
        }
        
        let h2_request = h2_request
            .body(())
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_h2_request_failed", &[("msg", &e.to_string())])))?;
        
        // 发送请求
        let (response, mut send_stream) = client
            .send_request(h2_request, false)
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2_send_request_failed", &[("msg", &e.to_string())])))?;
        
        // 发送请求体
        let body_bytes = request.into_body().collect().await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("read_request_body_failed_http", &[("msg", &e.to_string())])))?
            .to_bytes();
        
        if !body_bytes.is_empty() {
            send_stream.send_data(body_bytes, true)
                .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2_send_data_failed", &[("msg", &e.to_string())])))?;
        } else {
            send_stream.send_data(Bytes::new(), true)
                .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2_send_empty_data_failed", &[("msg", &e.to_string())])))?;
        }
        
        // 等待响应
        let h2_response = timeout(self.request_timeout, response)
            .await
            .map_err(|_| RatError::TimeoutError(rat_embed_lang::tf("h2_response_timeout", &[("msg", &format!("{} {}", method, uri))])))?
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("h2_receive_response_failed", &[("msg", &e.to_string())])))?;
        
        Ok(h2_response)
    }

    /// 发送 H2 请求（流调用版本 - 返回流响应）
    pub async fn send_h2_request_stream(&self, request: Request<Full<Bytes>>) -> RatResult<Response<h2::RecvStream>> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        debug!("🔗 使用 H2 发送 gRPC 流请求: {} {}", method, uri);
        
        // 建立 H2 连接
        let client = self.establish_h2_connection(&uri).await?;
        
        // 发送请求并获取响应
        let h2_response = self.send_h2_request_internal(client, request).await?;
        
        debug!("📥 收到 H2 流响应: {} {} - 状态码: {}", method, uri, h2_response.status());
        
        // 对于流请求，错误状态在 trailers 中处理，不在初始响应头中
        // 直接返回流响应，不读取响应体
        let (parts, body_stream) = h2_response.into_parts();
        let response = Response::from_parts(parts, body_stream);
        
        Ok(response)
    }
    /// 关闭委托模式的双向流连接
    /// 
    /// # 参数
    /// * `stream_id` - 流ID
    pub async fn close_bidirectional_stream_delegated(&self, stream_id: u64) -> RatResult<()> {
        info!("🛑 开始关闭委托模式双向流: {}", stream_id);
        
        // 从委托管理器中关闭流，这会自动处理所有任务的取消和资源清理
        self.delegated_manager.close_stream(stream_id).await;
        
        info!("✅ 委托模式双向流 {} 已成功关闭", stream_id);
        Ok(())
    }
    /// 关闭客户端并清理所有资源
    /// 
    /// 这个方法会：
    /// 1. 关闭所有活跃的委托模式双向流
    /// 2. 停止连接池维护任务
    /// 3. 关闭所有连接
    pub async fn shutdown(&mut self) {
        info!("🛑 开始关闭 gRPC 客户端");

        // 关闭所有活跃的委托模式双向流
        if let Err(e) = self.delegated_manager.close_all_streams().await {
            warn!("⚠️ 关闭委托模式双向流失败: {}", e);
        }

        // 发送连接池关闭信号并等待处理
        self.connection_pool.send_shutdown_signal().await;
        
        info!("✅ gRPC 客户端已关闭");
    }
}

