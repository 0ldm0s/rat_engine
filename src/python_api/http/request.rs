//! HTTP请求对象封装

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use std::collections::HashMap;
use serde_json::Value;
use crate::python_api::HttpMethod;

/// HTTP请求对象
#[pyclass]
pub struct RatRequest {
    /// HTTP方法
    method: HttpMethod,
    /// 请求路径
    path: String,
    /// 查询参数
    args: HashMap<String, String>,
    /// 请求头
    headers: HashMap<String, String>,
    /// 请求体
    data: Option<Vec<u8>>,
    /// JSON数据
    json_data: Option<Value>,
    /// 表单数据
    form_data: HashMap<String, String>,
}

#[pymethods]
impl RatRequest {
    /// 创建新的请求对象
    #[new]
    pub fn new(
        method: String,
        path: String,
        data: Option<Vec<u8>>,
    ) -> PyResult<Self> {
        let method = HttpMethod::new(&method);

        let args = HashMap::new();
        let headers = HashMap::new();

        // 尝试解析JSON数据
        let json_data = if let Some(ref data) = data {
            if let Ok(text) = String::from_utf8(data.clone()) {
                serde_json::from_str(&text).ok()
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            method,
            path,
            args,
            headers,
            data,
            json_data,
            form_data: HashMap::new(),
        })
    }

    /// 获取HTTP方法
    #[getter]
    pub fn method(&self) -> String {
        self.method.as_str().to_string()
    }

    /// 获取请求路径
    #[getter]
    pub fn path(&self) -> String {
        self.path.clone()
    }

    /// 获取查询参数
    #[getter]
    pub fn args(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, value) in &self.args {
            dict.set_item(key, value)?;
        }
        Ok(dict.into())
    }

    /// 获取请求头
    #[getter]
    pub fn headers(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, value) in &self.headers {
            dict.set_item(key, value)?;
        }
        Ok(dict.into())
    }

    /// 获取原始请求体
    #[getter]
    pub fn data(&self) -> Option<Vec<u8>> {
        self.data.clone()
    }

    /// 获取JSON数据
    pub fn get_json(&self, py: Python) -> PyResult<PyObject> {
        match &self.json_data {
            Some(json) => {
                let json_str = serde_json::to_string(json)
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("JSON序列化失败: {}", e)))?;
                let json_module = py.import("json")?;
                Ok(json_module.call_method1("loads", (json_str,))?.into())
            }
            None => Ok(py.None()),
        }
    }

    /// 获取表单数据
    pub fn get_form(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, value) in &self.form_data {
            dict.set_item(key, value)?;
        }
        Ok(dict.into())
    }

    fn __repr__(&self) -> String {
        format!("RatRequest(method='{}', path='{}', args_count={}, headers_count={})", 
                self.method, self.path, self.args.len(), self.headers.len())
    }

    /// 获取指定查询参数
    pub fn get_arg(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.args.get(key).cloned().or_else(|| default.map(|s| s.to_string()))
    }

    /// 获取指定请求头
    pub fn get_header(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.headers.get(key).cloned().or_else(|| default.map(|s| s.to_string()))
    }

    /// 检查是否为JSON请求
    pub fn is_json(&self) -> bool {
        self.json_data.is_some()
    }

    /// 获取Content-Type
    pub fn content_type(&self) -> Option<String> {
        self.get_header("content-type", None)
    }
}

impl RatRequest {
    /// 辅助方法：将PyDict转换为HashMap
    fn dict_to_hashmap(dict: Option<&PyDict>) -> Option<HashMap<String, String>> {
        dict.map(|d| {
            let mut map = HashMap::new();
            for (key, value) in d.iter() {
                if let (Ok(k), Ok(v)) = (key.extract::<String>(), value.extract::<String>()) {
                    map.insert(k, v);
                }
            }
            map
        })
    }

    /// 获取内部HTTP方法枚举
    pub fn get_method(&self) -> &HttpMethod {
        &self.method
    }

    /// 设置表单数据
    pub fn set_form_data(&mut self, form_data: HashMap<String, String>) {
        self.form_data = form_data;
    }
    
    /// 从Rust代码创建RatRequest实例（用于内部使用）
    pub fn from_parts(
        method: HttpMethod,
        path: String,
        args: HashMap<String, String>,
        headers: HashMap<String, String>,
        data: Option<Vec<u8>>,
        json_data: Option<Value>,
        form_data: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            method,
            path,
            args,
            headers,
            data,
            json_data,
            form_data: form_data.unwrap_or_else(HashMap::new),
        }
    }
}