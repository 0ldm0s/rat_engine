use std::sync::Once;

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// 确保 OpenSSL 初始化只执行一次
///
/// 这个函数使用 std::sync::Once 确保无论被调用多少次，
/// OpenSSL 的初始化只会执行一次
pub fn ensure_crypto_provider_installed() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        // OpenSSL 会自动初始化，这里只需要记录日志
        crate::utils::logger::debug!("🔐 OpenSSL 已初始化");
    });
}