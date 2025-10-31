    use crate::utils::logger::{info, warn, error, debug};
use crate::server::cert_manager::{CertificateManager, CertManagerConfig, CertificateInfo};
#[cfg(feature = "acme")]
use acme_commander::certificate::{IssuanceOptions, IssuanceResult, issue_certificate};
#[cfg(feature = "acme")]
use acme_commander::convenience::{create_production_client, create_staging_client, create_cloudflare_dns};
#[cfg(feature = "acme")]
use acme_commander::crypto::KeyPair as AcmeKeyPair;
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
    /// 处理 ACME 证书签发和管理
    #[cfg(feature = "acme")]
    pub(crate) async fn handle_acme_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 克隆配置信息以避免借用冲突
        let acme_production = self.config.acme_production;
        let acme_email = self.config.acme_email.clone()
            .ok_or("ACME 模式需要指定邮箱地址")?;
        let cloudflare_token = self.config.cloudflare_api_token.clone()
            .ok_or("ACME 模式需要指定 Cloudflare API Token")?;
        let acme_cert_dir = self.config.acme_cert_dir.clone()
            .ok_or("ACME 模式需要指定证书存储目录")?;
        let renewal_days = self.config.acme_renewal_days;

        // 确保证书目录存在
        fs::create_dir_all(&acme_cert_dir).await?;

        let cert_path = Path::new(&acme_cert_dir).join("cert.pem");
        let key_path = Path::new(&acme_cert_dir).join("key.pem");

        // 检查是否存在有效证书
        if cert_path.exists() && key_path.exists() {
            info!("📋 检查现有 ACME 证书");
            
            // 尝试加载现有证书
            if let Ok(()) = self.load_acme_certificate(&cert_path, &key_path).await {
                // 检查证书是否需要续期
                if !self.is_certificate_expiring(renewal_days) {
                    info!("✅ 现有 ACME 证书仍然有效");
                    return Ok(());
                } else {
                    info!("⏰ 现有 ACME 证书即将过期，开始续期");
                }
            } else {
                warn!("⚠️  现有 ACME 证书无效，重新签发");
            }
        } else {
            info!("🆕 首次签发 ACME 证书");
        }

        // 签发新证书
        self.issue_new_acme_certificate(
            acme_production,
            &acme_email,
            &cloudflare_token,
            &cert_path,
            &key_path,
        ).await?;

        Ok(())
    }

    /// 加载 ACME 证书
    #[cfg(feature = "acme")]
    async fn load_acme_certificate(
        &mut self,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 读取证书文件
        let cert_file = fs::read(cert_path).await?;
        let key_file = fs::read(key_path).await?;

        // 解析证书
        let certificate = X509::from_pem(&cert_file)?;
        let private_key = PKey::private_key_from_pem(&key_file)?;

        // 验证证书算法
        self.validate_certificate_algorithm(&certificate)?;

        // 创建服务器配置
        let mut server_config = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        server_config.set_certificate(&certificate)?;
        server_config.set_private_key(&private_key)?;

        self.server_config = Some(Arc::new(server_config.build()));

        // 创建客户端配置（使用系统根证书）
        let mut client_config = SslConnector::builder(SslMethod::tls())?;
        client_config.set_verify(SslVerifyMode::PEER);

        self.client_config = Some(Arc::new(client_config.build()));

        // 解析证书信息
        self.certificate_info = Some(self.parse_certificate_info(&certificate)?);

        Ok(())
    }

    /// 签发新的 ACME 证书
    #[cfg(feature = "acme")]
    async fn issue_new_acme_certificate(
        &mut self,
        acme_production: bool,
        acme_email: &str,
        cloudflare_token: &str,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🔄 开始签发新的 ACME 证书...");

        // 生成账户密钥和证书密钥
        let account_key = AcmeKeyPair::generate()
            .map_err(|e| format!("生成账户密钥失败: {}", e))?;
        let certificate_key = AcmeKeyPair::generate()
            .map_err(|e| format!("生成证书密钥失败: {}", e))?;

        // 创建 Cloudflare DNS 管理器
        let dns_manager = create_cloudflare_dns(cloudflare_token.to_string())
            .map_err(|e| format!("创建 Cloudflare DNS 管理器失败: {}", e))?;

        // 配置签发选项
        let issuance_options = IssuanceOptions {
            domains: self.config.hostnames.clone(),
            email: acme_email.to_string(),
            production: acme_production,
            dry_run: false,
            dns_manager,
            certificate_request: None,
        };

        // 执行证书签发
        let result = issue_certificate(
            account_key,
            certificate_key,
            issuance_options,
        ).await
            .map_err(|e| format!("ACME 证书签发失败: {}", e))?;
        
        info!("✅ ACME 证书签发成功");
        
        // 保存证书和私钥
        fs::write(cert_path, &result.fullchain_pem).await
            .map_err(|e| format!("保存证书文件失败: {}", e))?;
        fs::write(key_path, &result.private_key_pem).await
            .map_err(|e| format!("保存私钥文件失败: {}", e))?;

        info!("💾 ACME 证书已保存到: {:?}", cert_path);
        info!("🔑 ACME 私钥已保存到: {:?}", key_path);

        // 加载新签发的证书
        self.load_acme_certificate(cert_path, key_path).await?;

        info!("✅ ACME 证书加载成功");
        if let Some(info) = &self.certificate_info {
            info!("   主题: {}", info.subject);
            info!("   有效期: {:?} - {:?}", info.not_before, info.not_after);
            info!("   域名: {:?}", info.hostnames);
        }

        Ok(())
    }
}
