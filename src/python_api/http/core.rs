//! 简化的 PyO3 核心接口
//! 
//! 专注于高性能通信，提供最小化但完整的 HTTP 接口：
//! - 零拷贝数据传递
//! - 真实 IP 获取
//! - 客户端信息提取
//! - 响应头修改

use pyo3::prelude::*;
use std::collections::HashMap;
use std::fmt;
use serde::{Serialize, Deserialize};
use serde_json;

/// 响应类型枚举
#[pyclass(name = "ResponseType", module = "rat_engine")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseType {
    HTML,
    JSON,
    TEXT,
    SSE,
    FILE,
    CUSTOM,
    CUSTOM_BYTES,
    SSE_JSON,
    SSE_TEXT,
    CHUNK,
    REDIRECT,
}

#[pymethods]
impl ResponseType {
    fn __str__(&self) -> &'static str {
        match self {
            ResponseType::HTML => "HTML",
            ResponseType::JSON => "JSON",
            ResponseType::TEXT => "TEXT",
            ResponseType::SSE => "SSE",
            ResponseType::FILE => "FILE",
            ResponseType::CUSTOM => "CUSTOM",
            ResponseType::CUSTOM_BYTES => "CUSTOM_BYTES",
            ResponseType::SSE_JSON => "SSE_JSON",
            ResponseType::SSE_TEXT => "SSE_TEXT",
            ResponseType::CHUNK => "CHUNK",
            ResponseType::REDIRECT => "REDIRECT",
        }
    }
}

/// 带类型标识的响应对象
#[pyclass(name = "TypedResponse", module = "rat_engine")]
#[derive(Debug, Clone)]
pub struct TypedResponse {
    #[pyo3(get, set)]
    pub content: PyObject,
    #[pyo3(get, set)]
    pub response_type: ResponseType,
    #[pyo3(get, set)]
    pub kwargs: PyObject,
}

#[pymethods]
impl TypedResponse {
    #[new]
    pub fn new(content: PyObject, response_type: ResponseType, kwargs: PyObject) -> Self {
        Self {
            content,
            response_type,
            kwargs,
        }
    }
}

/// HTTP 方法枚举
#[pyclass(name = "HttpMethod", module = "rat_engine")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
    Trace,
    Connect,
}

#[pymethods]
impl HttpMethod {
    #[new]
    pub fn new(method: &str) -> Self {
        match method.to_uppercase().as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            "PATCH" => HttpMethod::Patch,
            "TRACE" => HttpMethod::Trace,
            "CONNECT" => HttpMethod::Connect,
            _ => HttpMethod::Get, // 默认为 GET
        }
    }
    
    fn __str__(&self) -> &'static str {
        self.as_str()
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Trace => "TRACE",
            HttpMethod::Connect => "CONNECT",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.__str__())
    }
}

/// 简化的 HTTP 请求对象
/// 
/// 专注于高性能通信，提供最小化但完整的 HTTP 接口
#[pyclass(name = "HttpRequest", module = "rat_engine")]
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct HttpRequest {
    /// HTTP 方法 (GET, POST, PUT, DELETE, etc.)
    #[pyo3(get)]
    pub method: String,
    
    /// 请求路径 (不包含查询字符串)
    #[pyo3(get)]
    pub path: String,
    
    /// 查询字符串 (原始格式)
    #[pyo3(get)]
    pub query_string: String,
    
    /// 请求头 (键值对，键已转为小写)
    #[pyo3(get)]
    pub headers: HashMap<String, String>,
    
    /// 原始请求体 (字节数组)
    #[pyo3(get)]
    pub body: Vec<u8>,
    
    /// 远程地址 (直连 IP:端口)
    #[pyo3(get)]
    pub remote_addr: String,
    
    /// 真实 IP 地址 (考虑代理头)
    #[pyo3(get)]
    pub real_ip: String,
    
    /// 路径参数 (从动态路由中提取的参数)
    #[pyo3(get)]
    pub path_params: HashMap<String, String>,

    /// Python处理器名字 (仅用于Python集成，避免Python层二次路由匹配)
    #[pyo3(get)]
    pub python_handler_name: Option<String>,
}

#[pymethods]
impl HttpRequest {
    /// 创建新的请求对象
    #[new]
    pub fn new(
        method: Option<String>,
        path: Option<String>,
        query_string: Option<String>,
        headers: Option<HashMap<String, String>>,
        body: Option<Vec<u8>>,
        remote_addr: Option<String>,
        real_ip: Option<String>,
        path_params: Option<HashMap<String, String>>,
        python_handler_name: Option<String>
    ) -> Self {
        Self {
            method: method.unwrap_or_else(|| "GET".to_string()),
            path: path.unwrap_or_else(|| "/".to_string()),
            query_string: query_string.unwrap_or_default(),
            headers: headers.unwrap_or_default(),
            body: body.unwrap_or_default(),
            remote_addr: remote_addr.unwrap_or_else(|| "127.0.0.1:8080".to_string()),
            real_ip: real_ip.unwrap_or_else(|| "127.0.0.1".to_string()),
            path_params: path_params.unwrap_or_default(),
            python_handler_name,
        }
    }
    
    /// 获取查询参数字典
    /// 
    /// 返回解析后的查询参数键值对
    pub fn get_query_params(&self) -> HashMap<String, String> {
        if self.query_string.is_empty() {
            return HashMap::new();
        }
        
        url::form_urlencoded::parse(self.query_string.as_bytes())
            .into_owned()
            .collect()
    }
    
    /// 获取单个查询参数
    /// 
    /// # 参数
    /// * `key` - 参数名
    /// * `default` - 默认值
    pub fn get_query_param(&self, key: &str, default: Option<&str>) -> Option<String> {
        let params = self.get_query_params();
        params.get(key).cloned().or_else(|| default.map(|s| s.to_string()))
    }
    
    /// 获取单个路径参数
    /// 
    /// # 参数
    /// * `key` - 参数名
    /// * `default` - 默认值
    pub fn get_path_param(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.path_params.get(key).cloned().or_else(|| default.map(|s| s.to_string()))
    }
    
    /// 获取 JSON 数据
    /// 
    /// 解析请求体为 JSON 对象
    pub fn get_json(&self) -> PyResult<PyObject> {
        if self.body.is_empty() {
            return Python::with_gil(|py| Ok(py.None()));
        }
        
        Python::with_gil(|py| {
            let json_module = py.import("json")?;
            let json_str = String::from_utf8_lossy(&self.body);
            Ok(json_module.call_method1("loads", (json_str.as_ref(),))?.to_object(py))
        })
    }
    
    /// 获取表单数据
    /// 
    /// 解析 application/x-www-form-urlencoded 数据
    pub fn get_form_data(&self) -> HashMap<String, String> {
        if self.headers.get("content-type")
            .map(|ct| ct.starts_with("application/x-www-form-urlencoded"))
            .unwrap_or(false) {
            url::form_urlencoded::parse(&self.body)
                .into_owned()
                .collect()
        } else {
            HashMap::new()
        }
    }
    
    /// 获取请求体文本
    /// 
    /// 将请求体解码为 UTF-8 字符串
    pub fn get_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    
    /// 获取请求头
    /// 
    /// # 参数
    /// * `key` - 头部名称 (不区分大小写)
    /// * `default` - 默认值
    pub fn get_header(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.headers.get(&key.to_lowercase())
            .cloned()
            .or_else(|| default.map(|s| s.to_string()))
    }
    
    /// 检查是否为 AJAX 请求
    pub fn is_ajax(&self) -> bool {
        self.headers.get("x-requested-with")
            .map(|v| v.to_lowercase() == "xmlhttprequest")
            .unwrap_or(false)
    }
    
    /// 检查是否为 JSON 请求
    pub fn is_json(&self) -> bool {
        self.headers.get("content-type")
            .map(|ct| ct.contains("application/json"))
            .unwrap_or(false)
    }
    
    /// 获取用户代理
    pub fn get_user_agent(&self) -> Option<String> {
        self.headers.get("user-agent").cloned()
    }
    
    /// 获取客户端信息摘要
    pub fn get_client_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        
        info.insert("real_ip".to_string(), self.real_ip.clone());
        info.insert("remote_addr".to_string(), self.remote_addr.clone());
        
        if let Some(ua) = self.get_user_agent() {
            info.insert("user_agent".to_string(), ua);
        }
        
        if let Some(referer) = self.headers.get("referer") {
            info.insert("referer".to_string(), referer.clone());
        }
        
        if let Some(accept_lang) = self.headers.get("accept-language") {
            info.insert("accept_language".to_string(), accept_lang.clone());
        }
        
        info
    }
    
    /// 获取内容长度
    pub fn get_content_length(&self) -> usize {
        self.body.len()
    }
    
    /// 检查是否为安全连接 (HTTPS)
    pub fn is_secure(&self) -> bool {
        // 检查各种 HTTPS 指示头
        self.headers.get("x-forwarded-proto")
            .map(|v| v.to_lowercase() == "https")
            .or_else(|| self.headers.get("x-forwarded-ssl")
                .map(|v| v.to_lowercase() == "on"))
            .or_else(|| self.headers.get("x-url-scheme")
                .map(|v| v.to_lowercase() == "https"))
            .unwrap_or(false)
    }
}

/// 简化的 HTTP 响应对象
/// 
/// 提供灵活的响应构建和头部管理
#[pyclass(name = "HttpResponse", module = "rat_engine")]
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct HttpResponse {
    /// HTTP 状态码
    #[pyo3(get, set)]
    pub status: u16,
    
    /// 响应头
    #[pyo3(get)]
    pub headers: HashMap<String, String>,
    
    /// 响应体
    #[pyo3(get, set)]
    pub body: Vec<u8>,
}

#[pymethods]
impl HttpResponse {
    /// 创建新的响应对象
    #[new]
    pub fn new(
        status: Option<u16>,
        headers: Option<HashMap<String, String>>,
        body: Option<Vec<u8>>
    ) -> Self {
        Self {
            status: status.unwrap_or(200),
            headers: headers.unwrap_or_default(),
            body: body.unwrap_or_default(),
        }
    }
    
    /// 创建 JSON 响应
    #[staticmethod]
    pub fn json(data: PyObject, status: Option<u16>) -> PyResult<Self> {
        Python::with_gil(|py| {
            let json_module = py.import("json")?;
            let json_str: String = json_module
                .call_method1("dumps", (data,))?
                .extract()?;
            
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json; charset=utf-8".to_string());
            
            Ok(Self {
                status: status.unwrap_or(200),
                headers,
                body: json_str.into_bytes(),
            })
        })
    }
    
    /// 创建文本响应
    #[staticmethod]
    pub fn text(content: String, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        
        Self {
            status: status.unwrap_or(200),
            headers,
            body: content.into_bytes(),
        }
    }
    
    /// 创建 HTML 响应
    #[staticmethod]
    pub fn html(content: String, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        
        Self {
            status: status.unwrap_or(200),
            headers,
            body: content.into_bytes(),
        }
    }
    
    /// 创建重定向响应
    #[staticmethod]
    pub fn redirect(url: String, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Location".to_string(), url);
        
        Self {
            status: status.unwrap_or(302),
            headers,
            body: Vec::new(),
        }
    }
    
    /// 创建错误响应
    #[staticmethod]
    pub fn error(message: String, status: Option<u16>) -> Self {
        let status_code = status.unwrap_or(500);
        let error_json = format!(r#"{{"error": "{}", "status": {}}}"#, message, status_code);
        
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json; charset=utf-8".to_string());
        
        Self {
            status: status_code,
            headers,
            body: error_json.into_bytes(),
        }
    }
    
    /// 创建分块传输响应
    #[staticmethod]
    pub fn chunk(content: String, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Transfer-Encoding".to_string(), "chunked".to_string());
        headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        
        Self {
            status: status.unwrap_or(200),
            headers,
            body: content.into_bytes(),
        }
    }
    
    /// 创建 SSE 响应
    #[staticmethod]
    pub fn sse(content: String, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/event-stream".to_string());
        headers.insert("Cache-Control".to_string(), "no-cache".to_string());
        headers.insert("Connection".to_string(), "keep-alive".to_string());
        
        Self {
            status: status.unwrap_or(200),
            headers,
            body: content.into_bytes(),
        }
    }
    
    /// 创建 SSE 文本响应
    #[staticmethod]
    pub fn sse_text(content: String, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        headers.insert("Cache-Control".to_string(), "no-cache".to_string());
        
        Self {
            status: status.unwrap_or(200),
            headers,
            body: content.into_bytes(),
        }
    }
    
    /// 创建文件响应
    #[staticmethod]
    pub fn file(file_path: String, content_type: Option<String>, filename: Option<String>) -> PyResult<Self> {
        use std::fs;
        use std::path::Path;
        
        // 读取文件内容
        let content = fs::read(&file_path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(
                format!("Failed to read file '{}': {}", file_path, e)
            ))?;
        
        let mut headers = HashMap::new();
        
        // 如果没有提供 content_type，尝试根据文件扩展名推断
        let content_type = if let Some(ct) = content_type {
            ct
        } else {
            let path = Path::new(&file_path);
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("html") => "text/html; charset=utf-8".to_string(),
                Some("css") => "text/css; charset=utf-8".to_string(),
                Some("js") => "application/javascript; charset=utf-8".to_string(),
                Some("json") => "application/json; charset=utf-8".to_string(),
                Some("png") => "image/png".to_string(),
                Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
                Some("gif") => "image/gif".to_string(),
                Some("svg") => "image/svg+xml".to_string(),
                Some("pdf") => "application/pdf".to_string(),
                Some("txt") => "text/plain; charset=utf-8".to_string(),
                _ => "application/octet-stream".to_string(),
            }
        };
        
        headers.insert("Content-Type".to_string(), content_type);
        
        // 设置文件名，如果没有提供则从路径中提取
        let filename = if let Some(name) = filename {
            name
        } else {
            Path::new(&file_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download")
                .to_string()
        };
        
        headers.insert("Content-Disposition".to_string(), format!("attachment; filename=\"{}\"", filename));
        
        Ok(Self {
            status: 200,
            headers,
            body: content,
        })
    }
    
    /// 设置响应头
    pub fn set_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }
    
    /// 获取响应头
    pub fn get_header(&self, key: &str) -> Option<String> {
        self.headers.get(key).cloned()
    }
    
    /// 删除响应头
    pub fn remove_header(&mut self, key: &str) -> Option<String> {
        self.headers.remove(key)
    }
    
    /// 设置多个响应头
    pub fn set_headers(&mut self, headers: HashMap<String, String>) {
        for (key, value) in headers {
            self.headers.insert(key, value);
        }
    }
    
    /// 设置 Cookie
    pub fn set_cookie(
        &mut self,
        name: String,
        value: String,
        max_age: Option<i64>,
        path: Option<String>,
        domain: Option<String>,
        secure: Option<bool>,
        http_only: Option<bool>
    ) {
        let mut cookie = format!("{}={}", name, value);
        
        if let Some(age) = max_age {
            cookie.push_str(&format!("; Max-Age={}", age));
        }
        
        if let Some(p) = path {
            cookie.push_str(&format!("; Path={}", p));
        }
        
        if let Some(d) = domain {
            cookie.push_str(&format!("; Domain={}", d));
        }
        
        if secure.unwrap_or(false) {
            cookie.push_str("; Secure");
        }
        
        if http_only.unwrap_or(false) {
            cookie.push_str("; HttpOnly");
        }
        
        self.headers.insert("Set-Cookie".to_string(), cookie);
    }
    
    /// 设置 CORS 头
    pub fn set_cors(
        &mut self,
        origin: Option<String>,
        methods: Option<Vec<String>>,
        headers: Option<Vec<String>>,
        credentials: Option<bool>
    ) {
        if let Some(o) = origin {
            self.headers.insert("Access-Control-Allow-Origin".to_string(), o);
        }
        
        if let Some(m) = methods {
            self.headers.insert("Access-Control-Allow-Methods".to_string(), m.join(", "));
        }
        
        if let Some(h) = headers {
            self.headers.insert("Access-Control-Allow-Headers".to_string(), h.join(", "));
        }
        
        if credentials.unwrap_or(false) {
            self.headers.insert("Access-Control-Allow-Credentials".to_string(), "true".to_string());
        }
    }
    
    /// 设置缓存控制
    pub fn set_cache_control(&mut self, directive: String) {
        self.headers.insert("Cache-Control".to_string(), directive);
    }
    
    /// 禁用缓存
    pub fn no_cache(&mut self) {
        self.headers.insert("Cache-Control".to_string(), "no-cache, no-store, must-revalidate".to_string());
        self.headers.insert("Pragma".to_string(), "no-cache".to_string());
        self.headers.insert("Expires".to_string(), "0".to_string());
    }
    
    /// 获取响应体文本
    pub fn get_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    
    /// 设置响应体文本
    pub fn set_text(&mut self, content: String) {
        self.body = content.into_bytes();
        if !self.headers.contains_key("Content-Type") {
            self.headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        }
    }
    
    /// 设置 JSON 响应体
    pub fn set_json(&mut self, data: PyObject) -> PyResult<()> {
        Python::with_gil(|py| {
            let json_module = py.import("json")?;
            let json_str: String = json_module
                .call_method1("dumps", (data,))?
                .extract()?;
            
            self.body = json_str.into_bytes();
            self.headers.insert("Content-Type".to_string(), "application/json; charset=utf-8".to_string());
            Ok(())
        })
    }
    
    /// 获取内容长度
    pub fn get_content_length(&self) -> usize {
        self.body.len()
    }
    
    /// 检查是否为成功响应
    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
    
    /// 检查是否为重定向响应
    pub fn is_redirect(&self) -> bool {
        self.status >= 300 && self.status < 400
    }
    
    /// 检查是否为错误响应
    pub fn is_error(&self) -> bool {
        self.status >= 400
    }
    
    /// 设置状态码
    pub fn set_status_code(&mut self, status: u16) {
        self.status = status;
    }
    
    /// 设置响应体内容
    pub fn set_body_bytes(&mut self, body: Vec<u8>) {
        self.body = body;
    }
    
    /// 设置响应体（字符串）
    pub fn set_body_text(&mut self, content: String) {
        self.body = content.into_bytes();
    }
}

// 流式响应构造方法（非 Python 绑定）
impl HttpResponse {
    /// 从 SSE 响应创建 HttpResponse
    pub fn from_sse(sse_response: crate::server::streaming::SseResponse) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/event-stream".to_string());
        headers.insert("Cache-Control".to_string(), "no-cache".to_string());
        headers.insert("Connection".to_string(), "keep-alive".to_string());
        
        Self {
            status: 200,
            headers,
            body: Vec::new(), // SSE 响应体由流处理
        }
    }
    
    /// 从分块响应创建 HttpResponse
    pub fn from_chunked(chunked_response: crate::server::streaming::ChunkedResponse) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Transfer-Encoding".to_string(), "chunked".to_string());
        
        Self {
            status: 200,
            headers,
            body: Vec::new(), // 分块响应体由流处理
        }
    }
    
    /// 从流式响应创建 HttpResponse
    pub fn from_streaming(streaming_response: crate::server::streaming::StreamingResponse) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/octet-stream".to_string());
        
        Self {
            status: 200,
            headers,
            body: Vec::new(), // 流式响应体由流处理
        }
    }
    
    // TODO: 从 SegQueue 统一响应创建 HttpResponse - 暂时注释掉，等待 segqueue_unified 模块实现
    /*
    pub fn from_segqueue_unified(segqueue_response: crate::python_api::streaming::segqueue_unified::SegQueueUnifiedResponse) -> Self {
        // 从 SegQueueUnifiedResponse 中提取状态码和头部信息
        let mut headers = HashMap::new();
        
        // 根据响应类型设置默认头部
        match segqueue_response.response_type() {
            crate::python_api::streaming::segqueue_unified::UnifiedResponseType::Sse => {
                headers.insert("Content-Type".to_string(), "text/event-stream".to_string());
                headers.insert("Cache-Control".to_string(), "no-cache".to_string());
                headers.insert("Connection".to_string(), "keep-alive".to_string());
                headers.insert("Access-Control-Allow-Origin".to_string(), "*".to_string());
            },
            crate::python_api::streaming::segqueue_unified::UnifiedResponseType::Chunked => {
                headers.insert("Transfer-Encoding".to_string(), "chunked".to_string());
            },
            crate::python_api::streaming::segqueue_unified::UnifiedResponseType::JsonStream => {
                headers.insert("Content-Type".to_string(), "application/json".to_string());
            },
            crate::python_api::streaming::segqueue_unified::UnifiedResponseType::TextStream => {
                headers.insert("Content-Type".to_string(), "text/plain".to_string());
            },
            crate::python_api::streaming::segqueue_unified::UnifiedResponseType::BinaryStream => {
                headers.insert("Content-Type".to_string(), "application/octet-stream".to_string());
            },
        }
        
        // 添加自定义头部 - SegQueueUnifiedResponse 不再提供 headers 访问
        // 头部信息已在响应类型匹配中设置
        
        Self {
            status: 200, // SegQueueUnifiedResponse 使用默认状态码
            headers,
            body: Vec::new(), // SegQueue 响应体由流处理
        }
    }
    */
}

/// 创建快速响应的便利函数
#[pyfunction]
pub fn ok(content: Option<String>) -> HttpResponse {
    HttpResponse::text(content.unwrap_or_else(|| "OK".to_string()), Some(200))
}

#[pyfunction]
pub fn not_found(message: Option<String>) -> HttpResponse {
    HttpResponse::error(message.unwrap_or_else(|| "Not Found".to_string()), Some(404))
}

#[pyfunction]
pub fn bad_request(message: Option<String>) -> HttpResponse {
    HttpResponse::error(message.unwrap_or_else(|| "Bad Request".to_string()), Some(400))
}

#[pyfunction]
pub fn internal_error(message: Option<String>) -> HttpResponse {
    HttpResponse::error(message.unwrap_or_else(|| "Internal Server Error".to_string()), Some(500))
}

#[pyfunction]
pub fn json_response(data: PyObject, status: Option<u16>) -> PyResult<HttpResponse> {
    HttpResponse::json(data, status)
}

/// 注册 Python 模块
pub fn register_module(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<ResponseType>()?;
    m.add_class::<TypedResponse>()?;
    m.add_class::<HttpRequest>()?;
    m.add_class::<HttpResponse>()?;
    m.add_function(wrap_pyfunction!(ok, m)?)?;
    m.add_function(wrap_pyfunction!(not_found, m)?)?;
    m.add_function(wrap_pyfunction!(bad_request, m)?)?;
    m.add_function(wrap_pyfunction!(internal_error, m)?)?;
    m.add_function(wrap_pyfunction!(json_response, m)?)?;
    Ok(())
}
