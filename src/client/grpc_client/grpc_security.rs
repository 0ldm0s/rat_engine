//! gRPC 客户端安全模块（rustls）
//!
//! 专注于 TLS/SSL 和 mTLS 配置，使用 rustls + ring

use std::sync::Arc;

use rustls::{
    ClientConfig,
    crypto::ring::default_provider,
    pki_types::{ServerName, CertificateDer, PrivateKeyDer},
};

use crate::error::{RatError, RatResult};
use crate::utils::logger::{info, warn};
use crate::client::grpc_client::RatGrpcClient;
use crate::client::grpc_builder::MtlsClientConfig;

// 导入 BuilderVerifierExt trait 以使用 with_platform_verifier()
use rustls_platform_verifier::BuilderVerifierExt;

impl RatGrpcClient {
    pub fn create_tls_config(&self) -> RatResult<Arc<ClientConfig>> {
        println!("🔧 [TLS] 开始创建 TLS 配置，h2c_mode={}, h2c_over_tls={}, has_mtls={}",
            self.h2c_mode, self.h2c_over_tls, self.mtls_config.is_some());

        // 确保 CryptoProvider 已安装
        crate::utils::crypto_provider::ensure_crypto_provider_installed();

        // 如果配置了 mTLS，使用 mTLS 配置
        if let Some(ref mtls_config) = self.mtls_config {
            println!("🔐 [TLS] 检测到 mTLS 配置，调用 mTLS 配置");
            return self.create_mtls_config(mtls_config);
        }

        if self.h2c_mode {
            warn!("⚠️  警告：gRPC 客户端已启用 h2c-over-TLS 模式，将跳过所有 TLS 证书验证！仅用于通过 HTTP 代理传输！");
            return self.create_skip_verification_config();
        } else {
            info!("✅ 使用标准 TLS 配置（系统证书）");
            return self.create_standard_config();
        }
    }

    fn create_mtls_config(&self, mtls_config: &MtlsClientConfig) -> RatResult<Arc<ClientConfig>> {
        println!("🔑 [mTLS] 开始创建 mTLS 配置");

        let provider = Arc::new(default_provider());
        println!("✅ [mTLS] CryptoProvider 和协议版本配置完成");

        // 配置客户端证书
        println!("📜 [mTLS] 配置客户端证书链，数量: {}", mtls_config.client_cert_chain.len());
        let cert_chain: Vec<CertificateDer<'static>> = mtls_config.client_cert_chain
            .iter()
            .map(|c| {
                println!("   [证书] 大小: {} 字节，DER 编码...", c.len());
                CertificateDer::from(c.to_vec())
            })
            .collect();
        println!("   证书链 DER 编码完成，最终证书链数量: {}", cert_chain.len());

        // 从 PEM 格式解析私钥
        println!("🔐 [mTLS] 解析客户端私钥，PEM 大小: {} 字节", mtls_config.client_private_key.len());
        let private_key = rustls_pemfile::private_key(&mut mtls_config.client_private_key.as_slice())
            .map_err(|e| RatError::RequestError(format!("解析客户端私钥失败: {}", e)))?
            .ok_or_else(|| RatError::RequestError("客户端私钥为空".to_string()))?;
        println!("   私钥解析成功");

        // mTLS 模式：跳过服务器证书验证（仅用于开发/测试）
        warn!("🔓 [mTLS] 跳过服务器证书验证（仅用于 mTLS 开发环境）");

        let mut config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerification))
            .with_client_auth_cert(cert_chain, private_key)
            .map_err(|e| RatError::RequestError(format!("配置客户端证书失败: {}", e)))?;
        println!("✅ [mTLS] 客户端证书配置成功");

        // 设置 ALPN 协议
        if self.h2c_over_tls {
            println!("📡 [mTLS] h2c-over-TLS 模式：不设置 ALPN");
        } else {
            config.alpn_protocols = vec![b"h2".to_vec()];
            println!("📡 [mTLS] 标准 ALPN: h2");
        }

        println!("✅ [mTLS] 配置完成");
        Ok(Arc::new(config))
    }

    fn create_skip_verification_config(&self) -> RatResult<Arc<ClientConfig>> {
        // h2c-over-TLS 模式：跳过证书验证（用于通过 HAProxy 等 HTTP 代理传输）
        let mut config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerification))
            .with_no_client_auth();

        // h2c-over-TLS 模式：禁用 ALPN，让代理认为是普通 TLS
        if self.h2c_over_tls {
            // 不设置 ALPN
            info!("✅ h2c代理模式 TLS 配置完成（h2c-over-TLS 模式：无 ALPN）");
        } else {
            // 设置 ALPN 协议，gRPC 强制 HTTP/2
            config.alpn_protocols = vec![b"h2".to_vec()];
            info!("✅ h2c代理模式 TLS 配置完成（ALPN: h2）");
        }

        Ok(Arc::new(config))
    }

    fn create_standard_config(&self) -> RatResult<Arc<ClientConfig>> {
        // 使用 rustls-platform-verifier 加载系统证书
        // 这会自动使用操作系统的原生证书存储：
        // - Windows: Crypt32/Schannel
        // - macOS: Security Framework
        // - Linux: OpenSSL/系统证书存储
        // - Android/iOS: 平台原生 API
        let provider = Arc::new(default_provider());
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_platform_verifier()
            .with_no_client_auth();

        // h2c-over-TLS 模式：禁用 ALPN，让代理认为是普通 TLS
        if self.h2c_over_tls {
            // 不设置 ALPN
            info!("✅ 标准模式 TLS 配置完成（h2c-over-TLS 模式：无 ALPN，使用系统证书）");
        } else {
            // 设置 ALPN 协议，gRPC 强制 HTTP/2
            config.alpn_protocols = vec![b"h2".to_vec()];
            info!("✅ 标准模式 TLS 配置完成（使用系统证书，ALPN: h2）");
        }

        Ok(Arc::new(config))
    }
}

/// 跳过证书验证（仅用于开发/测试）
#[derive(Debug)]
struct NoVerification;

impl rustls::client::danger::ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA1,
            rustls::SignatureScheme::ECDSA_SHA1_Legacy,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}
