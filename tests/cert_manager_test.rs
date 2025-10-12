//! 证书管理器测试
//!
//! 测试 OpenSSL 证书自动生成功能

use std::sync::{Arc, RwLock};
use rat_engine::server::cert_manager::{CertificateManager, CertManagerConfig};
use openssl::x509::X509;
use openssl::pkey::PKey;
use std::fs;

#[tokio::test]
async fn test_certificate_auto_generation() {
    // 创建测试证书目录
    let cert_dir = "./test_certs";
    let _ = fs::remove_dir_all(cert_dir); // 清理之前的测试
    let _ = fs::create_dir_all(cert_dir);

    // 配置证书管理器（开发模式，自动生成）
    let config = CertManagerConfig {
        development_mode: true,
        cert_path: None, // 开发模式不需要指定，使用acme_cert_dir
        key_path: None,
        ca_path: None,
        validity_days: 365,
        hostnames: vec!["localhost".to_string(), "127.0.0.1".to_string()],
        acme_enabled: false,
        acme_production: false,
        acme_email: None,
        cloudflare_api_token: None,
        acme_renewal_days: 30,
        acme_cert_dir: Some(cert_dir.to_string()), // 使用这个字段来指定证书保存目录
        mtls_enabled: true,
        client_cert_path: Some(format!("{}/client.crt", cert_dir)),
        client_key_path: Some(format!("{}/client.key", cert_dir)),
        client_ca_path: Some(format!("{}/ca.crt", cert_dir)),
        mtls_mode: Some("self_signed".to_string()),
        auto_generate_client_cert: true,
        client_cert_subject: Some("CN=Test Client,O=RAT Engine,C=CN".to_string()),
        auto_refresh_enabled: false, // 测试时禁用后台刷新
        refresh_check_interval: 3600,
        force_cert_rotation: false,
        mtls_whitelist_paths: Vec::new(),
    };

    // 创建并初始化证书管理器
    let mut cert_manager = CertificateManager::new(config.clone());
    let init_result = cert_manager.initialize().await;

    // 验证初始化成功
    assert!(init_result.is_ok(), "证书管理器初始化失败: {:?}", init_result.err());

    // 验证服务器证书生成
    let server_cert_path = format!("{}/server.crt", cert_dir);
    let server_key_path = format!("{}/server.key", cert_dir);

    assert!(fs::metadata(&server_cert_path).is_ok(), "服务器证书文件未生成: {}", server_cert_path);
    assert!(fs::metadata(&server_key_path).is_ok(), "服务器私钥文件未生成: {}", server_key_path);

    // 验证客户端证书生成
    let client_cert_path = format!("{}/client.crt", cert_dir);
    let client_key_path = format!("{}/client.key", cert_dir);

    assert!(fs::metadata(&client_cert_path).is_ok(), "客户端证书文件未生成: {}", client_cert_path);
    assert!(fs::metadata(&client_key_path).is_ok(), "客户端私钥文件未生成: {}", client_key_path);

    // 验证CA证书生成
    let ca_cert_path = format!("{}/ca.crt", cert_dir);
    assert!(fs::metadata(&ca_cert_path).is_ok(), "CA证书文件未生成: {}", ca_cert_path);

    // 测试证书加载和解析
    let server_cert_data = fs::read(&server_cert_path).expect("读取服务器证书失败");
    let server_cert = X509::from_pem(&server_cert_data).expect("解析服务器证书失败");

    let client_cert_data = fs::read(&client_cert_path).expect("读取客户端证书失败");
    let client_cert = X509::from_pem(&client_cert_data).expect("解析客户端证书失败");

    let ca_cert_data = fs::read(&ca_cert_path).expect("读取CA证书失败");
    let ca_cert = X509::from_pem(&ca_cert_data).expect("解析CA证书失败");

    // 验证证书不是空的
    assert!(!server_cert.to_der().unwrap().is_empty(), "服务器证书内容为空");
    assert!(!client_cert.to_der().unwrap().is_empty(), "客户端证书内容为空");
    assert!(!ca_cert.to_der().unwrap().is_empty(), "CA证书内容为空");

    // 测试私钥加载
    let server_key_data = fs::read(&server_key_path).expect("读取服务器私钥失败");
    let server_key = PKey::private_key_from_pem(&server_key_data).expect("解析服务器私钥失败");

    let client_key_data = fs::read(&client_key_path).expect("读取客户端私钥失败");
    let client_key = PKey::private_key_from_pem(&client_key_data).expect("解析客户端私钥失败");

    // 验证私钥不是空的
    assert!(!server_key.private_key_to_pem_pkcs8().unwrap().is_empty(), "服务器私钥内容为空");
    assert!(!client_key.private_key_to_pem_pkcs8().unwrap().is_empty(), "客户端私钥内容为空");

    // 测试SSL配置创建
    let cert_manager = Arc::new(RwLock::new(cert_manager));

    // 获取服务器配置
    {
        let manager = cert_manager.read().unwrap();
        let server_config = manager.get_server_config();
        assert!(server_config.is_some(), "服务器SSL配置创建失败");
    }

    // 获取客户端配置
    {
        let manager = cert_manager.read().unwrap();
        let client_config = manager.get_client_config();
        assert!(client_config.is_some(), "客户端SSL配置创建失败");
    }

    // 清理测试文件
    let _ = fs::remove_dir_all(cert_dir);

    println!("✅ 证书自动生成测试通过");
}

#[tokio::test]
async fn test_mtls_config_creation() {
    // 测试mTLS配置创建和证书加载
    let cert_dir = "./test_certs_mtls";
    let _ = fs::remove_dir_all(cert_dir);
    let _ = fs::create_dir_all(cert_dir);

    let config = CertManagerConfig {
        development_mode: true,
        cert_path: Some(format!("{}/server.crt", cert_dir)),
        key_path: Some(format!("{}/server.key", cert_dir)),
        ca_path: Some(format!("{}/ca.crt", cert_dir)),
        validity_days: 365,
        hostnames: vec!["localhost".to_string()],
        acme_enabled: false,
        acme_production: false,
        acme_email: None,
        cloudflare_api_token: None,
        acme_renewal_days: 30,
        acme_cert_dir: None,
        mtls_enabled: true,
        client_cert_path: Some(format!("{}/client.crt", cert_dir)),
        client_key_path: Some(format!("{}/client.key", cert_dir)),
        client_ca_path: Some(format!("{}/ca.crt", cert_dir)),
        mtls_mode: Some("self_signed".to_string()),
        auto_generate_client_cert: true,
        client_cert_subject: Some("CN=mTLS Test Client,O=RAT Engine,C=CN".to_string()),
        auto_refresh_enabled: false,
        refresh_check_interval: 3600,
        force_cert_rotation: false,
        mtls_whitelist_paths: Vec::new(),
    };

    let mut cert_manager = CertificateManager::new(config);
    cert_manager.initialize().await.expect("mTLS证书管理器初始化失败");

    let cert_manager = Arc::new(RwLock::new(cert_manager));

    // 验证mTLS配置可以正常创建
    {
        let manager = cert_manager.read().unwrap();
        let server_config = manager.get_server_config();
        let client_config = manager.get_client_config();

        assert!(server_config.is_some(), "mTLS服务器配置创建失败");
        assert!(client_config.is_some(), "mTLS客户端配置创建失败");
    }

    // 清理测试文件
    let _ = fs::remove_dir_all(cert_dir);

    println!("✅ mTLS配置创建测试通过");
}