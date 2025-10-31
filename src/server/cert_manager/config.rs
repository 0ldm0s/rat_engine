/// 证书管理器配置
#[derive(Debug, Clone)]
pub struct CertManagerConfig {
    /// 是否为开发模式
    pub development_mode: bool,
    /// 证书文件路径（严格验证模式）
    pub cert_path: Option<String>,
    /// 私钥文件路径（严格验证模式）
    pub key_path: Option<String>,
    /// CA 证书路径（可选）
    pub ca_path: Option<String>,
    /// 证书有效期（开发模式）
    pub validity_days: u32,
    /// 主机名列表
    pub hostnames: Vec<String>,
    /// 是否启用 ACME 自动证书
    pub acme_enabled: bool,
    /// 是否使用 ACME 生产环境（false 为测试环境）
    pub acme_production: bool,
    /// ACME 账户邮箱
    pub acme_email: Option<String>,
    /// Cloudflare API 令牌（用于 DNS-01 挑战）
    pub cloudflare_api_token: Option<String>,
    /// 证书自动续期天数阈值（默认30天）
    pub acme_renewal_days: u32,
    /// ACME 证书存储目录
    pub acme_cert_dir: Option<String>,
    /// 是否启用 mTLS 双向认证
    pub mtls_enabled: bool,
    /// 客户端证书路径（mTLS 模式）
    pub client_cert_path: Option<String>,
    /// 客户端私钥路径（mTLS 模式）
    pub client_key_path: Option<String>,
    /// 客户端 CA 证书路径（用于验证客户端证书）
    pub client_ca_path: Option<String>,
    /// mTLS 模式："self_signed" 或 "acme_mixed"
    /// - self_signed: 服务端和客户端都使用自签名证书（内网场景）
    /// - acme_mixed: 服务端使用 ACME 证书，客户端使用自签名证书
    pub mtls_mode: Option<String>,
    /// 是否自动生成客户端证书（开发模式或自签名模式）
    pub auto_generate_client_cert: bool,
    /// 客户端证书主题名称
    pub client_cert_subject: Option<String>,
    /// 是否启用自动证书刷新（后台任务）
    pub auto_refresh_enabled: bool,
    /// 证书刷新检查间隔（秒，默认3600秒=1小时）
    pub refresh_check_interval: u64,
    /// 是否强制证书轮转（删除现有证书并重新生成）
    pub force_cert_rotation: bool,
    /// MTLS 白名单路径（不需要客户端证书认证的路径列表）
    pub mtls_whitelist_paths: Vec<String>,
}
impl Default for CertManagerConfig {
    fn default() -> Self {
        Self {
            development_mode: true,
            cert_path: None,
            key_path: None,
            ca_path: None,
            validity_days: 3650,
            hostnames: vec!["localhost".to_string(), "127.0.0.1".to_string()],
            acme_enabled: false,
            acme_production: false,
            acme_email: None,
            cloudflare_api_token: None,
            acme_renewal_days: 30,
            acme_cert_dir: None,
            mtls_enabled: false,
            client_cert_path: None,
            client_key_path: None,
            client_ca_path: None,
            mtls_mode: None,
            auto_generate_client_cert: false,
            client_cert_subject: None,
            auto_refresh_enabled: true,
            refresh_check_interval: 3600,
            force_cert_rotation: false,
            mtls_whitelist_paths: Vec::new(),
        }
    }
}

