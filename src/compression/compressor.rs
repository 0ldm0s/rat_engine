//! 压缩器模块

use std::collections::HashSet;
use std::fmt;
use hyper::header::{HeaderMap, HeaderValue};
use bytes::Bytes;
use hyper::Response;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use bytes::BytesMut;

use super::types::CompressionType;
use super::config::CompressionConfig;

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
