use crate::utils::logger::{info, warn, error, debug};
use crate::server::cert_manager::{CertificateManager, CertManagerConfig, CertificateInfo};

pub struct CertManagerBuilder {
    config: CertManagerConfig,
}

impl Default for CertManagerBuilder {
    fn default() -> Self {
        Self {
            config: CertManagerConfig::default(),
        }
    }
}

impl CertManagerBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置开发模式
    pub fn development_mode(mut self, enabled: bool) -> Self {
        self.config.development_mode = enabled;
        self
    }
    
    /// 设置证书路径（严格验证模式）
    pub fn with_cert_path(mut self, path: impl Into<String>) -> Self {
        self.config.cert_path = Some(path.into());
        self
    }
    
    /// 设置私钥路径（严格验证模式）
    pub fn with_key_path(mut self, path: impl Into<String>) -> Self {
        self.config.key_path = Some(path.into());
        self
    }
    
    /// 设置 CA 证书路径
    pub fn with_ca_path(mut self, path: impl Into<String>) -> Self {
        self.config.ca_path = Some(path.into());
        self
    }
    
    /// 设置证书有效期（开发模式）
    pub fn with_validity_days(mut self, days: u32) -> Self {
        self.config.validity_days = days;
        self
    }
    
    /// 添加主机名
    pub fn add_hostname(mut self, hostname: impl Into<String>) -> Self {
        self.config.hostnames.push(hostname.into());
        self
    }
    
    /// 设置主机名列表
    pub fn with_hostnames(mut self, hostnames: Vec<String>) -> Self {
        self.config.hostnames = hostnames;
        self
    }
    
    /// 启用 ACME 自动证书
    pub fn enable_acme(mut self, enabled: bool) -> Self {
        self.config.acme_enabled = enabled;
        self
    }
    
    /// 设置 ACME 生产环境模式
    pub fn with_acme_production(mut self, production: bool) -> Self {
        self.config.acme_production = production;
        self
    }
    
    /// 设置 ACME 邮箱地址
    pub fn with_acme_email(mut self, email: impl Into<String>) -> Self {
        self.config.acme_email = Some(email.into());
        self
    }
    
    /// 设置 Cloudflare API Token
    pub fn with_cloudflare_api_token(mut self, token: impl Into<String>) -> Self {
        self.config.cloudflare_api_token = Some(token.into());
        self
    }
    
    /// 设置 ACME 证书续期天数阈值
    pub fn with_acme_renewal_days(mut self, days: u32) -> Self {
        self.config.acme_renewal_days = days;
        self
    }
    
    /// 设置 ACME 证书存储目录
    pub fn with_acme_cert_dir(mut self, dir: impl Into<String>) -> Self {
        self.config.acme_cert_dir = Some(dir.into());
        self
    }
    
    /// 启用 mTLS 双向认证
    pub fn enable_mtls(mut self, enabled: bool) -> Self {
        self.config.mtls_enabled = enabled;
        self
    }
    
    /// 设置客户端证书路径
    pub fn with_client_cert_path(mut self, path: impl Into<String>) -> Self {
        self.config.client_cert_path = Some(path.into());
        self
    }
    
    /// 设置客户端私钥路径
    pub fn with_client_key_path(mut self, path: impl Into<String>) -> Self {
        self.config.client_key_path = Some(path.into());
        self
    }
    
    /// 设置客户端 CA 路径
    pub fn with_client_ca_path(mut self, path: impl Into<String>) -> Self {
        self.config.client_ca_path = Some(path.into());
        self
    }
    
    /// 设置 mTLS 模式
    /// - "self_signed": 服务端和客户端都使用自签名证书
    /// - "acme_mixed": 服务端使用 ACME 证书，客户端使用自签名证书
    pub fn with_mtls_mode(mut self, mode: impl Into<String>) -> Self {
        self.config.mtls_mode = Some(mode.into());
        self
    }
    
    /// 启用自动生成客户端证书
    pub fn auto_generate_client_cert(mut self, enabled: bool) -> Self {
        self.config.auto_generate_client_cert = enabled;
        self
    }
    
    /// 设置客户端证书主题
    pub fn with_client_cert_subject(mut self, subject: impl Into<String>) -> Self {
        self.config.client_cert_subject = Some(subject.into());
        self
    }
    
    /// 启用自动证书刷新
    pub fn enable_auto_refresh(mut self, enabled: bool) -> Self {
        self.config.auto_refresh_enabled = enabled;
        self
    }
    
    /// 设置证书刷新检查间隔（秒）
    pub fn with_refresh_check_interval(mut self, interval_seconds: u64) -> Self {
        self.config.refresh_check_interval = interval_seconds;
        self
    }
    
    /// 设置强制证书轮转
    pub fn force_cert_rotation(mut self, force: bool) -> Self {
        self.config.force_cert_rotation = force;
        self
    }
    
    /// 添加 MTLS 白名单路径
    pub fn add_mtls_whitelist_path(mut self, path: impl Into<String>) -> Self {
        self.config.mtls_whitelist_paths.push(path.into());
        self
    }
    
    /// 添加多个 MTLS 白名单路径
    pub fn add_mtls_whitelist_paths(mut self, paths: Vec<impl Into<String>>) -> Self {
        for path in paths {
            self.config.mtls_whitelist_paths.push(path.into());
        }
        self
    }
    
    /// 构建证书管理器
    pub fn build(self) -> CertificateManager {
        CertificateManager::new(self.config)
    }
    
    /// 构建证书管理器配置
    pub fn build_config(self) -> CertManagerConfig {
        self.config
    }
}
