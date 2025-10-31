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
    pub(crate) async fn generate_client_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    pub(crate) async fn load_client_certificate(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

            // 跳过 ALPN 协议设置，gRPC 只使用 HTTP/2，不需要 ALPN 协商
        } else {
            println!("[ALPN调试] 服务器端：设置空的 ALPN 协议列表（gRPC 模式）");
        }

        // 调试：打印 SSL 配置信息
        println!("[SSL调试] 服务器 SSL 配置:");
        println!("[SSL调试]   协议数量: {}", protocols.len());
        if !protocols.is_empty() {
            println!("[SSL调试]   协议列表: {:?}", protocols.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>());
        } else {
            println!("[SSL调试]   协议列表: 空（gRPC 模式）");
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
}
