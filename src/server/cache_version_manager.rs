//! 多版本缓存管理器
//!
//! 基于rat_memcache的多版本缓存实现，支持同一响应的多种编码版本存储

use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool};
use bytes::Bytes;
use dashmap::DashMap;
use crate::server::cache_middleware::CacheMiddleware;
use crate::compression::{CompressionConfig, CompressionType, Compressor};
use crate::cache::Cache;

/// 缓存版本管理器配置
#[derive(Debug, Clone)]
pub struct CacheVersionManagerConfig {
    /// 是否启用预压缩
    pub enable_precompression: bool,
    /// 支持的编码列表（按优先级排序），用于客户端协商
    pub supported_encodings: Vec<String>,
    /// 预压缩的阈值（字节）
    pub precompression_threshold: usize,
    /// 是否启用统计信息收集（开发调试用）
    pub enable_stats: bool,
    /// 是否启用智能预压缩决策
    pub enable_smart_precompression: bool,
}

impl Default for CacheVersionManagerConfig {
    fn default() -> Self {
        Self {
            enable_precompression: true,
            supported_encodings: vec![
                "lz4".to_string(),     // 最高优先级（我们的客户端）
                "zstd".to_string(),    // 现代高效压缩
                "br".to_string(),      // Brotli（浏览器优先）
                "gzip".to_string(),    // 兼容性好
                "deflate".to_string(), // 传统压缩
                "identity".to_string(), // 不压缩
            ],
            precompression_threshold: 1024,
            enable_stats: false, // 默认关闭统计，生产环境不需要
            enable_smart_precompression: true, // 默认启用智能预压缩决策
        }
    }
}

/// 编码统计信息（原子性，可选）
#[cfg(feature = "cache")]
#[derive(Debug)]
pub struct EncodingStats {
    /// 各种编码的命中次数
    hit_counts: Arc<DashMap<String, AtomicU64>>,
    /// 总缓存命中次数
    total_hits: AtomicU64,
    /// 总缓存访问次数
    total_accesses: AtomicU64,
}

#[cfg(feature = "cache")]
impl Default for EncodingStats {
    fn default() -> Self {
        Self {
            hit_counts: Arc::new(DashMap::new()),
            total_hits: AtomicU64::new(0),
            total_accesses: AtomicU64::new(0),
        }
    }
}

/// 缓存查找结果
#[derive(Debug)]
pub struct CacheLookupResult {
    /// 缓存的数据
    pub data: Bytes,
    /// 使用的编码
    pub encoding: String,
}

/// 多版本缓存管理器
pub struct CacheVersionManager {
    /// 配置
    config: CacheVersionManagerConfig,
    /// 编码统计（可选，仅开发调试用）
    stats: Option<Arc<EncodingStats>>,
    /// 底层单版本缓存中间件
    cache_middleware: Arc<CacheMiddleware>,
}

#[cfg(feature = "cache")]
impl CacheVersionManager {
    /// 使用单版本缓存中间件创建管理器
    pub fn with_cache_middleware(cache_middleware: Arc<CacheMiddleware>) -> Self {
        let config = CacheVersionManagerConfig::default();
        Self {
            stats: if config.enable_stats { Some(Arc::new(EncodingStats::default())) } else { None },
            config,
            cache_middleware,
        }
    }

    /// 使用自定义配置创建管理器
    pub fn with_config(cache_middleware: Arc<CacheMiddleware>, config: CacheVersionManagerConfig) -> Self {
        Self {
            stats: if config.enable_stats { Some(Arc::new(EncodingStats::default())) } else { None },
            config,
            cache_middleware,
        }
    }

    /// 使用底层缓存和配置创建管理器（自动创建CacheMiddleware）
    pub fn with_cache_and_config(cache: Arc<dyn Cache>, config: CacheVersionManagerConfig, default_ttl: Option<u64>) -> Self {
        // 配置验证：检查是否满足特性要求
        Self::validate_config(&config);

        let cache_middleware = Arc::new(CacheMiddleware::new(cache, default_ttl));
        Self {
            stats: if config.enable_stats { Some(Arc::new(EncodingStats::default())) } else { None },
            config,
            cache_middleware,
        }
    }

    /// 验证配置是否符合当前启用的特性
    fn validate_config(config: &CacheVersionManagerConfig) {
        // 检查配置的压缩编码是否超出了当前特性的支持范围
        if !config.supported_encodings.is_empty() {
            let unsupported_encodings: Vec<&String> = config.supported_encodings.iter()
                .filter(|encoding| {
                    match encoding.as_str() {
                        "identity" => false, // identity 总是支持的
                        "gzip" | "deflate" => !cfg!(feature = "compression"),
                        "br" | "lz4" | "zstd" => !cfg!(feature = "compression-full"),
                        _ => true, // 未知编码总是不支持的
                    }
                })
                .collect();

            if !unsupported_encodings.is_empty() {
                crate::utils::logger::warn!(
                    "⚠️  CacheVersionManager 配置了不支持的压缩编码: {:?}。这些编码将被自动过滤。",
                    unsupported_encodings
                );
            }
        }

        // 检查是否需要 L2 缓存（如果启用了 cache-full 特性）
        if cfg!(feature = "cache-full") {
            // 这里可以添加对 L2 配置的验证逻辑
            // 目前 CacheVersionManager 不直接管理 L2，所以暂时不需要
        }
    }

    /// 使用底层缓存创建管理器（自动创建CacheMiddleware，使用默认配置）
    pub fn with_cache(cache: Arc<dyn Cache>, default_ttl: Option<u64>) -> Self {
        let config = CacheVersionManagerConfig::default();
        Self::with_cache_and_config(cache, config, default_ttl)
    }

    /// 处理缓存查找
    ///
    /// 根据Accept-Encoding头选择最佳编码版本
    pub async fn handle_cache_lookup(&self, base_key: &str, accept_encoding: &str) -> Option<CacheLookupResult> {
        // 更新统计（如果启用）
        if let Some(ref stats) = self.stats {
            stats.total_accesses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        // 解析客户端支持的编码
        let client_encodings: Vec<&str> = if accept_encoding.is_empty() {
            vec!["identity"]
        } else {
            accept_encoding
                .split(',')
                .map(|s| s.trim())
                .collect()
        };

        // 获取当前特性支持的编码列表
        let available_encodings: Vec<&String> = if cfg!(feature = "compression-full") {
            // 启用了 compression-full，支持所有压缩编码
            self.config.supported_encodings.iter().collect()
        } else if cfg!(feature = "compression") {
            // 启用了基础 compression，支持 identity 和基础压缩编码
            self.config.supported_encodings.iter()
                .filter(|enc| {
                    matches!(enc.as_str(), "identity" | "gzip" | "deflate")
                })
                .collect()
        } else {
            // 未启用任何压缩特性，只支持 identity
            self.config.supported_encodings.iter()
                .filter(|enc| enc.as_str() == "identity")
                .collect()
        };

        // 按优先级尝试获取不同编码的版本
        for server_encoding in available_encodings {
            // 检查客户端是否支持此编码
            let client_supports_encoding = client_encodings.iter().any(|client_enc| {
                client_enc.contains(server_encoding)
            });

            // 如果是identity，需要特殊处理：只有当没有其他编码匹配时才使用
            if server_encoding == "identity" {
                // 先检查是否有其他编码已经匹配
                let has_other_match = self.config.supported_encodings.iter().any(|enc| {
                    enc != "identity" && client_encodings.iter().any(|client_enc| client_enc.contains(enc))
                });

                // 如果有其他编码匹配，跳过identity
                if has_other_match {
                    continue;
                }
            }

            if client_supports_encoding || server_encoding == "identity" {
                // 构建编码特定的缓存键
                let encoded_key = format!("{}:{}", base_key, server_encoding);

                crate::utils::logger::debug!(
                    "🎯 [CacheVersionManager] 尝试查找缓存键: {}",
                    encoded_key
                );

                // 测量缓存查找时间
                let cache_start = std::time::Instant::now();

                // 尝试从单版本缓存获取
                match self.cache_middleware.get_direct(&encoded_key).await {
                    Ok(Some(data)) => {
                        let cache_duration = cache_start.elapsed();
                        let data_size = data.len();

                        crate::utils::logger::debug!(
                            "🎯 [CacheVersionManager] 缓存查找耗时: {:?}, 数据大小: {} 字节",
                            cache_duration, data_size
                        );

                        // 验证数据大小：如果是压缩编码，检查数据大小是否合理
                        if server_encoding != "identity" && server_encoding != "none" {
                            // 对于压缩数据，我们期望数据大小应该显著小于原始数据
                            // 原始数据通常在50KB-200KB范围（基于我们的测试数据）
                            let estimated_original_size = 100 * 1024; // 100KB估算值
                            let compression_ratio = data_size as f64 / estimated_original_size as f64;

                            crate::utils::logger::debug!(
                                "🔍 [CacheVersionManager] 编码验证: {}, 数据大小: {} 字节, 估算压缩率: {:.2}%",
                                server_encoding, data_size, compression_ratio * 100.0
                            );

                            // 如果数据大小接近原始估算大小，可能存储的是未压缩数据
                            if compression_ratio > 0.8 {
                                crate::utils::logger::warn!(
                                    "⚠️ [CacheVersionManager] 可疑数据: 编码为 '{}' 但数据大小 ({}) 接近原始大小，可能未正确压缩!",
                                    server_encoding, data_size
                                );
                            } else if compression_ratio > 0.5 {
                                crate::utils::logger::info!(
                                    "📊 [CacheVersionManager] 压缩效果一般: 编码 '{}', 压缩率: {:.1}%",
                                    server_encoding, compression_ratio * 100.0
                                );
                            } else {
                                crate::utils::logger::info!(
                                    "✅ [CacheVersionManager] 压缩效果良好: 编码 '{}', 压缩率: {:.1}%",
                                    server_encoding, compression_ratio * 100.0
                                );
                            }
                        } else {
                            crate::utils::logger::debug!(
                                "📦 [CacheVersionManager] 未压缩数据: 编码 '{}', 大小: {} 字节",
                                server_encoding, data_size
                            );
                        }

                        // 更新统计（如果启用）
                        if let Some(ref stats) = self.stats {
                            stats.hit_counts.entry(server_encoding.clone())
                                .or_insert_with(|| AtomicU64::new(0))
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            stats.total_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }

                        crate::utils::logger::debug!(
                            "🎯 [CacheVersionManager] 多版本缓存命中: {} -> {}",
                            base_key, server_encoding
                        );

                        return Some(CacheLookupResult {
                            data,
                            encoding: server_encoding.clone(),
                        });
                    }
                    Ok(None) => {
                        // 此编码版本不存在，继续尝试下一个
                        // 移除了不必要的统计更新
                        continue;
                    }
                    Err(e) => {
                        crate::utils::logger::warn!(
                            "⚠️ [CacheVersionManager] 缓存获取失败 {} {}: {}",
                            base_key, server_encoding, e
                        );
                        continue;
                    }
                }
            }
        }

        // 如果所有编码都未匹配，尝试返回identity版本（小数据策略）
        // 这样即使客户端不明确支持identity，也能获得未压缩的数据
        if !self.config.supported_encodings.is_empty() {
            let identity_key = format!("{}:identity", base_key);

            crate::utils::logger::debug!(
                "🎯 [CacheVersionManager] 尝试获取identity版本作为默认返回: {}",
                identity_key
            );

            match self.cache_middleware.get_direct(&identity_key).await {
                Ok(Some(data)) => {
                    crate::utils::logger::debug!(
                        "📦 [CacheVersionManager] 返回identity版本作为默认数据: {} (大小: {} 字节)",
                        base_key, data.len()
                    );

                    // 更新统计（如果启用）
                    if let Some(ref stats) = self.stats {
                        stats.hit_counts.entry("identity".to_string())
                            .or_insert_with(|| AtomicU64::new(0))
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        stats.total_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    return Some(CacheLookupResult {
                        data,
                        encoding: "identity".to_string(),
                    });
                }
                Ok(None) => {
                    // identity版本也不存在，继续返回未命中
                }
                Err(e) => {
                    crate::utils::logger::warn!(
                        "⚠️ [CacheVersionManager] 获取identity版本失败 {}: {}",
                        base_key, e
                    );
                }
            }
        }

        crate::utils::logger::debug!(
            "🎯 [CacheVersionManager] 多版本缓存未命中: {}",
            base_key
        );

        None
    }

    /// 处理缓存存储
    ///
    /// 存储原始数据并可选择性地生成预压缩版本
    pub async fn handle_cache_storage(
        &self,
        base_key: &str,
        _content_type: &str,
        data: Bytes,
        encoding: &str,
        ttl: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 存储原始版本（或指定编码版本）
        let original_key = format!("{}:{}", base_key, encoding);
        let original_size = data.len();

        // 记录存储数据的详细信息
        if encoding != "identity" && encoding != "none" {
            crate::utils::logger::info!(
                "💾 [CacheVersionManager] 存储压缩数据: 键={}, 编码={}, 大小={} 字节",
                original_key, encoding, original_size
            );

            // 如果声称是压缩数据但大小很大，发出警告
            if original_size > 50 * 1024 {
                crate::utils::logger::warn!(
                    "⚠️ [CacheVersionManager] 压缩数据异常大: 键={}, 编码={}, 大小={} 字节 - 可能未正确压缩!",
                    original_key, encoding, original_size
                );
            }
        } else {
            crate::utils::logger::debug!(
                "💾 [CacheVersionManager] 存储未压缩数据: 键={}, 编码={}, 大小={} 字节",
                original_key, encoding, original_size
            );
        }

        if let Some(ttl) = ttl {
            self.cache_middleware.set_direct_with_ttl(&original_key, data.clone(), ttl).await?;
        } else {
            self.cache_middleware.set_direct(&original_key, data.clone()).await?;
        }

        crate::utils::logger::debug!(
            "🎯 [CacheVersionManager] 原始数据已存储: {} (编码: {}, 大小: {} 字节)",
            base_key, encoding, original_size
        );

        // 检查是否应该进行预压缩
        if self.should_precompress(&data) {
            self.generate_precompressed_versions(base_key, &data, ttl).await?;
        }

        Ok(())
    }

    /// 智能预压缩决策 - 检查数据是否值得进行预压缩
    fn should_precompress(&self, data: &[u8]) -> bool {
        // 检查是否启用了任何压缩特性
        if !cfg!(any(feature = "compression", feature = "compression-full")) {
            return false;
        }

        // 检查是否启用预压缩
        if !self.config.enable_precompression {
            return false;
        }

        // 检查数据大小是否达到阈值
        if data.len() < self.config.precompression_threshold {
            return false;
        }

        // 如果启用智能预压缩决策，进行数据分析
        if self.config.enable_smart_precompression {
            let compression_config = CompressionConfig::new().enable_smart_compression(true);
            if !compression_config.estimate_compressibility(data) {
                #[cfg(feature = "compression")]
                crate::utils::logger::debug!("🧠 [CacheVersionManager] 数据压缩性低，跳过预压缩");
                return false;
            }
        }

        true
    }

    /// 生成预压缩版本
    async fn generate_precompressed_versions(
        &self,
        base_key: &str,
        data: &Bytes,
        ttl: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 创建压缩器配置，启用所有支持的编码和智能压缩
        let mut compression_config = CompressionConfig::new().enable_smart_compression(self.config.enable_smart_precompression);

        // 根据支持的编码列表配置压缩器
        for encoding in &self.config.supported_encodings {
            match encoding.as_str() {
                "gzip" => compression_config = compression_config.with_gzip(),
                "deflate" => compression_config = compression_config.with_deflate(),
                "br" => {
                    #[cfg(feature = "compression-br")]
                    { compression_config = compression_config.with_brotli(); }
                    #[cfg(not(feature = "compression-br"))]
                    {
                        crate::utils::logger::warn!(
                            "⚠️ [CacheVersionManager] Brotli压缩未启用，跳过: {}",
                            encoding
                        );
                        continue;
                    }
                },
                "zstd" => {
                    #[cfg(feature = "compression-zstd")]
                    { compression_config = compression_config.with_zstd(); }
                    #[cfg(not(feature = "compression-zstd"))]
                    {
                        crate::utils::logger::warn!(
                            "⚠️ [CacheVersionManager] Zstd压缩未启用，跳过: {}",
                            encoding
                        );
                        continue;
                    }
                },
                "lz4" => {
                    #[cfg(feature = "compression-lz4")]
                    { compression_config = compression_config.with_lz4(); }
                    #[cfg(not(feature = "compression-lz4"))]
                    {
                        crate::utils::logger::warn!(
                            "⚠️ [CacheVersionManager] LZ4压缩未启用，跳过: {}",
                            encoding
                        );
                        continue;
                    }
                },
                "identity" | "none" => continue, // 跳过不压缩
                _ => {
                    crate::utils::logger::warn!(
                        "⚠️ [CacheVersionManager] 不支持的编码: {}",
                        encoding
                    );
                    continue;
                }
            }
        }

        let compressor = Compressor::new(compression_config);

        // 为每个支持的编码生成压缩版本
        for encoding in &self.config.supported_encodings {
            if encoding == "identity" || encoding == "none" {
                continue; // 跳过identity，因为原始版本已经存储
            }

            // 解析压缩类型
            let compression_type = match CompressionType::from_str(encoding) {
                Some(ct) => ct,
                None => {
                    crate::utils::logger::warn!(
                        "⚠️ [CacheVersionManager] 无法解析压缩类型: {}",
                        encoding
                    );
                    continue;
                }
            };

            // 执行压缩
            match compressor.compress(data, compression_type) {
                Ok(compressed_data) => {
                    // 如果压缩后确实变小了，才存储压缩版本
                    if compressed_data.len() < data.len() {
                        let compressed_key = format!("{}:{}", base_key, encoding);
                        let compressed_bytes = Bytes::from(compressed_data);

                        // 存储压缩版本
                        if let Some(ttl) = ttl {
                            self.cache_middleware.set_direct_with_ttl(&compressed_key, compressed_bytes.clone(), ttl).await?;
                        } else {
                            self.cache_middleware.set_direct(&compressed_key, compressed_bytes.clone()).await?;
                        }

                        crate::utils::logger::debug!(
                            "🎯 [CacheVersionManager] 预压缩完成: {} -> {} ({} -> {} 字节, 压缩率: {:.1}%)",
                            base_key, encoding, data.len(), compressed_bytes.len(),
                            ((data.len() - compressed_bytes.len()) as f64 / data.len() as f64) * 100.0
                        );
                    } else {
                        crate::utils::logger::debug!(
                            "🎯 [CacheVersionManager] 压缩效果不佳，跳过: {} -> {} ({} -> {} 字节)",
                            base_key, encoding, data.len(), compressed_data.len()
                        );
                    }
                }
                Err(e) => {
                    crate::utils::logger::warn!(
                        "⚠️ [CacheVersionManager] 预压缩失败 {} {}: {}",
                        base_key, encoding, e
                    );
                }
            }
        }

        Ok(())
    }

    /// 清理指定基础键的所有版本
    pub async fn clear_all_versions(&self, base_key: &str) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let mut removed_count = 0;

        // 清理所有编码版本
        for encoding in &self.config.supported_encodings {
            let key = format!("{}:{}", base_key, encoding);
            match self.cache_middleware.delete_direct(&key).await {
                Ok(true) => {
                    removed_count += 1;
                    crate::utils::logger::debug!(
                        "🎯 [CacheVersionManager] 已清理缓存版本: {}:{}",
                        base_key, encoding
                    );
                }
                Ok(false) => {
                    // 键不存在，忽略
                }
                Err(e) => {
                    crate::utils::logger::warn!(
                        "⚠️ [CacheVersionManager] 清理缓存版本失败 {}:{}: {}",
                        base_key, encoding, e
                    );
                }
            }
        }

        crate::utils::logger::info!(
            "🎯 [CacheVersionManager] 清理完成: {} (共清理 {} 个版本)",
            base_key, removed_count
        );

        Ok(removed_count)
    }

    /// 获取缓存统计信息（如果启用）
    pub fn get_stats(&self) -> Option<&EncodingStats> {
        self.stats.as_ref().map(|v| &**v)
    }

    /// 计算缓存命中率（如果启用统计）
    pub fn get_hit_rate(&self) -> Option<f64> {
        if let Some(ref stats) = self.stats {
            let total_accesses = stats.total_accesses.load(std::sync::atomic::Ordering::Relaxed);
            if total_accesses == 0 {
                Some(0.0)
            } else {
                let total_hits = stats.total_hits.load(std::sync::atomic::Ordering::Relaxed);
                Some((total_hits as f64) / (total_accesses as f64))
            }
        } else {
            None
        }
    }

    /// 获取配置的编码列表（替代统计受欢迎的编码）
    pub fn get_supported_encodings(&self) -> &[String] {
        &self.config.supported_encodings
    }

    /// 获取编码命中统计（如果启用统计）
    pub fn get_encoding_hit_counts(&self) -> Option<HashMap<String, u64>> {
        self.stats.as_ref().map(|stats| {
            stats.hit_counts.iter()
                .map(|entry| (entry.key().clone(), entry.value().load(std::sync::atomic::Ordering::Relaxed)))
                .collect()
        })
    }
}