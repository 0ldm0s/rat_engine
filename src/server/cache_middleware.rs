//! 缓存中间件模块
//!
//! 这个模块提供了基于 rat_memcache 的HTTP响应缓存中间件功能。
//! 包含两个并行的缓存实现：
//! - 单版本缓存：基本的缓存功能，直接存储原始数据
//! - 多版本缓存：基于单版本扩展，支持多种编码版本的缓存管理
//!
//! 设计原则：
//! 1. 单版本缓存：直接存储原始二进制数据到rat_memcache
//! 2. 多版本缓存：基于单版本扩展，使用编码感知键生成多种版本
//! 3. 禁用rat_memcache的压缩功能（压缩由独立的压缩中间件处理）
//! 4. 单版本缓存与压缩中间件完全解耦，可以独立使用；多版本缓存依赖压缩中间件处理预压缩逻辑

use hyper::{Request, Response, body::Bytes, HeaderMap};
use http_body_util::{combinators::BoxBody, Full, BodyExt};
use std::error::Error;
use std::sync::Arc;
use bytes::Bytes as BytesType;
use std::time::Instant;
use crate::cache::Cache;

/// 简化的缓存中间件结构体
pub struct CacheMiddleware {
    /// 缓存实例
    cache: Arc<dyn Cache>,
    /// 默认TTL（秒）
    default_ttl: Option<u64>,
}

impl CacheMiddleware {
    /// 创建新的缓存中间件
    ///
    /// 注意：创建缓存实例时应该禁用rat_memcache的压缩功能，
    /// 因为压缩由独立的压缩中间件处理。
    ///
    /// 示例配置：
    /// ```rust
    /// let cache = CacheBuilder::new()
    ///     .with_compression_enabled(false) // 禁用rat_memcache压缩
    ///     .build()?;
    /// ```
    pub fn new(cache: Arc<dyn Cache>, default_ttl: Option<u64>) -> Self {
        Self {
            cache,
            default_ttl,
        }
    }

    /// 检查响应是否可以缓存
    fn is_cacheable(&self, headers: &HeaderMap) -> bool {
        // 检查 Cache-Control 头
        if let Some(cache_control) = headers.get("cache-control") {
            if let Ok(cache_control_str) = cache_control.to_str() {
                let cache_control_lower = cache_control_str.to_lowercase();
                crate::utils::logger::info!("🎯 [CacheMiddleware] 检查Cache-Control头: {}", cache_control_lower);

                // 如果包含 no-cache 或 no-store，则不可缓存
                if cache_control_lower.contains("no-cache") || cache_control_lower.contains("no-store") {
                    crate::utils::logger::info!("🎯 [CacheMiddleware] 检测到no-cache/no-store，响应不可缓存");
                    return false;
                }

                // 如果包含 must-revalidate，也视为不可缓存（对于动态数据测试场景）
                if cache_control_lower.contains("must-revalidate") {
                    crate::utils::logger::info!("🎯 [CacheMiddleware] 检测到must-revalidate，响应不可缓存");
                    return false;
                }

                // 如果包含 private，也不缓存
                if cache_control_lower.contains("private") {
                    crate::utils::logger::info!("🎯 [CacheMiddleware] 检测到private，响应不可缓存");
                    return false;
                }
            }
        }

        // 检查 Pragma 头（HTTP/1.0 兼容性）
        if let Some(pragma) = headers.get("pragma") {
            if let Ok(pragma_str) = pragma.to_str() {
                if pragma_str.to_lowercase().contains("no-cache") {
                    crate::utils::logger::debug!("🎯 [CacheMiddleware] 检测到Pragma: no-cache，响应不可缓存");
                    return false;
                }
            }
        }

        crate::utils::logger::debug!("🎯 [CacheMiddleware] 响应可以缓存");
        true
    }

    /// 处理请求和响应
    pub async fn process<B>(
        &self,
        req: &Request<B>,
        res: Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>,
    ) -> Result<Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>, hyper::Error> {
        // 只缓存GET请求
        if req.method() != hyper::Method::GET {
            return Ok(res);
        }

        // 生成缓存键
        let cache_key = self.generate_cache_key(req);
        crate::utils::logger::info!("🎯 [CacheMiddleware] 生成的缓存键: {}", cache_key);

        // 尝试从缓存获取
        let start_time = Instant::now();
        let cached_data = self.cache.get(&cache_key).await;

        match cached_data {
            Ok(Some(data)) => {
                // 缓存命中
                let elapsed = start_time.elapsed();
                crate::utils::logger::debug!("缓存命中! 键: {}, 耗时: {:?}", cache_key, elapsed);

                // 直接使用缓存的数据构建响应
                let full_body = http_body_util::Full::new(data);
                let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn Error + Send + Sync> { match never {} }));

                let response = Response::builder()
                    .status(200)
                    .header("content-type", "application/octet-stream") // 简化处理，实际应根据存储的内容类型
                    .header("x-cache", "HIT")
                    .header("x-cache-time", format!("{:?}", elapsed))
                    .body(boxed_body)
                    .unwrap();

                return Ok(response);
            },
            _ => {
                // 缓存未命中，继续处理
                crate::utils::logger::debug!("缓存未命中! 键: {}", cache_key);
            }
        }

        // 缓存未命中，需要处理原始响应并缓存
        crate::utils::logger::debug!("缓存未命中! 键: {}", cache_key);

        // 克隆响应以便我们可以返回原始响应
        let (parts, body) = res.into_parts();

        // 收集完整的响应体
        let bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                crate::utils::logger::error!("收集响应体时出错: {}", e);

                // 创建一个错误响应
                let error_body = format!("Error collecting response body: {}", e);
                let full_body = http_body_util::Full::new(Bytes::from(error_body));
                let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn Error + Send + Sync> { match never {} }));

                let error_response = Response::builder()
                    .status(500)
                    .header("content-type", "text/plain")
                    .header("x-cache", "ERROR")
                    .body(boxed_body)
                    .unwrap();

                return Ok(error_response);
            }
        };

        // 检查响应是否可以缓存
        if self.is_cacheable(&parts.headers) {
            crate::utils::logger::info!("🎯 [CacheMiddleware] 响应可以缓存，开始存储...");

            // 直接使用 rat_memcache 存储原始数据
            if let Some(ttl) = self.default_ttl {
                let _ = self.cache.set_with_ttl(
                    cache_key.clone(),
                    bytes.clone(),
                    ttl
                ).await;
            } else {
                let _ = self.cache.set(
                    cache_key.clone(),
                    bytes.clone()
                ).await;
            }

            crate::utils::logger::debug!("响应已缓存! 键: {}", cache_key);
        } else {
            crate::utils::logger::debug!("响应不可缓存，跳过缓存! 键: {}", cache_key);
        }

        // 重建响应
        let full_body = http_body_util::Full::new(bytes);
        let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn Error + Send + Sync> { match never {} }));
        let mut response = Response::from_parts(parts, boxed_body);

        // 添加缓存未命中标记
        response.headers_mut().insert("x-cache", "MISS".parse().unwrap());

        Ok(response)
    }

    /// 生成缓存键
    pub fn generate_cache_key<B>(&self, req: &Request<B>) -> String {
        // 简化的缓存键：方法 + 路径 + 查询参数
        let mut key = format!("{}{}", req.method(), req.uri().path());

        // 添加查询参数
        if let Some(query) = req.uri().query() {
            key.push_str("?");
            key.push_str(query);
        }

        key
    }

    /// 直接访问底层缓存的方法（供CacheVersionManager使用）
    pub async fn get_direct(&self, key: &str) -> Result<Option<bytes::Bytes>, Box<dyn std::error::Error + Send + Sync>> {
        self.cache.get(key).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    /// 直接设置底层缓存的方法（供CacheVersionManager使用）
    pub async fn set_direct(&self, key: &str, value: bytes::Bytes) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.cache.set(key.to_string(), value).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    /// 直接设置底层缓存带TTL的方法（供CacheVersionManager使用）
    pub async fn set_direct_with_ttl(&self, key: &str, value: bytes::Bytes, ttl: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.cache.set_with_ttl(key.to_string(), value, ttl).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    /// 直接删除底层缓存的方法（供CacheVersionManager使用）
    pub async fn delete_direct(&self, key: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.cache.delete(key).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

