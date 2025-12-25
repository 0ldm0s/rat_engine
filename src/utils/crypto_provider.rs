use std::sync::Once;

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// 确保 rustls CryptoProvider 只安装一次
///
/// 这个函数使用 std::sync::Once 确保无论被调用多少次，
/// ring CryptoProvider 的安装只会执行一次
pub fn ensure_crypto_provider_installed() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        let provider = rustls::crypto::ring::default_provider();
        match rustls::crypto::CryptoProvider::install_default(provider) {
            Ok(_) => {
                crate::utils::logger::debug!("🔐 rustls ring CryptoProvider 已安装");
            }
            Err(_) => {
                // 已经安装过，忽略错误
                crate::utils::logger::debug!("🔐 rustls CryptoProvider 已经安装过");
            }
        }
    });
}