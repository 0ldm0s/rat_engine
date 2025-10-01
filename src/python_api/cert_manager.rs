//! Python 证书管理模块
//! 
//! 提供 ACME 证书管理和 TLS 配置的 Python 接口

use pyo3::prelude::*;
use pyo3::types::PyType;
use crate::server::cert_manager::{CertManagerConfig, CertManagerBuilder, CertificateManager};
use crate::utils::logger::{info, warn, error};

/// Python 证书管理器配置类
/// 
/// 提供 ACME 自动证书管理、生产环境证书配置和 mTLS 双向认证功能
#[pyclass(name = "CertManagerConfig")]
pub struct PyCertManagerConfig {
    /// 是否启用 ACME 自动证书管理
    pub acme_enabled: bool,
    /// 是否使用 ACME 生产环境（Let's Encrypt 生产环境）
    pub acme_production: bool,
    /// ACME 注册邮箱地址
    pub acme_email: Option<String>,
    /// Cloudflare API Token（用于 DNS-01 挑战）
    pub cloudflare_api_token: Option<String>,
    /// 证书续期天数阈值（默认30天）
    pub acme_renewal_days: u64,
    /// ACME 证书存储目录
    pub acme_cert_dir: String,
    /// 生产环境证书文件路径
    pub cert_file: Option<String>,
    /// 生产环境私钥文件路径
    pub key_file: Option<String>,
    /// 是否启用 mTLS 双向认证
    pub mtls_enabled: bool,
    /// 客户端证书文件路径
    pub client_cert_path: Option<String>,
    /// 客户端私钥文件路径
    pub client_key_path: Option<String>,
    /// 客户端 CA 证书路径
    pub client_ca_path: Option<String>,
    /// mTLS 模式："self_signed" 或 "acme_mixed"
    pub mtls_mode: String,
    /// 是否自动生成客户端证书
    pub auto_generate_client_cert: bool,
    /// 客户端证书主题信息
    pub client_cert_subject: Option<String>,
}

#[pymethods]
impl PyCertManagerConfig {
    /// 创建新的证书管理器配置
    /// 
    /// # 参数
    /// - `acme_enabled`: 是否启用 ACME 自动证书管理，默认为 False
    /// - `acme_production`: 是否使用 ACME 生产环境，默认为 False（使用测试环境）
    /// - `acme_email`: ACME 注册邮箱地址，启用 ACME 时必须提供
    /// - `cloudflare_api_token`: Cloudflare API Token，用于 DNS-01 挑战验证
    /// - `acme_renewal_days`: 证书续期天数阈值，默认为 30 天
    /// - `acme_cert_dir`: ACME 证书存储目录，默认为 "./certs"
    /// - `cert_file`: 生产环境证书文件路径
    /// - `key_file`: 生产环境私钥文件路径
    /// - `mtls_enabled`: 是否启用 mTLS 双向认证，默认为 False
    /// - `client_cert_path`: 客户端证书文件路径
    /// - `client_key_path`: 客户端私钥文件路径
    /// - `client_ca_path`: 客户端 CA 证书路径
    /// - `mtls_mode`: mTLS 模式，"self_signed" 或 "acme_mixed"，默认为 "self_signed"
    /// - `auto_generate_client_cert`: 是否自动生成客户端证书，默认为 False
    /// - `client_cert_subject`: 客户端证书主题信息
    /// 
    /// # 返回值
    /// - PyCertManagerConfig: 证书管理器配置实例
    #[new]
    #[pyo3(signature = (
        acme_enabled = false,
        acme_production = false,
        acme_email = None,
        cloudflare_api_token = None,
        acme_renewal_days = 30,
        acme_cert_dir = "./certs".to_string(),
        cert_file = None,
        key_file = None,
        mtls_enabled = false,
        client_cert_path = None,
        client_key_path = None,
        client_ca_path = None,
        mtls_mode = "self_signed".to_string(),
        auto_generate_client_cert = false,
        client_cert_subject = None
    ))]
    fn new(
        acme_enabled: bool,
        acme_production: bool,
        acme_email: Option<String>,
        cloudflare_api_token: Option<String>,
        acme_renewal_days: u64,
        acme_cert_dir: String,
        cert_file: Option<String>,
        key_file: Option<String>,
        mtls_enabled: bool,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
        client_ca_path: Option<String>,
        mtls_mode: String,
        auto_generate_client_cert: bool,
        client_cert_subject: Option<String>,
    ) -> PyResult<Self> {
        // 验证配置的合法性
        if acme_enabled {
            if acme_email.is_none() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "启用 ACME 时必须提供 acme_email 参数"
                ));
            }
            
            if acme_production {
                warn!("使用 ACME 生产环境，请确保域名配置正确");
            } else {
                info!("使用 ACME 测试环境（Let's Encrypt Staging）");
            }
        }
        
        if cert_file.is_some() && key_file.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "提供 cert_file 时必须同时提供 key_file"
            ));
        }
        
        if key_file.is_some() && cert_file.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "提供 key_file 时必须同时提供 cert_file"
            ));
        }
        
        // 验证 mTLS 配置
        if mtls_enabled {
            // 验证 mTLS 模式
            if mtls_mode != "self_signed" && mtls_mode != "acme_mixed" {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "mtls_mode 必须是 'self_signed' 或 'acme_mixed'"
                ));
            }
            
            // 如果不是自动生成客户端证书，则需要提供证书路径
            if !auto_generate_client_cert {
                if client_cert_path.is_none() || client_key_path.is_none() {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "启用 mTLS 且不自动生成证书时，必须提供 client_cert_path 和 client_key_path"
                    ));
                }
            }
            
            // ACME 混合模式需要启用 ACME
            if mtls_mode == "acme_mixed" && !acme_enabled {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "使用 acme_mixed 模式时必须启用 ACME (acme_enabled=True)"
                ));
            }
            
            info!("启用 mTLS 双向认证，模式: {}", mtls_mode);
        }
        
        Ok(Self {
            acme_enabled,
            acme_production,
            acme_email,
            cloudflare_api_token,
            acme_renewal_days,
            acme_cert_dir,
            cert_file,
            key_file,
            mtls_enabled,
            client_cert_path,
            client_key_path,
            client_ca_path,
            mtls_mode,
            auto_generate_client_cert,
            client_cert_subject,
        })
    }
    
    /// 创建 ACME 配置的便捷方法
    /// 
    /// # 参数
    /// - `email`: ACME 注册邮箱地址
    /// - `production`: 是否使用生产环境，默认为 False
    /// - `cloudflare_token`: Cloudflare API Token，可选
    /// - `renewal_days`: 证书续期天数阈值，默认为 30 天
    /// - `cert_dir`: 证书存储目录，默认为 "./certs"
    /// 
    /// # 返回值
    /// - PyCertManagerConfig: 配置了 ACME 的证书管理器配置实例
    #[classmethod]
    #[pyo3(signature = (
        email,
        production = false,
        cloudflare_token = None,
        renewal_days = 30,
        cert_dir = "./certs".to_string()
    ))]
    fn acme_config(
        _cls: &PyType,
        email: String,
        production: bool,
        cloudflare_token: Option<String>,
        renewal_days: u64,
        cert_dir: String,
    ) -> PyResult<Self> {
        info!("创建 ACME 证书配置: email={}, production={}", email, production);
        
        Ok(Self {
            acme_enabled: true,
            acme_production: production,
            acme_email: Some(email),
            cloudflare_api_token: cloudflare_token,
            acme_renewal_days: renewal_days,
            acme_cert_dir: cert_dir,
            cert_file: None,
            key_file: None,
            mtls_enabled: false,
            client_cert_path: None,
            client_key_path: None,
            client_ca_path: None,
            mtls_mode: "self_signed".to_string(),
            auto_generate_client_cert: false,
            client_cert_subject: None,
        })
    }
    
    /// 创建生产环境证书配置的便捷方法
    /// 
    /// # 参数
    /// - `cert_file`: 证书文件路径
    /// - `key_file`: 私钥文件路径
    /// 
    /// # 返回值
    /// - PyCertManagerConfig: 配置了生产环境证书的证书管理器配置实例
    #[classmethod]
    fn production_config(_cls: &PyType, cert_file: String, key_file: String) -> PyResult<Self> {
        info!("创建生产环境证书配置: cert={}, key={}", cert_file, key_file);
        
        Ok(Self {
            acme_enabled: false,
            acme_production: false,
            acme_email: None,
            cloudflare_api_token: None,
            acme_renewal_days: 30,
            acme_cert_dir: "./certs".to_string(),
            cert_file: Some(cert_file),
            key_file: Some(key_file),
            mtls_enabled: false,
            client_cert_path: None,
            client_key_path: None,
            client_ca_path: None,
            mtls_mode: "self_signed".to_string(),
            auto_generate_client_cert: false,
            client_cert_subject: None,
        })
    }
    
    /// 创建自签名 mTLS 配置的便捷方法
    /// 
    /// # 参数
    /// - `auto_generate`: 是否自动生成客户端证书，默认为 True
    /// - `client_cert_subject`: 客户端证书主题信息，默认为 "CN=Client,O=RAT Engine"
    /// - `client_cert_path`: 客户端证书路径（当 auto_generate=False 时必须提供）
    /// - `client_key_path`: 客户端私钥路径（当 auto_generate=False 时必须提供）
    /// - `cert_dir`: 证书存储目录，默认为 "./certs"
    /// 
    /// # 返回值
    /// - PyCertManagerConfig: 自签名 mTLS 配置实例
    #[classmethod]
    #[pyo3(signature = (
        auto_generate = true,
        client_cert_subject = None,
        client_cert_path = None,
        client_key_path = None,
        cert_dir = "./certs".to_string()
    ))]
    fn mtls_self_signed_config(
        _cls: &PyType,
        auto_generate: bool,
        client_cert_subject: Option<String>,
        client_cert_path: Option<String>,
        client_key_path: Option<String>,
        cert_dir: String,
    ) -> PyResult<Self> {
        info!("创建自签名 mTLS 配置: auto_generate={}", auto_generate);
        
        let subject = client_cert_subject.unwrap_or_else(|| "CN=Client,O=RAT Engine".to_string());
        
        // 在自动生成模式下，如果没有指定客户端证书路径，使用默认路径
        let (final_client_cert_path, final_client_key_path) = if auto_generate {
            let cert_path = client_cert_path.unwrap_or_else(|| format!("{}/client.crt", cert_dir));
            let key_path = client_key_path.unwrap_or_else(|| format!("{}/client.key", cert_dir));
            (Some(cert_path), Some(key_path))
        } else {
            (client_cert_path, client_key_path)
        };
        
        Ok(Self {
            acme_enabled: false,
            acme_production: false,
            acme_email: None,
            cloudflare_api_token: None,
            acme_renewal_days: 30,
            acme_cert_dir: cert_dir,
            cert_file: None,
            key_file: None,
            mtls_enabled: true,
            client_cert_path: final_client_cert_path,
            client_key_path: final_client_key_path,
            client_ca_path: None,
            mtls_mode: "self_signed".to_string(),
            auto_generate_client_cert: auto_generate,
            client_cert_subject: Some(subject),
        })
    }
    
    /// 创建 ACME 混合 mTLS 配置的便捷方法
    /// 
    /// # 参数
    /// - `email`: ACME 注册邮箱地址
    /// - `production`: 是否使用 ACME 生产环境，默认为 False
    /// - `cloudflare_token`: Cloudflare API Token，可选
    /// - `auto_generate_client`: 是否自动生成客户端证书，默认为 True
    /// - `client_cert_subject`: 客户端证书主题信息，默认为 "CN=Client,O=RAT Engine"
    /// - `cert_dir`: 证书存储目录，默认为 "./certs"
    /// 
    /// # 返回值
    /// - PyCertManagerConfig: ACME 混合 mTLS 配置实例
    #[classmethod]
    #[pyo3(signature = (
        email,
        production = false,
        cloudflare_token = None,
        auto_generate_client = true,
        client_cert_subject = None,
        cert_dir = "./certs".to_string()
    ))]
    fn mtls_acme_mixed_config(
        _cls: &PyType,
        email: String,
        production: bool,
        cloudflare_token: Option<String>,
        auto_generate_client: bool,
        client_cert_subject: Option<String>,
        cert_dir: String,
    ) -> PyResult<Self> {
        info!("创建 ACME 混合 mTLS 配置: email={}, production={}", email, production);
        
        let subject = client_cert_subject.unwrap_or_else(|| "CN=Client,O=RAT Engine".to_string());
        
        Ok(Self {
            acme_enabled: true,
            acme_production: production,
            acme_email: Some(email),
            cloudflare_api_token: cloudflare_token,
            acme_renewal_days: 30,
            acme_cert_dir: cert_dir,
            cert_file: None,
            key_file: None,
            mtls_enabled: true,
            client_cert_path: None,
            client_key_path: None,
            client_ca_path: None,
            mtls_mode: "acme_mixed".to_string(),
            auto_generate_client_cert: auto_generate_client,
            client_cert_subject: Some(subject),
        })
    }
    
    /// 获取配置的字符串表示
    fn __repr__(&self) -> String {
        if self.acme_enabled {
            format!(
                "CertManagerConfig(acme_enabled=True, acme_production={}, acme_email={:?}, renewal_days={}, cert_dir='{}')",
                self.acme_production,
                self.acme_email,
                self.acme_renewal_days,
                self.acme_cert_dir
            )
        } else if self.cert_file.is_some() {
            format!(
                "CertManagerConfig(production_certs=True, cert_file={:?}, key_file={:?})",
                self.cert_file,
                self.key_file
            )
        } else {
            "CertManagerConfig(disabled)".to_string()
        }
    }
    
    /// 验证配置是否有效
    /// 
    /// # 返回值
    /// - bool: 配置是否有效
    pub fn is_valid(&self) -> bool {
        if self.acme_enabled {
            self.acme_email.is_some()
        } else if self.cert_file.is_some() || self.key_file.is_some() {
            self.cert_file.is_some() && self.key_file.is_some()
        } else {
            true // 禁用证书管理也是有效的配置
        }
    }
    
    /// 获取配置类型描述
    /// 
    /// # 返回值
    /// - str: 配置类型描述
    fn get_config_type(&self) -> &str {
        if self.acme_enabled {
            if self.acme_production {
                "ACME 生产环境"
            } else {
                "ACME 测试环境"
            }
        } else if self.cert_file.is_some() {
            "生产环境证书"
        } else {
            "禁用证书管理"
        }
    }
}

impl PyCertManagerConfig {
    /// 将 Python 配置转换为 Rust 配置
    pub fn to_cert_manager_config(&self) -> PyResult<CertManagerConfig> {
        let mut config = CertManagerBuilder::new();

        // ACME 配置
        if self.acme_enabled {
            config = config.enable_acme(true);
            if self.acme_production {
                config = config.with_acme_production(true);
            }
            if let Some(email) = &self.acme_email {
                config = config.with_acme_email(email.clone());
            }
            if let Some(token) = &self.cloudflare_api_token {
                config = config.with_cloudflare_api_token(token.clone());
            }
            config = config.with_acme_renewal_days(self.acme_renewal_days as u32);
            if !self.acme_cert_dir.is_empty() {
                config = config.with_acme_cert_dir(self.acme_cert_dir.clone());
            }
        }

        // 生产环境证书配置
        if let Some(cert_file) = &self.cert_file {
            config = config.with_cert_path(cert_file.clone());
        }
        if let Some(key_file) = &self.key_file {
            config = config.with_key_path(key_file.clone());
        }

        // mTLS 配置
        if self.mtls_enabled {
            config = config.enable_mtls(true);
            
            config = config.with_mtls_mode(self.mtls_mode.clone());
            
            if self.auto_generate_client_cert {
                config = config.auto_generate_client_cert(true);
                if let Some(subject) = &self.client_cert_subject {
                    config = config.with_client_cert_subject(subject.clone());
                }
                // 即使是自动生成模式，也要设置客户端证书路径用于保存
                if let Some(cert_path) = &self.client_cert_path {
                    config = config.with_client_cert_path(cert_path.clone());
                }
                if let Some(key_path) = &self.client_key_path {
                    config = config.with_client_key_path(key_path.clone());
                }
            } else {
                if let Some(cert_path) = &self.client_cert_path {
                    config = config.with_client_cert_path(cert_path.clone());
                }
                if let Some(key_path) = &self.client_key_path {
                    config = config.with_client_key_path(key_path.clone());
                }
            }
            
            if let Some(ca_path) = &self.client_ca_path {
                config = config.with_client_ca_path(ca_path.clone());
            }
        }

        Ok(config.build_config())
    }
}

/// 注册证书管理模块
pub fn register_cert_manager_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    parent_module.add_class::<PyCertManagerConfig>()?;
    Ok(())
}