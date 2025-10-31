//! 压缩算法类型模块

use hyper::header::HeaderValue;

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
