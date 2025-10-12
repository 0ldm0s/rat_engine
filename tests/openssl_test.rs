//! OpenSSL 功能测试
//!
//! 验证 OpenSSL 迁移是否成功

#[test]
fn test_openssl_basic_functionality() {
    // 测试基本的 OpenSSL 功能
    let acceptor = openssl::ssl::SslAcceptor::mozilla_intermediate(openssl::ssl::SslMethod::tls());
    assert!(acceptor.is_ok(), "Failed to create SSL acceptor");
}

#[test]
fn test_openssl_import() {
    // 测试 OpenSSL 模块导入
    use openssl::ssl::{SslAcceptor, SslMethod};
    use openssl::x509::X509;
    use openssl::pkey::PKey;

    // 如果能编译通过，说明 OpenSSL 已正确导入
    assert!(true, "OpenSSL imported successfully");
}