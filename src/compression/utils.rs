//! 压缩工具函数模块

pub mod utils {
    use crate::compression::types::CompressionType;
    use crate::compression::compressor::Compressor;
    use bytes::Bytes;

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
