//! 证书管理器配置
//!
//! 支持同端口模式和分端口模式的证书配置：
//! - 同端口模式：gRPC 和 HTTP 共用同一证书
//! - 分端口模式：gRPC 和 HTTP 可以配置不同的证书

use std::path::PathBuf;

/// 单个证书配置
#[derive(Debug, Clone)]
pub struct CertConfig {
    /// 证书文件路径
    pub cert_path: PathBuf,
    /// 私钥文件路径
    pub key_path: PathBuf,
    /// CA 证书路径（可选，用于客户端证书验证）
    pub ca_path: Option<PathBuf>,
    /// 支持的域名列表（用于 SNI）
    pub domains: Vec<String>,
}

impl CertConfig {
    /// 从文件路径创建证书配置
    pub fn from_paths(cert_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            ca_path: None,
            domains: Vec::new(),
        }
    }

    /// 添加域名
    pub fn with_domains(mut self, domains: Vec<String>) -> Self {
        self.domains = domains;
        self
    }

    /// 设置 CA 证书路径
    pub fn with_ca(mut self, ca_path: impl Into<PathBuf>) -> Self {
        self.ca_path = Some(ca_path.into());
        self
    }

    /// 验证证书文件是否存在
    pub fn validate(&self) -> Result<(), String> {
        if !self.cert_path.exists() {
            return Err(format!("证书文件不存在: {}", self.cert_path.display()));
        }
        if !self.key_path.exists() {
            return Err(format!("私钥文件不存在: {}", self.key_path.display()));
        }
        if let Some(ca_path) = &self.ca_path {
            if !ca_path.exists() {
                return Err(format!("CA 证书文件不存在: {}", ca_path.display()));
            }
        }
        Ok(())
    }
}

/// 证书管理器配置
///
/// 支持两种模式：
/// 1. 同端口模式：使用 `shared_cert`，gRPC 和 HTTP 共用证书
/// 2. 分端口模式：使用 `grpc_cert` 和 `http_cert`，分别配置证书
#[derive(Debug, Clone)]
pub struct CertManagerConfig {
    /// 共用证书配置（同端口模式）
    pub shared_cert: Option<CertConfig>,
    /// gRPC 专用证书配置（分端口模式）
    pub grpc_cert: Option<CertConfig>,
    /// HTTP 专用证书配置（分端口模式）
    pub http_cert: Option<CertConfig>,
    /// 是否为分端口模式
    pub separated_mode: bool,
}

impl Default for CertManagerConfig {
    fn default() -> Self {
        Self {
            shared_cert: None,
            grpc_cert: None,
            http_cert: None,
            separated_mode: false,
        }
    }
}

impl CertManagerConfig {
    /// 创建同端口模式配置（共用证书）
    pub fn shared(cert_config: CertConfig) -> Self {
        Self {
            shared_cert: Some(cert_config),
            grpc_cert: None,
            http_cert: None,
            separated_mode: false,
        }
    }

    /// 创建分端口模式配置（分离证书）
    pub fn separated(grpc_cert: CertConfig, http_cert: Option<CertConfig>) -> Self {
        Self {
            shared_cert: None,
            grpc_cert: Some(grpc_cert),
            http_cert,
            separated_mode: true,
        }
    }

    /// 获取 gRPC 证书配置
    pub fn get_grpc_cert(&self) -> Option<&CertConfig> {
        if self.separated_mode {
            self.grpc_cert.as_ref()
        } else {
            self.shared_cert.as_ref()
        }
    }

    /// 获取 HTTP 证书配置
    pub fn get_http_cert(&self) -> Option<&CertConfig> {
        if self.separated_mode {
            self.http_cert.as_ref()
        } else {
            self.shared_cert.as_ref()
        }
    }

    /// 验证配置
    ///
    /// 规则：
    /// - 同端口模式：必须有 shared_cert
    /// - 分端口模式：必须有 grpc_cert，http_cert 可选
    /// - gRPC 证书必须存在且有效
    pub fn validate(&self, require_grpc: bool, require_http: bool) -> Result<(), String> {
        if self.separated_mode {
            // 分端口模式
            if require_grpc {
                if let Some(grpc_cert) = &self.grpc_cert {
                    grpc_cert.validate().map_err(|e| format!("gRPC 证书验证失败: {}", e))?;
                } else {
                    return Err("分端口模式下启用 gRPC 必须配置 gRPC 证书".to_string());
                }
            }

            if require_http {
                if let Some(http_cert) = &self.http_cert {
                    http_cert.validate().map_err(|e| format!("HTTP 证书验证失败: {}", e))?;
                }
            }
        } else {
            // 同端口模式
            if let Some(shared_cert) = &self.shared_cert {
                // 同端口模式只要启用了 gRPC 或 HTTP，就需要证书
                if require_grpc || require_http {
                    shared_cert.validate().map_err(|e| format!("证书验证失败: {}", e))?;
                }
            } else if require_grpc {
                return Err("同端口模式下启用 gRPC 必须配置证书".to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_mode() {
        let config = CertManagerConfig::shared(CertConfig::from_paths("/cert.pem", "/key.pem"));
        assert!(config.shared_cert.is_some());
        assert!(!config.separated_mode);
        assert!(config.get_grpc_cert().is_some());
        assert!(config.get_http_cert().is_some());
    }

    #[test]
    fn test_separated_mode() {
        let grpc_cert = CertConfig::from_paths("/grpc-cert.pem", "/grpc-key.pem");
        let http_cert = CertConfig::from_paths("/http-cert.pem", "/http-key.pem");
        let config = CertManagerConfig::separated(grpc_cert, Some(http_cert));
        assert!(config.separated_mode);
        assert!(config.get_grpc_cert().is_some());
        assert!(config.get_http_cert().is_some());
    }

    #[test]
    fn test_validate_shared_requires_cert() {
        let config = CertManagerConfig::default();
        // 同端口模式启用 gRPC 必须有证书
        assert!(config.validate(true, false).is_err());
    }
}
