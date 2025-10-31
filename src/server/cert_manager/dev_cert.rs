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
    pub(crate) async fn initialize_mtls_certificates(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mtls_mode = self.config.mtls_mode.as_deref().unwrap_or("self_signed");
        
        match mtls_mode {
            "self_signed" => {
                // 自签名模式：服务端和客户端都使用自签名证书
                if self.config.auto_generate_client_cert {
                    info!("🔧 自动生成客户端自签名证书");
                    self.generate_client_certificate().await?;
                } else {
                    info!("📂 加载现有客户端证书");
                    self.load_client_certificate().await?;
                }
            },
            "acme_mixed" => {
                // ACME 混合模式：服务端使用 ACME 证书，客户端使用自签名证书
                if !self.config.acme_enabled {
                    return Err("ACME 混合模式需要启用 ACME 功能".into());
                }
                info!("🌐 ACME 混合模式：生成客户端自签名证书");
                self.generate_client_certificate().await?;
            },
            _ => {
                return Err(format!("不支持的 mTLS 模式: {}", mtls_mode).into());
            }
        }
        
        Ok(())
    }
    
    /// 生成开发模式证书
    pub(crate) async fn generate_development_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 在开发模式下，如果配置了 acme_cert_dir，先检查现有证书
        if let Some(cert_dir) = &self.config.acme_cert_dir {
            let ca_cert_path = format!("{}/ca.crt", cert_dir);
            let server_cert_path = format!("{}/server.crt", cert_dir);
            let server_key_path = format!("{}/server.key", cert_dir);
            
            // 检查证书文件是否已存在
            if Path::new(&ca_cert_path).exists() && Path::new(&server_cert_path).exists() && Path::new(&server_key_path).exists() {
                info!("📋 检查现有开发模式证书");
                
                // 如果强制证书轮转，删除现有证书
                if self.config.force_cert_rotation {
                    info!("🔄 强制证书轮转：删除现有证书");
                    fs::remove_file(&ca_cert_path).await.ok();
                    fs::remove_file(&server_cert_path).await.ok();
                    fs::remove_file(&server_key_path).await.ok();
                    
                    // 也删除客户端证书文件
                    let client_cert_path = format!("{}/client.crt", cert_dir);
                    let client_key_path = format!("{}/client.key", cert_dir);
                    fs::remove_file(&client_cert_path).await.ok();
                    fs::remove_file(&client_key_path).await.ok();
                    
                    info!("✅ 现有证书已删除，将重新生成");
                } else {
                    // 尝试加载现有证书
                    if let Ok(()) = self.load_existing_development_certificate(&server_cert_path, &server_key_path).await {
                        // 检查证书是否需要重新生成
                        if !self.should_regenerate_certificate() {
                            info!("✅ 现有开发模式证书仍然有效，继续使用");
                            return Ok(());
                        } else {
                            info!("⏰ 现有开发模式证书即将过期或需要更新，重新生成");
                        }
                    } else {
                        warn!("⚠️  现有开发模式证书无效，重新生成");
                    }
                }
            }
        }
        
        // 生成新的证书
        self.create_new_development_certificate().await
    }
    
    /// 检查是否需要重新生成证书
    pub(crate) fn should_regenerate_certificate(&self) -> bool {
        if let Some(info) = &self.certificate_info {
            // 检查证书是否在3天内过期（更严格的检查）
            let threshold = SystemTime::now() + Duration::from_secs(3 * 24 * 3600);
            let should_regenerate = info.not_after < threshold;
            
            if should_regenerate {
                info!("⚠️  证书将在3天内过期，需要重新生成: 当前时间={:?}, 过期时间={:?}", 
                      SystemTime::now(), info.not_after);
            }
            
            should_regenerate
        } else {
            true // 没有证书信息，需要重新生成
        }
    }
    
    /// 加载现有的开发模式证书
    pub(crate) async fn load_existing_development_certificate(
        &mut self,
        cert_path: &str,
        key_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 读取证书文件
        let cert_file = fs::read(cert_path).await?;
        let key_file = fs::read(key_path).await?;
        
        // 解析证书
        let certificate = X509::from_pem(&cert_file)?;
        let private_key = PKey::private_key_from_pem(&key_file)?;

        // 验证证书算法
        self.validate_certificate_algorithm(&certificate)?;

        // 存储服务器证书和私钥数据
        self.server_cert = Some(certificate.clone());
        self.server_private_key = Some(private_key.clone());

        // 创建服务器配置
        let mut server_config = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        server_config.set_certificate(&certificate)?;
        server_config.set_private_key(&private_key)?;

        self.server_config = Some(Arc::new(server_config.build()));

        // 创建客户端配置（开发模式跳过证书验证）
        let mut client_config = SslConnector::builder(SslMethod::tls())?;

        if self.config.development_mode {
            // 开发模式：跳过服务端证书验证
            client_config.set_verify(SslVerifyMode::NONE);
        } else {
            // 生产模式：正常验证
            client_config.set_verify(SslVerifyMode::PEER);
        }

        self.client_config = Some(Arc::new(client_config.build()));

        // 解析证书信息
        self.certificate_info = Some(self.parse_certificate_info(&certificate)?);
        
        info!("✅ 开发模式证书加载成功");
        if let Some(info) = &self.certificate_info {
            info!("   主题: {}", info.subject);
            info!("   有效期: {:?} - {:?}", info.not_before, info.not_after);
            info!("   主机名: {:?}", info.hostnames);
        }
        
        Ok(())
    }
    
    /// 创建新的开发模式证书
    pub(crate) async fn create_new_development_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🔧 生成新的开发模式 ECDSA+secp384r1 证书");
        
        // 生成 ECDSA P-384 密钥对
        let key_pair = KeyPair::generate(&PKCS_ECDSA_P384_SHA384)?;
        
        // 创建证书参数
        let mut params = CertificateParams::new(self.config.hostnames.clone());
        params.key_pair = Some(key_pair);
        params.alg = &PKCS_ECDSA_P384_SHA384;
        
        // 设置证书主题
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, "RAT Engine Development");
        distinguished_name.push(DnType::OrganizationName, "RAT Engine");
        distinguished_name.push(DnType::CountryName, "CN");
        params.distinguished_name = distinguished_name;
        
        // 设置有效期
        let not_before = SystemTime::now();
        let not_after = not_before + Duration::from_secs(self.config.validity_days as u64 * 24 * 3600);
        params.not_before = not_before.into();
        params.not_after = not_after.into();
        
        // 生成证书
        let cert = RcgenCertificate::from_params(params)?;
        
        // 转换为 OpenSSL 格式
        let cert_pem = cert.serialize_pem()?;
        let key_pem = cert.serialize_private_key_pem();

        let certificate = X509::from_pem(cert_pem.as_bytes())?;
        let private_key = PKey::private_key_from_pem(key_pem.as_bytes())?;

        // 存储服务器证书和私钥数据
        self.server_cert = Some(certificate.clone());
        self.server_private_key = Some(private_key.clone());
        
        // 在开发模式下，如果配置了 acme_cert_dir，保存服务器证书作为 CA 证书
        if let Some(cert_dir) = &self.config.acme_cert_dir {
            let ca_cert_path = format!("{}/ca.crt", cert_dir);
            let server_cert_path = format!("{}/server.crt", cert_dir);
            let server_key_path = format!("{}/server.key", cert_dir);
            
            // 确保证书目录存在
            fs::create_dir_all(cert_dir).await?;
            
            // 保存证书文件
            let cert_pem = cert.serialize_pem()?;
            let key_pem = cert.serialize_private_key_pem();
            
            // 在开发模式下，服务器证书同时作为 CA 证书使用
            fs::write(&ca_cert_path, &cert_pem).await?;
            fs::write(&server_cert_path, &cert_pem).await?;
            fs::write(&server_key_path, &key_pem).await?;
            
            info!("💾 开发模式证书已保存:");
            info!("   CA 证书: {}", ca_cert_path);
            info!("   服务器证书: {}", server_cert_path);
            info!("   服务器私钥: {}", server_key_path);
        }
        
        // 创建服务器配置
        let mut server_config = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        server_config.set_certificate(&certificate)?;
        server_config.set_private_key(&private_key)?;

        self.server_config = Some(Arc::new(server_config.build()));

        // 创建客户端配置（开发模式跳过证书验证）
        let mut client_config = SslConnector::builder(SslMethod::tls())?;

        if self.config.development_mode {
            // 开发模式：跳过服务端证书验证
            client_config.set_verify(SslVerifyMode::NONE);
        } else {
            // 生产模式：正常验证
            client_config.set_verify(SslVerifyMode::PEER);
        }

        self.client_config = Some(Arc::new(client_config.build()));

        // 解析证书信息
        self.certificate_info = Some(self.parse_certificate_info(&certificate)?);
        
        info!("✅ 开发证书生成成功");
        if let Some(info) = &self.certificate_info {
            info!("   主题: {}", info.subject);
            info!("   有效期: {:?} - {:?}", info.not_before, info.not_after);
            info!("   主机名: {:?}", info.hostnames);
        }
        
        Ok(())
    }
    
    /// 加载严格验证模式证书
    pub(crate) async fn load_production_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cert_path = self.config.cert_path.as_ref()
            .ok_or("严格验证模式需要指定证书路径")?;
        let key_path = self.config.key_path.as_ref()
            .ok_or("严格验证模式需要指定私钥路径")?;
        
        // 读取证书文件
        let cert_file = std::fs::read(cert_path)?;
        let key_file = std::fs::read(key_path)?;
        
        // 解析证书
        let certificate = X509::from_pem(&cert_file)?;
        let private_key = PKey::private_key_from_pem(&key_file)?;
        
        // 验证证书是否为 ECDSA+secp384r1
        self.validate_certificate_algorithm(&certificate)?;

        // 创建服务器配置
        let mut server_config = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        server_config.set_certificate(&certificate)?;
        server_config.set_private_key(&private_key)?;

        self.server_config = Some(Arc::new(server_config.build()));

        // 创建客户端配置
        let mut client_config = SslConnector::builder(SslMethod::tls())?;

        // 如果指定了 CA 证书，加载它
        if let Some(ca_path) = &self.config.ca_path {
            let ca_file = std::fs::read(ca_path)?;
            let ca_cert = X509::from_pem(&ca_file)?;
            client_config.cert_store_mut().add_cert(ca_cert)?;
        }

        // 在生产模式下，使用标准验证
        client_config.set_verify(SslVerifyMode::PEER);

        self.client_config = Some(Arc::new(client_config.build()));

        // 解析证书信息
        self.certificate_info = Some(self.parse_certificate_info(&certificate)?);
        
        info!("✅ 生产证书加载成功");
        if let Some(info) = &self.certificate_info {
            info!("   主题: {}", info.subject);
            info!("   有效期: {:?} - {:?}", info.not_before, info.not_after);
            info!("   签名算法: {}", info.signature_algorithm);
        }
  
        Ok(())
    }
}
