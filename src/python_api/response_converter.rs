//! Python 响应转换模块
//! 
//! 提供将 Python 装饰器函数返回值转换为 HTTP 响应的功能
//! 支持智能 MIME 类型检测和各种响应类型处理

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyBytes};
use std::collections::HashMap;
use std::path::Path;

/// MIME 类型映射表
static MIME_TYPES: &[(&str, &str)] = &[
    // 图片类型
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("ico", "image/x-icon"),
    ("bmp", "image/bmp"),
    ("tiff", "image/tiff"),
    
    // 文档类型
    ("pdf", "application/pdf"),
    ("doc", "application/msword"),
    ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    ("xls", "application/vnd.ms-excel"),
    ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    
    // 文本类型
    ("html", "text/html; charset=utf-8"),
    ("htm", "text/html; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("js", "application/javascript; charset=utf-8"),
    ("json", "application/json; charset=utf-8"),
    ("xml", "application/xml; charset=utf-8"),
    ("txt", "text/plain; charset=utf-8"),
    
    // 字体类型
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("ttf", "font/ttf"),
    ("otf", "font/otf"),
    
    // 压缩文件
    ("zip", "application/zip"),
    ("rar", "application/x-rar-compressed"),
    ("tar", "application/x-tar"),
    ("gz", "application/gzip"),
    
    // 音视频
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("mp4", "video/mp4"),
    ("avi", "video/x-msvideo"),
    ("mov", "video/quicktime"),
];

/// 根据文件扩展名获取 MIME 类型
fn get_mime_type_by_extension(file_path: &str) -> String {
    if let Some(extension) = Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
    {
        let ext_lower = extension.to_lowercase();
        for (ext, mime) in MIME_TYPES {
            if *ext == ext_lower {
                return mime.to_string();
            }
        }
    }
    "application/octet-stream".to_string()
}

/// 检测二进制数据的 MIME 类型
fn detect_binary_mime_type(data: &[u8]) -> String {
    if data.len() < 4 {
        return "application/octet-stream".to_string();
    }
    
    // 检测常见的文件头
    match &data[0..4] {
        [0xFF, 0xD8, 0xFF, _] => "image/jpeg".to_string(),
        [0x89, 0x50, 0x4E, 0x47] => "image/png".to_string(),
        [0x47, 0x49, 0x46, 0x38] => "image/gif".to_string(),
        [0x25, 0x50, 0x44, 0x46] => "application/pdf".to_string(),
        [0x50, 0x4B, 0x03, 0x04] | [0x50, 0x4B, 0x05, 0x06] => "application/zip".to_string(),
        _ => {
            // 检测 WebP
            if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
                return "image/webp".to_string();
            }
            // 检测 SVG (XML 开头)
            if data.starts_with(b"<?xml") || data.starts_with(b"<svg") {
                return "image/svg+xml".to_string();
            }
            // 检测 HTML
            if data.starts_with(b"<!DOCTYPE html") || data.starts_with(b"<html") {
                return "text/html; charset=utf-8".to_string();
            }
            "application/octet-stream".to_string()
        }
    }
}

/// 将 Python 响应转换为 HTTP 响应数据
/// 
/// 返回 (status_code, headers, body)
pub fn convert_python_response(
    py: Python,
    response: PyObject,
) -> PyResult<(u16, HashMap<String, String>, Vec<u8>)> {
    // 首先尝试提取 HttpResponse 对象
    if let Ok(http_response) = response.extract::<crate::python_api::HttpResponse>(py) {
        return Ok((
            http_response.status,
            http_response.headers,
            http_response.body,
        ));
    }
    
    // 尝试提取 TypedResponse 对象（优先处理，因为它包含明确的类型信息）
    if let Ok(typed_response) = response.extract::<crate::python_api::http::core::TypedResponse>(py) {
        return convert_typed_response(py, typed_response);
    }
    
    // 尝试提取字节数据
    if let Ok(bytes_data) = response.downcast::<PyBytes>(py) {
        let data = bytes_data.as_bytes().to_vec();
        let mut headers = HashMap::new();
        let content_type = detect_binary_mime_type(&data);
        headers.insert("Content-Type".to_string(), content_type);
        return Ok((200, headers, data));
    }
    
    // 尝试提取字符串
    if let Ok(string_data) = response.extract::<String>(py) {
        let mut headers = HashMap::new();
        // 智能检测字符串内容类型
        let content_type = if string_data.trim_start().starts_with('{') || string_data.trim_start().starts_with('[') {
            "application/json; charset=utf-8".to_string()
        } else if string_data.trim_start().starts_with("<!DOCTYPE") || string_data.trim_start().starts_with("<html") {
            "text/html; charset=utf-8".to_string()
        } else {
            "text/plain; charset=utf-8".to_string()
        };
        headers.insert("Content-Type".to_string(), content_type);
        return Ok((200, headers, string_data.into_bytes()));
    }
    
    // 尝试提取字典（作为 JSON 响应）
    if let Ok(dict) = response.downcast::<PyDict>(py) {
        let json_value = crate::python_api::streaming::python_object_to_json_value(dict.as_ref())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 转换失败: {}", e)))?;
        let json_str = serde_json::to_string(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?;
        
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json; charset=utf-8".to_string());
        return Ok((200, headers, json_str.into_bytes()));
    }
    
    // 如果都不匹配，尝试转换为字符串
    let fallback_str = format!("{:?}", response);
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
    Ok((200, headers, fallback_str.into_bytes()))
}

/// 处理 TypedResponse 对象
fn convert_typed_response(
    py: Python,
    typed_response: crate::python_api::http::core::TypedResponse,
) -> PyResult<(u16, HashMap<String, String>, Vec<u8>)> {
    let mut headers = HashMap::new();
    
    // 从 kwargs 中提取额外的头部信息
    if let Ok(kwargs_dict) = typed_response.kwargs.downcast::<PyDict>(py) {
        for (key, value) in kwargs_dict {
            if let (Ok(key_str), Ok(value_str)) = (key.extract::<String>(), value.extract::<String>()) {
                if key_str.to_lowercase() != "content_type" { // content_type 单独处理
                    headers.insert(key_str, value_str);
                }
            }
        }
    }
    
    let (content_type, body, status) = match typed_response.response_type {
        crate::python_api::http::core::ResponseType::JSON => {
            let json_str = if let Ok(s) = typed_response.content.extract::<String>(py) {
                s
            } else {
                // 将 PyObject 转换为 JSON 字符串
                let json_module = py.import("json")?;
                let dumps_fn = json_module.getattr("dumps")?;
                let json_result = dumps_fn.call1((typed_response.content,))?;
                json_result.extract::<String>()?
            };
            ("application/json; charset=utf-8".to_string(), json_str.into_bytes(), 200)
        }
        crate::python_api::http::core::ResponseType::HTML => {
            let content_str = typed_response.content.extract::<String>(py)
                .unwrap_or_else(|_| format!("{:?}", typed_response.content));
            ("text/html; charset=utf-8".to_string(), content_str.into_bytes(), 200)
        }
        crate::python_api::http::core::ResponseType::TEXT => {
            let content_str = typed_response.content.extract::<String>(py)
                .unwrap_or_else(|_| format!("{:?}", typed_response.content));
            ("text/plain; charset=utf-8".to_string(), content_str.into_bytes(), 200)
        }
        crate::python_api::http::core::ResponseType::FILE => {
            // 处理文件响应
            if let Ok(file_path) = typed_response.content.extract::<String>(py) {
                match std::fs::read(&file_path) {
                    Ok(file_data) => {
                        let mime_type = get_mime_type_by_extension(&file_path);
                        (mime_type, file_data, 200)
                    }
                    Err(_) => {
                        let error_msg = format!("文件未找到: {}", file_path);
                        ("text/plain; charset=utf-8".to_string(), error_msg.into_bytes(), 404)
                    }
                }
            } else {
                let error_msg = "无效的文件路径";
                ("text/plain; charset=utf-8".to_string(), error_msg.as_bytes().to_vec(), 400)
            }
        }
        crate::python_api::http::core::ResponseType::CUSTOM_BYTES => {
            // 处理自定义字节响应
            let data = if let Ok(bytes_obj) = typed_response.content.downcast::<PyBytes>(py) {
                bytes_obj.as_bytes().to_vec()
            } else if let Ok(byte_list) = typed_response.content.extract::<Vec<u8>>(py) {
                byte_list
            } else {
                // 尝试转换为字符串再转字节
                let content_str = typed_response.content.extract::<String>(py)
                    .unwrap_or_else(|_| format!("{:?}", typed_response.content));
                content_str.into_bytes()
            };
            
            // 从 kwargs 中获取 content_type，或者自动检测
            let content_type = if let Ok(kwargs_dict) = typed_response.kwargs.downcast::<PyDict>(py) {
                if let Ok(Some(ct)) = kwargs_dict.get_item("content_type") {
                    ct.extract::<String>().ok()
                        .unwrap_or_else(|| detect_binary_mime_type(&data))
                } else {
                    detect_binary_mime_type(&data)
                }
            } else {
                detect_binary_mime_type(&data)
            };
            
            (content_type, data, 200)
        }
        crate::python_api::http::core::ResponseType::CUSTOM => {
            // 处理自定义响应
            let content_str = typed_response.content.extract::<String>(py)
                .unwrap_or_else(|_| format!("{:?}", typed_response.content));
            
            // 从 kwargs 中获取 content_type
            let content_type = if let Ok(kwargs_dict) = typed_response.kwargs.downcast::<PyDict>(py) {
                if let Ok(Some(ct)) = kwargs_dict.get_item("content_type") {
                    ct.extract::<String>().ok()
                        .unwrap_or_else(|| "text/plain; charset=utf-8".to_string())
                } else {
                    "text/plain; charset=utf-8".to_string()
                }
            } else {
                "text/plain; charset=utf-8".to_string()
            };
            
            (content_type, content_str.into_bytes(), 200)
        }
        crate::python_api::http::core::ResponseType::REDIRECT => {
            // 处理重定向响应
            let location = typed_response.content.extract::<String>(py)
                .unwrap_or_else(|_| "/".to_string());
            headers.insert("Location".to_string(), location);
            ("text/plain; charset=utf-8".to_string(), b"Redirecting...".to_vec(), 302)
        }
        crate::python_api::http::core::ResponseType::SSE | 
        crate::python_api::http::core::ResponseType::SSE_JSON | 
        crate::python_api::http::core::ResponseType::SSE_TEXT => {
            // SSE 响应需要特殊处理，这里先返回基本响应
            headers.insert("Cache-Control".to_string(), "no-cache".to_string());
            headers.insert("Connection".to_string(), "keep-alive".to_string());
            let content_str = typed_response.content.extract::<String>(py)
                .unwrap_or_else(|_| "data: SSE stream\n\n".to_string());
            ("text/event-stream; charset=utf-8".to_string(), content_str.into_bytes(), 200)
        }
        crate::python_api::http::core::ResponseType::CHUNK => {
            // 分块响应
            headers.insert("Transfer-Encoding".to_string(), "chunked".to_string());
            let content_str = typed_response.content.extract::<String>(py)
                .unwrap_or_else(|_| format!("{:?}", typed_response.content));
            ("text/plain; charset=utf-8".to_string(), content_str.into_bytes(), 200)
        }
    };
    
    // 设置 Content-Type（如果还没有设置的话）
    if !headers.contains_key("Content-Type") {
        headers.insert("Content-Type".to_string(), content_type);
    }
    
    Ok((status, headers, body))
}