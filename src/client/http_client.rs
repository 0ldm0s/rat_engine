//! RAT Engine HTTP 客户端实现
//! 
//! 支持 HTTP/1.1（hyper）和 HTTP/2（h2，含 h2c）的高性能客户端

use std::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;
use hyper::{Request, Response, Method, Uri, StatusCode, Version};
use hyper::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT, CONTENT_TYPE, CONTENT_LENGTH, ACCEPT_ENCODING, CONTENT_ENCODING, CONNECTION, UPGRADE};
use hyper::body::{Incoming, Body};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::{Full, BodyExt};
use hyper::body::Bytes;
use serde::{Serialize, Deserialize};
use tokio::time::timeout;
use tokio::net::TcpStream;
use h2::client::{self, SendRequest};
use h2::{RecvStream, SendStream};
use rustls::{ClientConfig, RootCertStore};
use rustls::pki_types::ServerName;
use tokio_rustls::{TlsConnector, client::TlsStream};
use webpki_roots;
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::error::{RatError, RatResult};
use crate::compression::{CompressionType, CompressionConfig};
use crate::client::builder::ClientProtocolMode;
use crate::client::connection_pool::ClientConnectionPool;
use crate::client::http_client_delegated::{HttpRequestHandler, HttpRequestManager};
use crate::utils::logger::{debug, warn, info};

/// HTTP 协议版本枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HttpProtocol {
    /// HTTP/1.1
    Http1,
    /// HTTP/2 over TLS
    Http2,
    /// HTTP/2 Cleartext (h2c)
    Http2Cleartext,
}

/// RAT Engine HTTP 客户端
/// 
/// 支持多协议的高性能 HTTP 客户端实现：
/// - HTTP/1.1（使用 hyper）
/// - HTTP/2 over TLS（使用 h2）
/// - HTTP/2 Cleartext/h2c（使用 h2）
/// - 统一连接池管理（所有连接由连接池统一处理）
/// - 超时控制
/// - JSON 序列化/反序列化
/// - 自动压缩协商（lz4 > zstd > gzip > 不压缩）
/// - 自动解压缩
/// - 请求/响应日志
pub struct RatHttpClient {
    /// HTTP/1.1 客户端（仅用于兼容性，实际请求通过连接池处理）
    #[deprecated(note = "所有请求现在通过连接池统一处理")]
    pub(crate) http1_client: Client<HttpConnector, Full<Bytes>>,
    /// 已弃用：HTTP/1.1 连接池（现在使用统一的 connection_pool）
    #[deprecated(note = "使用 connection_pool 替代")]
    http1_connections: Arc<dashmap::DashMap<String, Client<HttpConnector, Full<Bytes>>>>,
    /// 已弃用：HTTP/2 连接池（现在使用统一的 connection_pool）
    #[deprecated(note = "使用 connection_pool 替代")]
    h2_connections: Arc<dashmap::DashMap<String, SendRequest<Bytes>>>,
    /// TLS 连接器（已弃用，连接池内部管理）
    #[deprecated(note = "TLS 连接由连接池内部管理")]
    tls_connector: TlsConnector,
    /// 请求超时时间
    pub(crate) request_timeout: Duration,
    /// 用户代理字符串
    pub(crate) user_agent: String,
    /// 压缩配置
    pub(crate) compression_config: CompressionConfig,
    /// 是否启用压缩
    pub(crate) enable_compression: bool,
    /// 默认协议版本
    pub(crate) default_protocol: HttpProtocol,
    /// 协议模式配置
    pub(crate) protocol_mode: ClientProtocolMode,
    /// 统一连接池（负责所有连接的创建、管理和复用）
    pub(crate) connection_pool: Arc<ClientConnectionPool>,
    /// 委托管理器
    delegated_manager: Arc<std::sync::RwLock<Arc<HttpRequestManager>>>,
}

impl RatHttpClient {
    /// 创建新的 HTTP 客户端实例
    /// 
    /// # 参数
    /// * `http1_client` - HTTP/1.1 hyper 客户端实例
    /// * `request_timeout` - 请求超时时间
    /// * `user_agent` - 用户代理字符串
    /// * `compression_config` - 压缩配置
    /// * `enable_compression` - 是否启用压缩
    /// * `default_protocol` - 默认协议版本
    /// * `protocol_mode` - 协议模式配置
    /// * `development_mode` - 是否为开发模式（跳过证书验证）
    #[doc(hidden)]
    pub fn new(
        http1_client: Client<HttpConnector, Full<Bytes>>,
        request_timeout: Duration,
        user_agent: String,
        compression_config: CompressionConfig,
        enable_compression: bool,
        default_protocol: HttpProtocol,
        protocol_mode: ClientProtocolMode,
        development_mode: bool,
        connection_pool: Arc<ClientConnectionPool>,
    ) -> Self {
        use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
        use rustls::pki_types::{CertificateDer, ServerName as RustlsServerName, UnixTime};

        /// 不验证证书的验证器（仅用于开发环境）
        #[derive(Debug)]
        struct NoVerifier;

        impl ServerCertVerifier for NoVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &RustlsServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                vec![
                    rustls::SignatureScheme::RSA_PKCS1_SHA1,
                    rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                    rustls::SignatureScheme::RSA_PKCS1_SHA384,
                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                    rustls::SignatureScheme::RSA_PKCS1_SHA512,
                    rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                    rustls::SignatureScheme::RSA_PSS_SHA256,
                    rustls::SignatureScheme::RSA_PSS_SHA384,
                    rustls::SignatureScheme::RSA_PSS_SHA512,
                    rustls::SignatureScheme::ED25519,
                    rustls::SignatureScheme::ED448,
                ]
            }
        }

        // 创建 TLS 配置
        let mut tls_config = if development_mode {
            // 开发模式：跳过证书验证
            warn!("⚠️  警告：已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifier))
                .with_no_client_auth()
        } else {
            // 非开发模式：正常证书验证
            let mut root_store = RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth()
        };
        
        // 配置 ALPN 协议协商，支持 HTTP/2
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            
        let tls_connector = TlsConnector::from(Arc::new(tls_config));

        #[allow(deprecated)]
        let http1_connections = Arc::new(dashmap::DashMap::new());
        #[allow(deprecated)]
        let h2_connections = Arc::new(dashmap::DashMap::new());
        
        // 连接池维护任务已在构建器中启动
        
        Self {
            http1_client,
            #[allow(deprecated)]
            http1_connections,
            #[allow(deprecated)]
            h2_connections,
            tls_connector,
            request_timeout,
            user_agent,
            compression_config,
            enable_compression,
            default_protocol,
            protocol_mode,
            connection_pool,
            delegated_manager: Arc::new(std::sync::RwLock::new(Arc::new(HttpRequestManager::placeholder()))),
        }
    }
}

impl Clone for RatHttpClient {
    fn clone(&self) -> Self {
        Self {
            http1_client: self.http1_client.clone(),
            #[allow(deprecated)]
            http1_connections: self.http1_connections.clone(),
            #[allow(deprecated)]
            h2_connections: self.h2_connections.clone(),
            tls_connector: self.tls_connector.clone(),
            request_timeout: self.request_timeout,
            user_agent: self.user_agent.clone(),
            compression_config: self.compression_config.clone(),
            enable_compression: self.enable_compression,
            default_protocol: self.default_protocol,
            protocol_mode: self.protocol_mode,
            connection_pool: self.connection_pool.clone(),
            delegated_manager: self.delegated_manager.clone(),
        }
    }
}

impl RatHttpClient {
    /// 更新委托管理器
    /// 
    /// # 参数
    /// * `new_manager` - 新的委托管理器实例
    pub fn update_delegated_manager(&self, new_manager: Arc<HttpRequestManager>) {
        if let Ok(mut manager) = self.delegated_manager.write() {
            *manager = new_manager;
        }
    }

    /// 获取委托管理器的只读引用
    pub fn get_delegated_manager(&self) -> Option<Arc<HttpRequestManager>> {
        self.delegated_manager.read().ok().map(|manager| manager.clone())
    }
}

impl RatHttpClient {
    /// 创建带有委托管理器的 HTTP 客户端实例
    /// 
    /// # 参数
    /// * `http1_client` - HTTP/1.1 hyper 客户端实例
    /// * `request_timeout` - 请求超时时间
    /// * `user_agent` - 用户代理字符串
    /// * `compression_config` - 压缩配置
    /// * `enable_compression` - 是否启用压缩
    /// * `default_protocol` - 默认协议版本
    /// * `protocol_mode` - 协议模式配置
    /// * `development_mode` - 是否为开发模式（跳过证书验证）
    /// * `connection_pool` - 连接池引用
    /// * `delegated_manager` - 委托模式请求管理器
    #[doc(hidden)]
    pub fn new_with_delegated_manager(
        http1_client: Client<HttpConnector, Full<Bytes>>,
        request_timeout: Duration,
        user_agent: String,
        compression_config: CompressionConfig,
        enable_compression: bool,
        default_protocol: HttpProtocol,
        protocol_mode: ClientProtocolMode,
        development_mode: bool,
        connection_pool: Arc<ClientConnectionPool>,
        delegated_manager: Arc<HttpRequestManager>,
    ) -> Self {
        use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
        use rustls::pki_types::{CertificateDer, ServerName as RustlsServerName, UnixTime};

        /// 不验证证书的验证器（仅用于开发环境）
        #[derive(Debug)]
        struct NoVerifier;

        impl ServerCertVerifier for NoVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &RustlsServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                vec![
                    rustls::SignatureScheme::RSA_PKCS1_SHA1,
                    rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                    rustls::SignatureScheme::RSA_PKCS1_SHA384,
                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                    rustls::SignatureScheme::RSA_PKCS1_SHA512,
                    rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                    rustls::SignatureScheme::RSA_PSS_SHA256,
                    rustls::SignatureScheme::RSA_PSS_SHA384,
                    rustls::SignatureScheme::RSA_PSS_SHA512,
                    rustls::SignatureScheme::ED25519,
                    rustls::SignatureScheme::ED448,
                ]
            }
        }

        // 创建 TLS 配置
        let mut tls_config = if development_mode {
            // 开发模式：跳过证书验证
            warn!("⚠️  警告：已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifier))
                .with_no_client_auth()
        } else {
            // 非开发模式：正常证书验证
            let mut root_store = RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth()
        };
        
        // 配置 ALPN 协议协商，支持 HTTP/2
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            
        let tls_connector = TlsConnector::from(Arc::new(tls_config));

        #[allow(deprecated)]
        let http1_connections = Arc::new(dashmap::DashMap::new());
        #[allow(deprecated)]
        let h2_connections = Arc::new(dashmap::DashMap::new());
        
        // 连接池维护任务已在构建器中启动
        
        Self {
            http1_client,
            #[allow(deprecated)]
            http1_connections,
            #[allow(deprecated)]
            h2_connections,
            tls_connector,
            request_timeout,
            user_agent,
            compression_config,
            enable_compression,
            default_protocol,
            protocol_mode,
            connection_pool,
            delegated_manager: Arc::new(std::sync::RwLock::new(delegated_manager)),
        }
    }

    /// 获取或创建 HTTP/2 连接（已弃用，使用连接池统一处理）
    #[deprecated(note = "使用连接池统一处理，不再需要单独获取连接")]
    async fn get_or_create_h2_connection(&self, uri: &Uri, use_tls: bool) -> RatResult<SendRequest<Bytes>> {
        debug!("🔗 HTTP/2 请求使用异步连接池: {} (TLS: {})", uri, use_tls);
        
        // 直接使用连接池获取连接
        let connection = self.connection_pool.get_connection(uri).await?;
        let send_request = connection.send_request.clone();
        
        Ok(send_request)
    }

    /// 检测 URI 应该使用的协议
    fn detect_protocol(&self, uri: &Uri) -> HttpProtocol {
        match uri.scheme_str() {
            Some("https") => {
                // HTTPS 协议处理
                match self.protocol_mode {
                    ClientProtocolMode::Http1Only => HttpProtocol::Http1,
                    ClientProtocolMode::Http1WithHttp2Tls => HttpProtocol::Http2,
                    ClientProtocolMode::Auto => HttpProtocol::Http2,
                }
            }
            Some("http") => {
                // HTTP 协议处理
                match self.protocol_mode {
                    ClientProtocolMode::Http1Only => HttpProtocol::Http1,
                    ClientProtocolMode::Http1WithHttp2Tls => HttpProtocol::Http1, // 禁用 H2C
                    ClientProtocolMode::Auto => {
                        // 自动模式：使用配置的默认协议
                        match self.default_protocol {
                            HttpProtocol::Http2Cleartext => HttpProtocol::Http2Cleartext,
                            _ => HttpProtocol::Http1,
                        }
                    }
                }
            }
            _ => HttpProtocol::Http1,
        }
    }

    /// 发送 GET 请求
    /// 
    /// # 参数
    /// * `uri` - 请求 URI
    /// * `headers` - 可选的请求头
    /// 
    /// # 返回
    /// 返回响应体的字节数据
    pub async fn get<U>(&self, uri: U, headers: Option<HeaderMap>) -> RatResult<RatHttpResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        self.send_request_with_protocol(Method::GET, uri, None, headers, None).await
    }

    /// 发送 POST 请求
    /// 
    /// # 参数
    /// * `uri` - 请求 URI
    /// * `body` - 请求体数据
    /// * `headers` - 可选的请求头
    pub async fn post<U>(&self, uri: U, body: Option<Bytes>, headers: Option<HeaderMap>) -> RatResult<RatHttpResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        self.send_request_with_protocol(Method::POST, uri, body, headers, None).await
    }

    /// 发送 POST JSON 请求
    /// 
    /// # 参数
    /// * `uri` - 请求 URI
    /// * `json_data` - 可序列化的 JSON 数据
    /// * `headers` - 可选的请求头
    pub async fn post_json<U, T>(&self, uri: U, json_data: &T, headers: Option<HeaderMap>) -> RatResult<RatHttpResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        T: Serialize,
    {
        let json_bytes = serde_json::to_vec(json_data)
            .map_err(|e| RatError::SerializationError(format!("JSON 序列化失败: {}", e)))?;
        
        let mut request_headers = headers.unwrap_or_default();
        request_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        self.send_request_with_protocol(Method::POST, uri, Some(Bytes::from(json_bytes)), Some(request_headers), None).await
    }

    /// 发送 GET 请求并返回流式响应
    /// 
    /// # 参数
    /// * `uri` - 请求 URI
    /// * `headers` - 可选的额外请求头
    pub async fn get_stream<U>(&self, uri: U, headers: Option<HeaderMap>) -> RatResult<RatHttpStreamResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let uri: Uri = uri.try_into()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e.into())))?;

        let protocol = self.detect_protocol(&uri);
        
        match protocol {
            HttpProtocol::Http1 => self.get_stream_http1(uri, headers).await,
            HttpProtocol::Http2 => self.get_stream_http2(uri, headers, true).await,
            HttpProtocol::Http2Cleartext => self.get_stream_http2(uri, headers, false).await,
        }
    }

    /// 发送 HTTP/1.1 流式 GET 请求
    async fn get_stream_http1(&self, uri: Uri, headers: Option<HeaderMap>) -> RatResult<RatHttpStreamResponse> {
        let request = self.build_http1_request(Method::GET, uri.clone(), None, headers)?;
        
        debug!("🌐 发送 HTTP/1.1 流式请求: GET {}", uri);
        
        let response = timeout(self.request_timeout, self.http1_client.request(request))
            .await
            .map_err(|_| RatError::TimeoutError(format!("HTTP/1.1 流式请求超时: GET {}", uri)))?
            .map_err(|e| RatError::NetworkError(format!("HTTP/1.1 流式网络请求失败: {}", e)))?;

        let status = response.status();
        let headers = response.headers().clone();
        
        // 创建流式响应体
        let body_stream = response.into_body();
        let stream = Box::pin(BodyStreamAdapter::new(body_stream));

        Ok(RatHttpStreamResponse {
            status,
            headers,
            body: stream,
        })
    }

    /// 发送 HTTP/2 流式 GET 请求
    async fn get_stream_http2(&self, uri: Uri, headers: Option<HeaderMap>, use_tls: bool) -> RatResult<RatHttpStreamResponse> {
        debug!("🌐 发送 HTTP/2 流式请求: GET {}", uri);

        let mut h2_client = self.get_or_create_h2_connection(&uri, use_tls).await?;

        let mut request_builder = Request::builder()
            .method(Method::GET)
            .uri(uri.clone())
            .version(Version::HTTP_2)
            .header(USER_AGENT, &self.user_agent);

        if self.enable_compression {
            let accept_encoding = "lz4, zstd, gzip, deflate";
            request_builder = request_builder.header(ACCEPT_ENCODING, accept_encoding);
        }

        if let Some(headers) = headers {
            for (name, value) in headers.iter() {
                request_builder = request_builder.header(name, value);
            }
        }

        let request = request_builder
            .body(())
            .map_err(|e| RatError::RequestError(format!("构建 HTTP/2 流式请求失败: {}", e)))?;

        let (response, mut send_stream) = h2_client.send_request(request, false)
            .map_err(|e| RatError::NetworkError(format!("HTTP/2 发送流式请求失败: {}", e)))?;

        // 发送空请求体并结束
        send_stream.send_data(Bytes::new(), true)
            .map_err(|e| RatError::NetworkError(format!("HTTP/2 发送空请求体失败: {}", e)))?;

        let response = timeout(self.request_timeout, response)
            .await
            .map_err(|_| RatError::TimeoutError(format!("HTTP/2 流式响应超时: GET {}", uri)))?
            .map_err(|e| RatError::NetworkError(format!("HTTP/2 接收流式响应失败: {}", e)))?;

        let status = response.status();
        let headers = response.headers().clone();
        let recv_stream = response.into_body();

        // 创建流式响应体
        let stream = Box::pin(H2StreamAdapter::new(recv_stream));

        Ok(RatHttpStreamResponse {
            status,
            headers,
            body: stream,
        })
    }

    /// 发送 PUT 请求
    /// 
    /// # 参数
    /// * `uri` - 请求 URI
    /// * `body` - 请求体数据
    /// * `headers` - 可选的请求头
    pub async fn put<U>(&self, uri: U, body: Option<Bytes>, headers: Option<HeaderMap>) -> RatResult<RatHttpResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        self.send_request_with_protocol(Method::PUT, uri, body, headers, None).await
    }

    /// 发送 DELETE 请求
    /// 
    /// # 参数
    /// * `uri` - 请求 URI
    /// * `headers` - 可选的请求头
    pub async fn delete<U>(&self, uri: U, headers: Option<HeaderMap>) -> RatResult<RatHttpResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        self.send_request_with_protocol(Method::DELETE, uri, None, headers, None).await
    }

    /// 使用指定协议发送请求
    /// 
    /// # 参数
    /// * `method` - HTTP 方法
    /// * `uri` - 请求 URI
    /// * `body` - 请求体数据
    /// * `headers` - 可选的请求头
    /// * `protocol` - 可选的协议版本（如果为 None，则自动检测）
    pub async fn send_request_with_protocol<U>(
        &self,
        method: Method,
        uri: U,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
        protocol: Option<HttpProtocol>,
    ) -> RatResult<RatHttpResponse>
    where
        U: TryInto<Uri>,
        U::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let uri = uri.try_into()
            .map_err(|e| RatError::RequestError(format!("无效的 URI: {}", e.into())))?;

        let protocol = protocol.unwrap_or_else(|| self.detect_protocol(&uri));

        debug!("🔍 [HTTP客户端] 协议检测结果: {} {} -> {:?}", method, uri, protocol);

        match protocol {
            HttpProtocol::Http1 => {
                // 统一使用连接池处理HTTP/1.1请求
                self.send_http1_request_via_pool(method, uri, body, headers).await
            }
            HttpProtocol::Http2 => {
                self.send_http2_request_via_pool(method, uri, body, headers, true).await
            }
            HttpProtocol::Http2Cleartext => {
                self.send_http2_request_via_pool(method, uri, body, headers, false).await
            }
        }
    }

    /// 构建 HTTP/1.1 请求
    fn build_http1_request(
        &self,
        method: Method,
        uri: Uri,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
    ) -> RatResult<Request<Full<Bytes>>> {
        let mut request_builder = Request::builder()
            .method(method)
            .uri(uri)
            .version(Version::HTTP_11)
            .header(USER_AGENT, &self.user_agent);

        // 添加压缩协商头部（优先级：lz4 > zstd > gzip > 不压缩）
        if self.enable_compression {
            let accept_encoding = "lz4, zstd, gzip, deflate";
            request_builder = request_builder.header(ACCEPT_ENCODING, accept_encoding);
        }

        // 添加自定义请求头
        if let Some(headers) = headers {
            for (name, value) in headers.iter() {
                request_builder = request_builder.header(name, value);
            }
        }

        let body = body.unwrap_or_default();
        let request = request_builder
            .body(Full::new(body))
            .map_err(|e| RatError::RequestError(format!("构建 HTTP/1.1 请求失败: {}", e)))?;

        Ok(request)
    }

    /// 直接发送 HTTP/1.1 请求（不使用连接池）
    async fn send_http1_request_direct(
        &self,
        method: Method,
        uri: Uri,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
    ) -> RatResult<RatHttpResponse> {
        debug!("[客户端] 🔗 HTTP/1.1 直接请求: {} {}", method, uri);
        
        // 构建 HTTP/1.1 请求
        let request = self.build_http1_request(method.clone(), uri.clone(), body, headers)?;
        
        // 使用弃用的 HTTP/1.1 客户端直接发送请求
        #[allow(deprecated)]
        let response = self.http1_client.request(request).await
            .map_err(|e| RatError::NetworkError(format!("HTTP/1.1 请求失败: {}", e)))?;
        
        // 手动转换响应
        let (parts, body) = response.into_parts();
        let body_bytes = body.collect().await
            .map_err(|e| RatError::NetworkError(format!("读取响应体失败: {}", e)))?
            .to_bytes();
        
        // 解压缩响应体
        let decompressed_body = self.decompress_response_body(body_bytes.clone(), parts.headers.get(CONTENT_ENCODING))?;
        
        debug!("📥 收到 HTTP/1.1 直接响应: {} {} - 状态码: {}, 原始大小: {} bytes, 解压后大小: {} bytes", 
                          method, uri, parts.status, body_bytes.len(), decompressed_body.len());
        
        Ok(RatHttpResponse {
            status: parts.status,
            headers: parts.headers,
            body: decompressed_body,
            original_size: body_bytes.len(),
        })
    }

    /// 通过连接池发送 HTTP/1.1 请求
    async fn send_http1_request_via_pool(
        &self,
        method: Method,
        uri: Uri,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
    ) -> RatResult<RatHttpResponse> {
        debug!("[客户端] 🔗 HTTP/1.1 请求处理: {} {}", method, uri);
        
        // 在 Http1Only 模式下，直接使用 HTTP/1.1 客户端，不使用连接池
        if self.protocol_mode == ClientProtocolMode::Http1Only {
            // 直接使用 HTTP/1.1 客户端发送请求
            return self.send_http1_request_direct(method, uri, body, headers).await;
        }
        
        // 其他模式使用连接池处理HTTP/1.1请求
        let connection = self.connection_pool.get_connection(&uri).await?;
        let connection_id = connection.connection_id.clone();
        
        // 使用 RAII 模式确保连接在任何情况下都能正确释放
        let _connection_guard = ConnectionGuard {
            pool: &self.connection_pool,
            connection_id: &connection_id,
        };
        
        // 使用连接池提供的H2连接发送请求
        // 注意：这里我们统一使用H2协议，连接池负责协议协商
        let mut h2_client = connection.send_request.clone();
        
        // 构建 HTTP/2 请求（连接池统一使用H2）
        let mut request_builder = Request::builder()
            .method(method.clone())
            .uri(uri.clone())
            .version(Version::HTTP_2)
            .header(USER_AGENT, &self.user_agent);

        // 添加压缩协商头部
        if self.enable_compression {
            let accept_encoding = "lz4, zstd, gzip, deflate";
            request_builder = request_builder.header(ACCEPT_ENCODING, accept_encoding);
        }

        // 添加自定义请求头
        if let Some(headers) = headers {
            for (name, value) in headers.iter() {
                request_builder = request_builder.header(name, value);
            }
        }

        let body_data = body.unwrap_or_default();
        let request = request_builder
            .body(())
            .map_err(|e| RatError::RequestError(format!("构建请求失败: {}", e)))?;

        // 发送请求
        let (response, mut send_stream) = h2_client.send_request(request, false)
            .map_err(|e| RatError::NetworkError(format!("发送请求失败: {}", e)))?;

        // 发送请求体
        if !body_data.is_empty() {
            send_stream.send_data(body_data, true)
                .map_err(|e| RatError::NetworkError(format!("发送请求体失败: {}", e)))?;
        } else {
            send_stream.send_data(Bytes::new(), true)
                .map_err(|e| RatError::NetworkError(format!("结束请求流失败: {}", e)))?;
        }

        // 等待响应
        let response = timeout(self.request_timeout, response)
            .await
            .map_err(|_| RatError::TimeoutError(format!("请求超时: {} {}", method, uri)))?
            .map_err(|e| RatError::NetworkError(format!("接收响应失败: {}", e)))?;

        let status = response.status();
        let headers = response.headers().clone();

        // 读取响应体
        let mut body_stream = response.into_body();
        let mut body_data = Vec::new();
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.map_err(|e| RatError::NetworkError(format!("读取响应体失败: {}", e)))?;
            body_data.extend_from_slice(&chunk);
        }
        let body_bytes = Bytes::from(body_data);

        // 自动解压缩响应体
        let decompressed_body = self.decompress_response_body(body_bytes.clone(), headers.get(CONTENT_ENCODING))?;

        debug!("📥 收到响应: {} {} - 状态码: {}, 原始大小: {} bytes, 解压后大小: {} bytes", 
                          method, uri, status, body_bytes.len(), decompressed_body.len());

        Ok(RatHttpResponse {
            status,
            headers,
            body: decompressed_body,
            original_size: body_bytes.len(),
        })
        // ConnectionGuard 在这里自动释放连接
    }

    /// 发送 HTTP/1.1 请求（已弃用，使用连接池统一处理）
    #[deprecated(note = "使用 send_http1_request_via_pool 替代")]
    async fn send_http1_request(&self, request: Request<Full<Bytes>>) -> RatResult<RatHttpResponse> {
        let uri = request.uri().clone();
        let method = request.method().clone();
        
        // 提取请求体和头部
        let (parts, body) = request.into_parts();
        let body_bytes = body.collect().await
            .map_err(|e| RatError::RequestError(format!("读取请求体失败: {}", e)))?
            .to_bytes();
        
        let body = if body_bytes.is_empty() { None } else { Some(body_bytes) };
        let headers = if parts.headers.is_empty() { None } else { Some(parts.headers) };
        
        // 委托给连接池处理
        self.send_http1_request_via_pool(method, uri, body, headers).await
    }

    /// 使用委托模式发送HTTP请求
    pub async fn send_request_delegated<H>(
        &self,
        method: Method,
        uri: Uri,
        headers: Option<HeaderMap>,
        body: Option<Bytes>,
        handler: H,
    ) -> Result<u64, RatError>
    where
        H: HttpRequestHandler + Send + Sync + 'static,
    {
        let manager = self.delegated_manager.read()
            .map_err(|_| RatError::NetworkError("无法获取委托管理器锁".to_string()))?
            .clone();
        
        manager.send_request_delegated(
            method,
            uri,
            headers,
            body,
            Arc::new(handler),
        ).await
    }

    /// 通过连接池发送 HTTP/2 请求
    async fn send_http2_request_via_pool(
        &self,
        method: Method,
        uri: Uri,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
        use_tls: bool,
    ) -> RatResult<RatHttpResponse> {
        debug!("[客户端] 🔗 HTTP/2 请求委托给连接池处理: {} {}", method, uri);

        // 委托给连接池处理HTTP/2请求
        let connection = self.connection_pool.get_connection(&uri).await?;
        let connection_id = connection.connection_id.clone();
        
        // 使用 RAII 模式确保连接在任何情况下都能正确释放
        let _connection_guard = ConnectionGuard {
            pool: &self.connection_pool,
            connection_id: &connection_id,
        };
        
        let mut h2_client = connection.send_request.clone();

        // 构建 HTTP/2 请求
        let mut request_builder = Request::builder()
            .method(method.clone())
            .uri(uri.clone())
            .version(Version::HTTP_2)
            .header(USER_AGENT, &self.user_agent);

        // 添加压缩协商头部
        if self.enable_compression {
            let accept_encoding = "lz4, zstd, gzip, deflate";
            request_builder = request_builder.header(ACCEPT_ENCODING, accept_encoding);
        }

        // 添加自定义请求头
        if let Some(headers) = headers {
            for (name, value) in headers.iter() {
                request_builder = request_builder.header(name, value);
            }
        }

        let body_data = body.unwrap_or_default();
        let request = request_builder
            .body(())
            .map_err(|e| RatError::RequestError(format!("构建 HTTP/2 请求失败: {}", e)))?;

        // 发送请求
        let (response, mut send_stream) = h2_client.send_request(request, false)
            .map_err(|e| RatError::NetworkError(format!("HTTP/2 发送请求失败: {}", e)))?;

        // 发送请求体
        if !body_data.is_empty() {
            send_stream.send_data(body_data, true)
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 发送请求体失败: {}", e)))?;
        } else {
            send_stream.send_data(Bytes::new(), true)
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 发送空请求体失败: {}", e)))?;
        }

        // 接收响应
        let response = timeout(self.request_timeout, response)
            .await
            .map_err(|_| RatError::TimeoutError(format!("HTTP/2 响应超时: {} {}", method, uri)))?
            .map_err(|e| RatError::NetworkError(format!("HTTP/2 接收响应失败: {}", e)))?;

        let status = response.status();
        let headers = response.headers().clone();
        let mut recv_stream = response.into_body();

        // 读取响应体
        let mut body_data = Vec::new();
        while let Some(chunk) = recv_stream.data().await {
            let chunk = chunk.map_err(|e| RatError::NetworkError(format!("HTTP/2 读取响应块失败: {}", e)))?;
            body_data.extend_from_slice(&chunk);
            recv_stream.flow_control().release_capacity(chunk.len())
                .map_err(|e| RatError::NetworkError(format!("HTTP/2 流量控制失败: {}", e)))?;
        }

        let body_bytes = Bytes::from(body_data);

        // 自动解压缩响应体
        let decompressed_body = self.decompress_response_body(body_bytes.clone(), headers.get(CONTENT_ENCODING))?;

        debug!("📥 收到 HTTP/2 响应: {} {} - 状态码: {}, 原始大小: {} bytes, 解压后大小: {} bytes", 
                          method, uri, status, body_bytes.len(), decompressed_body.len());

        Ok(RatHttpResponse {
            status,
            headers,
            body: decompressed_body,
            original_size: body_bytes.len(),
        })
        // ConnectionGuard 在这里自动释放连接
    }

    /// 发送 HTTP/2 请求（已弃用，使用连接池统一处理）
    #[deprecated(note = "使用 send_http2_request_via_pool 替代")]
    async fn send_http2_request(
        &self,
        method: Method,
        uri: Uri,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
        use_tls: bool,
    ) -> RatResult<RatHttpResponse> {
        // 委托给连接池处理
        self.send_http2_request_via_pool(method, uri, body, headers, use_tls).await
    }

    /// 解压缩响应体
    fn decompress_response_body(&self, body: Bytes, encoding: Option<&HeaderValue>) -> RatResult<Bytes> {
        if !self.enable_compression {
            return Ok(body);
        }

        let encoding = match encoding {
            Some(value) => match value.to_str() {
                Ok(s) => s,
                Err(_) => return Ok(body), // 无法解析编码，返回原始数据
            },
            None => return Ok(body), // 没有编码头，返回原始数据
        };

        match encoding.to_lowercase().as_str() {
            "lz4" => {
                self.decompress_lz4(body)
            },
            "zstd" => {
                self.decompress_zstd(body)
            },
            "gzip" => {
                self.decompress_gzip(body)
            },
            "deflate" => {
                self.decompress_deflate(body)
            },
            _ => Ok(body), // 未知编码，返回原始数据
        }
    }

    /// LZ4 解压缩
    fn decompress_lz4(&self, data: Bytes) -> RatResult<Bytes> {
        #[cfg(feature = "compression")]
        {
            let decompressed = lz4_flex::block::decompress(&data, data.len() * 4)
                .map_err(|e| RatError::DecodingError(format!("LZ4 解压缩失败: {}", e)))?;
            Ok(Bytes::from(decompressed))
        }
        #[cfg(not(feature = "compression"))]
        {
            Err(RatError::DecodingError("LZ4 压缩功能未启用".to_string()))
        }
    }

    /// Zstd 解压缩
    fn decompress_zstd(&self, data: Bytes) -> RatResult<Bytes> {
        #[cfg(feature = "compression-zstd")]
        {
            let decompressed = zstd::bulk::decompress(&data, 1024 * 1024) // 1MB 最大解压缩大小
                .map_err(|e| RatError::DecodingError(format!("Zstd 解压缩失败: {}", e)))?;
            Ok(Bytes::from(decompressed))
        }
        #[cfg(not(feature = "compression-zstd"))]
        {
            Err(RatError::DecodingError("Zstd 压缩功能未启用".to_string()))
        }
    }

    /// Gzip 解压缩
    fn decompress_gzip(&self, data: Bytes) -> RatResult<Bytes> {
        #[cfg(feature = "compression")]
        {
            use flate2::read::GzDecoder;
            use std::io::Read;

            let mut decoder = GzDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)
                .map_err(|e| RatError::DecodingError(format!("Gzip 解压缩失败: {}", e)))?;
            Ok(Bytes::from(decompressed))
        }
        #[cfg(not(feature = "compression"))]
        {
            Err(RatError::DecodingError("Gzip 压缩功能未启用".to_string()))
        }
    }

    /// Deflate 解压缩
    fn decompress_deflate(&self, data: Bytes) -> RatResult<Bytes> {
        #[cfg(feature = "compression")]
        {
            use flate2::read::DeflateDecoder;
            use std::io::Read;

            let mut decoder = DeflateDecoder::new(&data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)
                .map_err(|e| RatError::DecodingError(format!("Deflate 解压缩失败: {}", e)))?;
            Ok(Bytes::from(decompressed))
        }
        #[cfg(not(feature = "compression"))]
        {
            Err(RatError::DecodingError("Deflate 压缩功能未启用".to_string()))
        }
    }


}

/// HTTP/1.1 响应体流适配器，将 hyper::body::Incoming 转换为 Stream<Item = RatResult<Bytes>>
struct BodyStreamAdapter {
    body: Incoming,
}

impl BodyStreamAdapter {
    fn new(body: Incoming) -> Self {
        Self { body }
    }
}

impl Stream for BodyStreamAdapter {
    type Item = RatResult<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use http_body_util::BodyExt;
        
        match Pin::new(&mut self.body).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(chunk) = frame.data_ref() {
                    Poll::Ready(Some(Ok(chunk.clone())))
                } else {
                    // 跳过非数据帧，继续读取下一帧
                    self.poll_next(cx)
                }
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Some(Err(RatError::NetworkError(format!("读取流式数据失败: {}", e)))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// HTTP/2 流适配器，将 h2::RecvStream 转换为 Stream<Item = RatResult<Bytes>>
struct H2StreamAdapter {
    recv_stream: RecvStream,
}

impl H2StreamAdapter {
    fn new(recv_stream: RecvStream) -> Self {
        Self { recv_stream }
    }
}

impl Stream for H2StreamAdapter {
    type Item = RatResult<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use h2::RecvStream;
        use futures_util::ready;
        
        match ready!(self.recv_stream.poll_data(cx)) {
            Some(Ok(chunk)) => {
                // 释放流量控制
                if let Err(e) = self.recv_stream.flow_control().release_capacity(chunk.len()) {
                    return Poll::Ready(Some(Err(RatError::NetworkError(format!("HTTP/2 流量控制失败: {}", e)))));
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Some(Err(e)) => {
                Poll::Ready(Some(Err(RatError::NetworkError(format!("HTTP/2 流读取失败: {}", e)))))
            }
            None => Poll::Ready(None),
        }
    }
}

impl std::fmt::Debug for RatHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RatHttpClient")
            .field("http1_client", &"<hyper::Client>")
            .field("h2_connections", &format!("DashMap with {} connections", {
                #[allow(deprecated)]
                self.h2_connections.len()
            }))
            .field("tls_connector", &"<TlsConnector>")
            .field("request_timeout", &self.request_timeout)
            .field("user_agent", &self.user_agent)
            .field("compression_config", &self.compression_config)
            .field("enable_compression", &self.enable_compression)
            .field("default_protocol", &self.default_protocol)
            .field("protocol_mode", &self.protocol_mode)
            .finish()
    }
}

/// RAT Engine HTTP 响应
/// 
/// 封装 HTTP 响应的状态码、头部和响应体
#[derive(Debug)]
pub struct RatHttpResponse {
    /// HTTP 状态码
    pub status: StatusCode,
    /// 响应头
    pub headers: HeaderMap,
    /// 响应体（已解压）
    pub body: Bytes,
    /// 原始响应大小（压缩后的大小，字节）
    pub original_size: usize,
}

/// RAT Engine HTTP 流式响应
/// 
/// 封装 HTTP 流式响应的状态码、头部和响应体流
pub struct RatHttpStreamResponse {
    /// HTTP 状态码
    pub status: StatusCode,
    /// 响应头
    pub headers: HeaderMap,
    /// 响应体流
    pub body: Pin<Box<dyn Stream<Item = RatResult<Bytes>> + Send>>,
}

impl std::fmt::Debug for RatHttpStreamResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RatHttpStreamResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &"<Stream>")
            .finish()
    }
}

/// RAII 连接守卫，确保连接在任何情况下都能正确释放
struct ConnectionGuard<'a> {
    pool: &'a ClientConnectionPool,
    connection_id: &'a str,
}

impl<'a> Drop for ConnectionGuard<'a> {
    fn drop(&mut self) {
        self.pool.release_connection(self.connection_id);
    }
}

impl RatHttpResponse {
    /// 检查响应是否成功（状态码 2xx）
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// 获取响应体的字符串表示
    pub fn text(&self) -> RatResult<String> {
        String::from_utf8(self.body.to_vec())
            .map_err(|e| RatError::DecodingError(format!("响应体不是有效的 UTF-8: {}", e)))
    }

    /// 将响应体反序列化为 JSON
    pub fn json<T>(&self) -> RatResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(&self.body)
            .map_err(|e| RatError::DeserializationError(format!("JSON 反序列化失败: {}", e)))
    }

    /// 获取响应体的字节数据
    pub fn bytes(&self) -> &Bytes {
        &self.body
    }

    /// 获取指定响应头的值
    pub fn header(&self, name: &HeaderName) -> Option<&HeaderValue> {
        self.headers.get(name)
    }

    /// 获取 Content-Type 头
    pub fn content_type(&self) -> Option<&HeaderValue> {
        self.header(&CONTENT_TYPE)
    }

    /// 打印响应数据用于调试
    /// 
    /// 输出响应的状态码、头部信息和响应体内容
    /// 对于二进制数据，只显示前 200 字节的十六进制表示
    pub fn debug_print(&self) {
        info!("📊 HTTP 响应调试信息:");
        info!("   状态码: {}", self.status);
        info!("   响应体大小: {} bytes", self.body.len());
        
        // 打印重要的响应头
        info!("   重要响应头:");
        if let Some(content_type) = self.content_type() {
            info!("     Content-Type: {:?}", content_type);
        }
        if let Some(content_length) = self.header(&CONTENT_LENGTH) {
            info!("     Content-Length: {:?}", content_length);
        }
        if let Some(content_encoding) = self.header(&CONTENT_ENCODING) {
            info!("     Content-Encoding: {:?}", content_encoding);
        }
        
        // 打印响应体内容
        if self.body.is_empty() {
            warn!("   ⚠️  响应体为空！");
        } else {
            // 尝试将响应体解析为文本
            match String::from_utf8(self.body.to_vec()) {
                Ok(text) => {
                    if text.len() <= 500 {
                        info!("   响应体内容: {}", text);
                    } else {
                        info!("   响应体内容（前500字符）: {}", &text[..500]);
                        info!("   ... (总共 {} 字符)", text.len());
                    }
                }
                Err(_) => {
                    // 二进制数据，显示十六进制
                    let hex_data = if self.body.len() <= 200 {
                        hex::encode(&self.body)
                    } else {
                        format!("{}... (总共 {} bytes)", hex::encode(&self.body[..200]), self.body.len())
                    };
                    info!("   响应体内容（十六进制）: {}", hex_data);
                }
            }
        }
    }
}