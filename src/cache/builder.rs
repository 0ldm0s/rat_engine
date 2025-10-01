//! 缓存构建器
//!
//! 此模块提供了缓存构建器，用于创建缓存实例
//! 支持完全自定义配置

use std::sync::Arc;
use std::path::PathBuf;
use rat_memcache::{RatMemCache, RatMemCacheBuilder, CacheOptions};
use rat_memcache::config::{CacheConfig, L1Config, L2Config, TtlConfig, PerformanceConfig};
use rat_memcache::error::CacheError;
use rat_memcache::types::EvictionStrategy;

/// 缓存构建器
///
/// 用于创建缓存实例，对最终调用者完全透明。
///
/// ⚠️ 重要设计原则：
/// - 此构建器会自动强制禁用rat_memcache的L2压缩功能
/// - 压缩由独立的压缩中间件处理，确保架构解耦
/// - 如需L2缓存，请使用 `with_l2_config()` 方法，它会自动确保压缩被禁用
pub struct CacheBuilder {
    /// 内部构建器
    builder: RatMemCacheBuilder,
    /// 缓存选项
    options: CacheOptions,
}

impl CacheBuilder {
    /// 创建一个新的缓存构建器
    ///
    /// 返回的构建器会自动遵循rat_engine的设计原则：
    /// - 禁用rat_memcache的L2压缩功能
    /// - 支持通过 `with_l2_config()` 配置L2缓存（自动禁用压缩）
    /// - 压缩由独立的压缩中间件处理
    pub fn new() -> Self {
        Self {
            builder: RatMemCacheBuilder::new(),
            options: CacheOptions::default(),
        }
    }

    /// 设置 L1 缓存配置
    pub fn with_l1_config(mut self, config: L1Config) -> Self {
        self.builder = self.builder.l1_config(config);
        self
    }

    /// 设置 L2 缓存配置
    ///
    /// ⚠️ 重要：这是rat_engine中配置L2缓存的唯一推荐方法。
    /// 此方法会自动强制禁用L2压缩功能，因为压缩由独立的压缩中间件处理。
    /// 不管传入的config中enable_lz4设置为何值，都会被强制设为false。
    ///
    /// # 参数
    /// * `config` - L2缓存配置，enable_lz4会被强制设为false
    ///
    /// # 返回
    /// * `Self` - 配置好的builder实例
    #[cfg(any(feature = "melange-storage", feature = "cache-full"))]
    pub fn with_l2_config(mut self, mut config: L2Config) -> Self {
        // 强制禁用L2压缩：压缩由独立的压缩中间件处理
        config.enable_lz4 = false;
        self.builder = self.builder.l2_config(config);
        self
    }

    /// 设置 TTL 配置
    pub fn with_ttl_config(mut self, config: TtlConfig) -> Self {
        self.builder = self.builder.ttl_config(config);
        self
    }

    /// 设置性能配置
    pub fn with_performance_config(mut self, config: PerformanceConfig) -> Self {
        self.builder = self.builder.performance_config(config);
        self
    }

  
    /// 设置默认过期时间（秒）
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        // 直接设置 options 的 ttl_seconds 字段
        self.options.ttl_seconds = Some(ttl);
        self
    }

    
    /// 设置是否强制使用 L2 缓存
    pub fn with_force_l2(mut self, force: bool) -> Self {
        self.options.force_l2 = force;
        self
    }

    /// 设置是否跳过 L1 缓存
    pub fn with_skip_l1(mut self, skip: bool) -> Self {
        self.options.skip_l1 = skip;
        self
    }

    /// 构建缓存实例
    ///
    /// ⚠️ 重要提醒：此方法会自动强制禁用rat_memcache的L2压缩功能，
    /// 因为压缩由独立的压缩中间件处理。如果需要L2缓存功能，
    /// 请使用 `with_l2_config()` 方法，该方法会自动确保压缩被禁用。
    pub async fn build(self) -> Result<Arc<RatMemCache>, CacheError> {
        // 构建缓存实例
        let cache = self.builder.build().await?;
        Ok(Arc::new(cache))
    }

    /// 构建缓存实例（同步版本）
    ///
    /// ⚠️ 重要提醒：此方法会自动强制禁用rat_memcache的L2压缩功能，
    /// 因为压缩由独立的压缩中间件处理。如果需要L2缓存功能，
    /// 请使用 `with_l2_config()` 方法，该方法会自动确保压缩被禁用。
    pub fn build_sync(self) -> Result<Arc<RatMemCache>, CacheError> {
        // 使用 tokio 运行时构建缓存实例
        let runtime = tokio::runtime::Runtime::new().map_err(|e| {
            CacheError::other(&format!("无法创建 tokio 运行时: {}", e))
        })?;

        let cache = runtime.block_on(self.builder.build())?;
        Ok(Arc::new(cache))
    }
}

impl Default for CacheBuilder {
    fn default() -> Self {
        Self::new()
    }
}