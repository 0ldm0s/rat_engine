//! 缓存中间件实现模块
//!
//! 这个模块提供了缓存中间件的具体实现。
//! 支持单版本缓存和多版本缓存（基于CacheVersionManager）。

use crate::server::cache_middleware::CacheMiddleware;
use crate::server::cache_version_manager::{CacheVersionManager, CacheLookupResult};
use hyper::{Request, Response, body::Bytes};
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use http_body_util::Full;
use std::error::Error;
use std::sync::Arc;
use bytes::Bytes as BytesType;

/// 缓存中间件实现
pub enum CacheMiddlewareImpl {
    /// 单版本缓存
    SingleVersion(Arc<CacheMiddleware>),
    /// 多版本缓存
    #[cfg(feature = "cache")]
    MultiVersion(Arc<CacheVersionManager>),
}

impl CacheMiddlewareImpl {
    /// 创建单版本缓存中间件实现
    pub fn new_single_version(middleware: CacheMiddleware) -> Self {
        Self::SingleVersion(Arc::new(middleware))
    }

    /// 创建多版本缓存中间件实现
    #[cfg(feature = "cache")]
    pub fn new_multi_version(version_manager: CacheVersionManager) -> Self {
        Self::MultiVersion(Arc::new(version_manager))
    }

    /// 当没有启用缓存特性时的占位符实现
    #[cfg(not(feature = "cache"))]
    pub fn new_multi_version(_version_manager: ()) -> Self {
        panic!("多版本缓存需要启用 cache 特性");
    }

    /// 处理请求和响应
    pub async fn process<B>(
        &self,
        req: &Request<B>,
        res: Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>,
    ) -> Result<Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>, hyper::Error> {
        match self {
            CacheMiddlewareImpl::SingleVersion(middleware) => {
                // 单版本缓存处理
                middleware.process(req, res).await
            },
            #[cfg(feature = "cache")]
CacheMiddlewareImpl::MultiVersion(version_manager) => {
                // 多版本缓存处理（只要有cache特性就可以工作）
                {
                    // 多版本缓存处理
                    let manager = &**version_manager; // 解包Arc引用

                // 只缓存GET请求
                if req.method() != hyper::Method::GET {
                    return Ok(res);
                }

                // 生成基础缓存键
                let base_cache_key = Self::generate_cache_key_internal(req);

                // 获取客户端支持的编码
                let accept_encoding = req.headers()
                    .get("accept-encoding")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");

                // 尝试从多版本缓存获取最佳匹配
                if let Some(cache_result) = manager.handle_cache_lookup(&base_cache_key, accept_encoding).await {
                    let full_body = http_body_util::Full::new(cache_result.data);
                    let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn Error + Send + Sync> { match never {} }));

                    let mut response = Response::builder()
                        .status(200)
                        .header("content-type", "application/octet-stream")
                        .header("x-cache", "HIT")
                        .header("x-cache-type", "MULTI-VERSION")
                        .body(boxed_body)
                        .unwrap();

                    // 设置正确的 Content-Encoding 头部
                    if cache_result.encoding != "identity" {
                        response.headers_mut().insert("content-encoding", cache_result.encoding.parse().unwrap());
                    }

                    return Ok(response);
                }

                // 缓存未命中，处理原始响应
                let (parts, body) = res.into_parts();

                // 收集响应体
                let bytes = match body.collect().await {
                    Ok(collected) => collected.to_bytes(),
                    Err(e) => {
                        crate::utils::logger::error!("收集响应体时出错: {}", e);
                        let error_body = format!("Error collecting response body: {}", e);
                        let full_body = http_body_util::Full::new(Bytes::from(error_body));
                        let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn Error + Send + Sync> { match never {} }));

                        return Ok(Response::builder()
                            .status(500)
                            .header("content-type", "text/plain")
                            .header("x-cache", "ERROR")
                            .body(boxed_body)
                            .unwrap());
                    }
                };

                // 检查响应是否可以缓存
                if Self::is_cacheable(&parts.headers) {
                    let content_type = parts.headers
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("application/octet-stream");

                    // 使用CacheVersionManager存储数据
                    if let Err(e) = manager.handle_cache_storage(
                        &base_cache_key,
                        content_type,
                        bytes.clone(),
                        "identity",
                        None
                    ).await {
                        crate::utils::logger::error!("多版本缓存存储失败: {}", e);
                    }
                }

                // 重建响应
                let full_body = http_body_util::Full::new(bytes);
                let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn Error + Send + Sync> { match never {} }));
                let mut response = Response::from_parts(parts, boxed_body);

                response.headers_mut().insert("x-cache", "MISS".parse().unwrap());
                response.headers_mut().insert("x-cache-type", "MULTI-VERSION".parse().unwrap());

                Ok(response)
                }
            },
            #[cfg(not(all(feature = "cache-full", feature = "compression-full")))]
            _ => {
                // 当没有启用多版本缓存特性时，直接返回原始响应
                Ok(res)
            }
        }
    }

    /// 生成缓存键
    ///
    /// # 参数
    /// * `method` - HTTP方法
    /// * `path` - 请求路径
    /// * `query` - 查询参数
    ///
    /// # 返回值
    /// 返回生成的缓存键
    pub fn generate_cache_key(&self, method: &hyper::Method, path: &str, query: Option<&str>) -> String {
        // 创建一个临时的hyper请求来使用内部中间件的generate_cache_key方法
        let path_with_query = format!("{}{}", path, query.map(|q| format!("?{}", q)).unwrap_or_default());
        let uri = hyper::Uri::builder()
            .path_and_query(path_with_query)
            .build()
            .unwrap();

        let hyper_req = hyper::Request::builder()
            .method(method.clone())
            .uri(uri)
            .body(())
            .unwrap();

        match self {
            CacheMiddlewareImpl::SingleVersion(middleware) => {
                middleware.generate_cache_key(&hyper_req)
            },
            #[cfg(all(feature = "cache-full", feature = "compression-full"))]
CacheMiddlewareImpl::MultiVersion(_) => {
                // 多版本缓存使用相同的键生成逻辑
                Self::generate_cache_key_internal(&hyper_req)
            },
            #[cfg(not(all(feature = "cache-full", feature = "compression-full")))]
_ => {
                // 当没有启用多版本缓存特性时，使用默认键生成逻辑
                Self::generate_cache_key_internal(&hyper_req)
            }
        }
    }

    /// 内部缓存键生成方法
    fn generate_cache_key_internal<B>(req: &Request<B>) -> String {
        let mut key = format!("{}{}", req.method(), req.uri().path());
        if let Some(query) = req.uri().query() {
            key.push_str("?");
            key.push_str(query);
        }
        key
    }

    /// 检查响应是否可以缓存
    fn is_cacheable(headers: &hyper::HeaderMap) -> bool {
        if let Some(cache_control) = headers.get("cache-control") {
            if let Ok(cache_control_str) = cache_control.to_str() {
                let cache_control_lower = cache_control_str.to_lowercase();
                if cache_control_lower.contains("no-cache") ||
                   cache_control_lower.contains("no-store") ||
                   cache_control_lower.contains("must-revalidate") ||
                   cache_control_lower.contains("private") {
                    return false;
                }
            }
        }
        true
    }
}