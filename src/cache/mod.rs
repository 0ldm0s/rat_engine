//! 缓存模块
//!
//! 此模块提供了对 rat_memcache 的封装，使最终调用者不需要直接依赖 rat_memcache

#[cfg(feature = "cache")]
mod builder;

#[cfg(feature = "cache")]
pub use builder::CacheBuilder;

#[cfg(feature = "cache")]
pub use rat_memcache::RatMemCache;

#[cfg(feature = "cache")]
// 重新导出必要的公共类型
pub use rat_memcache::config::{L1Config, L2Config, TtlConfig, PerformanceConfig, CacheWarmupStrategy};

#[cfg(feature = "cache")]
// CompressionConfig 已移除，压缩功能现在集成在 L2Config 中
pub use rat_memcache::types::EvictionStrategy;

#[cfg(feature = "cache")]
use std::sync::Arc;
use bytes::Bytes;

#[cfg(feature = "cache")]
use crate::error::RatResult;

#[cfg(feature = "cache")]
/// 缓存接口
///
/// 提供缓存的基本操作，对最终调用者完全透明
#[async_trait::async_trait]
pub trait Cache: Send + Sync + 'static {
    /// 设置缓存
    async fn set(&self, key: String, value: Bytes) -> RatResult<()>;

    /// 设置带 TTL 的缓存
    async fn set_with_ttl(&self, key: String, value: Bytes, ttl_seconds: u64) -> RatResult<()>;

    /// 获取缓存
    async fn get(&self, key: &str) -> RatResult<Option<Bytes>>;

    /// 删除缓存
    async fn delete(&self, key: &str) -> RatResult<bool>;

    /// 清空缓存
    async fn clear(&self) -> RatResult<()>;

    /// 检查键是否存在
    async fn exists(&self, key: &str) -> RatResult<bool>;

    /// 获取缓存统计信息
    async fn get_stats(&self) -> RatResult<String>;
}

#[cfg(feature = "cache")]
// 为 RatMemCache 实现 Cache trait
#[async_trait::async_trait]
impl Cache for RatMemCache {
    async fn set(&self, key: String, value: Bytes) -> RatResult<()> {
        rat_memcache::RatMemCache::set(self, key, value).await.map_err(|e| crate::error::RatError::from(e))
    }

    async fn set_with_ttl(&self, key: String, value: Bytes, ttl_seconds: u64) -> RatResult<()> {
        rat_memcache::RatMemCache::set_with_ttl(self, key, value, ttl_seconds).await.map_err(|e| crate::error::RatError::from(e))
    }

    async fn get(&self, key: &str) -> RatResult<Option<Bytes>> {
        rat_memcache::RatMemCache::get(self, key).await.map_err(|e| crate::error::RatError::from(e))
    }

    async fn delete(&self, key: &str) -> RatResult<bool> {
        rat_memcache::RatMemCache::delete(self, key).await.map_err(|e| crate::error::RatError::from(e))
    }

    async fn clear(&self) -> RatResult<()> {
        rat_memcache::RatMemCache::clear(self).await.map_err(|e| crate::error::RatError::from(e))
    }

    async fn exists(&self, key: &str) -> RatResult<bool> {
        rat_memcache::RatMemCache::exists(self, key).await.map_err(|e| crate::error::RatError::from(e))
    }

    async fn get_stats(&self) -> RatResult<String> {
        let stats = rat_memcache::RatMemCache::get_stats(self).await.map_err(|e| crate::error::RatError::from(e))?;
        Ok(stats.to_string())
    }
}