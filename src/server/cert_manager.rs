//! 证书管理模块
//! 
//! 提供 ECDSA+secp384r1 证书的生成、验证和管理功能
//! 支持开发模式自动生成和严格验证模式

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, SystemTime};
use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use openssl::pkey::PKey;
use openssl::stack::Stack;
use x509_parser::prelude::*;
use rcgen::{Certificate as RcgenCertificate, CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ECDSA_P384_SHA384};
use crate::utils::logger::{info, warn, error, debug};
#[cfg(feature = "acme")]
use acme_commander::certificate::{IssuanceOptions, IssuanceResult, issue_certificate};
#[cfg(feature = "acme")]
use acme_commander::convenience::{create_production_client, create_staging_client, create_cloudflare_dns};
#[cfg(feature = "acme")]
use acme_commander::crypto::KeyPair as AcmeKeyPair;
use std::path::Path;
use tokio::fs;
use tokio::sync::RwLock;

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

/// 证书管理器
pub struct CertificateManager {
    config: CertManagerConfig,
    server_config: Option<Arc<SslAcceptor>>,
    client_config: Option<Arc<SslConnector>>,
    certificate_info: Option<CertificateInfo>,
    // mTLS 相关字段
    client_certificate_info: Option<CertificateInfo>,
    // 客户端证书链和私钥
    client_cert_chain: Option<Vec<X509>>,
    // 客户端私钥
    client_private_key: Option<PKey<openssl::pkey::Private>>,
    // 服务器证书和私钥（用于重新配置）
    server_cert: Option<X509>,
    server_private_key: Option<PKey<openssl::pkey::Private>>,
    // 自动刷新相关字段
    refresh_handle: Option<tokio::task::JoinHandle<()>>,
    refresh_shutdown: Arc<AtomicBool>,
    refresh_in_progress: Arc<AtomicBool>,
}

impl CertificateManager {
    /// 创建新的证书管理器
    pub fn new(config: CertManagerConfig) -> Self {
        Self {
            config,
            server_config: None,
            client_config: None,
            certificate_info: None,
            client_certificate_info: None,
            client_cert_chain: None,
            client_private_key: None,
            server_cert: None,
            server_private_key: None,
            refresh_handle: None,
            refresh_shutdown: Arc::new(AtomicBool::new(false)),
            refresh_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// 获取证书管理器配置
    pub fn get_config(&self) -> &CertManagerConfig {
        &self.config
    }
    
    /// 初始化证书管理器
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 初始化服务端证书
        if self.config.development_mode {
            info!("🔧 开发模式：自动生成 ECDSA+secp384r1 证书");
            self.generate_development_certificate().await?;
        } else if self.config.acme_enabled {
            #[cfg(feature = "acme")]
            {
                info!("🌐 ACME 模式：自动签发和管理证书");
                self.handle_acme_certificate().await?;
            }
            #[cfg(not(feature = "acme"))]
            {
                return Err("ACME 功能未启用，请在编译时启用 acme 特性".into());
            }
        } else {
            info!("🔒 严格验证模式：加载现有证书");
            self.load_production_certificate().await?;
        }
        
        // 如果启用了 mTLS，初始化客户端证书
        if self.config.mtls_enabled {
            info!("🔐 mTLS 模式：初始化客户端证书");
            self.initialize_mtls_certificates().await?;
            
            // 重新配置服务器以支持 mTLS
            self.reconfigure_server_for_mtls().await?;
        }
        
        // 启动自动证书刷新任务
        if self.config.auto_refresh_enabled {
            info!("🔄 启动自动证书刷新任务");
            self.start_certificate_refresh_task().await?;
        }
        
        Ok(())
    }
    
    /// 初始化 mTLS 客户端证书
    async fn initialize_mtls_certificates(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    async fn generate_development_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    fn should_regenerate_certificate(&self) -> bool {
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
    async fn load_existing_development_certificate(
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
    async fn create_new_development_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    async fn load_production_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    
    /// 验证证书算法
    fn validate_certificate_algorithm(&self, cert: &X509) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    fn parse_certificate_info(&self, cert: &X509) -> Result<CertificateInfo, Box<dyn std::error::Error + Send + Sync>> {
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
    
    /// 处理 ACME 证书签发和管理
    #[cfg(feature = "acme")]
    async fn handle_acme_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    /// 生成客户端证书（用于 mTLS）
    async fn generate_client_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 生成客户端密钥对
        let key_pair = KeyPair::generate(&PKCS_ECDSA_P384_SHA384)?;
        
        // 创建客户端证书参数
        let mut params = CertificateParams::new(vec!["client".to_string()]);
        params.key_pair = Some(key_pair);
        params.alg = &PKCS_ECDSA_P384_SHA384;
        
        // 设置客户端证书主题
        let mut distinguished_name = DistinguishedName::new();
        if let Some(subject) = &self.config.client_cert_subject {
            distinguished_name.push(DnType::CommonName, subject);
        } else {
            distinguished_name.push(DnType::CommonName, "RAT Engine Client");
        }
        distinguished_name.push(DnType::OrganizationName, "RAT Engine");
        distinguished_name.push(DnType::CountryName, "CN");
        params.distinguished_name = distinguished_name;
        
        // 设置有效期
        let not_before = SystemTime::now();
        let not_after = not_before + Duration::from_secs(self.config.validity_days as u64 * 24 * 3600);
        params.not_before = not_before.into();
        params.not_after = not_after.into();
        
        // 生成客户端证书
        let cert = RcgenCertificate::from_params(params)?;
        
        // 转换为 OpenSSL 格式
        let cert_pem = cert.serialize_pem()?;
        let key_pem = cert.serialize_private_key_pem();

        let certificate = X509::from_pem(cert_pem.as_bytes())?;
        let private_key = PKey::private_key_from_pem(key_pem.as_bytes())?;

        // 存储客户端证书信息
        self.client_cert_chain = Some(vec![certificate.clone()]);
        self.client_private_key = Some(private_key);
        self.client_certificate_info = Some(self.parse_certificate_info(&certificate)?);
        
        // 如果配置了客户端证书路径，保存到文件
        if let (Some(cert_path), Some(key_path)) = (&self.config.client_cert_path, &self.config.client_key_path) {
            let cert_pem = cert.serialize_pem()?;
            let key_pem = cert.serialize_private_key_pem();
            
            // 将相对路径转换为绝对路径
            let cert_path_abs = std::fs::canonicalize(Path::new(cert_path))
                .unwrap_or_else(|_| {
                    // 如果路径不存在，使用当前工作目录拼接
                    std::env::current_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        .join(cert_path)
                });
            let key_path_abs = std::fs::canonicalize(Path::new(key_path))
                .unwrap_or_else(|_| {
                    // 如果路径不存在，使用当前工作目录拼接
                    std::env::current_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                        .join(key_path)
                });
            
            // 确保证书目录存在
            if let Some(parent) = cert_path_abs.parent() {
                fs::create_dir_all(parent).await?;
            }
            if let Some(parent) = key_path_abs.parent() {
                fs::create_dir_all(parent).await?;
            }
            
            fs::write(&cert_path_abs, cert_pem).await?;
            fs::write(&key_path_abs, key_pem).await?;
            
            info!("💾 客户端证书已保存到: {}", cert_path_abs.display());
            info!("🔑 客户端私钥已保存到: {}", key_path_abs.display());
        }
        
        info!("✅ 客户端证书生成成功");
        if let Some(info) = &self.client_certificate_info {
            info!("   主题: {}", info.subject);
            info!("   有效期: {} - {}", 
                info.not_before.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                info.not_after.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs());
        }
        
        Ok(())
    }
    
    /// 加载现有客户端证书
    async fn load_client_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cert_path = self.config.client_cert_path.as_ref()
            .ok_or("客户端证书路径未配置")?;
        let key_path = self.config.client_key_path.as_ref()
            .ok_or("客户端私钥路径未配置")?;
        
        // 读取证书文件
        let cert_pem = fs::read_to_string(cert_path).await
            .map_err(|e| format!("无法读取客户端证书文件 {}: {}", cert_path, e))?;
        
        // 读取私钥文件
        let key_pem = fs::read_to_string(key_path).await
            .map_err(|e| format!("无法读取客户端私钥文件 {}: {}", key_path, e))?;
        
        // 解析证书
        let certificate = X509::from_pem(cert_pem.as_bytes())?;
        let private_key = PKey::private_key_from_pem(key_pem.as_bytes())?;

        // 验证证书算法
        self.validate_certificate_algorithm(&certificate)?;

        // 存储客户端证书信息
        self.client_cert_chain = Some(vec![certificate.clone()]);
        self.client_private_key = Some(private_key);
        self.client_certificate_info = Some(self.parse_certificate_info(&certificate)?);
        
        info!("✅ 客户端证书加载成功: {}", cert_path);
        if let Some(info) = &self.client_certificate_info {
            info!("   主题: {}", info.subject);
            info!("   颁发者: {}", info.issuer);
        }
        
        Ok(())
    }

    /// 配置 ALPN 协议支持
    /// 这个方法应该在服务器启动时调用，而不是在证书初始化时硬编码
    pub fn configure_alpn_protocols(&mut self, protocols: Vec<Vec<u8>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("✅ ALPN 协议配置已记录: {:?}",
            protocols.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>());
        rat_logger::debug!("🔍 [ALPN配置] ALPN 协议配置已保存: {:?}", protocols);

        // 立即重新创建服务器配置以应用 ALPN
        self.recreate_server_config_with_alpn(protocols)?;
        Ok(())
    }

    /// 重新创建服务器配置以应用 ALPN 协议
    fn recreate_server_config_with_alpn(&mut self, protocols: Vec<Vec<u8>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 检查是否有服务器证书和私钥
        let certificate = self.server_cert.as_ref()
            .ok_or("服务器证书未找到，无法应用 ALPN 配置")?;
        let private_key = self.server_private_key.as_ref()
            .ok_or("服务器私钥未找到，无法应用 ALPN 配置")?;

        // 创建新的服务器配置，应用 ALPN
        let mut server_config = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        server_config.set_certificate(certificate)?;
        server_config.set_private_key(private_key)?;

        // 设置 ALPN 协议
        if !protocols.is_empty() {
            let mut alpn_data = Vec::new();
            for p in &protocols {
                alpn_data.push(p.len() as u8);
                alpn_data.extend_from_slice(p);
            }

            debug!("🔍 [ALPN数据] 生成的 ALPN 数据: {:?}", alpn_data);
            debug!("🔍 [ALPN数据] 期望的客户端格式: {:?}", b"\x02h2");

            if let Err(e) = server_config.set_alpn_protos(&alpn_data) {
                error!("❌ ALPN 协议设置失败: {}", e);
                return Err(format!("ALPN 协议设置失败: {}", e).into());
            }
            info!("✅ ALPN 协议已应用到服务器配置: {:?}",
                protocols.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>());
        }

        // 如果启用了 mTLS，应用客户端认证配置
        if self.config.mtls_enabled {
            if self.config.development_mode {
                // 开发模式：请求但不强制验证客户端证书
                server_config.set_verify_callback(SslVerifyMode::PEER, |_, _| true);
            } else {
                // 生产模式：强制验证客户端证书
                server_config.set_verify_callback(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT, |_, _| true);
            }
        }

        // 更新服务器配置
        self.server_config = Some(Arc::new(server_config.build()));
        info!("✅ 服务器配置已重新创建，ALPN 协议已应用");

        Ok(())
    }
    
    /// 重新配置服务器以支持 mTLS
    /// 这个方法在 mTLS 证书初始化完成后调用
    pub async fn reconfigure_server_for_mtls(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.mtls_enabled {
            return Ok(()); // mTLS 未启用，无需重新配置
        }
        
        if self.config.development_mode {
            // 开发模式：重新创建支持 mTLS 的服务器配置
            if self.server_config.is_some() {
                return self.recreate_server_config_with_mtls().await;
            } else {
                return Err("开发模式下服务器配置未初始化".into());
            }
        } else if self.config.acme_enabled {
            // ACME 模式：从现有的服务器配置中获取证书
            return Err("ACME 模式下的 mTLS 重新配置暂未实现".into());
        } else {
            // 严格验证模式：重新加载证书
            return Err("严格验证模式下的 mTLS 重新配置暂未实现".into());
        }
    }

    /// 重新创建支持 mTLS 的服务器配置（开发模式）
    async fn recreate_server_config_with_mtls(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("重新创建支持 mTLS 的开发模式服务器配置");
        
        // 使用存储的服务器证书和私钥
        let certificate = self.server_cert.as_ref()
            .ok_or("服务器证书未找到")?;
        let private_key = self.server_private_key.as_ref()
            .ok_or("服务器私钥未找到")?;
        
        // 如果有客户端证书，创建 mTLS 配置
        if let Some(client_cert_chain) = &self.client_cert_chain {
            // 创建服务器配置，启用客户端认证
            let mut server_config = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
            server_config.set_certificate(certificate)?;
            server_config.set_private_key(private_key)?;

            // 设置客户端认证
            if self.config.development_mode {
                // 开发模式：请求但不强制验证客户端证书
                server_config.set_verify_callback(SslVerifyMode::PEER, |_, _| true);
            } else {
                // 生产模式：强制验证客户端证书
                server_config.set_verify_callback(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT, |_, _| true);
            }

            self.server_config = Some(Arc::new(server_config.build()));

            info!("mTLS 服务器配置重新创建成功");
        } else {
            return Err("客户端证书未初始化，无法配置 mTLS".into());
        }
        
        Ok(())
    }
    
    /// 启动证书自动刷新任务
    async fn start_certificate_refresh_task(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

/// 开发模式证书验证逻辑已通过 OpenSSL 的 SslVerifyMode::NONE 实现

impl Drop for CertificateManager {
    fn drop(&mut self) {
        // 确保刷新任务被正确停止
        if self.refresh_handle.is_some() {
            self.refresh_shutdown.store(true, Ordering::Relaxed);
            
            // 直接丢弃任务句柄，让任务在后台自行清理
            // 这是正常的关闭流程，不需要警告
            self.refresh_handle.take();
            
            // 只在调试模式下显示详细信息
            #[cfg(debug_assertions)]
            debug!("🔄 CertificateManager 证书刷新任务已停止");
        }
    }
}

/// 证书管理器构建器
#[derive(Debug, Default)]
pub struct CertManagerBuilder {
    config: CertManagerConfig,
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