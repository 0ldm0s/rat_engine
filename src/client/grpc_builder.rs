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
use std::sync::Arc;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::collections::HashMap;

/// 默认压缩配置（在compression特性未启用时使用）
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    enabled: bool,
    min_size: usize,
    level: u32,
}

// =============================================================================
// mTLS 客户端配置（rustls 版本）
// =============================================================================

/// mTLS 客户端配置
#[derive(Debug, Clone)]
pub struct MtlsClientConfig {
    /// 客户端证书链（PEM 格式）
    pub client_cert_chain: Vec<Vec<u8>>,
    /// 客户端私钥（PEM 格式）
    pub client_private_key: Vec<u8>,
    /// 自定义 CA 证书（可选，用于验证服务器证书）
    pub ca_certs: Option<Vec<Vec<u8>>>,
    /// 服务器名称（用于 SNI）
    pub server_name: Option<String>,
    /// 客户端证书文件路径（用于调试日志）
    pub client_cert_path: Option<String>,
    /// 客户端私钥文件路径（用于调试日志）
    pub client_key_path: Option<String>,
    /// CA 证书文件路径（用于调试日志）
    pub ca_cert_path: Option<String>,
}

// =============================================================================



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
    /// 是否启用 h2c-over-TLS 模式
    /// 警告：此模式会跳过 TLS 证书验证，仅用于通过 HTTP 代理传输 h2c 流
    h2c_mode: Option<bool>,
    /// mTLS 配置
    mtls_config: Option<MtlsClientConfig>,
    /// DNS解析映射表（域名 -> 预解析IP）
    dns_mapping: Option<std::collections::HashMap<String, String>>,
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
            h2c_mode: None,
            mtls_config: None,
            dns_mapping: None,
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

    /// 启用 h2c-over-TLS 模式（跳过证书验证）
    ///
    /// **警告：此模式会跳过所有 TLS 证书验证，仅用于通过 HTTP 代理传输 h2c 流！**
    ///
    /// h2c-over-TLS 模式下将：
    /// - 跳过 TLS 证书验证
    /// - 接受自签名证书
    /// - 接受过期证书
    /// - 接受主机名不匹配的证书
    pub fn h2c_mode(mut self) -> Self {
        self.h2c_mode = Some(true);
        self
    }

    /// 设置 h2c-over-TLS 模式状态
    ///
    /// # 参数
    /// * `enabled` - 是否启用 h2c-over-TLS 模式
    pub fn with_h2c_mode(mut self, enabled: bool) -> RatResult<Self> {
        self.h2c_mode = Some(enabled);
        Ok(self)
    }


    // =============================================================================
    // mTLS 方法（rustls 版本）
    // =============================================================================

    /// 配置 mTLS 客户端认证（从文件路径加载）
    ///
    /// 启用双向 TLS 认证，客户端将提供证书给服务器验证
    ///
    /// # 参数
    /// - `client_cert_path`: 客户端证书文件路径（PEM 格式）
    /// - `client_key_path`: 客户端私钥文件路径（PEM 格式）
    ///
    /// # 返回值
    /// - RatResult<Self>: 成功返回构建器实例，失败返回错误
    pub fn with_client_certs<S: Into<String>>(
        mut self,
        client_cert_path: S,
        client_key_path: S,
    ) -> RatResult<Self> {
        self.with_client_certs_and_ca(client_cert_path, client_key_path, None::<String>)
    }

    /// 配置 mTLS 客户端认证（包含自定义 CA 证书）
    ///
    /// 启用双向 TLS 认证，客户端将提供证书给服务器验证，
    /// 同时使用指定的 CA 证书验证服务器证书
    ///
    /// # 参数
    /// - `client_cert_path`: 客户端证书文件路径（PEM 格式）
    /// - `client_key_path`: 客户端私钥文件路径（PEM 格式）
    /// - `ca_cert_path`: CA 证书文件路径（用于验证服务器证书，可选）
    ///
    /// # 返回值
    /// - RatResult<Self>: 成功返回构建器实例，失败返回错误
    pub fn with_client_certs_and_ca<S1, S2>(
        mut self,
        client_cert_path: S1,
        client_key_path: S2,
        ca_cert_path: Option<String>,
    ) -> RatResult<Self>
    where
        S1: Into<String>,
        S2: Into<String>,
    {
        let cert_path = client_cert_path.into();
        let key_path = client_key_path.into();

        // 读取客户端证书
        let cert_pem = std::fs::read_to_string(&cert_path)
            .map_err(|e| RatError::RequestError(format!("读取客户端证书失败: {}", e)))?;
        let cert_chain = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| RatError::RequestError(format!("解析客户端证书失败: {}", e)))?;

        if cert_chain.is_empty() {
            return Err(RatError::RequestError("客户端证书链为空".to_string()));
        }

        // 读取客户端私钥
        let key_pem = std::fs::read_to_string(&key_path)
            .map_err(|e| RatError::RequestError(format!("读取客户端私钥失败: {}", e)))?;
        let private_key = rustls_pemfile::private_key(&mut key_pem.as_bytes())
            .map_err(|e| RatError::RequestError(format!("解析客户端私钥失败: {}", e)))?
            .ok_or_else(|| RatError::RequestError("客户端私钥为空".to_string()))?;

        // 将私钥编码为 PEM 格式字节
        let private_key_pem = key_pem.into_bytes();

        // 读取 CA 证书（如果提供且非空）
        let (ca_certs, ca_cert_path_opt) = if let Some(ca_path_str) = ca_cert_path.as_ref() {
            if !ca_path_str.is_empty() {
                let ca_pem = std::fs::read_to_string(ca_path_str)
                    .map_err(|e| RatError::RequestError(format!("读取 CA 证书失败: {}", e)))?;
                let ca_certs_der = rustls_pemfile::certs(&mut ca_pem.as_bytes())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| RatError::RequestError(format!("解析 CA 证书失败: {}", e)))?;
                (Some(ca_certs_der.iter().map(|c| c.to_vec()).collect()), Some(ca_path_str.clone()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        self.mtls_config = Some(MtlsClientConfig {
            client_cert_chain: cert_chain.iter().map(|c| c.to_vec()).collect(),
            client_private_key: private_key_pem,
            ca_certs,
            server_name: None,
            client_cert_path: Some(cert_path),
            client_key_path: Some(key_path),
            ca_cert_path: ca_cert_path_opt,
        });

        Ok(self)
    }

    /// 配置 mTLS 客户端认证（完整版本，支持自定义 CA）
    ///
    /// 启用双向 TLS 认证，客户端将提供证书给服务器验证
    ///
    /// # 参数
    /// - `client_cert_chain`: 客户端证书链（DER 格式）
    /// - `client_private_key`: 客户端私钥（DER 格式）
    /// - `ca_certs`: 可选的 CA 证书（用于验证服务器证书）
    /// - `server_name`: 可选的服务器名称，用于 SNI
    ///
    /// # 返回值
    /// - RatResult<Self>: 成功返回构建器实例，失败返回错误
    pub fn with_mtls(
        mut self,
        client_cert_chain: Vec<Vec<u8>>,
        client_private_key: Vec<u8>,
        ca_certs: Option<Vec<Vec<u8>>>,
        server_name: Option<String>,
    ) -> RatResult<Self> {
        if client_cert_chain.is_empty() {
            return Err(RatError::RequestError("客户端证书链不能为空".to_string()));
        }

        self.mtls_config = Some(MtlsClientConfig {
            client_cert_chain,
            client_private_key,
            ca_certs,
            server_name,
            client_cert_path: None,
            client_key_path: None,
            ca_cert_path: None,
        });

        Ok(self)
    }

    // =============================================================================


    /// 配置 DNS 预解析映射
    ///
    /// 允许将域名映射到预解析的IP地址，避免DNS解析延迟
    ///
    /// # 参数
    /// - `mapping`: 域名到IP地址的映射表
    ///
    /// # 示例
    /// ```
    /// let mut dns_map = HashMap::new();
    /// dns_map.insert("api.example.com".to_string(), "192.168.1.100".to_string());
    /// dns_map.insert("service.example.com".to_string(), "10.0.0.50".to_string());
    /// ```
    ///
    /// # 返回值
    /// - RatResult<Self>: 成功返回构建器实例，失败返回错误
    pub fn with_dns_mapping(
        mut self,
        mapping: HashMap<String, String>,
    ) -> RatResult<Self> {
        // 验证IP地址格式
        for (domain, ip) in &mapping {
            if domain.is_empty() {
                return Err(RatError::RequestError("DNS映射中的域名不能为空".to_string()));
            }
            if ip.is_empty() {
                return Err(RatError::RequestError(format!("DNS映射中域名 '{}' 的IP地址不能为空", domain)));
            }
            // 简单验证IPv4或IPv6格式
            if !is_valid_ip_address(ip) {
                return Err(RatError::RequestError(format!("DNS映射中域名 '{}' 的IP '{}' 格式无效", domain, ip)));
            }
        }

        self.dns_mapping = Some(mapping);
        Ok(self)
    }

    /// 构建 gRPC 客户端实例
    ///
    /// # 错误
    /// 如果任何必需的配置项未设置，将返回错误
    ///
    /// # 必需配置项
    /// - connect_timeout: 连接超时时间
    /// - request_timeout: 请求超时时间
    /// - max_idle_connections: 最大空闲连接数
    /// - http2_only: HTTP 协议模式
    /// - user_agent: 用户代理字符串
    /// - compression_mode: 压缩模式
    pub fn build(self) -> RatResult<RatGrpcClient> {
        // 验证所有必需配置项

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

        let h2c_mode = self.h2c_mode.unwrap_or(false);  // 默认不启用 h2c 模式

        // 创建连接器
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(connect_timeout));

        // 创建客户端构建器
        let mut client_builder = Client::builder(TokioExecutor::new());

        // 配置 HTTP/2
        let client = if http2_only {
            // HTTP/2 prior knowledge 模式
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
                // 当compression特性未启用时，创建一个默认配置
                CompressionConfig {
                    enabled: false,
                    min_size: 1024,
                    level: 1,
                }
            }
        };

        Ok(RatGrpcClient::new(
            client,
            connect_timeout,
            request_timeout,
            max_idle_connections,
            user_agent,
            compression_config,
            compression_mode != GrpcCompressionMode::Disabled,
            false,
            3,
            compression_mode,
            h2c_mode,
            self.mtls_config,
            self.dns_mapping,
            false,  // h2c_over_tls = false（标准模式）
        ))
    }

    /// 构建 h2c-over-TLS 模式的 gRPC 客户端（Xray-core 风格）
    ///
    /// 此模式用于通过 HAProxy 等 HTTP 模式代理，特性：
    /// - TLS 连接时不进行 ALPN 协商（让代理认为是普通 TLS）
    /// - 在 TLS 通道内发送 h2c 格式的 HTTP/2 帧
    /// - 代理无法解析 HTTP/2 帧，只能透传 TLS 流量
    ///
    /// # 错误
    /// 如果任何必需的配置项未设置，将返回错误
    ///
    /// # 必需配置项
    /// - connect_timeout: 连接超时时间
    /// - request_timeout: 请求超时时间
    /// - max_idle_connections: 最大空闲连接数
    /// - http2_only: HTTP 协议模式
    /// - user_agent: 用户代理字符串
    /// - compression_mode: 压缩模式
    pub fn build_h2c_over_tls(self) -> RatResult<RatGrpcClient> {
        // 验证所有必需配置项
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

        let h2c_mode = self.h2c_mode.unwrap_or(false);  // 默认不启用 h2c 模式

        // 创建连接器
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(connect_timeout));

        // 创建客户端构建器
        let mut client_builder = Client::builder(TokioExecutor::new());

        // 配置 HTTP/2
        let client = if http2_only {
            // HTTP/2 prior knowledge 模式
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
                // 当compression特性未启用时，创建一个默认配置
                CompressionConfig {
                    enabled: false,
                    min_size: 1024,
                    level: 1,
                }
            }
        };

        Ok(RatGrpcClient::new(
            client,
            connect_timeout,
            request_timeout,
            max_idle_connections,
            user_agent,
            compression_config,
            compression_mode != GrpcCompressionMode::Disabled,
            false,
            3,
            compression_mode,
            h2c_mode,
            self.mtls_config,
            self.dns_mapping,
            true,  // h2c_over_tls = true
        ))
    }
}

/// 验证IP地址格式是否有效
fn is_valid_ip_address(ip: &str) -> bool {
    // 尝试解析为IPv4或IPv6地址
    ip.parse::<IpAddr>().is_ok()
}

impl Default for RatGrpcClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
