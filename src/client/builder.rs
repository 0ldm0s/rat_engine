//! RAT Engine HTTP 客户端构建器
//! 
//! 提供构建器模式的客户端配置，严格遵循项目的构建器规范

use std::time::Duration;
use std::sync::Arc;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::Full;
use hyper::body::Bytes;
use crate::error::{RatError, RatResult};
use crate::client::http_client::RatHttpClient;
#[cfg(feature = "compression")]
use crate::compression::CompressionConfig;
use crate::client::grpc_builder::MtlsClientConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// 客户端协议模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClientProtocolMode {
    /// 自动模式：根据 http2_only 设置自动选择
    Auto,
    /// 纯 HTTP/1.1 模式（禁用所有 HTTP/2）
    Http1Only,
    /// HTTP/1.1 + HTTP/2 over TLS（禁用 H2C）
    Http1WithHttp2Tls,
}

/// RAT Engine HTTP 客户端构建器
/// 
/// 严格遵循项目规范，要求所有配置项必须显式设置
/// 不提供任何默认值，确保配置的明确性和可控性
#[derive(Debug)]
pub struct RatHttpClientBuilder {
    /// 连接超时时间
    connect_timeout: Option<Duration>,
    /// 请求超时时间
    request_timeout: Option<Duration>,
    /// 最大空闲连接数
    max_idle_connections: Option<usize>,
    /// 是否仅使用 HTTP/2
    http2_only: Option<bool>,
    /// 用户代理字符串
    user_agent: Option<String>,
    /// 压缩配置
    #[cfg(feature = "compression")]
    compression_config: Option<CompressionConfig>,
    #[cfg(not(feature = "compression"))]
    compression_config: Option<()>,
    /// 是否启用压缩
    enable_compression: Option<bool>,
    
    /// 是否为开发模式（跳过证书验证）
    /// 警告：仅用于开发环境，生产环境必须设置为 false
    development_mode: Option<bool>,
    
    /// mTLS 配置
    mtls_config: Option<MtlsClientConfig>,
    
    /// 协议模式配置
    protocol_mode: ClientProtocolMode,
}

impl RatHttpClientBuilder {
    /// 创建新的构建器实例
    /// 
    /// 所有配置项初始为 None，必须通过相应方法显式设置
    pub fn new() -> Self {
        Self {
            connect_timeout: None,
            request_timeout: None,
            max_idle_connections: None,
            http2_only: None,
            user_agent: None,
            #[cfg(feature = "compression")]
            compression_config: None,
            #[cfg(not(feature = "compression"))]
            compression_config: None,
            enable_compression: None,
            development_mode: None,
            mtls_config: None,
            protocol_mode: ClientProtocolMode::Auto,
        }
    }

    /// 设置连接超时时间
    /// 
    /// # 参数
    /// * `timeout` - 连接超时时间，建议范围：1-30秒
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// 设置请求超时时间
    /// 
    /// # 参数
    /// * `timeout` - 请求超时时间，建议范围：5-300秒
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// 设置每个主机的最大空闲连接数
    /// 
    /// # 参数
    /// * `max_idle` - 最大空闲连接数，建议范围：1-100
    pub fn max_idle_per_host(mut self, max_idle: usize) -> Self {
        self.max_idle_connections = Some(max_idle);
        self
    }

    /// 设置是否仅使用 HTTP/2
    /// 
    /// # 参数
    /// * `http2_only` - true 表示仅使用 HTTP/2，false 表示支持 HTTP/1.1 和 HTTP/2
    pub fn http2_only(mut self, http2_only: bool) -> Self {
        self.http2_only = Some(http2_only);
        self
    }

    /// 启用纯 HTTP/1.1 模式（禁用 H2C）
    /// 
    /// 此模式下：
    /// - 对于 `http://` URL，强制使用 HTTP/1.1
    /// - 对于 `https://` URL，仍然支持 HTTP/2 over TLS
    /// - 完全禁用 HTTP/2 Cleartext (H2C)
    /// 
    /// 适用于需要与不支持 H2C 的服务器兼容的场景
    pub fn http1_only_for_cleartext(mut self) -> Self {
        self.http2_only = Some(false);
        self.protocol_mode = ClientProtocolMode::Http1WithHttp2Tls;
        self
    }

    /// 启用完全的 HTTP/1.1 模式
    /// 
    /// 此模式下：
    /// - 所有请求都使用 HTTP/1.1
    /// - 禁用 HTTP/2 over TLS
    /// - 禁用 HTTP/2 Cleartext (H2C)
    /// 
    /// 适用于需要完全兼容 HTTP/1.1 的场景
    pub fn http1_only(mut self) -> Self {
        self.http2_only = Some(false);
        self.protocol_mode = ClientProtocolMode::Http1Only;
        self
    }

    /// 设置用户代理字符串
    /// 
    /// # 参数
    /// * `user_agent` - 用户代理字符串，不能为空
    /// 
    /// # 验证规则
    /// - 不能为空字符串
    /// - 长度不能超过 200 个字符
    /// - 不能包含控制字符
    pub fn user_agent<S: Into<String>>(mut self, user_agent: S) -> RatResult<Self> {
        let user_agent = user_agent.into();
        
        // 验证用户代理字符串
        if user_agent.is_empty() {
            return Err(RatError::RequestError("用户代理字符串不能为空".to_string()));
        }
        
        if user_agent.len() > 200 {
            return Err(RatError::RequestError("用户代理字符串长度不能超过 200 个字符".to_string()));
        }
        
        if user_agent.chars().any(|c| c.is_control()) {
            return Err(RatError::RequestError("用户代理字符串不能包含控制字符".to_string()));
        }
        
        self.user_agent = Some(user_agent);
        Ok(self)
    }

    /// 获取默认用户代理字符串
    /// 
    /// 格式：RAT-Engine-Client/{version}
    /// 版本号从 Cargo.toml 中读取
    pub fn default_user_agent() -> String {
        format!("RAT-Engine-Client/{}", env!("CARGO_PKG_VERSION"))
    }

    /// 设置压缩配置
    /// 
    /// # 参数
    /// * `config` - 压缩配置实例
    #[cfg(feature = "compression")]
    pub fn compression_config(mut self, config: CompressionConfig) -> Self {
        self.compression_config = Some(config);
        self
    }

    /// 设置压缩配置（无压缩特性时的空实现）
    #[cfg(not(feature = "compression"))]
    pub fn compression_config(mut self, _config: ()) -> Self {
        self
    }

    /// 启用压缩功能
    /// 
    /// 启用后客户端将自动协商压缩算法（优先级：lz4 > zstd > gzip > deflate）
    /// 并自动解压缩响应体
    pub fn enable_compression(mut self) -> Self {
        self.enable_compression = Some(true);
        self
    }

    /// 禁用压缩功能
    pub fn disable_compression(mut self) -> Self {
        self.enable_compression = Some(false);
        self
    }

    /// 启用开发模式（跳过证书验证）
    /// 
    /// **警告：此模式会跳过所有 TLS 证书验证，仅用于开发和测试环境！**
    /// **生产环境绝对不能使用此模式！**
    /// 
    /// 开发模式下将：
    /// - 跳过 TLS 证书验证
    /// - 接受自签名证书
    /// - 接受过期证书
    /// - 接受主机名不匹配的证书
    pub fn development_mode(mut self) -> Self {
        self.development_mode = Some(true);
        self
    }

    
    /// 配置 mTLS 双向认证
    /// 
    /// # 参数
    /// * `client_cert_chain` - 客户端证书链
    /// * `client_private_key` - 客户端私钥
    /// * `ca_certs` - 自定义 CA 证书（可选）
    /// * `skip_server_verification` - 是否跳过服务器证书验证（仅开发模式）
    /// * `server_name` - 服务器名称（用于 SNI）
    /// * `client_cert_path` - 客户端证书文件路径（用于调试日志）
    /// * `client_key_path` - 客户端私钥文件路径（用于调试日志）
    /// * `ca_cert_path` - CA 证书文件路径（用于调试日志）
    /// 
    /// # 错误
    /// 如果证书链为空或私钥无效，将返回配置错误
    pub fn with_mtls(
        mut self,
        client_cert_chain: Vec<CertificateDer<'static>>,
        client_private_key: PrivateKeyDer<'static>,
        ca_certs: Option<Vec<CertificateDer<'static>>>,
        skip_server_verification: bool,
        server_name: Option<String>,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
        ca_cert_path: Option<String>,
    ) -> RatResult<Self> {
        if client_cert_chain.is_empty() {
            return Err(RatError::ConfigError("客户端证书链不能为空".to_string()));
        }

        // 验证服务器名称格式（如果提供）
        if let Some(ref name) = server_name {
            if name.trim().is_empty() {
                return Err(RatError::ConfigError("服务器名称不能为空".to_string()));
            }
        }

        self.mtls_config = Some(MtlsClientConfig {
            client_cert_chain,
            client_private_key,
            ca_certs,
            skip_server_verification,
            server_name,
            client_cert_path,
            client_key_path,
            ca_cert_path,
        });

        Ok(self)
    }

    /// 配置自签名 mTLS（开发模式）
    /// 
    /// 这是 `with_mtls` 的简化版本，自动跳过服务器证书验证
    /// 
    /// # 参数
    /// * `client_cert_chain` - 客户端证书链
    /// * `client_private_key` - 客户端私钥
    /// * `server_name` - 服务器名称（用于 SNI）
    /// * `client_cert_path` - 客户端证书文件路径（用于调试日志）
    /// * `client_key_path` - 客户端私钥文件路径（用于调试日志）
    pub fn with_self_signed_mtls(
        mut self,
        client_cert_chain: Vec<CertificateDer<'static>>,
        client_private_key: PrivateKeyDer<'static>,
        server_name: Option<String>,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
    ) -> RatResult<Self> {
        self.with_mtls(
            client_cert_chain,
            client_private_key,
            None, // 不使用自定义 CA
            true, // 跳过服务器验证
            server_name,
            client_cert_path,
            client_key_path,
            None, // 自签名模式不使用 CA 证书路径
        )
    }

    /// 构建 HTTP 客户端实例
    /// 
    /// # 错误
    /// 如果任何必需的配置项未设置，将返回 `RatError::ConfigError`
    /// 
    /// # 验证规则
    /// - 连接超时时间必须在 1-30 秒之间
    /// - 请求超时时间必须在 5-300 秒之间
    /// - 最大空闲连接数必须在 1-100 之间
    /// - 用户代理字符串不能为空
    pub fn build(self) -> RatResult<RatHttpClient> {
        // 验证所有必需配置项
        let connect_timeout = self.connect_timeout
            .ok_or_else(|| RatError::ConfigError("连接超时时间未设置".to_string()))?;
        
        let request_timeout = self.request_timeout
            .ok_or_else(|| RatError::ConfigError("请求超时时间未设置".to_string()))?;
        
        let max_idle_connections = self.max_idle_connections
            .ok_or_else(|| RatError::ConfigError("最大空闲连接数未设置".to_string()))?;
        
        let http2_only = self.http2_only
            .ok_or_else(|| RatError::ConfigError("HTTP/2 设置未指定".to_string()))?;
        
        let user_agent = self.user_agent
            .unwrap_or_else(|| Self::default_user_agent());

        let enable_compression = self.enable_compression
            .ok_or_else(|| RatError::ConfigError("压缩模式未设置".to_string()))?;

        let development_mode = self.development_mode.unwrap_or(false);  // 默认不启用开发模式

        // 验证配置项的合法性
        if connect_timeout.as_secs() < 1 || connect_timeout.as_secs() > 30 {
            return Err(RatError::ConfigError("连接超时时间必须在 1-30 秒之间".to_string()));
        }

        if request_timeout.as_secs() < 5 || request_timeout.as_secs() > 300 {
            return Err(RatError::ConfigError("请求超时时间必须在 5-300 秒之间".to_string()));
        }

        if max_idle_connections < 1 || max_idle_connections > 100 {
            return Err(RatError::ConfigError("最大空闲连接数必须在 1-100 之间".to_string()));
        }

        if user_agent.trim().is_empty() {
            return Err(RatError::ConfigError("用户代理字符串不能为空".to_string()));
        }

        // 如果启用压缩但未设置压缩配置，使用默认配置
        #[cfg(feature = "compression")]
        let compression_config = if enable_compression {
            self.compression_config.unwrap_or_else(|| {
                // 创建客户端专用的压缩配置（优先级：lz4 > zstd > gzip > deflate）
                CompressionConfig::new()
                    .enable_compression(true)
                    .min_size(1024) // 小于1KB的不压缩
            })
        } else {
            CompressionConfig::new().enable_compression(false) // 禁用所有压缩
        };

        #[cfg(not(feature = "compression"))]
        let compression_config = ();

        // 创建连接器
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(connect_timeout));
        
        // 在 Http1Only 模式下，强制只使用 HTTP/1.1
        if self.protocol_mode == ClientProtocolMode::Http1Only {
            connector.enforce_http(true); // 强制 HTTP，禁用 HTTPS
        } else {
            connector.enforce_http(false); // 允许 HTTPS
        }

        // 创建 hyper 客户端
        let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
            .http2_only(false)  // 禁用 HTTP/2 only 模式
            .build(connector);

        // 根据协议模式和 http2_only 设置确定默认协议
        let default_protocol = match self.protocol_mode {
            ClientProtocolMode::Http1Only => {
                // 完全的 HTTP/1.1 模式
                crate::client::http_client::HttpProtocol::Http1
            }
            ClientProtocolMode::Http1WithHttp2Tls => {
                // HTTP/1.1 + HTTP/2 over TLS 模式（禁用 H2C）
                crate::client::http_client::HttpProtocol::Http1
            }
            ClientProtocolMode::Auto => {
                // 自动模式：根据 http2_only 设置
                if http2_only {
                    crate::client::http_client::HttpProtocol::Http2
                } else {
                    // 当 http2_only 为 false 时，对于 http:// URL 默认使用 HTTP/1.1
                    // 这样可以确保与不支持 H2C 的服务器兼容
                    crate::client::http_client::HttpProtocol::Http1
                }
            }
        };

        // 创建连接池并启动维护任务
        let pool_config = crate::client::connection_pool::ConnectionPoolConfig {
            max_connections: max_idle_connections * 10, // 总连接数为空闲连接数的10倍
            idle_timeout: Duration::from_secs(60),
            keepalive_interval: Duration::from_secs(30),
            connect_timeout,
            cleanup_interval: Duration::from_secs(30),
            max_connections_per_target: max_idle_connections,
            development_mode,
            mtls_config: self.mtls_config.clone(), // 传递 mTLS 配置
            protocol_mode: self.protocol_mode, // 传递协议模式
        };
        let mut connection_pool = crate::client::connection_pool::ClientConnectionPool::new(pool_config);
        connection_pool.start_maintenance_tasks();
        let connection_pool = Arc::new(connection_pool);
        
        #[cfg(feature = "compression")]
        {
            Ok(RatHttpClient::new(
                client,
                request_timeout,
                user_agent,
                compression_config,
                enable_compression,
                default_protocol,
                self.protocol_mode,
                development_mode,
                connection_pool,
            ))
        }

        #[cfg(not(feature = "compression"))]
        {
            // 当没有压缩特性时，创建一个默认的压缩配置
            Ok(RatHttpClient::new(
                client,
                request_timeout,
                user_agent,
                crate::compression::CompressionConfig::new().enable_compression(false),
                enable_compression,
                default_protocol,
                self.protocol_mode,
                development_mode,
                connection_pool,
            ))
        }
    }
}

impl Default for RatHttpClientBuilder {
    /// 创建默认构建器实例
    /// 
    /// 注意：即使是默认实例，所有配置项仍需显式设置
    fn default() -> Self {
        Self::new()
    }
}