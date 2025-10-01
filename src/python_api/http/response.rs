//! Python响应对象封装

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use std::collections::HashMap;
use serde_json::Value;

/// HTTP响应对象
#[pyclass]
#[derive(Debug, Clone)]
pub struct RatResponse {
    /// 响应内容
    content: String,
    /// 状态码
    status_code: u16,
    /// 响应头
    headers: HashMap<String, String>,
    /// 内容类型
    content_type: String,
}

#[pymethods]
impl RatResponse {
    /// 创建新的响应对象
    #[new]
    #[pyo3(signature = (content, status_code = 200, content_type = None))]
    pub fn new(
        content: String,
        status_code: u16,
        content_type: Option<String>,
    ) -> PyResult<Self> {
        let mut response_headers = HashMap::new();
        
        // 设置默认Content-Type
        let ct = content_type.unwrap_or_else(|| "text/html; charset=utf-8".to_string());
        response_headers.insert("Content-Type".to_string(), ct.clone());
        
        Ok(Self {
            content,
            status_code,
            headers: response_headers,
            content_type: ct,
        })
    }

    fn __repr__(&self) -> String {
        format!("RatResponse(content='{}', status_code={}, content_type='{}')", 
                self.content, self.status_code, self.content_type)
    }

    fn __str__(&self) -> String {
        format!("Response: {} ({})", self.status_code, self.content_type)
    }

    /// 创建JSON响应
    #[staticmethod]
    #[pyo3(signature = (data, status_code = 200))]
    pub fn json(py: Python, data: PyObject, status_code: u16) -> PyResult<Self> {
        // 将Python对象转换为JSON字符串
        let json_module = py.import("json")?;
        let json_str: String = json_module
            .call_method1("dumps", (data,))?
            .extract()?;
        
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        
        Ok(Self {
            content: json_str,
            status_code,
            headers,
            content_type: "application/json".to_string(),
        })
    }

    /// 创建重定向响应
    #[staticmethod]
    #[pyo3(signature = (location, status_code = 302))]
    pub fn redirect(location: String, status_code: u16) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Location".to_string(), location);
        headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        
        Self {
            content: String::new(),
            status_code,
            headers,
            content_type: "text/html; charset=utf-8".to_string(),
        }
    }

    /// 获取响应内容
    #[getter]
    pub fn data(&self) -> String {
        self.content.clone()
    }

    /// 获取状态码
    #[getter]
    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    /// 设置状态码
    #[setter]
    pub fn set_status_code(&mut self, status_code: u16) {
        self.status_code = status_code;
    }

    /// 获取响应头
    #[getter]
    pub fn headers(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, value) in &self.headers {
            dict.set_item(key, value)?;
        }
        Ok(dict.into())
    }

    /// 设置响应头
    pub fn set_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }

    /// 获取内容类型
    #[getter]
    pub fn content_type(&self) -> String {
        self.content_type.clone()
    }

    /// 设置内容类型
    #[setter]
    pub fn set_content_type(&mut self, content_type: String) {
        self.content_type = content_type.clone();
        self.headers.insert("Content-Type".to_string(), content_type);
    }

    /// 获取响应内容长度
    #[getter]
    pub fn content_length(&self) -> usize {
        self.content.len()
    }

    /// 检查是否为JSON响应
    pub fn is_json(&self) -> bool {
        self.content_type.starts_with("application/json")
    }

    /// 添加Cookie
    pub fn set_cookie(
        &mut self,
        key: String,
        value: String,
        max_age: Option<i64>,
        path: Option<String>,
        domain: Option<String>,
        secure: Option<bool>,
        httponly: Option<bool>,
    ) {
        let mut cookie = format!("{}={}", key, value);
        
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
        if httponly.unwrap_or(false) {
            cookie.push_str("; HttpOnly");
        }
        
        self.headers.insert("Set-Cookie".to_string(), cookie);
    }
}

impl RatResponse {
    /// 内部方法：获取响应内容
    pub fn get_content(&self) -> &str {
        &self.content
    }

    /// 内部方法：获取响应头映射
    pub fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    /// 内部方法：获取状态码
    pub fn get_status_code(&self) -> u16 {
        self.status_code
    }
}

/// 将Rust HTTP响应转换为Python响应
pub fn to_py_response(response: RatResponse) -> RatResponse {
    // 这里可以添加转换逻辑
    response
}