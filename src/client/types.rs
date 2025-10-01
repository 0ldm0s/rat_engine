//! RAT Engine HTTP 客户端类型定义
//! 
//! 提供透明化的 HTTP 类型，隐藏底层 hyper 实现细节

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// HTTP 方法枚举（透明化版本）
/// 
/// 重新定义 HTTP 方法，避免用户直接依赖 hyper
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpMethod {
    /// GET 方法
    Get,
    /// POST 方法
    Post,
    /// PUT 方法
    Put,
    /// DELETE 方法
    Delete,
    /// PATCH 方法
    Patch,
    /// HEAD 方法
    Head,
    /// OPTIONS 方法
    Options,
    /// TRACE 方法
    Trace,
    /// CONNECT 方法
    Connect,
    /// 自定义方法
    Custom(String),
}

impl From<HttpMethod> for hyper::Method {
    fn from(method: HttpMethod) -> Self {
        match method {
            HttpMethod::Get => hyper::Method::GET,
            HttpMethod::Post => hyper::Method::POST,
            HttpMethod::Put => hyper::Method::PUT,
            HttpMethod::Delete => hyper::Method::DELETE,
            HttpMethod::Patch => hyper::Method::PATCH,
            HttpMethod::Head => hyper::Method::HEAD,
            HttpMethod::Options => hyper::Method::OPTIONS,
            HttpMethod::Trace => hyper::Method::TRACE,
            HttpMethod::Connect => hyper::Method::CONNECT,
            HttpMethod::Custom(method) => {
                method.parse().unwrap_or(hyper::Method::GET)
            }
        }
    }
}

impl From<hyper::Method> for HttpMethod {
    fn from(method: hyper::Method) -> Self {
        match method {
            hyper::Method::GET => HttpMethod::Get,
            hyper::Method::POST => HttpMethod::Post,
            hyper::Method::PUT => HttpMethod::Put,
            hyper::Method::DELETE => HttpMethod::Delete,
            hyper::Method::PATCH => HttpMethod::Patch,
            hyper::Method::HEAD => HttpMethod::Head,
            hyper::Method::OPTIONS => HttpMethod::Options,
            hyper::Method::TRACE => HttpMethod::Trace,
            hyper::Method::CONNECT => HttpMethod::Connect,
            _ => HttpMethod::Custom(method.to_string()),
        }
    }
}

/// HTTP 状态码（透明化版本）
/// 
/// 重新定义常用的 HTTP 状态码，避免用户直接依赖 hyper
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HttpStatusCode(pub u16);

impl HttpStatusCode {
    /// 200 OK
    pub const OK: Self = Self(200);
    /// 201 Created
    pub const CREATED: Self = Self(201);
    /// 204 No Content
    pub const NO_CONTENT: Self = Self(204);
    /// 400 Bad Request
    pub const BAD_REQUEST: Self = Self(400);
    /// 401 Unauthorized
    pub const UNAUTHORIZED: Self = Self(401);
    /// 403 Forbidden
    pub const FORBIDDEN: Self = Self(403);
    /// 404 Not Found
    pub const NOT_FOUND: Self = Self(404);
    /// 500 Internal Server Error
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);
    /// 502 Bad Gateway
    pub const BAD_GATEWAY: Self = Self(502);
    /// 503 Service Unavailable
    pub const SERVICE_UNAVAILABLE: Self = Self(503);

    /// 检查是否为成功状态码（2xx）
    pub fn is_success(&self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    /// 检查是否为客户端错误（4xx）
    pub fn is_client_error(&self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    /// 检查是否为服务器错误（5xx）
    pub fn is_server_error(&self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    /// 获取状态码数值
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

impl From<hyper::StatusCode> for HttpStatusCode {
    fn from(status: hyper::StatusCode) -> Self {
        Self(status.as_u16())
    }
}

impl From<HttpStatusCode> for hyper::StatusCode {
    fn from(status: HttpStatusCode) -> Self {
        hyper::StatusCode::from_u16(status.0).unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl std::fmt::Display for HttpStatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// HTTP 请求头（透明化版本）
/// 
/// 简化的请求头类型，避免用户直接操作 hyper 的 HeaderMap
pub type HttpHeaders = HashMap<String, String>;

/// HTTP 请求构建器（透明化版本）
/// 
/// 提供简化的请求构建接口，隐藏 hyper 的复杂性
#[derive(Debug, Clone)]
pub struct HttpRequestBuilder {
    /// HTTP 方法
    pub method: HttpMethod,
    /// 请求 URL
    pub url: String,
    /// 请求头
    pub headers: HttpHeaders,
    /// 请求体
    pub body: Option<Vec<u8>>,
}

impl HttpRequestBuilder {
    /// 创建新的请求构建器
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            body: None,
        }
    }

    /// 添加请求头
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// 设置请求体
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// 设置 JSON 请求体
    pub fn json<T: Serialize>(mut self, data: &T) -> Result<Self, serde_json::Error> {
        let json_bytes = serde_json::to_vec(data)?;
        self.headers.insert("content-type".to_string(), "application/json".to_string());
        self.body = Some(json_bytes);
        Ok(self)
    }

    /// 设置表单请求体
    pub fn form(mut self, data: &HashMap<String, String>) -> Self {
        let form_data = data
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        
        self.headers.insert("content-type".to_string(), "application/x-www-form-urlencoded".to_string());
        self.body = Some(form_data.into_bytes());
        self
    }
}