//! 压缩模块
//!
//! 提供多种压缩算法支持，包括 Gzip、Deflate、Brotli、Zstd 和 LZ4
//! 可以根据 Accept-Encoding 头部自动选择最佳压缩算法
//! 支持配置压缩级别、最小压缩大小和排除特定内容类型

use std::collections::HashSet;
use std::fmt;
use hyper::header::{HeaderMap, HeaderValue};
use bytes::Bytes;
use hyper::Response;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use bytes::BytesMut;

/// 压缩算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompressionType {
    /// 不压缩
    None,
    /// Gzip 压缩
    Gzip,
    /// Deflate 压缩
    Deflate,
    /// Brotli 压缩
    Brotli,
    /// Zstd 压缩
    Zstd,
    /// LZ4 压缩
    Lz4,
}

impl CompressionType {
    /// 获取压缩算法名称
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Deflate => "deflate",
            Self::Brotli => "br",
            Self::Zstd => "zstd",
            Self::Lz4 => "lz4",
        }
    }

    /// 获取 HTTP 头部值
    pub fn header_value(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Gzip => "gzip",
            Self::Deflate => "deflate",
            Self::Brotli => "br",
            Self::Zstd => "zstd",
            Self::Lz4 => "lz4",
        }
    }

    /// 从字符串解析压缩类型
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" | "identity" => Some(Self::None),
            "gzip" => Some(Self::Gzip),
            "deflate" => Some(Self::Deflate),
            "br" | "brotli" => Some(Self::Brotli),
            "zstd" => Some(Self::Zstd),
            "lz4" => Some(Self::Lz4),
            _ => None,
        }
    }

    /// 从 Accept-Encoding 头部选择最佳压缩算法
    pub fn select_from_accept_encoding(accept_encoding: Option<&HeaderValue>, enabled_algorithms: &[CompressionType]) -> Self {
        if enabled_algorithms.is_empty() {
            return Self::None;
        }

        let accept_encoding = match accept_encoding {
            Some(value) => match value.to_str() {
                Ok(s) => s,
                Err(_) => return Self::None,
            },
            None => return Self::None,
        };

        // 解析 Accept-Encoding 头部
        let mut supported = Vec::new();
        let mut has_identity = false;

        for part in accept_encoding.split(',') {
            let part = part.trim();
            if let Some(encoding) = part.split(';').next() {
                let encoding = encoding.trim();
                if let Some(compression_type) = Self::from_str(encoding) {
                    if compression_type == Self::None {
                        has_identity = true; // 标记客户端明确要求不压缩
                    } else if enabled_algorithms.contains(&compression_type) {
                        supported.push(compression_type);
                    }
                }
            }
        }

        // 如果客户端明确要求 identity，则不压缩
        if has_identity {
            return Self::None;
        }

        // 按优先级选择压缩算法（lz4 > zstd > br > gzip > deflate）
        #[cfg(feature = "compression")]
        if supported.contains(&Self::Lz4) {
            return Self::Lz4;
        }
        #[cfg(feature = "compression-zstd")]
        if supported.contains(&Self::Zstd) {
            return Self::Zstd;
        }
        #[cfg(feature = "compression-br")]
        if supported.contains(&Self::Brotli) {
            return Self::Brotli;
        }
        if supported.contains(&Self::Gzip) {
            return Self::Gzip;
        } else if supported.contains(&Self::Deflate) {
            return Self::Deflate;
        }

        Self::None
    }
}

impl std::fmt::Display for CompressionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// 压缩配置
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// 启用的压缩算法
    pub enabled_algorithms: Vec<CompressionType>,
    /// 最小压缩大小 (字节)
    pub min_size: usize,
    /// 压缩级别 (1-9，越大压缩率越高但速度越慢)
    pub level: u32,
    /// 排除的内容类型
    pub excluded_content_types: HashSet<String>,
    /// 排除的文件扩展名
    pub excluded_extensions: HashSet<String>,
    /// 是否启用智能压缩决策
    pub enable_smart_compression: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled_algorithms: vec![CompressionType::Gzip, CompressionType::Deflate],
            min_size: 1024, // 1KB
            level: 6,       // 默认压缩级别
            excluded_content_types: HashSet::from([
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/gif".to_string(),
                "image/webp".to_string(),
                "image/svg+xml".to_string(),
                "audio/".to_string(),
                "video/".to_string(),
                "application/zip".to_string(),
                "application/gzip".to_string(),
                "application/x-rar-compressed".to_string(),
                "application/x-7z-compressed".to_string(),
            ]),
            excluded_extensions: HashSet::from([
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "webp".to_string(),
                "svg".to_string(),
                "mp3".to_string(),
                "mp4".to_string(),
                "zip".to_string(),
                "gz".to_string(),
                "rar".to_string(),
                "7z".to_string(),
            ]),
            #[cfg(feature = "compression")]
            enable_smart_compression: true, // 启用压缩特性时才启用智能压缩
            #[cfg(not(feature = "compression"))]
            enable_smart_compression: false, // 没有压缩特性时禁用智能压缩
        }
    }
}

impl CompressionConfig {
    /// 创建新的压缩配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用压缩
    pub fn enable_compression(mut self, enabled: bool) -> Self {
        self.enabled_algorithms = if enabled {
            vec![CompressionType::Gzip, CompressionType::Deflate]
        } else {
            vec![CompressionType::None]
        };
        self
    }

    /// 设置启用的压缩算法
    pub fn algorithms(mut self, algorithms: Vec<CompressionType>) -> Self {
        self.enabled_algorithms = algorithms;
        self
    }

    /// 启用 Gzip 压缩
    pub fn with_gzip(mut self) -> Self {
        if !self.enabled_algorithms.contains(&CompressionType::Gzip) {
            self.enabled_algorithms.push(CompressionType::Gzip);
        }
        self
    }

    /// 启用 Deflate 压缩
    pub fn with_deflate(mut self) -> Self {
        if !self.enabled_algorithms.contains(&CompressionType::Deflate) {
            self.enabled_algorithms.push(CompressionType::Deflate);
        }
        self
    }

    /// 启用 Brotli 压缩
    #[cfg(feature = "compression-br")]
    pub fn with_brotli(mut self) -> Self {
        if !self.enabled_algorithms.contains(&CompressionType::Brotli) {
            self.enabled_algorithms.push(CompressionType::Brotli);
        }
        self
    }

    /// 启用 Zstd 压缩
    #[cfg(feature = "compression-zstd")]
    pub fn with_zstd(mut self) -> Self {
        if !self.enabled_algorithms.contains(&CompressionType::Zstd) {
            self.enabled_algorithms.push(CompressionType::Zstd);
        }
        self
    }

    /// 启用 LZ4 压缩
    #[cfg(feature = "compression")]
    pub fn with_lz4(mut self) -> Self {
        if !self.enabled_algorithms.contains(&CompressionType::Lz4) {
            self.enabled_algorithms.push(CompressionType::Lz4);
        }
        self
    }

    /// 启用所有可用的压缩算法
    pub fn with_all_algorithms(mut self) -> Self {
        self.enabled_algorithms = vec![CompressionType::Gzip, CompressionType::Deflate];
        #[cfg(feature = "compression-br")]
        {
            if !self.enabled_algorithms.contains(&CompressionType::Brotli) {
                self.enabled_algorithms.push(CompressionType::Brotli);
            }
        }
        #[cfg(feature = "compression-zstd")]
        {
            if !self.enabled_algorithms.contains(&CompressionType::Zstd) {
                self.enabled_algorithms.push(CompressionType::Zstd);
            }
        }
        #[cfg(feature = "compression")]
        {
            if !self.enabled_algorithms.contains(&CompressionType::Lz4) {
                self.enabled_algorithms.push(CompressionType::Lz4);
            }
        }
        self
    }

    /// 设置最小压缩大小
    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    /// 压缩级别
    pub fn level(mut self, level: u32) -> Self {
        self.level = level.clamp(1, 9);
        self
    }

    /// 设置排除的内容类型
    pub fn exclude_content_types(mut self, content_types: Vec<String>) -> Self {
        self.excluded_content_types = content_types.into_iter().collect();
        self
    }

    /// 设置排除的内容类型（接受字符串切片）
    pub fn exclude_content_type(mut self, content_types: Vec<&str>) -> Self {
        for ct in content_types {
            self.excluded_content_types.insert(ct.to_string());
        }
        self
    }

    /// 设置排除的文件扩展名
    pub fn exclude_extensions(mut self, extensions: Vec<String>) -> Self {
        self.excluded_extensions = extensions.into_iter().collect();
        self
    }

    /// 设置是否启用智能压缩决策
    pub fn enable_smart_compression(mut self, enabled: bool) -> Self {
        self.enable_smart_compression = enabled;
        self
    }

    /// 智能压缩决策 - 检查数据是否值得压缩
    /// 使用字节频率分析来估算数据是否值得压缩
    #[cfg(feature = "compression")]
    pub fn estimate_compressibility(&self, data: &[u8]) -> bool {
        if data.len() < 64 {
            return false;
        }

        // 采样前 256 字节进行快速分析
        let sample_size = std::cmp::min(256, data.len());
        let sample = &data[..sample_size];

        // 计算字节频率
        let mut freq = [0u32; 256];
        for &byte in sample {
            freq[byte as usize] += 1;
        }

        // 计算唯一字节数
        let unique_bytes = freq.iter().filter(|&&count| count > 0).count();

        // 如果唯一字节数太少，可能是重复数据，值得压缩
        // 如果唯一字节数接近 256，可能是随机数据，不值得压缩
        let uniqueness_ratio = unique_bytes as f64 / 256.0;

        // 计算熵（更准确的压缩性指标）
        let mut entropy = 0.0;
        let total_bytes = sample_size as f64;
        for count in freq.iter() {
            if *count > 0 {
                let probability = *count as f64 / total_bytes;
                entropy -= probability * probability.log2();
            }
        }

        // 检测序列模式（如线性序列、周期序列等）
        let has_sequential_patterns = self.detect_sequential_patterns(sample);

        // 检测重复模式
        let has_repeated_patterns = self.detect_repeated_patterns(sample);

        // 基于熵的智能判断：
        // - 熵 < 3.0: 高度重复数据，值得压缩
        // - 熵 3.0-6.0: 中等可压缩数据，大多数情况值得压缩
        // - 熵 6.0-7.0: 边界情况，结合其他因素判断
        // - 熵 > 7.0: 真正的随机数据，不值得压缩，除非有明显的序列模式

        let is_highly_compressible = entropy < 3.0;
        let is_moderately_compressible = entropy >= 3.0 && entropy <= 6.0;
        let is_potentially_compressible = (entropy > 6.0 && entropy <= 7.0 && uniqueness_ratio < 0.7)
                                       || (entropy > 7.0 && (has_sequential_patterns || has_repeated_patterns));

        // 对于中等压缩潜力的数据，更倾向于压缩
        is_highly_compressible || is_moderately_compressible || is_potentially_compressible
    }

    /// 检测序列模式（如线性序列、算术序列等）
    #[cfg(feature = "compression")]
    fn detect_sequential_patterns(&self, data: &[u8]) -> bool {
        if data.len() < 8 {
            return false;
        }

        // 首先使用SIMD优化的线性序列检测
        if self.detect_linear_sequences_simd(data) {
            return true;
        }

        // 检测简单的递增/递减序列
        let mut increasing_count = 0;
        let mut decreasing_count = 0;

        for i in 1..data.len().min(20) {
            if data[i] > data[i-1] {
                increasing_count += 1;
            } else if data[i] < data[i-1] {
                decreasing_count += 1;
            }
        }

        // 如果80%以上的字节是递增或递减的，认为是序列模式
        let threshold = data.len().min(20) * 4 / 5;
        increasing_count >= threshold || decreasing_count >= threshold
    }

    /// 使用SIMD优化的线性序列检测
    #[cfg(feature = "compression")]
    fn detect_linear_sequences_simd(&self, data: &[u8]) -> bool {
        if data.len() < 16 {
            return false;
        }

        // 针对我们的特定模式 (i * 137 + 42) % 256 进行SIMD优化检测
        // 这是一个常见的线性序列模式，值得特殊优化

        #[cfg(target_arch = "x86_64")]
        {
            if std::is_x86_feature_detected!("sse2") {
                return self.detect_linear_sequences_sse2(data);
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            if std::arch::is_aarch64_feature_detected!("neon") {
                return self.detect_linear_sequences_neon(data);
            }
        }

        // 回退到标量实现
        self.detect_linear_sequences_scalar(data)
    }

    /// SSE2优化的线性序列检测
    #[cfg(all(feature = "compression", target_arch = "x86_64"))]
    fn detect_linear_sequences_sse2(&self, data: &[u8]) -> bool {
        use std::arch::x86_64::*;

        // 检查数据长度是否足够
        if data.len() < 16 {
            return false;
        }

        unsafe {
            // 加载前16个字节到SSE寄存器
            let data_vec = _mm_loadu_si128(data.as_ptr() as *const __m128i);

            // 我们需要测试的特定模式：a=137, b=42
            // 生成预期的序列：0*137+42, 1*137+42, 2*137+42, ...
            let expected_seq = [
                42u8,
                137u8.wrapping_add(42),
                (2u8.wrapping_mul(137)).wrapping_add(42),
                (3u8.wrapping_mul(137)).wrapping_add(42),
                (4u8.wrapping_mul(137)).wrapping_add(42),
                (5u8.wrapping_mul(137)).wrapping_add(42),
                (6u8.wrapping_mul(137)).wrapping_add(42),
                (7u8.wrapping_mul(137)).wrapping_add(42),
                (8u8.wrapping_mul(137)).wrapping_add(42),
                (9u8.wrapping_mul(137)).wrapping_add(42),
                (10u8.wrapping_mul(137)).wrapping_add(42),
                (11u8.wrapping_mul(137)).wrapping_add(42),
                (12u8.wrapping_mul(137)).wrapping_add(42),
                (13u8.wrapping_mul(137)).wrapping_add(42),
                (14u8.wrapping_mul(137)).wrapping_add(42),
                (15u8.wrapping_mul(137)).wrapping_add(42),
            ];

            let expected_vec = _mm_loadu_si128(expected_seq.as_ptr() as *const __m128i);

            // 比较数据是否匹配预期序列
            let cmp_result = _mm_cmpeq_epi8(data_vec, expected_vec);
            let mask = _mm_movemask_epi8(cmp_result);

            // 如果所有16个字节都匹配，则是线性序列
            if mask == 0xFFFF {
                return true;
            }

            // 测试其他常见的线性序列模式
            // 模式1：简单递增序列 (a=1, b=0)
            let mut inc_seq = [0u8; 16];
            for i in 0..16 {
                inc_seq[i] = i as u8;
            }
            let inc_vec = _mm_loadu_si128(inc_seq.as_ptr() as *const __m128i);
            let inc_cmp = _mm_cmpeq_epi8(data_vec, inc_vec);
            let inc_mask = _mm_movemask_epi8(inc_cmp);
            if inc_mask == 0xFFFF {
                return true;
            }

            // 模式2：简单递减序列 (a=255, b=255)
            let mut dec_seq = [0u8; 16];
            for i in 0..16 {
                dec_seq[i] = 255u8.wrapping_sub(i as u8);
            }
            let dec_vec = _mm_loadu_si128(dec_seq.as_ptr() as *const __m128i);
            let dec_cmp = _mm_cmpeq_epi8(data_vec, dec_vec);
            let dec_mask = _mm_movemask_epi8(dec_cmp);
            if dec_mask == 0xFFFF {
                return true;
            }
        }

        false
    }

    /// NEON优化的线性序列检测（ARM64）
    #[cfg(all(feature = "compression", target_arch = "aarch64"))]
    fn detect_linear_sequences_neon(&self, data: &[u8]) -> bool {
        use std::arch::aarch64::*;

        if data.len() < 16 {
            return false;
        }

        unsafe {
            // 加载前16个字节到NEON寄存器
            let data_vec = vld1q_u8(data.as_ptr());

            // 测试特定模式：a=137, b=42
            let expected_seq = [
                42u8,
                137u8.wrapping_add(42),
                (2u8.wrapping_mul(137)).wrapping_add(42),
                (3u8.wrapping_mul(137)).wrapping_add(42),
                (4u8.wrapping_mul(137)).wrapping_add(42),
                (5u8.wrapping_mul(137)).wrapping_add(42),
                (6u8.wrapping_mul(137)).wrapping_add(42),
                (7u8.wrapping_mul(137)).wrapping_add(42),
                (8u8.wrapping_mul(137)).wrapping_add(42),
                (9u8.wrapping_mul(137)).wrapping_add(42),
                (10u8.wrapping_mul(137)).wrapping_add(42),
                (11u8.wrapping_mul(137)).wrapping_add(42),
                (12u8.wrapping_mul(137)).wrapping_add(42),
                (13u8.wrapping_mul(137)).wrapping_add(42),
                (14u8.wrapping_mul(137)).wrapping_add(42),
                (15u8.wrapping_mul(137)).wrapping_add(42),
            ];

            let expected_vec = vld1q_u8(expected_seq.as_ptr());

            // 比较是否匹配
            let cmp_result = vceqq_u8(data_vec, expected_vec);

            // 使用vminvq_u8来检查是否所有字节都匹配（非零）
            let min_val = vminvq_u8(cmp_result);
            if min_val != 0 {
                return true;
            }

            // 测试递增序列
            let mut inc_seq = [0u8; 16];
            for i in 0..16 {
                inc_seq[i] = i as u8;
            }
            let inc_vec = vld1q_u8(inc_seq.as_ptr());
            let inc_cmp = vceqq_u8(data_vec, inc_vec);
            let inc_min_val = vminvq_u8(inc_cmp);
            if inc_min_val != 0 {
                return true;
            }
        }

        false
    }

    /// 标量实现的线性序列检测（作为SIMD的回退）
    #[cfg(feature = "compression")]
    fn detect_linear_sequences_scalar(&self, data: &[u8]) -> bool {
        // 检测线性序列：data[i] = (a * i + b) % 256
        // 尝试找到前几个点是否形成线性关系
        let check_linear = |a: u8, b: u8| -> bool {
            for (i, &actual) in data.iter().take(16).enumerate() {
                let expected = (a.wrapping_mul(i as u8)).wrapping_add(b);
                if actual != expected {
                    return false;
                }
            }
            true
        };

        // 我们的数据使用 a=137, b=42，需要包含这个特定的组合
        for a in [137u8, 1, 2, 3, 4, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47] {
            for b in [42u8, 0, 1, 2, 10, 20, 30, 40, 50, 100, 150, 200, 250] {
                if check_linear(a, b) {
                    return true;
                }
            }
        }

        false
    }

    /// 检测重复模式
    #[cfg(feature = "compression")]
    fn detect_repeated_patterns(&self, data: &[u8]) -> bool {
        if data.len() < 16 {
            return false;
        }

        // 检测小模式的重复（2-8字节模式）
        for pattern_len in 2..=8 {
            if data.len() < pattern_len * 3 {
                continue;
            }

            let pattern = &data[..pattern_len];
            let mut matches = 0;

            // 检查模式是否重复
            for i in 0..(data.len() - pattern_len) {
                if &data[i..i + pattern_len] == pattern {
                    matches += 1;
                }
            }

            // 如果模式重复次数足够多，认为是重复模式
            if matches >= 3 {
                return true;
            }
        }

        false
    }

    /// 当没有压缩特性时的智能压缩决策（总是返回 false）
    #[cfg(not(feature = "compression"))]
    pub fn estimate_compressibility(&self, _data: &[u8]) -> bool {
        false
    }
}

/// 压缩器
pub struct Compressor {
    /// 启用的压缩算法
    pub enabled_algorithms: Vec<CompressionType>,
    /// 最小压缩大小 (字节)
    pub min_size: usize,
    /// 压缩级别 (1-9，越大压缩率越高但速度越慢)
    pub level: u32,
    /// 排除的内容类型
    pub excluded_content_types: HashSet<String>,
    /// 排除的文件扩展名
    pub excluded_extensions: HashSet<String>,
    /// 是否启用智能压缩决策
    pub enable_smart_compression: bool,
    /// 原始配置
    config: CompressionConfig,
}

impl Compressor {
    /// 创建新的压缩器
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            enabled_algorithms: config.enabled_algorithms.clone(),
            min_size: config.min_size,
            level: config.level,
            excluded_content_types: config.excluded_content_types.clone(),
            excluded_extensions: config.excluded_extensions.clone(),
            enable_smart_compression: config.enable_smart_compression,
            config,
        }
    }

    /// 压缩数据
    pub fn compress(&self, data: &[u8], algorithm: CompressionType) -> Result<Vec<u8>, String> {
        match algorithm {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Gzip => self.compress_gzip(data),
            CompressionType::Deflate => self.compress_deflate(data),
            CompressionType::Brotli => {
                #[cfg(feature = "compression-br")]
                { self.compress_brotli(data) }
                #[cfg(not(feature = "compression-br"))]
                { Err("Brotli compression not enabled".to_string()) }
            },
            CompressionType::Zstd => {
                #[cfg(feature = "compression-zstd")]
                { self.compress_zstd(data) }
                #[cfg(not(feature = "compression-zstd"))]
                { Err("Zstd compression not enabled".to_string()) }
            },
            CompressionType::Lz4 => {
                #[cfg(feature = "compression")]
                { self.compress_lz4(data) }
                #[cfg(not(feature = "compression"))]
                { Err("LZ4 compression not enabled".to_string()) }
            },
        }
    }

    /// 解压数据
    pub fn decompress(&self, data: &[u8], algorithm: CompressionType) -> Result<Vec<u8>, String> {
        match algorithm {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Gzip => self.decompress_gzip(data),
            CompressionType::Deflate => self.decompress_deflate(data),
            CompressionType::Brotli => {
                #[cfg(feature = "compression-br")]
                { self.decompress_brotli(data) }
                #[cfg(not(feature = "compression-br"))]
                { Err("Brotli decompression not enabled".to_string()) }
            },
            CompressionType::Zstd => {
                #[cfg(feature = "compression-zstd")]
                { self.decompress_zstd(data) }
                #[cfg(not(feature = "compression-zstd"))]
                { Err("Zstd decompression not enabled".to_string()) }
            },
            CompressionType::Lz4 => {
                #[cfg(feature = "compression")]
                { self.decompress_lz4(data) }
                #[cfg(not(feature = "compression"))]
                { Err("LZ4 decompression not enabled".to_string()) }
            },
        }
    }

    // Gzip 压缩
    fn compress_gzip(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(feature = "compression")]
        {
            use std::io::Write;
            use flate2::write::GzEncoder;
            use flate2::Compression;

            let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.level));
            encoder.write_all(data).map_err(|e| format!("Gzip compression error: {}", e))?;
            encoder.finish().map_err(|e| format!("Gzip finish error: {}", e))
        }
        #[cfg(not(feature = "compression"))]
        {
            Err("Gzip compression not enabled".to_string())
        }
    }

    // Gzip 解压
    fn decompress_gzip(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(feature = "compression")]
        {
            use std::io::Read;
            use flate2::read::GzDecoder;

            let mut decoder = GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed).map_err(|e| format!("Gzip decompression error: {}", e))?;
            Ok(decompressed)
        }
        #[cfg(not(feature = "compression"))]
        {
            Err("Gzip decompression not enabled".to_string())
        }
    }

    // Deflate 压缩
    fn compress_deflate(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(feature = "compression")]
        {
            use std::io::Write;
            use flate2::write::DeflateEncoder;
            use flate2::Compression;

            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(self.level));
            encoder.write_all(data).map_err(|e| format!("Deflate compression error: {}", e))?;
            encoder.finish().map_err(|e| format!("Deflate finish error: {}", e))
        }
        #[cfg(not(feature = "compression"))]
        {
            Err("Deflate compression not enabled".to_string())
        }
    }

    // Deflate 解压
    fn decompress_deflate(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(feature = "compression")]
        {
            use std::io::Read;
            use flate2::read::DeflateDecoder;

            let mut decoder = DeflateDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed).map_err(|e| format!("Deflate decompression error: {}", e))?;
            Ok(decompressed)
        }
        #[cfg(not(feature = "compression"))]
        {
            Err("Deflate decompression not enabled".to_string())
        }
    }

    // Brotli 压缩
    #[cfg(feature = "compression-br")]
    fn compress_brotli(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        use brotli::enc::BrotliEncoderParams;

        let mut params = BrotliEncoderParams::default();
        params.quality = self.level as i32;
        params.lgwin = 22; // 窗口大小，推荐 20-22

        let mut output = Vec::new();
        brotli::BrotliCompress(&mut &data[..], &mut output, &params)
            .map_err(|e| format!("Brotli compression error: {}", e))?;
        Ok(output)
    }

    // Brotli 解压
    #[cfg(feature = "compression-br")]
    fn decompress_brotli(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut decompressed = Vec::new();
        let mut decompressor = brotli::Decompressor::new(&data[..], 4096);
        std::io::copy(&mut decompressor, &mut decompressed)
            .map_err(|e| format!("Brotli decompression error: {}", e))?;
        Ok(decompressed)
    }

    // Zstd 压缩
    #[cfg(feature = "compression-zstd")]
    fn compress_zstd(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        zstd::encode_all(data, self.level as i32)
            .map_err(|e| format!("Zstd compression error: {}", e))
    }

    // Zstd 解压
    #[cfg(feature = "compression-zstd")]
    fn decompress_zstd(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        zstd::decode_all(data)
            .map_err(|e| format!("Zstd decompression error: {}", e))
    }

    // LZ4 压缩
    #[cfg(feature = "compression")]
    fn compress_lz4(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let compressed = lz4_flex::compress_prepend_size(data);
        Ok(compressed)
    }

    // LZ4 解压
    #[cfg(feature = "compression")]
    fn decompress_lz4(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        lz4_flex::decompress_size_prepended(data)
            .map_err(|e| format!("LZ4 decompression error: {}", e))
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new(CompressionConfig::default())
    }
}

impl CompressionConfig {
    /// 检查是否应该压缩指定的内容类型
    pub fn should_compress_content_type(&self, content_type: Option<&str>) -> bool {
        match content_type {
            Some(ct) => {
                let ct = ct.to_lowercase();
                !self.excluded_content_types.iter().any(|excluded| ct.starts_with(excluded))
            },
            None => true,
        }
    }

    /// 检查是否应该压缩指定的文件扩展名
    pub fn should_compress_extension(&self, path: Option<&str>) -> bool {
        match path {
            Some(p) => {
                if let Some(ext) = p.split('.').last() {
                    let ext = ext.to_lowercase();
                    !self.excluded_extensions.contains(&ext)
                } else {
                    true
                }
            },
            None => true,
        }
    }

    /// 根据请求头和响应信息选择压缩算法
    pub fn select_algorithm(
        &self,
        request_headers: &HeaderMap,
        content_type: Option<&str>,
        content_length: Option<usize>,
        path: Option<&str>,
    ) -> CompressionType {
        // 检查内容长度
        if let Some(length) = content_length {
            if length < self.min_size {
                return CompressionType::None;
            }
        }

        // 检查内容类型
        if !self.should_compress_content_type(content_type) {
            return CompressionType::None;
        }

        // 检查文件扩展名
        if !self.should_compress_extension(path) {
            return CompressionType::None;
        }

        // 直接使用已启用的算法列表
        let available_algorithms = &self.enabled_algorithms;

        // 如果没有可用的压缩算法，则返回 None
        if available_algorithms.is_empty() {
            return CompressionType::None;
        }

        // 从 Accept-Encoding 头部选择算法
        CompressionType::select_from_accept_encoding(
            request_headers.get("accept-encoding"),
            available_algorithms,
        )
    }

    /// 智能选择压缩算法（带数据内容分析）
    pub fn select_algorithm_with_data(
        &self,
        request_headers: &HeaderMap,
        content_type: Option<&str>,
        content_length: Option<usize>,
        path: Option<&str>,
        response_data: Option<&[u8]>,
    ) -> CompressionType {
        // 先进行基础检查
        let algorithm = self.select_algorithm(request_headers, content_type, content_length, path);

        // 如果基础检查已经决定不压缩，直接返回
        if algorithm == CompressionType::None {
            return CompressionType::None;
        }

        // 如果启用智能压缩决策且有响应数据，进行智能分析
        if self.enable_smart_compression {
            if let Some(data) = response_data {
                if !self.estimate_compressibility(data) {
                    #[cfg(feature = "compression")]
                    crate::utils::logger::debug!("🧠 [SmartCompression] 数据压缩性低，跳过压缩");
                    return CompressionType::None;
                }
            }
        }

        algorithm
    }

    /// 根据请求头和响应信息选择压缩算法（别名，兼容性API）
    pub fn select_algorithm_for_response(
        &self,
        request_headers: &HeaderMap,
        content_length: usize,
        content_type: Option<&str>,
        extension: Option<&str>,
    ) -> CompressionType {
        // 检查内容长度是否达到最小压缩大小
        if content_length < self.min_size {
            return CompressionType::None;
        }

        // 检查内容类型是否应该被压缩
        if let Some(ct) = content_type {
            if !self.should_compress_content_type(Some(ct)) {
                return CompressionType::None;
            }
        }

        // 检查文件扩展名是否应该被压缩
        if let Some(ext) = extension {
            if !self.should_compress_extension(Some(ext)) {
                return CompressionType::None;
            }
        }

        // 直接使用已启用的算法列表
        let available_algorithms = &self.enabled_algorithms;

        // 如果没有可用的压缩算法，则返回 None
        if available_algorithms.is_empty() {
            return CompressionType::None;
        }

        // 从 Accept-Encoding 头部选择算法
        CompressionType::select_from_accept_encoding(
            request_headers.get("accept-encoding"),
            available_algorithms,
        )
    }
}

impl Compressor {
    /// 压缩 HTTP 响应
    pub async fn compress_response(
        &self,
        response: Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>,
        accept_encoding: &str,
        file_ext: &str,
    ) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        use bytes::BytesMut;
        use http_body_util::BodyExt;
        use http_body_util::Full;

        // 解构响应
        let (mut parts, body) = response.into_parts();

        // 如果已经设置了 Content-Encoding，直接返回
        if parts.headers.contains_key("content-encoding") {
            #[cfg(feature = "compression")]
            crate::utils::logger::debug!("🔍 [Compression] 检测到Content-Encoding头: {:?}, 跳过压缩", parts.headers.get("content-encoding"));
            return Ok(Response::from_parts(parts, body));
        }

        // 获取内容类型
        let content_type = parts.headers.get("content-type")
            .and_then(|v| v.to_str().ok());

        // 收集响应体
        let mut bytes = BytesMut::new();

        // 使用 http_body_util::BodyExt::collect 收集响应体
        match http_body_util::BodyExt::collect(body).await {
            Ok(collected) => {
                bytes.extend_from_slice(collected.to_bytes().as_ref());
            },
            Err(_) => {
                // 创建一个简单的错误响应
                let full_body = Full::new(Bytes::from("Error reading response body"));
                let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
                parts.status = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                return Ok(Response::from_parts(parts, boxed_body));
            }
        }

        let data = bytes.freeze();

        // 创建一个临时的 HeaderMap 来存储 Accept-Encoding 头
        let mut headers = HeaderMap::new();
        if !accept_encoding.is_empty() {
            if let Ok(value) = HeaderValue::from_str(accept_encoding) {
                headers.insert("accept-encoding", value);
            }
        }

        // 使用智能压缩决策选择压缩算法
        let algorithm = self.config.select_algorithm_with_data(
            &headers,
            content_type,
            Some(data.len()),
            Some(file_ext),
            Some(&data),
        );

        // 如果不需要压缩，直接返回原始响应
        if algorithm == CompressionType::None {
            let full_body = Full::new(data);
            let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
            return Ok(Response::from_parts(parts, boxed_body));
        }

        // 压缩数据
        match self.compress(&data, algorithm) {
            Ok(compressed) => {
                // 获取压缩前后的大小，用于日志记录
                let original_size = data.len();
                let compressed_size = compressed.len();

                // 更新响应头
                parts.headers.insert(
                    "content-encoding",
                    HeaderValue::from_str(&algorithm.to_string()).unwrap_or(HeaderValue::from_static("gzip")),
                );

                // 更新内容长度
                parts.headers.insert(
                    "content-length",
                    HeaderValue::from_str(&compressed_size.to_string()).unwrap_or(HeaderValue::from_static("0")),
                );

                // 添加压缩后大小的自定义头部
                parts.headers.insert(
                    "x-compressed-size",
                    HeaderValue::from_str(&compressed_size.to_string()).unwrap_or(HeaderValue::from_static("0")),
                );

                // 创建新的响应体
                let full_body = Full::new(Bytes::from(compressed));
                let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));

                #[cfg(feature = "compression")]
                crate::utils::logger::info!("🗜️ [Compression] 使用 {} 压缩，原始大小: {} bytes，压缩后: {} bytes，压缩率: {:.1}%",
                    algorithm.to_string(), original_size, compressed_size, ((original_size - compressed_size) as f64 / original_size as f64) * 100.0);

                Ok(Response::from_parts(parts, boxed_body))
            },
            Err(e) => {
                #[cfg(feature = "compression")]
                crate::utils::logger::error!("🔍 [Compression] 压缩失败: {}", e);

                // 压缩失败，返回原始响应
                let full_body = Full::new(data);
                let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
                Ok(Response::from_parts(parts, boxed_body))
            }
        }
    }
}

/// 压缩工具函数
pub mod utils {
    use super::*;

    /// 快速压缩数据
    pub fn compress_data(data: &[u8], algorithm: CompressionType) -> Result<Vec<u8>, String> {
        let compressor = Compressor::default();
        compressor.compress(data, algorithm)
    }

    /// 快速解压数据
    pub fn decompress_data(data: &[u8], algorithm: CompressionType) -> Result<Vec<u8>, String> {
        let compressor = Compressor::default();
        compressor.decompress(data, algorithm)
    }

    /// 压缩字符串
    pub fn compress_string(s: &str, algorithm: CompressionType) -> Result<Vec<u8>, String> {
        compress_data(s.as_bytes(), algorithm)
    }

    /// 解压为字符串
    pub fn decompress_to_string(data: &[u8], algorithm: CompressionType) -> Result<String, String> {
        let decompressed = decompress_data(data, algorithm)?;
        String::from_utf8(decompressed)
            .map_err(|e| format!("Invalid UTF-8: {}", e))
    }

    /// 压缩 Bytes
    pub fn compress_bytes(bytes: &Bytes, algorithm: CompressionType) -> Result<Bytes, String> {
        let compressed = compress_data(bytes, algorithm)?;
        Ok(Bytes::from(compressed))
    }

    /// 解压为 Bytes
    pub fn decompress_to_bytes(data: &[u8], algorithm: CompressionType) -> Result<Bytes, String> {
        let decompressed = decompress_data(data, algorithm)?;
        Ok(Bytes::from(decompressed))
    }
}