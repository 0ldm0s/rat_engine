    use crate::utils::logger::{info, warn, error, debug};
use crate::server::cert_manager::{CertificateManager, CertManagerConfig, CertificateInfo};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, SystemTime};
use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use openssl::pkey::PKey;
use openssl::stack::Stack;
use x509_parser::prelude::*;
use rcgen::{Certificate as RcgenCertificate, CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P384_SHA384};
use std::path::Path;
use tokio::fs;
use tokio::sync::RwLock;

impl CertificateManager {
    pub(crate) fn validate_certificate_algorithm(&self, cert: &X509) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 获取签名算法
        let sig_alg = cert.signature_algorithm().object().to_string();

        // 检查签名算法 - 支持的 ECDSA 签名算法
        let supported_sig_algs = [
            "ecdsa-with-SHA384", // ECDSA P-384 SHA-384
            "ecdsa-with-SHA256", // ECDSA P-256 SHA-256
            "ecdsa-with-SHA512", // ECDSA P-521 SHA-512
        ];

        let is_supported = supported_sig_algs.iter().any(|alg| sig_alg.contains(alg));
        if !is_supported && !sig_alg.contains("ecdsa") {
            return Err(format!("不支持的签名算法: {}，仅支持 ECDSA", sig_alg).into());
        }

        // 检查公钥算法
        let pub_key = cert.public_key()?;
        let pub_key_type = pub_key.id();

        if pub_key_type != openssl::pkey::Id::EC {
            return Err(format!("不支持的公钥算法: {:?}，仅支持 EC", pub_key_type).into());
        }

        // 检查椭圆曲线参数（secp384r1）
        if let Ok(ec_key) = pub_key.ec_key() {
            let curve = ec_key.group();
            let curve_nid = curve.curve_name();
            if let Some(nid) = curve_nid {
                let curve_name = openssl::nid::Nid::from_raw(nid.as_raw()).short_name()?;
                if curve_name != "secp384r1" && curve_name != "prime384v1" {
                    warn!("⚠️  证书使用的椭圆曲线可能不是 secp384r1: {}", curve_name);
                }
            }
        }

        info!("✅ 证书算法验证通过: 签名算法={}, 公钥算法=EC", sig_alg);
        Ok(())
    }
    
    /// 解析证书信息
    pub(crate) fn parse_certificate_info(&self, cert: &X509) -> Result<CertificateInfo, Box<dyn std::error::Error + Send + Sync>> {
        // 使用 OpenSSL 的文本格式
        let subject = format!("{:?}", cert.subject_name())
            .chars()
            .filter(|c| c.is_ascii())
            .collect();
        let issuer = format!("{:?}", cert.issuer_name())
            .chars()
            .filter(|c| c.is_ascii())
            .collect();

        // 转换时间 - 使用简化版本
        let not_before = std::time::SystemTime::UNIX_EPOCH;
        let not_after = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(86400 * 365); // 1年后
        let serial_number = hex::encode(cert.serial_number().to_bn()?.to_vec());
        let signature_algorithm = cert.signature_algorithm().object().to_string();
        let public_key_algorithm = "ECDSA";
        
        // 提取主机名
        let mut hostnames = Vec::new();
        
        // 从 Subject Alternative Name 扩展中提取
        let subject_alt_names = cert.subject_alt_names();
        if let Some(names) = subject_alt_names {
            for name in names.iter() {
                if let Some(dns) = name.dnsname() {
                    hostnames.push(dns.to_string());
                } else if let Some(ip) = name.ipaddress() {
                    let ip_str = if ip.len() == 4 {
                        let mut octets = [0u8; 4];
                        octets.copy_from_slice(ip);
                        std::net::IpAddr::from(octets).to_string()
                    } else if ip.len() == 16 {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(ip);
                        std::net::IpAddr::from(octets).to_string()
                    } else {
                        continue;
                    };
                    hostnames.push(ip_str);
                }
            }
        }
        
        // 从 Common Name 中提取
        if let Some(cn) = cert.subject_name().entries_by_nid(openssl::nid::Nid::COMMONNAME).next() {
            if let Ok(cn_str) = cn.data().as_utf8() {
                let cn_string = cn_str.to_string();
                if !hostnames.contains(&cn_string) {
                    hostnames.push(cn_string);
                }
            }
        }
        
        Ok(CertificateInfo {
            subject,
            issuer,
            not_before,
            not_after,
            serial_number,
            signature_algorithm,
            public_key_algorithm: public_key_algorithm.to_string(),
            hostnames,
        })
    }
    
    /// 获取服务器 TLS 配置
    pub fn get_server_config(&self) -> Option<Arc<SslAcceptor>> {
        self.server_config.clone()
    }

    /// 获取客户端 TLS 配置
    pub fn get_client_config(&self) -> Option<Arc<SslConnector>> {
        self.client_config.clone()
    }
    
    /// 获取证书信息
    pub fn get_certificate_info(&self) -> Option<&CertificateInfo> {
        self.certificate_info.as_ref()
    }
    
    /// 获取客户端证书信息
    pub fn get_client_certificate_info(&self) -> Option<&CertificateInfo> {
        self.client_certificate_info.as_ref()
    }
    
    /// 获取客户端证书链
    pub fn get_client_cert_chain(&self) -> Option<&Vec<X509>> {
        self.client_cert_chain.as_ref()
    }

    /// 获取客户端私钥
    pub fn get_client_private_key(&self) -> Option<&PKey<openssl::pkey::Private>> {
        self.client_private_key.as_ref()
    }
    
    /// 检查是否启用了 mTLS
    pub fn is_mtls_enabled(&self) -> bool {
        self.config.mtls_enabled
    }
    
    /// 检查路径是否在 MTLS 白名单中
    pub fn is_mtls_whitelisted(&self, path: &str) -> bool {
        if !self.config.mtls_enabled {
            return false; // 未启用 MTLS 时，白名单无意义
        }
        
        // 检查是否匹配白名单路径
        for whitelist_path in &self.config.mtls_whitelist_paths {
            if path == whitelist_path {
                return true;
            }
            // 支持通配符匹配，如 /api/* 
            if whitelist_path.ends_with("/*") {
                let base_path = &whitelist_path[..whitelist_path.len() - 2];
                if path.starts_with(base_path) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// 获取 MTLS 白名单路径列表
    pub fn get_mtls_whitelist_paths(&self) -> &Vec<String> {
        &self.config.mtls_whitelist_paths
    }
    
    /// 检查证书是否即将过期
    pub fn is_certificate_expiring(&self, days_threshold: u32) -> bool {
        if let Some(info) = &self.certificate_info {
            let threshold = SystemTime::now() + Duration::from_secs(days_threshold as u64 * 24 * 3600);
            info.not_after < threshold
        } else {
            false
        }
    }
    
    /// 设置强制证书轮转标志
    pub fn set_force_rotation(&mut self, force: bool) {
        self.config.force_cert_rotation = force;
        if force {
            info!("🔄 已设置强制证书轮转标志");
        }
    }
    
    /// 获取强制证书轮转标志
    pub fn get_force_rotation(&self) -> bool {
        self.config.force_cert_rotation
    }
}
