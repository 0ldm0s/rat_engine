//! 证书管理模块（基于 rustls + ring）
//!
//! 提供基于 rustls 的证书管理功能
//! 强制使用 ring 作为加密后端

pub mod config;
pub mod rustls_cert;
pub mod manager;

pub use config::{CertManagerConfig, CertConfig};
pub use rustls_cert::RustlsCertManager;
pub use manager::CertificateManager;
