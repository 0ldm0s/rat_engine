//! 基于 rustls 的证书管理器
//!
//! 使用 rustls + ring 作为加密后端
//! 参考 rat_ligproxy 实现

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rustls::server::{ServerConfig, ResolvesServerCertUsingSni, WebPkiClientVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, PrivatePkcs8KeyDer};
use rustls::sign::CertifiedKey;
use rustls_pemfile::{certs, private_key};
use rustls::RootCertStore;

use crate::utils::logger::{info, error};

/// Rustls 证书管理器
#[derive(Clone)]
pub struct RustlsCertManager {
    /// SNI 解析器
    sni_resolver: Arc<ResolvesServerCertUsingSni>,
    /// ServerConfig
    server_config: Arc<ServerConfig>,
    /// 支持的域名列表
    domains: Vec<String>,
}

impl RustlsCertManager {
    /// 从单个证书配置创建管理器
    pub fn from_config(cert_config: &super::config::CertConfig) -> Result<Self, String> {
        // 获取 provider（直接使用，不关心是否重复安装）
        let provider = rustls::crypto::ring::default_provider();

        info!("加载证书: {}", cert_config.cert_path.display());

        // 创建 SNI 解析器
        let mut sni_resolver = ResolvesServerCertUsingSni::new();

        // 读取证书和私钥
        let cert_file = File::open(&cert_config.cert_path)
            .map_err(|e| format!("打开证书文件失败: {}", e))?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Vec<CertificateDer<'static>> = certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析证书失败: {}", e))?
            .into_iter()
            .map(|cert| CertificateDer::from(cert.into_owned()))
            .collect();

        let key_file = File::open(&cert_config.key_path)
            .map_err(|e| format!("打开私钥文件失败: {}", e))?;
        let mut key_reader = BufReader::new(key_file);
        let key = private_key(&mut key_reader)
            .map_err(|e| format!("解析私钥失败: {}", e))?
            .ok_or("私钥文件为空")?;

        let static_key = PrivateKeyDer::from(key);

        // 验证证书
        if certs.is_empty() {
            return Err("证书为空".to_string());
        }

        // 创建 CertifiedKey
        let certified_key = CertifiedKey::from_der(certs, static_key, &provider)
            .map_err(|e| format!("创建 CertifiedKey 失败: {:?}", e))?;

        // 确定域名（优先使用配置中的域名，否则从证书提取）
        let domains = if cert_config.domains.is_empty() {
            // 从证书中提取域名（简化处理，使用第一个证书的主题）
            vec!["default".to_string()]
        } else {
            cert_config.domains.clone()
        };

        // 为每个域名添加证书
        for domain in &domains {
            sni_resolver.add(domain, certified_key.clone())
                .map_err(|e| format!("添加证书到 SNI 解析器失败: {:?}", e))?;
            info!("  ✓ 域名: {}", domain);
        }

        // 创建 ServerConfig，ALPN 只支持 h2
        let sni_resolver_arc = Arc::new(sni_resolver);

        // 根据是否配置了 CA 证书决定是否启用 mTLS
        let server_config = if let Some(ca_path) = &cert_config.ca_path {
            // 启用 mTLS（双向认证）
            info!("启用 mTLS，CA 证书: {}", ca_path.display());

            // 加载 CA 证书
            let ca_file = File::open(ca_path)
                .map_err(|e| format!("打开 CA 证书文件失败: {}", e))?;
            let mut ca_reader = BufReader::new(ca_file);
            let ca_certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut ca_reader)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("解析 CA 证书失败: {}", e))?
                .into_iter()
                .map(|cert| CertificateDer::from(cert.into_owned()))
                .collect();

            if ca_certs.is_empty() {
                return Err("CA 证书为空".to_string());
            }

            info!("  ✅ CA 证书已加载 ({} 个证书)", ca_certs.len());

            // 创建 RootCertStore 并添加 CA 证书
            let mut root_store = RootCertStore::empty();
            for ca_cert in ca_certs {
                root_store.add(ca_cert)
                    .map_err(|e| format!("添加 CA 证书到 RootCertStore 失败: {:?}", e))?;
            }

            info!("  ✅ RootCertStore 已创建，根证书数量: {}", root_store.len());

            // 创建客户端证书验证器（要求并验证客户端证书）
            let client_auth = WebPkiClientVerifier::builder(Arc::new(root_store))
                .build()
                .map_err(|e| format!("创建客户端证书验证器失败: {:?}", e))?;

            info!("  ✅ 客户端证书验证器已创建");

            ServerConfig::builder()
                .with_client_cert_verifier(client_auth)
                .with_cert_resolver(sni_resolver_arc.clone())
        } else {
            // 不启用 mTLS（单向认证）
            ServerConfig::builder()
                .with_no_client_auth()
                .with_cert_resolver(sni_resolver_arc.clone())
        };

        // 设置 ALPN 协议，同时支持 HTTP/2 和 HTTP/1.1
        // 这样 TLS 握手能成功完成，然后在应用层检查并拒绝非 HTTP/2
        let mut server_config = server_config;
        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let server_config = Arc::new(server_config);

        info!("证书加载完成，共 {} 个域名", domains.len());

        Ok(Self {
            sni_resolver: sni_resolver_arc,
            server_config,
            domains,
        })
    }

    /// 从证书目录加载所有证书（格式：domain.pem, domain-key.pem）
    pub fn from_dir(cert_dir: &str) -> Result<Self, String> {
        let path = Path::new(cert_dir);

        if !path.exists() {
            return Err(format!("证书目录不存在: {}", cert_dir));
        }

        let mut cert_files = HashMap::new();

        // 遍历目录中的 .pem 文件
        for entry in std::fs::read_dir(path)
            .map_err(|e| format!("无法读取证书目录: {}", e))?
        {
            let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("pem") {
                if let Some(filename) = path.file_stem() {
                    let filename = filename.to_string_lossy();

                    // 跳过密钥文件（以 -key.pem 结尾）
                    if filename.ends_with("-key") {
                        continue;
                    }

                    // 提取域名
                    let domain = if filename.contains('.') {
                        filename.to_string()
                    } else {
                        continue;
                    };

                    // 检查对应的密钥文件是否存在
                    let key_path = path.with_file_name(format!("{}-key.pem", domain));
                    if key_path.exists() {
                        cert_files.insert(domain.clone(), (path.to_string_lossy().to_string(), key_path.to_string_lossy().to_string()));
                        info!("发现证书: {} -> {}", domain, path.display());
                    }
                }
            }
        }

        if cert_files.is_empty() {
            return Err("证书目录中没有找到有效的证书".to_string());
        }

        // 获取 provider
        let provider = rustls::crypto::ring::default_provider();

        info!("预加载 SSL 证书...");

        // 创建 SNI 解析器
        let mut sni_resolver = ResolvesServerCertUsingSni::new();
        let mut domains = Vec::new();

        for (domain, (cert_path, key_path)) in cert_files {
            info!("  预加载证书: {}", domain);

            // 读取证书
            let cert_pem = std::fs::read(&cert_path)
                .map_err(|e| format!("读取证书文件失败: {}", e))?;
            let mut cert_cursor = std::io::Cursor::new(cert_pem);
            let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_cursor)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("解析证书失败: {}", e))?
                .into_iter()
                .map(|cert| CertificateDer::from(cert.into_owned()))
                .collect();

            // 读取私钥
            let key_pem = std::fs::read(&key_path)
                .map_err(|e| format!("读取私钥文件失败: {}", e))?;
            let mut key_cursor = std::io::Cursor::new(key_pem);
            let key = private_key(&mut key_cursor)
                .map_err(|e| format!("解析私钥失败: {}", e))?
                .ok_or_else(|| format!("未找到私钥: {}", key_path))?;

            let static_key = PrivateKeyDer::from(key);

            // 验证证书
            if certs.is_empty() {
                return Err(format!("证书为空: {}", domain));
            }

            // 创建 CertifiedKey
            let certified_key = CertifiedKey::from_der(certs, static_key, &provider)
                .map_err(|e| format!("创建 CertifiedKey 失败: {:?}", e))?;

            // 添加到 SNI 解析器
            sni_resolver.add(&domain, certified_key)
                .map_err(|e| format!("添加证书到 SNI 解析器失败: {:?}", e))?;

            domains.push(domain);
            info!("    ✓ 证书加载成功");
        }

        info!("SSL 证书预加载完成，共 {} 个证书", domains.len());

        // 创建 ServerConfig，ALPN 支持 HTTP/2 和 HTTP/1.1
        let sni_resolver_arc = Arc::new(sni_resolver);
        let mut server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(sni_resolver_arc.clone());

        // 设置 ALPN 协议，同时支持 HTTP/2 和 HTTP/1.1
        // 这样 TLS 握手能成功完成，然后在应用层检查并拒绝非 HTTP/2
        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let server_config = Arc::new(server_config);

        Ok(Self {
            sni_resolver: sni_resolver_arc,
            server_config,
            domains,
        })
    }

    /// 获取 ServerConfig
    pub fn get_server_config(&self) -> Arc<ServerConfig> {
        self.server_config.clone()
    }

    /// 获取支持的域名列表
    pub fn get_domains(&self) -> &[String] {
        &self.domains
    }

    /// 检查是否支持某个域名
    pub fn supports_domain(&self, domain: &str) -> bool {
        self.domains.contains(&domain.to_string())
    }
}

/// ALPN 协议检查工具
pub struct AlpnProtocol;

impl AlpnProtocol {
    /// 检查是否为 HTTP/2 连接
    pub fn is_http2(protocol: &Option<Vec<u8>>) -> bool {
        matches!(protocol, Some(p) if p == b"h2")
    }

    /// 检查是否为 HTTP/1.1 连接
    pub fn is_http11(protocol: &Option<Vec<u8>>) -> bool {
        matches!(protocol, Some(p) if p == b"http/1.1")
            || protocol.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpn_check() {
        assert!(AlpnProtocol::is_http2(&Some(b"h2".to_vec())));
        assert!(!AlpnProtocol::is_http2(&Some(b"http/1.1".to_vec())));
        assert!(!AlpnProtocol::is_http2(&None));
    }
}
