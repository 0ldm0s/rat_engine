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
    pub(crate) async fn start_certificate_refresh_task(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let shutdown_flag = self.refresh_shutdown.clone();
        let check_interval = Duration::from_secs(self.config.refresh_check_interval);
        
        // 克隆必要的配置用于刷新任务
        let config = self.config.clone();
        
        let handle = tokio::spawn(async move {
            let mut last_check = SystemTime::now();
            
            loop {
                tokio::time::sleep(check_interval).await;
                
                // 检查是否应该关闭
                if shutdown_flag.load(Ordering::Relaxed) {
                    info!("🔄 证书刷新任务收到关闭信号");
                    break;
                }
                
                // 简单的证书刷新逻辑
                if let Err(e) = Self::check_and_refresh_certificates_static(&config).await {
                    error!("❌ 证书刷新检查失败: {}", e);
                }
                
                last_check = SystemTime::now();
            }
        });
        
        self.refresh_handle = Some(handle);
        Ok(())
    }
    
    /// 检查并刷新证书（静态方法）
    async fn check_and_refresh_certificates_static(config: &CertManagerConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let renewal_days = config.acme_renewal_days;
        
        if config.development_mode {
            // 开发模式：检查证书是否过期
            if let Some(cert_dir) = &config.acme_cert_dir {
                let server_cert_path = format!("{}/server.crt", cert_dir);
                let server_key_path = format!("{}/server.key", cert_dir);
                
                if Path::new(&server_cert_path).exists() && Path::new(&server_key_path).exists() {
                    // 检查证书是否需要刷新
                    if Self::is_certificate_expiring_at_path_static(&server_cert_path, renewal_days).await? {
                        info!("🔄 开发模式证书即将过期，开始刷新");
                        
                        // 备份现有证书
                        let timestamp = chrono::Utc::now().timestamp();
                        let backup_cert_path = format!("{}/server.crt.{}", cert_dir, timestamp);
                        let backup_key_path = format!("{}/server.key.{}", cert_dir, timestamp);
                        
                        tokio::fs::rename(&server_cert_path, &backup_cert_path).await.ok();
                        tokio::fs::rename(&server_key_path, &backup_key_path).await.ok();
                        
                        info!("🔄 已备份现有证书到 {} 和 {}", backup_cert_path, backup_key_path);
                        
                        // 生成新证书
                        if let Err(e) = Self::generate_development_certificate_at_path_static(cert_dir, &config.hostnames, config.validity_days).await {
                            error!("❌ 开发模式证书刷新失败: {}", e);
                            
                            // 恢复备份
                            tokio::fs::rename(&backup_cert_path, &server_cert_path).await.ok();
                            tokio::fs::rename(&backup_key_path, &server_key_path).await.ok();
                            
                            return Err(e);
                        }
                        
                        info!("✅ 开发模式证书刷新成功");
                    }
                }
            }
        } else if config.acme_enabled {
            // ACME 模式：检查证书是否需要续期
            if let Some(cert_dir) = &config.acme_cert_dir {
                let cert_path = format!("{}/server.crt", cert_dir);
                let key_path = format!("{}/server.key", cert_dir);
                
                if Path::new(&cert_path).exists() && Path::new(&key_path).exists() {
                    if Self::is_certificate_expiring_at_path_static(&cert_path, renewal_days).await? {
                        info!("🔄 ACME 证书即将过期，开始续期");
                        warn!("⚠️  ACME 证书续期功能需要实现");
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 检查指定路径的证书是否即将过期（静态方法）
    async fn is_certificate_expiring_at_path_static(cert_path: &str, renewal_days: u32) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let cert_data = tokio::fs::read(cert_path).await?;
        let (_, cert) = X509Certificate::from_der(&cert_data)?;
        
        let not_after: std::time::SystemTime = cert.validity().not_after.to_datetime().into();
        let threshold = SystemTime::now() + Duration::from_secs(renewal_days as u64 * 24 * 3600);
        
        Ok(not_after < threshold)
    }
    
    /// 在指定路径生成开发模式证书（静态方法）
    async fn generate_development_certificate_at_path_static(
        cert_dir: &str,
        hostnames: &[String],
        validity_days: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server_cert_path = format!("{}/server.crt", cert_dir);
        let server_key_path = format!("{}/server.key", cert_dir);
        
        // 确保目录存在
        tokio::fs::create_dir_all(cert_dir).await?;
        
        // 生成密钥对
        let key_pair = rcgen::KeyPair::generate(&rcgen::PKCS_ECDSA_P384_SHA384)?;
        
        // 生成证书参数
        let mut params = rcgen::CertificateParams::default();
        params.not_before = std::time::SystemTime::now().into();
        params.not_after = (std::time::SystemTime::now() + Duration::from_secs(validity_days as u64 * 24 * 3600)).into();
        
        // 设置主题
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "RAT Engine Development Certificate");
        dn.push(rcgen::DnType::OrganizationName, "RAT Engine Development");
        params.distinguished_name = dn;
        
        // 添加主机名
        for hostname in hostnames {
            params.subject_alt_names.push(rcgen::SanType::DnsName(hostname.clone()));
        }
        
        // 生成证书
        let cert = rcgen::Certificate::from_params(params)?;
        let cert_pem = cert.serialize_pem()?;
        let key_pem = cert.serialize_private_key_pem();
        
        // 写入文件
        tokio::fs::write(&server_cert_path, cert_pem).await?;
        tokio::fs::write(&server_key_path, key_pem).await?;
        
        info!("✅ 开发模式证书已生成: {} {}", server_cert_path, server_key_path);
        
        Ok(())
    }
}
