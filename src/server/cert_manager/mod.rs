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

pub mod config;
pub mod certificate_info;
pub mod manager;
pub mod core;
pub mod dev_cert;
pub mod cert_validation;
#[cfg(feature = "acme")]
pub mod acme_integration;
pub mod client_cert;
pub mod auto_refresh;
pub mod builder;

pub use config::CertManagerConfig;
pub use certificate_info::CertificateInfo;
pub use manager::CertificateManager;
pub use builder::CertManagerBuilder;