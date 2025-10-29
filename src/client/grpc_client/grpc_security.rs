    //! gRPC 客户端安全模块

use std::sync::Arc;
use std::io::Read;

use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use openssl::pkey::{PKey, Private};
use h2;

use crate::error::{RatError, RatResult};
use crate::client::grpc_builder::MtlsClientConfig;
use crate::utils::logger::{info, warn, debug};
use crate::client::grpc_client::RatGrpcClient;

impl RatGrpcClient {
    pub fn create_tls_config(&self) -> RatResult<SslConnector> {
        println!("[客户端ALPN调试] 创建 TLS 配置，development_mode={}", self.development_mode);
        // 检查是否有 mTLS 配置
        if let Some(mtls_config) = &self.mtls_config {
            info!("🔐 启用 mTLS 客户端证书认证");

            // 创建 OpenSSL SSL 连接器
            let mut ssl_connector = SslConnector::builder(SslMethod::tls())
                .map_err(|e| RatError::TlsError(format!("创建 SSL 连接器失败: {}", e)))?;

  
            // 配置 CA 证书
            if let Some(ca_certs) = &mtls_config.ca_certs {
                // 使用自定义 CA 证书
                for ca_cert_der in ca_certs {
                    let ca_cert = X509::from_der(ca_cert_der)
                        .map_err(|e| RatError::TlsError(format!("解析 CA 证书失败: {}", e)))?;
                    ssl_connector.cert_store_mut()
                        .add_cert(ca_cert)
                        .map_err(|e| RatError::TlsError(format!("添加 CA 证书到存储失败: {}", e)))?;
                }
                info!("✅ 已加载 {} 个自定义 CA 证书", ca_certs.len());
            } else {
                // 使用系统默认根证书
                ssl_connector.set_default_verify_paths()
                    .map_err(|e| RatError::TlsError(format!("设置默认证书路径失败: {}", e)))?;
                info!("✅ 已加载系统默认根证书");
            }

            // 配置客户端证书和私钥
            let client_cert_chain = &mtls_config.client_cert_chain;
            let client_private_key = mtls_config.client_private_key.clone();

            // 使用第一个证书作为客户端证书
            if let Some(client_cert_der) = client_cert_chain.first() {
                let client_cert = X509::from_der(client_cert_der)
                    .map_err(|e| RatError::TlsError(format!("解析客户端证书失败: {}", e)))?;

                // 从私钥中提取私钥
                let private_key = PKey::private_key_from_der(&client_private_key)
                    .map_err(|e| RatError::TlsError(format!("解析私钥失败: {}", e)))?;

                ssl_connector.set_certificate(&client_cert)
                    .map_err(|e| RatError::TlsError(format!("设置客户端证书失败: {}", e)))?;
                ssl_connector.set_private_key(&private_key)
                    .map_err(|e| RatError::TlsError(format!("设置私钥失败: {}", e)))?;
                ssl_connector.check_private_key()
                    .map_err(|e| RatError::TlsError(format!("私钥证书匹配检查失败: {}", e)))?;

                info!("✅ 客户端证书配置完成");
            } else {
                return Err(RatError::TlsError("未找到客户端证书".to_string()));
            }

            // 设置 ALPN 协议 - gRPC 只支持 HTTP/2
            ssl_connector.set_alpn_protos(b"\x02h2")?;
            println!("[客户端ALPN调试] mTLS 模式设置 ALPN 协议: h2");

            if mtls_config.skip_server_verification {
                // 跳过服务器证书验证（仅用于测试）
                warn!("⚠️  警告：已启用跳过服务器证书验证模式！仅用于测试环境！");
                ssl_connector.set_verify(SslVerifyMode::NONE);
            } else {
                // 正常的服务器证书验证
                ssl_connector.set_verify(SslVerifyMode::PEER);
            }

            info!("✅ mTLS 客户端配置完成");
            Ok(ssl_connector.build())
        } else if self.development_mode {
            // 开发模式：跳过证书验证
            warn!("⚠️  警告：gRPC 客户端已启用开发模式，将跳过所有 TLS 证书验证！仅用于开发环境！");

            let mut ssl_connector = SslConnector::builder(SslMethod::tls())
                .map_err(|e| RatError::TlsError(format!("创建 SSL 连接器失败: {}", e)))?;

            // 设置 ALPN 协议 - gRPC 只支持 HTTP/2
            ssl_connector.set_alpn_protos(b"\x02h2")?;
            println!("[客户端ALPN调试] 开发模式设置 ALPN 协议: h2");

            // 跳过证书验证
            ssl_connector.set_verify(SslVerifyMode::NONE);

            // 开发模式下保持标准协议版本，仅跳过证书验证

            info!("✅ 开发模式 SSL 连接器配置完成");
            Ok(ssl_connector.build())
        } else {
            // 非开发模式：严格证书验证
            let mut ssl_connector = SslConnector::builder(SslMethod::tls())
                .map_err(|e| RatError::TlsError(format!("创建 SSL 连接器失败: {}", e)))?;

  
            // 设置系统默认证书路径
            ssl_connector.set_default_verify_paths()
                .map_err(|e| RatError::TlsError(format!("设置默认证书路径失败: {}", e)))?;

            // 设置 ALPN 协议 - gRPC 只支持 HTTP/2
            ssl_connector.set_alpn_protos(b"\x02h2")?;
            println!("[客户端ALPN调试] 标准模式设置 ALPN 协议: h2");

            // 严格证书验证
            ssl_connector.set_verify(SslVerifyMode::PEER);

            info!("✅ 标准模式 SSL 连接器配置完成");
            Ok(ssl_connector.build())
        }
    }
}
