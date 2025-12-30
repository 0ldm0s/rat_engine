//! 证书管理器（统一入口）
//!
//! 根据 CertManagerConfig 创建相应的证书管理器实例
//! 支持同端口模式和分端口模式

use std::sync::{Arc, RwLock};
use std::collections::HashMap;

use rustls::server::ServerConfig;

use super::config::{CertManagerConfig, CertConfig};
use super::rustls_cert::RustlsCertManager;

/// 证书管理器
///
/// 支持两种模式：
/// - 同端口模式：gRPC 和 HTTP 共用证书
/// - 分端口模式：gRPC 和 HTTP 使用不同证书
pub struct CertificateManager {
    /// 配置
    config: CertManagerConfig,
    /// gRPC 证书管理器
    grpc_manager: Option<RustlsCertManager>,
    /// HTTP 证书管理器（分端口模式）
    http_manager: Option<RustlsCertManager>,
    /// 共用证书管理器（同端口模式）
    shared_manager: Option<RustlsCertManager>,
}

impl CertificateManager {
    /// 从配置创建证书管理器
    pub fn from_config(config: CertManagerConfig) -> Result<Self, String> {
        if config.separated_mode {
            // 分端口模式
            let grpc_manager = if let Some(grpc_cert) = &config.grpc_cert {
                Some(RustlsCertManager::from_config(grpc_cert)?)
            } else {
                None
            };

            let http_manager = if let Some(http_cert) = &config.http_cert {
                Some(RustlsCertManager::from_config(http_cert)?)
            } else {
                None
            };

            Ok(Self {
                config,
                grpc_manager,
                http_manager,
                shared_manager: None,
            })
        } else {
            // 同端口模式
            let shared_manager = if let Some(shared_cert) = &config.shared_cert {
                Some(RustlsCertManager::from_config(shared_cert)?)
            } else {
                None
            };

            Ok(Self {
                config,
                grpc_manager: None,
                http_manager: None,
                shared_manager,
            })
        }
    }

    /// 获取 gRPC 的 ServerConfig
    ///
    /// 规则：
    /// - gRPC 必须有证书，否则 panic
    pub fn get_grpc_server_config(&self) -> Arc<ServerConfig> {
        let manager = if self.config.separated_mode {
            self.grpc_manager.as_ref()
        } else {
            self.shared_manager.as_ref()
        };

        manager
            .map(|m| m.get_server_config())
            .unwrap_or_else(|| {
                panic!("gRPC 服务必须配置证书！请在启动前配置证书。");
            })
    }

    /// 获取 HTTP 的 ServerConfig
    ///
    /// 规则：
    /// - 同端口模式：返回共用证书
    /// - 分端口模式：返回 HTTP 专用证书（如果有）
    /// - 如果没有证书，返回 None（允许 HTTP/1.1）
    pub fn get_http_server_config(&self) -> Option<Arc<ServerConfig>> {
        let manager = if self.config.separated_mode {
            self.http_manager.as_ref()
        } else {
            self.shared_manager.as_ref()
        };

        manager.map(|m| m.get_server_config())
    }

    /// 检查是否有 gRPC 证书
    pub fn has_grpc_cert(&self) -> bool {
        if self.config.separated_mode {
            self.grpc_manager.is_some()
        } else {
            self.shared_manager.is_some()
        }
    }

    /// 检查是否有 HTTP 证书
    pub fn has_http_cert(&self) -> bool {
        if self.config.separated_mode {
            self.http_manager.is_some()
        } else {
            self.shared_manager.is_some()
        }
    }

    /// 获取配置
    pub fn get_config(&self) -> &CertManagerConfig {
        &self.config
    }

    /// 获取支持的域名列表
    pub fn get_domains(&self) -> Vec<String> {
        let mut domains = Vec::new();

        if let Some(manager) = &self.shared_manager {
            domains.extend(manager.get_domains().iter().cloned());
        }
        if let Some(manager) = &self.grpc_manager {
            domains.extend(manager.get_domains().iter().cloned());
        }
        if let Some(manager) = &self.http_manager {
            domains.extend(manager.get_domains().iter().cloned());
        }

        domains.sort();
        domains.dedup();
        domains
    }

    /// 是否为分端口模式
    pub fn is_separated_mode(&self) -> bool {
        self.config.separated_mode
    }
}

// ============ mTLS 相关接口 ============

/// mTLS 检查
impl CertificateManager {
    /// 检查路径是否在 mTLS 白名单中（暂不使用）
    pub fn is_mtls_whitelisted(&self, _path: &str) -> bool {
        // mTLS 模式下全部要求客户端证书
        self.is_mtls_enabled()
    }

    /// 检查是否启用了 mTLS
    pub fn is_mtls_enabled(&self) -> bool {
        // 检查是否有证书配置启用了 CA（即启用了 mTLS）
        if self.config.separated_mode {
            // 分端口模式：检查 gRPC 和 HTTP 证书是否配置了 CA
            let grpc_has_ca = self.config.grpc_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some();
            let http_has_ca = self.config.http_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some();
            grpc_has_ca || http_has_ca
        } else {
            // 同端口模式：检查共用证书是否配置了 CA
            self.config.shared_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some()
        }
    }

    /// 检查 gRPC 是否启用了 mTLS
    pub fn is_grpc_mtls_enabled(&self) -> bool {
        if self.config.separated_mode {
            self.config.grpc_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some()
        } else {
            self.config.shared_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some()
        }
    }

    /// 检查 HTTP 是否启用了 mTLS
    pub fn is_http_mtls_enabled(&self) -> bool {
        if self.config.separated_mode {
            self.config.http_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some()
        } else {
            self.config.shared_cert.as_ref()
                .and_then(|c| c.ca_path.as_ref())
                .is_some()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_cert_config() -> CertConfig {
        CertConfig::from_paths(
            "/tmp/test-cert.pem",
            "/tmp/test-key.pem",
        )
    }

    #[test]
    #[should_panic(expected = "gRPC 服务必须配置证书")]
    fn test_grpc_requires_cert() {
        let config = CertManagerConfig::default();
        let manager = CertificateManager::from_config(config).unwrap();
        manager.get_grpc_server_config();
    }

    #[test]
    fn test_shared_mode() {
        let config = CertManagerConfig::shared(create_test_cert_config());
        // 注意：实际文件不存在会报错，这里只是测试配置结构
        assert!(!config.separated_mode);
        assert!(config.shared_cert.is_some());
    }

    #[test]
    fn test_separated_mode() {
        let grpc_cert = create_test_cert_config();
        let http_cert = create_test_cert_config();
        let config = CertManagerConfig::separated(grpc_cert, Some(http_cert));
        assert!(config.separated_mode);
        assert!(config.grpc_cert.is_some());
        assert!(config.http_cert.is_some());
    }
}
