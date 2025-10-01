//! RAT Engine 端口配置模块
//! 
//! 提供灵活的端口配置选项，支持：
//! - HTTP + gRPC 同端口模式（默认）
//! - HTTP 和 gRPC 分端口模式
//! - 强制 HTTPS 模式（自动锁定 80/443 端口）

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use serde::{Deserialize, Serialize};

/// 端口配置模式
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortMode {
    /// 同端口模式：HTTP 和 gRPC 共享同一端口（默认）
    Unified {
        /// 服务端口
        port: u16,
        /// 绑定地址
        bind_addr: IpAddr,
    },
    /// 分端口模式：HTTP 和 gRPC 使用不同端口
    Separated {
        /// HTTP 服务端口
        http_port: u16,
        /// gRPC 服务端口
        grpc_port: u16,
        /// 绑定地址
        bind_addr: IpAddr,
    },
}

impl Default for PortMode {
    fn default() -> Self {
        Self::Unified {
            port: 8080,
            bind_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }
}

/// 证书配置（仅限 ECDSA+secp384r1+X.509+PEM）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateConfig {
    /// 证书文件路径（PEM 格式）
    pub cert_path: String,
    /// 私钥文件路径（PEM 格式）
    pub key_path: String,
    /// CA 证书路径（可选，用于客户端验证）
    pub ca_path: Option<String>,
    /// 证书主机名列表
    pub hostnames: Vec<String>,
}

impl CertificateConfig {
    /// 创建新的证书配置
    pub fn new(cert_path: impl Into<String>, key_path: impl Into<String>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            ca_path: None,
            hostnames: vec!["localhost".to_string()],
        }
    }
    
    /// 添加主机名
    pub fn with_hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostnames.push(hostname.into());
        self
    }
    
    /// 设置主机名列表
    pub fn with_hostnames(mut self, hostnames: Vec<String>) -> Self {
        self.hostnames = hostnames;
        self
    }
    
    /// 设置 CA 证书路径
    pub fn with_ca_path(mut self, ca_path: impl Into<String>) -> Self {
        self.ca_path = Some(ca_path.into());
        self
    }
    
    /// 转换为证书管理器配置
    pub fn to_cert_manager_config(&self) -> CertManagerConfig {
        CertManagerConfig {
            development_mode: false,
            cert_path: Some(self.cert_path.clone()),
            key_path: Some(self.key_path.clone()),
            ca_path: self.ca_path.clone(),
            validity_days: 3650, // 严格验证模式下不使用，但需要设置默认值
            hostnames: self.hostnames.clone(),
            // ACME 相关字段设置为默认值
            acme_enabled: false,
            acme_production: false,
            acme_email: None,
            cloudflare_api_token: None,
            acme_renewal_days: 30,
            acme_cert_dir: None,
            // mTLS 相关字段设置为默认值
            mtls_enabled: false,
            mtls_mode: None,
            auto_generate_client_cert: false,
            client_cert_path: None,
            client_key_path: None,
            client_ca_path: None,
            client_cert_subject: None,
            auto_refresh_enabled: true,
            refresh_check_interval: 3600,
            force_cert_rotation: false,
            mtls_whitelist_paths: Vec::new(),
        }
    }
    
    /// 验证证书文件是否存在
    pub fn validate_files(&self) -> Result<(), PortConfigError> {
        if !std::path::Path::new(&self.cert_path).exists() {
            return Err(PortConfigError::CertificateNotFound(self.cert_path.clone()));
        }
        
        if !std::path::Path::new(&self.key_path).exists() {
            return Err(PortConfigError::PrivateKeyNotFound(self.key_path.clone()));
        }
        
        if let Some(ca_path) = &self.ca_path {
            if !std::path::Path::new(ca_path).exists() {
                return Err(PortConfigError::CaCertificateNotFound(ca_path.clone()));
            }
        }
        
        Ok(())
    }
}

/// HTTPS 强制配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpsConfig {
    /// 是否启用强制 HTTPS
    pub enabled: bool,
    /// 是否自动禁用 H2C（明文 HTTP/2）
    pub disable_h2c: bool,
    /// HTTP 重定向端口（默认 80）
    pub redirect_port: u16,
    /// HTTPS 服务端口（默认 443）
    pub https_port: u16,
    /// 绑定地址
    pub bind_addr: IpAddr,
    /// 重定向目标域名（用于生成重定向 URL）
    pub redirect_domain: Option<String>,
    /// 证书配置（可选，如果未配置则在强制 HTTPS 模式下自动进入开发模式）
    pub certificate: Option<CertificateConfig>,
}

impl Default for HttpsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            disable_h2c: true, // 强制 HTTPS 时默认禁用 H2C
            redirect_port: 80,
            https_port: 443,
            bind_addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED), // 0.0.0.0
            redirect_domain: None,
            certificate: None,
        }
    }
}

/// 完整的端口配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConfig {
    /// 端口模式配置
    pub mode: PortMode,
    /// HTTPS 强制配置
    pub https: HttpsConfig,
}

impl Default for PortConfig {
    fn default() -> Self {
        Self {
            mode: PortMode::default(),
            https: HttpsConfig::default(),
        }
    }
}

/// 端口配置构建器
#[derive(Debug)]
pub struct PortConfigBuilder {
    mode: Option<PortMode>,
    https: Option<HttpsConfig>,
}

impl PortConfigBuilder {
    /// 创建新的端口配置构建器
    pub fn new() -> Self {
        Self {
            mode: None,
            https: None,
        }
    }

    /// 设置同端口模式
    pub fn unified_port(mut self, port: u16, bind_addr: IpAddr) -> Self {
        self.mode = Some(PortMode::Unified { port, bind_addr });
        self
    }

    /// 设置分端口模式
    pub fn separated_ports(mut self, http_port: u16, grpc_port: u16, bind_addr: IpAddr) -> Self {
        self.mode = Some(PortMode::Separated {
            http_port,
            grpc_port,
            bind_addr,
        });
        self
    }

    /// 启用强制 HTTPS
    pub fn force_https(mut self, redirect_domain: Option<String>) -> Self {
        self.https = Some(HttpsConfig {
            enabled: true,
            disable_h2c: true,
            redirect_port: 80,
            https_port: 443,
            bind_addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            redirect_domain,
            certificate: None,
        });
        self
    }

    /// 启用强制 HTTPS 并配置证书
    pub fn force_https_with_cert(mut self, redirect_domain: Option<String>, certificate: CertificateConfig) -> Self {
        self.https = Some(HttpsConfig {
            enabled: true,
            disable_h2c: true,
            redirect_port: 80,
            https_port: 443,
            bind_addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            redirect_domain,
            certificate: Some(certificate),
        });
        self
    }

    /// 自定义 HTTPS 配置
    pub fn https_config(mut self, config: HttpsConfig) -> Self {
        self.https = Some(config);
        self
    }

    /// 构建端口配置
    pub fn build(mut self) -> Result<PortConfig, PortConfigError> {
        let mode = self.mode.take().ok_or(PortConfigError::MissingMode)?;
        let https = self.https.take().unwrap_or_default();

        // 验证配置的合法性
        Self::validate_config_static(&mode, &https)?;

        Ok(PortConfig { mode, https })
    }

    /// 验证配置合法性
    fn validate_config_static(mode: &PortMode, https: &HttpsConfig) -> Result<(), PortConfigError> {
        // 如果启用强制 HTTPS，验证端口配置
        if https.enabled {
            // 检查是否使用了标准 HTTP/HTTPS 端口
            if https.redirect_port != 80 {
                return Err(PortConfigError::InvalidHttpsPort(
                    "强制 HTTPS 模式下重定向端口必须是 80".to_string()
                ));
            }
            if https.https_port != 443 {
                return Err(PortConfigError::InvalidHttpsPort(
                    "强制 HTTPS 模式下 HTTPS 端口必须是 443".to_string()
                ));
            }

            // 验证端口模式与 HTTPS 配置的兼容性
            match mode {
                PortMode::Unified { port, .. } => {
                    if *port != 443 {
                        return Err(PortConfigError::IncompatibleConfig(
                            "强制 HTTPS 模式下同端口配置必须使用 443 端口".to_string()
                        ));
                    }
                }
                PortMode::Separated { http_port, grpc_port, .. } => {
                    if *http_port != 443 {
                        return Err(PortConfigError::IncompatibleConfig(
                            "强制 HTTPS 模式下 HTTP 端口必须是 443".to_string()
                        ));
                    }
                    if *grpc_port == 80 || *grpc_port == 443 {
                        return Err(PortConfigError::IncompatibleConfig(
                            "gRPC 端口不能使用 80 或 443".to_string()
                        ));
                    }
                }
            }
        }

        // 验证分端口模式下端口不冲突
        if let PortMode::Separated { http_port, grpc_port, .. } = mode {
            if http_port == grpc_port {
                return Err(PortConfigError::PortConflict(*http_port));
            }
        }

        Ok(())
    }
}

impl Default for PortConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 端口配置错误
#[derive(Debug, thiserror::Error)]
pub enum PortConfigError {
    #[error("缺少端口模式配置")]
    MissingMode,
    
    #[error("端口冲突: {0}")]
    PortConflict(u16),
    
    #[error("无效的 HTTPS 端口配置: {0}")]
    InvalidHttpsPort(String),
    
    #[error("配置不兼容: {0}")]
    IncompatibleConfig(String),
    
    #[error("证书文件未找到: {0}")]
    CertificateNotFound(String),
    
    #[error("私钥文件未找到: {0}")]
    PrivateKeyNotFound(String),
    
    #[error("CA 证书文件未找到: {0}")]
    CaCertificateNotFound(String),
}

impl PortConfig {
    /// 创建默认配置（同端口模式，端口 8080）
    pub fn default_unified() -> Self {
        Self {
            mode: PortMode::Unified {
                port: 8080,
                bind_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
            },
            https: HttpsConfig::default(),
        }
    }

    /// 创建分端口配置
    pub fn separated(http_port: u16, grpc_port: u16) -> Result<Self, PortConfigError> {
        PortConfigBuilder::new()
            .separated_ports(http_port, grpc_port, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .build()
    }

    /// 创建强制 HTTPS 配置（同端口模式）
    pub fn force_https_unified(redirect_domain: Option<String>) -> Result<Self, PortConfigError> {
        PortConfigBuilder::new()
            .unified_port(443, IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .force_https(redirect_domain)
            .build()
    }

    /// 创建强制 HTTPS 配置（分端口模式）
    pub fn force_https_separated(grpc_port: u16, redirect_domain: Option<String>) -> Result<Self, PortConfigError> {
        PortConfigBuilder::new()
            .separated_ports(443, grpc_port, IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .force_https(redirect_domain)
            .build()
    }

    /// 获取 HTTP 服务地址
    pub fn http_addr(&self) -> SocketAddr {
        match &self.mode {
            PortMode::Unified { port, bind_addr } => {
                if self.https.enabled {
                    SocketAddr::new(self.https.bind_addr, self.https.https_port)
                } else {
                    SocketAddr::new(*bind_addr, *port)
                }
            }
            PortMode::Separated { http_port, bind_addr, .. } => {
                if self.https.enabled {
                    SocketAddr::new(self.https.bind_addr, self.https.https_port)
                } else {
                    SocketAddr::new(*bind_addr, *http_port)
                }
            }
        }
    }

    /// 获取 gRPC 服务地址（仅在分端口模式下有效）
    pub fn grpc_addr(&self) -> Option<SocketAddr> {
        match &self.mode {
            PortMode::Unified { .. } => None,
            PortMode::Separated { grpc_port, bind_addr, .. } => {
                Some(SocketAddr::new(*bind_addr, *grpc_port))
            }
        }
    }

    /// 获取 HTTP 重定向服务地址（仅在强制 HTTPS 模式下有效）
    pub fn redirect_addr(&self) -> Option<SocketAddr> {
        if self.https.enabled {
            Some(SocketAddr::new(self.https.bind_addr, self.https.redirect_port))
        } else {
            None
        }
    }

    /// 是否启用强制 HTTPS
    pub fn is_https_forced(&self) -> bool {
        self.https.enabled
    }

    /// 是否禁用 H2C
    pub fn is_h2c_disabled(&self) -> bool {
        self.https.enabled && self.https.disable_h2c
    }

    /// 是否为分端口模式
    pub fn is_separated_mode(&self) -> bool {
        matches!(self.mode, PortMode::Separated { .. })
    }

    /// 获取重定向域名
    pub fn redirect_domain(&self) -> Option<&str> {
        self.https.redirect_domain.as_deref()
    }

    /// 获取证书配置
    pub fn certificate_config(&self) -> Option<&CertificateConfig> {
        self.https.certificate.as_ref()
    }

    /// 是否为开发模式（强制 HTTPS 但没有配置证书）
    pub fn is_development_mode(&self) -> bool {
        self.https.enabled && self.https.certificate.is_none()
    }

    /// 验证证书配置（如果存在）
    pub fn validate_certificate(&self) -> Result<(), PortConfigError> {
        if let Some(cert_config) = &self.https.certificate {
            cert_config.validate_files()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PortConfig::default();
        assert!(!config.is_https_forced());
        assert!(!config.is_separated_mode());
        assert_eq!(config.http_addr().port(), 8080);
    }

    #[test]
    fn test_separated_mode() {
        let config = PortConfig::separated(8080, 9090).unwrap();
        assert!(config.is_separated_mode());
        assert_eq!(config.http_addr().port(), 8080);
        assert_eq!(config.grpc_addr().unwrap().port(), 9090);
    }

    #[test]
    fn test_force_https_unified() {
        let config = PortConfig::force_https_unified(Some("example.com".to_string())).unwrap();
        assert!(config.is_https_forced());
        assert!(!config.is_separated_mode());
        assert_eq!(config.http_addr().port(), 443);
        assert_eq!(config.redirect_addr().unwrap().port(), 80);
    }

    #[test]
    fn test_force_https_separated() {
        let config = PortConfig::force_https_separated(9090, Some("example.com".to_string())).unwrap();
        assert!(config.is_https_forced());
        assert!(config.is_separated_mode());
        assert_eq!(config.http_addr().port(), 443);
        assert_eq!(config.grpc_addr().unwrap().port(), 9090);
        assert_eq!(config.redirect_addr().unwrap().port(), 80);
    }

    #[test]
    fn test_port_conflict() {
        let result = PortConfig::separated(8080, 8080);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PortConfigError::PortConflict(8080)));
    }

    #[test]
    fn test_invalid_https_config() {
        let result = PortConfigBuilder::new()
            .unified_port(8080, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .force_https(None)
            .build();
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PortConfigError::IncompatibleConfig(_)));
    }
}