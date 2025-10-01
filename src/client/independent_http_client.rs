//! RAT Engine 独立HTTP客户端实现
//!
//! 基于reqwest的高性能HTTP客户端，专注于：
//! - 标准HTTP协议验证
//! - 压缩协议协商测试
//! - SSE功能验证
//! - 外部服务测试
//!
//! 这是独立的客户端实现，与现有的gRPC客户端完全解耦

#![cfg(feature = "reqwest")]

use std::time::Duration;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use reqwest::{StatusCode, Response, RequestBuilder, Method};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT, CONTENT_TYPE, ACCEPT_ENCODING, CONTENT_ENCODING};
use crate::error::{RatError, RatResult};
use crate::utils::logger::{debug, info, warn};

/// RAT Engine 独立HTTP客户端
///
/// 基于reqwest的实现，专注于测试和验证功能
#[derive(Debug, Clone)]
pub struct RatIndependentHttpClient {
    /// reqwest客户端实例
    client: reqwest::Client,
    /// 请求超时时间
    request_timeout: Duration,
    /// 用户代理字符串
    user_agent: String,
    /// 是否启用自动解压缩
    auto_decompress: bool,
    /// 支持的压缩算法（按优先级排序）
    supported_compressions: Vec<String>,
    /// 默认请求头
    default_headers: HeaderMap,
}

/// HTTP响应结构
#[derive(Debug)]
pub struct RatIndependentHttpResponse {
    /// HTTP状态码
    pub status: StatusCode,
    /// 响应头
    pub headers: HeaderMap,
    /// 响应体
    pub body: bytes::Bytes,
    /// 原始响应大小（压缩后的大小，字节）
    pub original_size: usize,
    /// 是否被压缩
    pub was_compressed: bool,
    /// 使用的压缩算法
    pub compression_algorithm: Option<String>,
    /// 请求耗时（毫秒）
    pub request_time_ms: u64,
}

/// SSE事件结构
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// 事件ID
    pub id: Option<String>,
    /// 事件类型
    pub event_type: Option<String>,
    /// 事件数据
    pub data: String,
    /// 重试时间（毫秒）
    pub retry: Option<u64>,
}

impl RatIndependentHttpClient {
    /// 创建新的独立HTTP客户端
    pub fn new() -> RatResult<Self> {
        Self::builder().build()
    }

    /// 创建构建器
    pub fn builder() -> RatIndependentHttpClientBuilder {
        RatIndependentHttpClientBuilder::new()
    }

    /// 发送GET请求
    pub async fn get<U>(&self, url: U) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
    {
        self.request(self.client.get(url)).await
    }

    /// 发送POST请求
    pub async fn post<U>(&self, url: U, body: impl Into<reqwest::Body>) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
    {
        self.request(self.client.post(url).body(body)).await
    }

    /// 发送POST JSON请求
    pub async fn post_json<U, T>(&self, url: U, json: &T) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
        T: Serialize,
    {
        self.request(self.client.post(url).json(json)).await
    }

    /// 发送PUT请求
    pub async fn put<U>(&self, url: U, body: impl Into<reqwest::Body>) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
    {
        self.request(self.client.put(url).body(body)).await
    }

    /// 发送DELETE请求
    pub async fn delete<U>(&self, url: U) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
    {
        self.request(self.client.delete(url)).await
    }

    /// 发送HEAD请求
    pub async fn head<U>(&self, url: U) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
    {
        self.request(self.client.head(url)).await
    }

    /// 发送OPTIONS请求
    pub async fn options<U>(&self, url: U) -> RatResult<RatIndependentHttpResponse>
    where
        U: reqwest::IntoUrl,
    {
        self.request(self.client.request(Method::OPTIONS, url)).await
    }

    /// 内部请求处理方法
    async fn request(&self, request: RequestBuilder) -> RatResult<RatIndependentHttpResponse> {
        let start_time = std::time::Instant::now();

        // 构建最终请求
        let mut request_builder = request;

        // 添加默认请求头
        for (name, value) in &self.default_headers {
            request_builder = request_builder.header(name, value);
        }

        // 设置用户代理
        request_builder = request_builder.header(USER_AGENT, &self.user_agent);

        // 设置Accept-Encoding头
        if !self.supported_compressions.is_empty() {
            let accept_encoding = self.supported_compressions.join(", ");
            request_builder = request_builder.header(ACCEPT_ENCODING, accept_encoding);
        }

        debug!("🔍 [独立HTTP客户端] 发送请求: {:?}", request_builder);

        // 发送请求
        let response = request_builder
            .send()
            .await
            .map_err(|e| RatError::NetworkError(format!("请求失败: {}", e)))?;

        let elapsed = start_time.elapsed();
        let status = response.status();
        let headers = response.headers().clone();

        // 获取压缩信息
        let content_encoding = headers.get(CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let was_compressed = content_encoding.is_some();
        let compression_algorithm = content_encoding;

        // 获取响应体
        let body_bytes = if self.auto_decompress {
            // reqwest自动解压缩，获取解压后的数据
            response.bytes().await
                .map_err(|e| RatError::NetworkError(format!("读取响应体失败: {}", e)))?
        } else {
            // 获取原始压缩数据
            response.bytes().await
                .map_err(|e| RatError::NetworkError(format!("读取响应体失败: {}", e)))?
        };

        let original_size = body_bytes.len();

        debug!("📥 [独立HTTP客户端] 收到响应: {} - 大小: {}字节, 压缩: {:?}, 耗时: {:?}",
               status, original_size, compression_algorithm, elapsed);

        Ok(RatIndependentHttpResponse {
            status,
            headers,
            body: body_bytes,
            original_size,
            was_compressed,
            compression_algorithm,
            request_time_ms: elapsed.as_millis() as u64,
        })
    }

    /// 连接SSE流
    pub async fn connect_sse<U>(&self, url: U) -> RatResult<SseStream>
    where
        U: reqwest::IntoUrl,
    {
        let mut request_builder = self.client.get(url);

        // 添加SSE相关请求头
        request_builder = request_builder.header("Accept", "text/event-stream");
        request_builder = request_builder.header("Cache-Control", "no-cache");

        // 添加默认请求头
        for (name, value) in &self.default_headers {
            request_builder = request_builder.header(name, value);
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| RatError::NetworkError(format!("SSE连接失败: {}", e)))?;

        if !response.status().is_success() {
            return Err(RatError::NetworkError(format!("SSE连接失败: {}", response.status())));
        }

        // 检查Content-Type
        let content_type = response.headers().get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !content_type.contains("text/event-stream") {
            warn!("⚠️ 服务器返回的Content-Type不是text/event-stream: {}", content_type);
        }

        Ok(SseStream::new(response))
    }

    /// 测试压缩支持
    pub async fn test_compression<U>(&self, url: U) -> RatResult<CompressionTestResult>
    where
        U: reqwest::IntoUrl + Clone,
    {
        let mut results = HashMap::new();

        // 测试无压缩
        let test_response = self.client
            .get(url.clone())
            .header(ACCEPT_ENCODING, "identity")
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await;

        match test_response {
            Ok(response) => {
                let content_length = response.content_length().unwrap_or(0) as usize;
                let start = std::time::Instant::now();
                let bytes = response.bytes().await;
                let elapsed = start.elapsed();

                match bytes {
                    Ok(body) => {
                        results.insert("identity".to_string(), CompressionTestItem {
                            supported: true,
                            original_size: content_length,
                            compressed_size: body.len(),
                            response_time_ms: elapsed.as_millis() as u64,
                        });
                    }
                    Err(_) => {
                        results.insert("identity".to_string(), CompressionTestItem {
                            supported: false,
                            original_size: 0,
                            compressed_size: 0,
                            response_time_ms: 0,
                        });
                    }
                }
            }
            Err(_) => {
                results.insert("identity".to_string(), CompressionTestItem {
                    supported: false,
                    original_size: 0,
                    compressed_size: 0,
                    response_time_ms: 0,
                });
            }
        }

        // 测试各种压缩算法
        for compression in &["gzip", "deflate", "br"] {
            let test_response = self.client
                .get(url.clone())
                .header(ACCEPT_ENCODING, *compression)
                .header(USER_AGENT, &self.user_agent)
                .send()
                .await;

            match test_response {
                Ok(response) => {
                    // 提取所需信息，避免借用检查问题
                    let content_encoding = response.headers().get(CONTENT_ENCODING)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    let content_length = response.content_length().unwrap_or(0) as usize;

                    let start = std::time::Instant::now();
                    let bytes = response.bytes().await;
                    let elapsed = start.elapsed();

                    match bytes {
                        Ok(body) => {
                            results.insert(compression.to_string(), CompressionTestItem {
                                supported: content_encoding == *compression,
                                original_size: content_length,
                                compressed_size: body.len(),
                                response_time_ms: elapsed.as_millis() as u64,
                            });
                        }
                        Err(_) => {
                            results.insert(compression.to_string(), CompressionTestItem {
                                supported: false,
                                original_size: 0,
                                compressed_size: 0,
                                response_time_ms: 0,
                            });
                        }
                    }
                }
                Err(_) => {
                    results.insert(compression.to_string(), CompressionTestItem {
                        supported: false,
                        original_size: 0,
                        compressed_size: 0,
                        response_time_ms: 0,
                    });
                }
            }
        }

        Ok(CompressionTestResult { results })
    }

    /// 获取客户端统计信息
    pub fn stats(&self) -> ClientStats {
        // reqwest客户端的统计信息有限，这里返回基本信息
        ClientStats {
            request_timeout: self.request_timeout,
            user_agent: self.user_agent.clone(),
            supported_compressions: self.supported_compressions.clone(),
            auto_decompress: self.auto_decompress,
        }
    }
}

/// SSE流
pub struct SseStream {
    /// 字节流
    byte_stream: futures_util::stream::BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    /// 缓冲区
    buffer: String,
}

impl SseStream {
    fn new(response: Response) -> Self {
        let byte_stream = response.bytes_stream();
        Self {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
        }
    }

    /// 接收下一个SSE事件
    pub async fn next_event(&mut self) -> RatResult<Option<SseEvent>> {
        use futures_util::StreamExt;

        while let Some(chunk_result) = self.byte_stream.next().await {
            let chunk = chunk_result
                .map_err(|e| RatError::NetworkError(format!("读取SSE流失败: {}", e)))?;

            self.buffer.push_str(&String::from_utf8_lossy(&chunk));

            // 尝试解析完整的事件
            if let Some(event) = self.try_parse_event() {
                return Ok(Some(event));
            }
        }

        // 流结束，尝试解析缓冲区中剩余的事件
        if let Some(event) = self.try_parse_event() {
            return Ok(Some(event));
        }

        Ok(None)
    }

    /// 尝试从缓冲区解析事件
    fn try_parse_event(&mut self) -> Option<SseEvent> {
        let mut event = SseEvent {
            id: None,
            event_type: None,
            data: String::new(),
            retry: None,
        };

        let mut found_event = false;
        let mut lines: Vec<&str> = self.buffer.split('\n').collect();
        let mut consumed_lines = 0;

        for (i, line) in lines.iter().enumerate() {
            let line = line.trim();

            if line.is_empty() {
                // 空行表示事件结束
                if found_event {
                    consumed_lines = i + 1;
                    break;
                }
                continue;
            }

            found_event = true;

            if let Some(rest) = line.strip_prefix("data: ") {
                if !event.data.is_empty() {
                    event.data.push('\n');
                }
                event.data.push_str(rest);
            } else if let Some(rest) = line.strip_prefix("event: ") {
                event.event_type = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("id: ") {
                event.id = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("retry: ") {
                if let Ok(retry_ms) = rest.parse::<u64>() {
                    event.retry = Some(retry_ms);
                }
            }
            // 忽略注释行（以:开头的行）
        }

        if found_event && consumed_lines > 0 {
            // 从缓冲区中移除已处理的行
            self.buffer = lines[consumed_lines..].join("\n");
            return Some(event);
        }

        None
    }
}

/// 压缩测试结果
#[derive(Debug)]
pub struct CompressionTestResult {
    pub results: HashMap<String, CompressionTestItem>,
}

/// 压缩测试项
#[derive(Debug)]
pub struct CompressionTestItem {
    pub supported: bool,
    pub original_size: usize,
    pub compressed_size: usize,
    pub response_time_ms: u64,
}

/// 客户端统计信息
#[derive(Debug)]
pub struct ClientStats {
    pub request_timeout: Duration,
    pub user_agent: String,
    pub supported_compressions: Vec<String>,
    pub auto_decompress: bool,
}

/// HTTP客户端构建器
#[derive(Debug)]
pub struct RatIndependentHttpClientBuilder {
    timeout: Duration,
    user_agent: Option<String>,
    auto_decompress: bool,
    supported_compressions: Vec<String>,
    default_headers: HeaderMap,
    pool_max_idle_per_host: usize,
    pool_idle_timeout: Duration,
}

impl RatIndependentHttpClientBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            user_agent: None,
            auto_decompress: true,
            supported_compressions: vec!["gzip".to_string(), "deflate".to_string(), "br".to_string()],
            default_headers: HeaderMap::new(),
            pool_max_idle_per_host: 10,
            pool_idle_timeout: Duration::from_secs(90),
        }
    }

    /// 设置请求超时
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 设置用户代理
    pub fn user_agent<S: Into<String>>(mut self, user_agent: S) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// 启用/禁用自动解压缩
    pub fn auto_decompress(mut self, enabled: bool) -> Self {
        self.auto_decompress = enabled;
        self
    }

    /// 设置支持的压缩算法
    pub fn supported_compressions<I, S>(mut self, compressions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.supported_compressions = compressions.into_iter().map(Into::into).collect();
        self
    }

    /// 添加默认请求头
    pub fn default_header<K, V>(mut self, key: K, value: V) -> RatResult<Self>
    where
        K: TryInto<HeaderName>,
        K::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        V: TryInto<HeaderValue>,
        V::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let header_name = key.try_into().map_err(|e| RatError::RequestError(format!("无效的请求头名: {}", e.into())))?;
        let header_value = value.try_into().map_err(|e| RatError::RequestError(format!("无效的请求头值: {}", e.into())))?;
        self.default_headers.insert(header_name, header_value);
        Ok(self)
    }

    /// 设置连接池配置
    pub fn pool_config(mut self, max_idle_per_host: usize, idle_timeout: Duration) -> Self {
        self.pool_max_idle_per_host = max_idle_per_host;
        self.pool_idle_timeout = idle_timeout;
        self
    }

    /// 构建客户端
    pub fn build(self) -> RatResult<RatIndependentHttpClient> {
        let user_agent = self.user_agent.unwrap_or_else(|| "rat-engine-independent-client/1.0".to_string());

        // 构建reqwest客户端
        let mut client_builder = reqwest::Client::builder()
            .timeout(self.timeout)
            .pool_max_idle_per_host(self.pool_max_idle_per_host)
            .pool_idle_timeout(self.pool_idle_timeout);

        // 配置压缩
        if self.auto_decompress {
            // 启用自动解压缩
            client_builder = client_builder.gzip(true).brotli(true).deflate(true);
        }

        let client = client_builder
            .build()
            .map_err(|e| RatError::RequestError(format!("构建HTTP客户端失败: {}", e)))?;

        Ok(RatIndependentHttpClient {
            client,
            request_timeout: self.timeout,
            user_agent,
            auto_decompress: self.auto_decompress,
            supported_compressions: self.supported_compressions,
            default_headers: self.default_headers,
        })
    }
}

impl Default for RatIndependentHttpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RatIndependentHttpResponse {
    /// 检查响应是否成功
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// 获取响应文本
    pub fn text(&self) -> RatResult<String> {
        String::from_utf8(self.body.to_vec())
            .map_err(|e| RatError::DecodingError(format!("响应体不是有效的UTF-8: {}", e)))
    }

    /// 解析JSON响应
    pub fn json<T>(&self) -> RatResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(&self.body)
            .map_err(|e| RatError::DeserializationError(format!("JSON解析失败: {}", e)))
    }

    /// 获取指定请求头的值
    pub fn header(&self, name: &HeaderName) -> Option<&HeaderValue> {
        self.headers.get(name)
    }

    /// 获取Content-Type
    pub fn content_type(&self) -> Option<&HeaderValue> {
        self.header(&reqwest::header::CONTENT_TYPE)
    }

    /// 打印响应调试信息
    pub fn debug_print(&self) {
        info!("📊 独立HTTP客户端响应调试:");
        info!("   状态码: {}", self.status);
        info!("   响应时间: {}ms", self.request_time_ms);
        info!("   原始大小: {}字节", self.original_size);
        info!("   压缩: {:?}", self.compression_algorithm);

        if let Some(content_type) = self.content_type() {
            info!("   Content-Type: {:?}", content_type);
        }

        // 尝试显示响应体
        if self.body.len() <= 500 {
            if let Ok(text) = self.text() {
                info!("   响应体: {}", text);
            } else {
                info!("   响应体: {} bytes (二进制数据)", self.body.len());
            }
        } else {
            info!("   响应体: {} bytes (数据过大，省略显示)", self.body.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = RatIndependentHttpClient::new().unwrap();
        assert!(!client.user_agent.is_empty());
    }

    #[tokio::test]
    async fn test_builder() {
        let client = RatIndependentHttpClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent("test-agent")
            .build()
            .unwrap();

        assert_eq!(client.user_agent, "test-agent");
        assert_eq!(client.request_timeout, Duration::from_secs(10));
    }
}