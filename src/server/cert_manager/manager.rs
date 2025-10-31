use crate::utils::logger::{info, warn, error, debug};
use crate::server::cert_manager::CertManagerConfig;
use crate::server::cert_manager::CertificateInfo;
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

pub struct CertificateManager {
    pub(crate) config: CertManagerConfig,
    pub(crate) server_config: Option<Arc<SslAcceptor>>,
    pub(crate) client_config: Option<Arc<SslConnector>>,
    pub(crate) certificate_info: Option<CertificateInfo>,
    // mTLS 相关字段
    pub(crate) client_certificate_info: Option<CertificateInfo>,
    // 客户端证书链和私钥
    pub(crate) client_cert_chain: Option<Vec<X509>>,
    // 客户端私钥
    pub(crate) client_private_key: Option<PKey<openssl::pkey::Private>>,
    // 服务器证书和私钥（用于重新配置）
    pub(crate) server_cert: Option<X509>,
    pub(crate) server_private_key: Option<PKey<openssl::pkey::Private>>,
    // 自动刷新相关字段
    pub(crate) refresh_handle: Option<tokio::task::JoinHandle<()>>,
    pub(crate) refresh_shutdown: Arc<AtomicBool>,
    pub(crate) refresh_in_progress: Arc<AtomicBool>,
}
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
