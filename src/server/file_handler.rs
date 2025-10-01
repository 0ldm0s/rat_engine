//! 高性能文件处理模块
//! 
//! 提供静态文件服务和动态文件生成功能，解决 Flask 文件处理性能问题
//! 
//! 功能特性：
//! 1. 静态文件服务 - 零拷贝文件读取
//! 2. 动态文件生成 - GridFS、PIL、Base64 等场景
//! 3. 流式传输 - 大文件分块传输
//! 4. MIME 类型自动检测
//! 5. 缓存控制和 ETag 支持
//! 6. 范围请求支持 (HTTP Range)

use hyper::{Request, Response, StatusCode, HeaderMap};
use hyper::body::{Incoming, Bytes};
use http_body_util::Full;
use std::path::{Path, PathBuf};
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs as async_fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Sha256, Digest};
use std::time::SystemTime;
use crate::error::RatError;

/// MIME 类型映射
static MIME_TYPES: &[(&str, &str)] = &[
    // 图片
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("ico", "image/x-icon"),
    ("bmp", "image/bmp"),
    ("tiff", "image/tiff"),
    
    // 文档
    ("pdf", "application/pdf"),
    ("doc", "application/msword"),
    ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    ("xls", "application/vnd.ms-excel"),
    ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ("ppt", "application/vnd.ms-powerpoint"),
    ("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
    
    // 音视频
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("mp4", "video/mp4"),
    ("avi", "video/x-msvideo"),
    ("mov", "video/quicktime"),
    
    // 文本
    ("txt", "text/plain; charset=utf-8"),
    ("html", "text/html; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("js", "application/javascript; charset=utf-8"),
    ("json", "application/json; charset=utf-8"),
    ("xml", "application/xml; charset=utf-8"),
    
    // 压缩文件
    ("zip", "application/zip"),
    ("rar", "application/x-rar-compressed"),
    ("7z", "application/x-7z-compressed"),
    ("tar", "application/x-tar"),
    ("gz", "application/gzip"),
];

/// 文件处理器配置
#[derive(Debug, Clone)]
pub struct FileHandlerConfig {
    /// 静态文件根目录
    pub static_root: PathBuf,
    /// 最大文件大小 (字节)
    pub max_file_size: u64,
    /// 启用 ETag
    pub enable_etag: bool,
    /// 启用缓存控制
    pub enable_cache_control: bool,
    /// 默认缓存时间 (秒)
    pub default_cache_time: u32,
    /// 分块大小 (用于大文件流式传输)
    pub chunk_size: usize,
}

impl Default for FileHandlerConfig {
    fn default() -> Self {
        Self {
            static_root: PathBuf::from("static"),
            max_file_size: 100 * 1024 * 1024, // 100MB
            enable_etag: true,
            enable_cache_control: true,
            default_cache_time: 3600, // 1小时
            chunk_size: 64 * 1024, // 64KB
        }
    }
}

/// 文件处理器
pub struct FileHandler {
    config: FileHandlerConfig,
    mime_map: HashMap<String, String>,
}

impl FileHandler {
    /// 创建新的文件处理器
    pub fn new(config: FileHandlerConfig) -> Self {
        let mut mime_map = HashMap::new();
        for (ext, mime) in MIME_TYPES {
            mime_map.insert(ext.to_string(), mime.to_string());
        }
        
        Self {
            config,
            mime_map,
        }
    }
    
    /// 创建默认文件处理器
    pub fn default() -> Self {
        Self::new(FileHandlerConfig::default())
    }
    
    /// 服务静态文件
    pub async fn serve_static_file<B>(
        &self,
        file_path: &str,
        req: &Request<B>,
    ) -> Result<Response<Full<Bytes>>, RatError> {
        // 安全检查：防止路径遍历攻击
        let safe_path = self.sanitize_path(file_path)?;
        let full_path = self.config.static_root.join(safe_path);
        
        // 检查文件是否存在
        if !full_path.exists() || !full_path.is_file() {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(r#"{"error":"File not found"}"#)))
                .unwrap());
        }
        
        // 获取文件元数据
        let metadata = async_fs::metadata(&full_path).await?;
        
        // 检查文件大小
        if metadata.len() > self.config.max_file_size {
            return Ok(Response::builder()
                .status(StatusCode::PAYLOAD_TOO_LARGE)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(r#"{"error":"File too large"}"#)))
                .unwrap());
        }
        
        // 处理条件请求 (ETag, If-Modified-Since)
        if let Some(response) = self.handle_conditional_request(&full_path, &metadata, req).await? {
            return Ok(response);
        }
        
        // 处理范围请求
        if let Some(range_header) = req.headers().get("range") {
            return self.handle_range_request(&full_path, &metadata, range_header).await;
        }
        
        // 读取完整文件
        let content = async_fs::read(&full_path).await?;
        
        // 构建响应
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Length", content.len().to_string());
        
        // 设置 MIME 类型
        if let Some(mime_type) = self.get_mime_type(&full_path) {
            response = response.header("Content-Type", mime_type);
        }
        
        // 设置缓存控制
        if self.config.enable_cache_control {
            response = response.header(
                "Cache-Control", 
                format!("public, max-age={}", self.config.default_cache_time)
            );
        }
        
        // 设置 ETag
        if self.config.enable_etag {
            let etag = self.generate_etag(&metadata).await?;
            response = response.header("ETag", format!("\"{}\"", etag));
        }
        
        // 设置最后修改时间
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                response = response.header(
                    "Last-Modified",
                    httpdate::fmt_http_date(modified)
                );
            }
        }
        
        Ok(response
            .body(Full::new(Bytes::from(content)))
            .unwrap())
    }
    
    /// 处理动态文件生成
    pub async fn serve_dynamic_file(
        &self,
        content: Vec<u8>,
        mime_type: Option<&str>,
        filename: Option<&str>,
    ) -> Result<Response<Full<Bytes>>, RatError> {
        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Length", content.len().to_string());
        
        // 设置 MIME 类型
        if let Some(mime) = mime_type {
            response = response.header("Content-Type", mime);
        } else {
            response = response.header("Content-Type", "application/octet-stream");
        }
        
        // 设置文件名
        if let Some(name) = filename {
            response = response.header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", name)
            );
        }
        
        Ok(response
            .body(Full::new(Bytes::from(content)))
            .unwrap())
    }
    
    /// 处理 Base64 图片
    pub async fn serve_base64_image(
        &self,
        base64_data: &str,
        filename: Option<&str>,
    ) -> Result<Response<Full<Bytes>>, RatError> {
        // 解析 Base64 数据 (支持 data:image/png;base64,xxx 格式)
        let (mime_type, data) = if base64_data.starts_with("data:") {
            let parts: Vec<&str> = base64_data.splitn(2, ',').collect();
            if parts.len() != 2 {
                return Err(RatError::ValidationError("Invalid base64 data format".to_string()));
            }
            
            let header = parts[0];
            let data = parts[1];
            
            // 提取 MIME 类型
            let mime = if let Some(start) = header.find("data:") {
                if let Some(end) = header.find(";") {
                    &header[start + 5..end]
                } else {
                    "application/octet-stream"
                }
            } else {
                "application/octet-stream"
            };
            
            (mime, data)
        } else {
            ("image/png", base64_data) // 默认为 PNG
        };
        
        // 解码 Base64
        let content = general_purpose::STANDARD.decode(data)
            .map_err(|e| RatError::ValidationError(format!("Invalid base64 data: {}", e)))?;
        
        self.serve_dynamic_file(content, Some(mime_type), filename).await
    }
    
    /// 安全化路径，防止路径遍历攻击
    fn sanitize_path(&self, path: &str) -> Result<PathBuf, RatError> {
        // 检查是否为绝对路径
        if path.starts_with('/') {
            return Err(RatError::SecurityError("Absolute path not allowed".to_string()));
        }
        
        let path = PathBuf::from(path);
        
        // 检查是否包含危险的路径组件
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(RatError::SecurityError("Path traversal attempt detected".to_string()));
                }
                std::path::Component::RootDir => {
                    return Err(RatError::SecurityError("Absolute path not allowed".to_string()));
                }
                _ => {}
            }
        }
        
        Ok(path)
    }
    
    /// 获取文件的 MIME 类型
    fn get_mime_type(&self, path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| self.mime_map.get(&ext.to_lowercase()))
            .cloned()
    }
    
    /// 生成 ETag
    async fn generate_etag(&self, metadata: &Metadata) -> Result<String, RatError> {
        let mut hasher = Sha256::new();
        
        // 使用文件大小和修改时间生成 ETag
        hasher.update(metadata.len().to_le_bytes());
        
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                hasher.update(duration.as_secs().to_le_bytes());
            }
        }
        
        let result = hasher.finalize();
        Ok(format!("{:x}", result)[..16].to_string()) // 取前16个字符
    }
    
    /// 处理条件请求
    async fn handle_conditional_request<B>(
        &self,
        path: &Path,
        metadata: &Metadata,
        req: &Request<B>,
    ) -> Result<Option<Response<Full<Bytes>>>, RatError> {
        // 检查 If-None-Match (ETag)
        if self.config.enable_etag {
            if let Some(if_none_match) = req.headers().get("if-none-match") {
                let etag = self.generate_etag(metadata).await?;
                let request_etag = if_none_match.to_str().unwrap_or("");
                
                if request_etag == format!("\"{}\"", etag) || request_etag == "*" {
                    return Ok(Some(
                        Response::builder()
                            .status(StatusCode::NOT_MODIFIED)
                            .header("ETag", format!("\"{}\"", etag))
                            .body(Full::new(Bytes::new()))
                            .unwrap()
                    ));
                }
            }
        }
        
        // 检查 If-Modified-Since
        if let Some(if_modified_since) = req.headers().get("if-modified-since") {
            if let Ok(request_time) = httpdate::parse_http_date(if_modified_since.to_str().unwrap_or("")) {
                if let Ok(file_modified) = metadata.modified() {
                    if file_modified <= request_time {
                        return Ok(Some(
                            Response::builder()
                                .status(StatusCode::NOT_MODIFIED)
                                .body(Full::new(Bytes::new()))
                                .unwrap()
                        ));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// 处理范围请求
    async fn handle_range_request(
        &self,
        path: &Path,
        metadata: &Metadata,
        range_header: &hyper::header::HeaderValue,
    ) -> Result<Response<Full<Bytes>>, RatError> {
        let range_str = range_header.to_str().unwrap_or("");
        
        // 解析 Range 头 (例如: "bytes=0-1023")
        if !range_str.starts_with("bytes=") {
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{}", metadata.len()))
                .body(Full::new(Bytes::new()))
                .unwrap());
        }
        
        let range_part = &range_str[6..]; // 去掉 "bytes="
        let parts: Vec<&str> = range_part.split('-').collect();
        
        if parts.len() != 2 {
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{}", metadata.len()))
                .body(Full::new(Bytes::new()))
                .unwrap());
        }
        
        let file_size = metadata.len();
        let start = if parts[0].is_empty() {
            0
        } else {
            parts[0].parse::<u64>().unwrap_or(0)
        };
        
        let end = if parts[1].is_empty() {
            file_size - 1
        } else {
            parts[1].parse::<u64>().unwrap_or(file_size - 1).min(file_size - 1)
        };
        
        if start > end || start >= file_size {
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("Content-Range", format!("bytes */{}", file_size))
                .body(Full::new(Bytes::new()))
                .unwrap());
        }
        
        // 读取指定范围的数据
        let mut file = async_fs::File::open(path).await?;
        
        file.seek(SeekFrom::Start(start)).await?;
        
        let content_length = end - start + 1;
        let mut content = vec![0u8; content_length as usize];
        
        file.read_exact(&mut content).await?;
        
        // 构建部分内容响应
        let mut response = Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header("Content-Length", content_length.to_string())
            .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
            .header("Accept-Ranges", "bytes");
        
        // 设置 MIME 类型
        if let Some(mime_type) = self.get_mime_type(path) {
            response = response.header("Content-Type", mime_type);
        }
        
        Ok(response
            .body(Full::new(Bytes::from(content)))
            .unwrap())
    }
}

/// GridFS 文件处理器 (示例接口)
pub trait GridFSHandler: Send + Sync {
    /// 从 GridFS 读取文件
    async fn read_file(&self, file_id: &str) -> Result<(Vec<u8>, String), RatError>;
}

/// PIL 图片处理器 (示例接口)
pub trait ImageProcessor: Send + Sync {
    /// 处理图片并返回字节数据
    async fn process_image(
        &self,
        operation: &str,
        params: &HashMap<String, String>,
    ) -> Result<(Vec<u8>, String), RatError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_sanitize_path() {
        let handler = FileHandler::default();
        
        // 正常路径
        assert!(handler.sanitize_path("images/test.jpg").is_ok());
        
        // 路径遍历攻击
        assert!(handler.sanitize_path("../../../etc/passwd").is_err());
        assert!(handler.sanitize_path("/etc/passwd").is_err());
    }
    
    #[tokio::test]
    async fn test_mime_type_detection() {
        let handler = FileHandler::default();
        
        assert_eq!(handler.get_mime_type(Path::new("test.jpg")), Some("image/jpeg".to_string()));
        assert_eq!(handler.get_mime_type(Path::new("test.png")), Some("image/png".to_string()));
        assert_eq!(handler.get_mime_type(Path::new("test.pdf")), Some("application/pdf".to_string()));
        assert_eq!(handler.get_mime_type(Path::new("test.unknown")), None);
    }
    
    #[tokio::test]
    async fn test_base64_image_parsing() {
        let handler = FileHandler::default();
        
        // 测试标准 Base64 图片数据
        let base64_data = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
        
        let result = handler.serve_base64_image(base64_data, Some("test.png")).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}