//! 压缩模块
//!
//! 提供多种压缩算法支持，包括 Gzip、Deflate、Brotli、Zstd 和 LZ4
//! 可以根据 Accept-Encoding 头部自动选择最佳压缩算法
//! 支持配置压缩级别、最小压缩大小和排除特定内容类型

pub mod types;
pub mod config;
pub mod compressor;
pub mod utils;

// 重新导出主要的公共类型
pub use types::CompressionType;
pub use config::CompressionConfig;
pub use compressor::Compressor;