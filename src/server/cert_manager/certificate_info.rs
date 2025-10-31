use std::time::SystemTime;

/// 证书信息
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    /// 证书主题
    pub subject: String,
    /// 证书颁发者
    pub issuer: String,
    /// 有效期开始时间
    pub not_before: SystemTime,
    /// 有效期结束时间
    pub not_after: SystemTime,
    /// 序列号
    pub serial_number: String,
    /// 签名算法
    pub signature_algorithm: String,
    /// 公钥算法
    pub public_key_algorithm: String,
    /// 主机名列表
    pub hostnames: Vec<String>,
}
