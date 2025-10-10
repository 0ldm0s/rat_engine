//! RAT Engine gRPC+Bincode 客户端构建器
//! 
//! 严格遵循项目规范，要求所有配置项必须显式设置

use std::time::Duration;
use hyper::{Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::Full;
use hyper::body::Bytes;
use crate::error::{RatError, RatResult};
use crate::client::grpc_client::{RatGrpcClient, GrpcCompressionMode};
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use std::sync::Arc;

/// mTLS 客户端配置
#[derive(Debug)]
pub struct MtlsClientConfig {
    /// 客户端证书链
    pub client_cert_chain: Vec<Vec<u8>>,
    /// 客户端私钥
    pub client_private_key: Vec<u8>,
    /// 自定义 CA 证书（可选，用于验证服务器证书）
    pub ca_certs: Option<Vec<Vec<u8>>>,
    /// 是否跳过服务器证书验证（仅开发模式）
    pub skip_server_verification: bool,
    /// 服务器名称（用于 SNI）
    pub server_name: Option<String>,
    /// 客户端证书文件路径（用于调试日志）
    pub client_cert_path: Option<String>,
    /// 客户端私钥文件路径（用于调试日志）
    pub client_key_path: Option<String>,
    /// CA 证书文件路径（用于调试日志）
    pub ca_cert_path: Option<String>,
}

impl Clone for MtlsClientConfig {
    fn clone(&self) -> Self {
        Self {
            client_cert_chain: self.client_cert_chain.clone(),
            client_private_key: self.client_private_key.clone(),
            ca_certs: self.ca_certs.clone(),
            skip_server_verification: self.skip_server_verification,
            server_name: self.server_name.clone(),
            client_cert_path: self.client_cert_path.clone(),
            client_key_path: self.client_key_path.clone(),
            ca_cert_path: self.ca_cert_path.clone(),
        }
    }
}


/// RAT Engine gRPC+Bincode 客户端构建器
/// 
/// 严格遵循项目规范，要求所有配置项必须显式设置
/// 不提供任何默认值，确保配置的明确性和可控性
#[derive(Debug)]
pub struct RatGrpcClientBuilder {
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
    /// 压缩模式
    compression_mode: Option<GrpcCompressionMode>,
    /// 是否为开发模式（跳过证书验证）
    /// 警告：仅用于开发环境，生产环境必须设置为 false
    development_mode: Option<bool>,
    /// mTLS 配置
    mtls_config: Option<MtlsClientConfig>,
}

impl RatGrpcClientBuilder {
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
            compression_mode: None,
            development_mode: None,
            mtls_config: None,
        }
    }



    /// 设置连接超时时间
    /// 
    /// # 参数
    /// * `timeout` - 连接超时时间，必须在 1-30 秒之间
    pub fn connect_timeout(mut self, timeout: Duration) -> RatResult<Self> {
        if timeout.as_secs() < 1 || timeout.as_secs() > 30 {
            return Err(RatError::RequestError("连接超时时间必须在 1-30 秒之间".to_string()));
        }
        
        self.connect_timeout = Some(timeout);
        Ok(self)
    }

    /// 设置请求超时时间
    /// 
    /// # 参数
    /// * `timeout` - 请求超时时间，必须在 1-300 秒之间
    pub fn request_timeout(mut self, timeout: Duration) -> RatResult<Self> {
        if timeout.as_secs() < 1 || timeout.as_secs() > 300 {
            return Err(RatError::RequestError("请求超时时间必须在 1-300 秒之间".to_string()));
        }
        
        self.request_timeout = Some(timeout);
        Ok(self)
    }

    /// 设置最大空闲连接数
    /// 
    /// # 参数
    /// * `max_connections` - 最大空闲连接数，必须在 1-100 之间
    pub fn max_idle_connections(mut self, max_connections: usize) -> RatResult<Self> {
        if max_connections < 1 || max_connections > 100 {
            return Err(RatError::RequestError("最大空闲连接数必须在 1-100 之间".to_string()));
        }
        
        self.max_idle_connections = Some(max_connections);
        Ok(self)
    }

    /// 启用仅 HTTP/2 模式
    /// 
    /// 启用后客户端将仅使用 HTTP/2 协议进行通信
    pub fn http2_only(mut self) -> Self {
        self.http2_only = Some(true);
        self
    }

    /// 启用 HTTP/1.1 和 HTTP/2 混合模式
    /// 
    /// 客户端将根据服务器支持情况自动选择协议版本
    pub fn http_mixed(mut self) -> Self {
        self.http2_only = Some(false);
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

    /// 启用 LZ4 压缩
    /// 
    /// 启用后客户端将使用 LZ4 算法压缩请求和响应数据
    pub fn enable_lz4_compression(mut self) -> Self {
        self.compression_mode = Some(GrpcCompressionMode::Lz4);
        self
    }

    /// 禁用压缩（默认）
    /// 
    /// 客户端将不使用任何压缩算法
    pub fn disable_compression(mut self) -> Self {
        self.compression_mode = Some(GrpcCompressionMode::Disabled);
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

    /// 设置开发模式状态
    /// 
    /// # 参数
    /// * `enabled` - 是否启用开发模式
    pub fn with_development_mode(mut self, enabled: bool) -> RatResult<Self> {
        self.development_mode = Some(enabled);
        Ok(self)
    }

    
    /// 配置 mTLS 客户端认证
    ///
    /// 启用双向 TLS 认证，客户端将提供证书给服务器验证
    ///
    /// # 参数
    /// - `client_cert_chain`: 客户端证书链
    /// - `client_private_key`: 客户端私钥
    /// - `ca_certs`: 可选的 CA 证书，用于验证服务器证书
    /// - `skip_server_verification`: 是否跳过服务器证书验证（仅开发模式）
    /// - `server_name`: 可选的服务器名称，用于 SNI
    /// - `client_cert_path`: 客户端证书文件路径（用于调试日志）
    /// - `client_key_path`: 客户端私钥文件路径（用于调试日志）
    /// - `ca_cert_path`: CA 证书文件路径（用于调试日志）
    ///
    /// # 返回值
    /// - RatResult<Self>: 成功返回构建器实例，失败返回错误
    pub fn with_mtls(
        mut self,
        client_cert_chain: Vec<Vec<u8>>,
        client_private_key: Vec<u8>,
        ca_certs: Option<Vec<Vec<u8>>>,
        skip_server_verification: bool,
        server_name: Option<String>,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
        ca_cert_path: Option<String>,
    ) -> RatResult<Self> {
        if client_cert_chain.is_empty() {
            return Err(RatError::RequestError("客户端证书链不能为空".to_string()));
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

    /// 使用自签名证书配置 mTLS（开发模式）
    ///
    /// 便捷方法，自动跳过服务器证书验证，适用于开发环境
    ///
    /// # 参数
    /// - `client_cert_chain`: 客户端证书链
    /// - `client_private_key`: 客户端私钥
    /// - `server_name`: 可选的服务器名称
    /// - `client_cert_path`: 客户端证书文件路径（用于调试日志）
    /// - `client_key_path`: 客户端私钥文件路径（用于调试日志）
    ///
    /// # 返回值
    /// - RatResult<Self>: 成功返回构建器实例，失败返回错误
    pub fn with_self_signed_mtls(
        mut self,
        client_cert_chain: Vec<Vec<u8>>,
        client_private_key: Vec<u8>,
        server_name: Option<String>,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
    ) -> RatResult<Self> {
        self.with_mtls(client_cert_chain, client_private_key, None, true, server_name, client_cert_path, client_key_path, None)
    }

    /// 构建 gRPC 客户端实例
    /// 
    /// # 错误
    /// 如果任何必需的配置项未设置，将返回错误
    /// 
    /// # 必需配置项
    /// - base_uri: 服务器基础 URI
    /// - connect_timeout: 连接超时时间
    /// - request_timeout: 请求超时时间
    /// - max_idle_connections: 最大空闲连接数
    /// - http2_only: HTTP 协议模式
    /// - user_agent: 用户代理字符串
    /// - compression_mode: 压缩模式
    pub fn build(self) -> RatResult<RatGrpcClient> {
        // 验证所有必需配置项
        // 不再需要 base_uri，因为现在在每次请求时传入完整的 URI
        
        let connect_timeout = self.connect_timeout
            .ok_or_else(|| RatError::RequestError("连接超时时间未设置".to_string()))?;
        
        let request_timeout = self.request_timeout
            .ok_or_else(|| RatError::RequestError("请求超时时间未设置".to_string()))?;
        
        let max_idle_connections = self.max_idle_connections
            .ok_or_else(|| RatError::RequestError("最大空闲连接数未设置".to_string()))?;
        
        let http2_only = self.http2_only
            .ok_or_else(|| RatError::RequestError("HTTP 协议模式未设置".to_string()))?;
        
        let user_agent = self.user_agent
            .ok_or_else(|| RatError::RequestError("用户代理字符串未设置".to_string()))?;

        let compression_mode = self.compression_mode
            .ok_or_else(|| RatError::RequestError("压缩模式未设置".to_string()))?;

        let development_mode = self.development_mode
            .ok_or_else(|| RatError::RequestError("开发模式未设置".to_string()))?;

        // 创建连接器
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(connect_timeout));
        
        // 创建客户端构建器
        let mut client_builder = Client::builder(TokioExecutor::new());
        
        // 配置 HTTP/2
        let client = if http2_only {
            client_builder
                .http2_only(true)
                .build(connector)
        } else {
            client_builder
                .build(connector)
        };

        // 创建默认的压缩配置
        let compression_config = {
            #[cfg(feature = "compression")]
            {
                crate::compression::CompressionConfig::new()
                    .enable_compression(compression_mode != GrpcCompressionMode::Disabled)
                    .min_size(1024)
                    .level(1)
            }
            #[cfg(not(feature = "compression"))]
            {
                crate::compression::CompressionConfig::new()
                    .enable_compression(false)
            }
        };

        Ok(RatGrpcClient::new(
            client,
            connect_timeout,
            request_timeout,
            max_idle_connections,
            user_agent,
            compression_config,
            compression_mode != GrpcCompressionMode::Disabled, // enable_compression
            false, // enable_retry
            3, // max_retries
            compression_mode,
            development_mode,
            self.mtls_config,
        ))
    }
}

impl Default for RatGrpcClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}