use crate::utils::logger::{info, warn, error, debug};
use crate::server::cert_manager::CertManagerConfig;
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

impl crate::server::cert_manager::CertificateManager {
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
}
