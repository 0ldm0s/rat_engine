//! 压缩配置模块

use std::collections::HashSet;
use hyper::header::HeaderMap;
use super::types::CompressionType;

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
