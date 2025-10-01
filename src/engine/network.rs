//! 零拷贝网络 I/O 模块
//! 
//! 实现高性能的 HTTP 请求处理：
//! - 零拷贝缓冲区管理
//! - 原地 HTTP 解析
//! - 异步 I/O 优化
//! - 内存复用策略

use bytes::{Bytes, BytesMut, Buf, BufMut};
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use crate::engine::memory::MemoryPool;
use crate::utils::ip_extractor::IpExtractor;

/// HTTP 任务
/// 
/// 封装了一个完整的 HTTP 请求处理任务，包括网络连接、
/// 缓冲区管理和响应发送。
pub struct HttpTask {
    /// TCP 连接流
    stream: TcpStream,
    /// 客户端地址
    remote_addr: SocketAddr,
    /// 零拷贝缓冲区
    buffer: ZeroCopyBuffer,
    /// 任务创建时间
    created_at: Instant,
    /// 内存池引用
    memory_pool: Arc<MemoryPool>,
}

impl HttpTask {
    /// 创建新的 HTTP 任务
    pub fn new(stream: TcpStream, remote_addr: SocketAddr, memory_pool: Arc<MemoryPool>) -> Self {
        let buffer = ZeroCopyBuffer::new_from_pool(&memory_pool);
        
        Self {
            stream,
            remote_addr,
            buffer,
            created_at: Instant::now(),
            memory_pool,
        }
    }
    
    /// 读取 HTTP 请求
    pub async fn read_request(&mut self) -> Result<crate::engine::HttpRequest, HttpError> {
        // 读取请求数据到缓冲区
        self.buffer.read_http_request(&mut self.stream).await?;
        
        // 解析 HTTP 请求
        let request = self.buffer.parse_http_request(self.remote_addr)?;
        
        Ok(request)
    }
    
    /// 发送 HTTP 响应（原始字节）
    pub async fn send_response(&mut self, response_bytes: Vec<u8>) -> Result<(), HttpError> {
        // 发送响应
        self.stream.write_all(&response_bytes).await
            .map_err(|e| HttpError::IoError(e))?;
        
        self.stream.flush().await
            .map_err(|e| HttpError::IoError(e))?;
        
        Ok(())
    }
    
    /// 发送结构化 HTTP 响应
    pub async fn send_structured_response(&mut self, response: crate::engine::HttpResponse) -> Result<(), HttpError> {
        // 构建 HTTP 响应
        let response_bytes = self.build_http_response(response)?;
        
        // 发送响应
        self.send_response(response_bytes).await
    }
    
    /// 构建 HTTP 响应字节
    fn build_http_response(&self, response: crate::engine::HttpResponse) -> Result<Vec<u8>, HttpError> {
        let mut response_bytes = Vec::with_capacity(1024);
        
        // 状态行
        response_bytes.extend_from_slice(format!("HTTP/1.1 {} {}\r\n", 
            response.status_code, 
            status_text(response.status_code)
        ).as_bytes());
        
        // 响应头
        for (key, value) in &response.headers {
            response_bytes.extend_from_slice(format!("{}:{}\r\n", key, value).as_bytes());
        }
        
        // Content-Length
        response_bytes.extend_from_slice(format!("Content-Length: {}\r\n", response.body.len()).as_bytes());
        
        // 服务器标识
        response_bytes.extend_from_slice(format!("Server: RAT-Engine/{}\r\n", env!("CARGO_PKG_VERSION")).as_bytes());
        
        // 空行
        response_bytes.extend_from_slice(b"\r\n");
        
        // 响应体
        response_bytes.extend_from_slice(&response.body);
        
        Ok(response_bytes)
    }
    
    /// 获取任务年龄
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
    
    /// 获取远程地址
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
}

/// 零拷贝缓冲区
/// 
/// 实现高效的内存管理和原地数据解析，最小化内存分配和拷贝操作。
pub struct ZeroCopyBuffer {
    /// 主缓冲区
    buffer: BytesMut,
    /// 读取位置
    read_pos: usize,
    /// 写入位置
    write_pos: usize,
    /// 缓冲区容量
    capacity: usize,
}

impl ZeroCopyBuffer {
    /// 创建新的零拷贝缓冲区
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
            read_pos: 0,
            write_pos: 0,
            capacity,
        }
    }
    
    /// 从内存池创建缓冲区
    pub fn new_from_pool(memory_pool: &MemoryPool) -> Self {
        let buffer = memory_pool.get_buffer();
        let capacity = buffer.capacity();
        
        Self {
            buffer,
            read_pos: 0,
            write_pos: 0,
            capacity,
        }
    }
    
    /// 零拷贝读取 HTTP 请求
    pub async fn read_http_request(&mut self, stream: &mut TcpStream) -> Result<(), HttpError> {
        // 确保缓冲区有足够空间
        if self.buffer.remaining_mut() < 4096 {
            self.buffer.reserve(4096);
        }
        
        // 读取数据直到找到完整的 HTTP 请求
        loop {
            let n = stream.read_buf(&mut self.buffer).await
                .map_err(HttpError::IoError)?;
            
            if n == 0 {
                return Err(HttpError::ConnectionClosed);
            }
            
            self.write_pos += n;
            
            // 检查是否有完整的 HTTP 请求
            if self.has_complete_request() {
                break;
            }
            
            // 防止缓冲区过大
            if self.buffer.len() > 1024 * 1024 { // 1MB 限制
                return Err(HttpError::RequestTooLarge);
            }
        }
        
        Ok(())
    }
    
    /// 检查是否有完整的 HTTP 请求
    fn has_complete_request(&self) -> bool {
        let data = &self.buffer[self.read_pos..self.write_pos];
        
        // 查找请求头结束标记
        if let Some(headers_end) = self.find_headers_end(data) {
            // 检查是否需要读取请求体
            if let Some(content_length) = self.extract_content_length(data) {
                let body_start = headers_end + 4; // "\r\n\r\n" 的长度
                let available_body = data.len().saturating_sub(body_start);
                available_body >= content_length
            } else {
                true // 没有请求体，只需要头部完整
            }
        } else {
            false
        }
    }
    
    /// 查找请求头结束位置
    fn find_headers_end(&self, data: &[u8]) -> Option<usize> {
        data.windows(4).position(|w| w == b"\r\n\r\n")
    }
    
    /// 提取 Content-Length
    fn extract_content_length(&self, data: &[u8]) -> Option<usize> {
        let headers_str = String::from_utf8_lossy(data);
        
        for line in headers_str.lines() {
            if line.to_lowercase().starts_with("content-length:") {
                if let Some(value) = line.split(':').nth(1) {
                    return value.trim().parse().ok();
                }
            }
        }
        
        None
    }
    
    /// 原地解析 HTTP 请求
    pub fn parse_http_request(&mut self, remote_addr: SocketAddr) -> Result<crate::engine::HttpRequest, HttpError> {
        let data = &self.buffer[self.read_pos..self.write_pos];
        
        // 查找请求头结束位置
        let headers_end = self.find_headers_end(data)
            .ok_or(HttpError::InvalidRequest("Headers not complete".to_string()))?;
        
        let headers_data = &data[..headers_end];
        let body_start = headers_end + 4;
        
        // 解析请求行
        let (method, path, query_string) = self.parse_request_line(headers_data)?;
        
        // 解析请求头
        let headers = self.parse_headers(headers_data)?;
        
        // 提取请求体
        let body = if body_start < data.len() {
            data[body_start..].to_vec()
        } else {
            Vec::new()
        };
        
        // 提取真实 IP
        let real_ip = IpExtractor::extract_real_ip(&headers, &remote_addr.to_string());
        
        Ok(crate::engine::HttpRequest {
            method,
            path,
            query_string,
            headers,
            body,
            remote_addr: remote_addr.to_string(),
            real_ip,
        })
    }
    
    /// 解析请求行
    fn parse_request_line(&self, headers_data: &[u8]) -> Result<(String, String, String), HttpError> {
        let headers_str = String::from_utf8_lossy(headers_data);
        let first_line = headers_str.lines().next()
            .ok_or_else(|| HttpError::InvalidRequest("No request line".to_string()))?;
        
        let parts: Vec<&str> = first_line.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err(HttpError::InvalidRequest("Invalid request line format".to_string()));
        }
        
        let method = parts[0].to_string();
        let url = parts[1];
        
        // 分离路径和查询字符串
        let (path, query_string) = if let Some(pos) = url.find('?') {
            (url[..pos].to_string(), url[pos + 1..].to_string())
        } else {
            (url.to_string(), String::new())
        };
        
        Ok((method, path, query_string))
    }
    
    /// 解析请求头
    fn parse_headers(&self, headers_data: &[u8]) -> Result<HashMap<String, String>, HttpError> {
        let headers_str = String::from_utf8_lossy(headers_data);
        let mut headers = HashMap::new();
        
        for line in headers_str.lines().skip(1) { // 跳过请求行
            if line.is_empty() {
                break;
            }
            
            if let Some(pos) = line.find(':') {
                let key = line[..pos].trim().to_lowercase();
                let value = line[pos + 1..].trim().to_string();
                headers.insert(key, value);
            }
        }
        
        Ok(headers)
    }
    
    /// 提取真实 IP 地址

    
    /// 重置缓冲区以供重用
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.read_pos = 0;
        self.write_pos = 0;
    }
}

/// HTTP 错误类型
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Request too large")]
    RequestTooLarge,
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Timeout")]
    Timeout,
}

/// 获取 HTTP 状态码对应的文本
fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_request_line() {
        let buffer = ZeroCopyBuffer::new(1024);
        let headers_data = b"GET /api/test?param=value HTTP/1.1\r\n";
        
        let (method, path, query) = buffer.parse_request_line(headers_data).unwrap();
        
        assert_eq!(method, "GET");
        assert_eq!(path, "/api/test");
        assert_eq!(query, "param=value");
    }
    
    #[test]
    fn test_parse_headers() {
        let buffer = ZeroCopyBuffer::new(1024);
        let headers_data = b"GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test\r\n";
        
        let headers = buffer.parse_headers(headers_data).unwrap();
        
        assert_eq!(headers.get("host"), Some(&"example.com".to_string()));
        assert_eq!(headers.get("user-agent"), Some(&"test".to_string()));
    }
    
    #[test]
    fn test_ip_extractor_integration() {
        let mut headers = HashMap::new();
        let remote_addr = "203.0.113.100:8080"; // 使用公网IP作为remote_addr
        
        // 测试 X-Real-IP 头部 (使用公网IP)
        headers.insert("x-real-ip".to_string(), "203.0.113.1".to_string());
        
        let real_ip = IpExtractor::extract_real_ip(&headers, remote_addr);
        assert_eq!(real_ip, "203.0.113.1");
        
        // 测试 X-Forwarded-For 多个 IP
        headers.clear();
        headers.insert("x-forwarded-for".to_string(), "203.0.113.2, 192.168.1.100".to_string());
        
        let real_ip = IpExtractor::extract_real_ip(&headers, remote_addr);
        assert_eq!(real_ip, "203.0.113.2"); // 应该取第一个公网IP
        
        // 测试无代理头的情况 (fallback到remote_addr)
        headers.clear();
        let real_ip = IpExtractor::extract_real_ip(&headers, remote_addr);
        assert_eq!(real_ip, "203.0.113.100"); // 从remote_addr提取的IP
    }
    
    #[test]
    fn test_ip_extractor_integration_allow_private() {
        let mut headers = HashMap::new();
        let remote_addr = "127.0.0.1:8080"; // 使用私有IP作为remote_addr
        
        // 测试 X-Real-IP 头部 (使用私有IP)
        headers.insert("x-real-ip".to_string(), "192.168.1.100".to_string());
        
        let real_ip = IpExtractor::extract_real_ip_allow_private(&headers, remote_addr);
        assert_eq!(real_ip, "192.168.1.100");
        
        // 测试 X-Forwarded-For 多个私有IP
        headers.clear();
        headers.insert("x-forwarded-for".to_string(), "192.168.1.1, 10.0.0.1".to_string());
        
        let real_ip = IpExtractor::extract_real_ip_allow_private(&headers, remote_addr);
        assert_eq!(real_ip, "192.168.1.1"); // 应该取第一个IP
        
        // 测试无代理头的情况 (fallback到remote_addr)
        headers.clear();
        let real_ip = IpExtractor::extract_real_ip_allow_private(&headers, remote_addr);
        assert_eq!(real_ip, "127.0.0.1"); // 从remote_addr提取的IP
    }
}