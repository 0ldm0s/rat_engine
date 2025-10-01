use std::sync::Once;

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// 确保 rustls CryptoProvider 只安装一次
/// 
/// 这个函数使用 std::sync::Once 确保无论被调用多少次，
/// CryptoProvider 的安装只会执行一次，避免重复安装导致的警告
pub fn ensure_crypto_provider_installed() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        // 优先尝试安装 aws_lc_rs provider
        if let Err(_) = rustls::crypto::aws_lc_rs::default_provider().install_default() {
            // 如果 aws_lc_rs 安装失败，尝试 ring provider
            if let Err(_) = rustls::crypto::ring::default_provider().install_default() {
                crate::utils::logger::warn!("⚠️  无法安装 rustls CryptoProvider，TLS 功能可能不可用");
            } else {
                crate::utils::logger::debug!("🔐 已安装 rustls ring CryptoProvider");
            }
        } else {
            crate::utils::logger::debug!("🔐 已安装 rustls aws_lc_rs CryptoProvider");
        }
    });
}