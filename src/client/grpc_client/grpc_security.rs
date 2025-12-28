//! gRPC 客户端安全模块（rustls）
//!
//! 专注于 TLS/SSL 配置，使用 rustls-platform-verifier 加载系统证书

use std::sync::Arc;

use rustls::{
    ClientConfig,
    crypto::ring::default_provider,
    pki_types::{ServerName, CertificateDer},
};

use crate::error::{RatError, RatResult};
use crate::utils::logger::{info, warn};
use crate::client::grpc_client::RatGrpcClient;

// 导入 BuilderVerifierExt trait 以使用 with_platform_verifier()
use rustls_platform_verifier::BuilderVerifierExt;

impl RatGrpcClient {
    pub fn create_tls_config(&self) -> RatResult<Arc<ClientConfig>> {
        // 确保 CryptoProvider 已安装
        crate::utils::crypto_provider::ensure_crypto_provider_installed();

        if self.development_mode {
            warn!("⚠️  警告：gRPC 客户端已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");
            return self.create_development_config();
        } else {
            info!("✅ 使用标准 TLS 配置（系统证书）");
            return self.create_standard_config();
        }
    }

    fn create_development_config(&self) -> RatResult<Arc<ClientConfig>> {
        // 开发模式：跳过证书验证
        let mut config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerification))
            .with_no_client_auth();

        // h2c-over-TLS 模式：禁用 ALPN，让代理认为是普通 TLS
        if self.h2c_over_tls {
            // 不设置 ALPN
            info!("✅ 开发模式 TLS 配置完成（h2c-over-TLS 模式：无 ALPN）");
        } else {
            // 设置 ALPN 协议，gRPC 强制 HTTP/2
            config.alpn_protocols = vec![b"h2".to_vec()];
            info!("✅ 开发模式 TLS 配置完成（ALPN: h2）");
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
