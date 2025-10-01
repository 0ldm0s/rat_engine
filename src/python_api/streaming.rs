//! Python 流式响应模块
//! 
//! 提供与 streaming_demo.rs 一致的流式响应功能

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString};
use crate::server::streaming::{SseResponse, ChunkedResponse, StreamingResponse};
use hyper::body::{Frame, Bytes};
use tokio::sync::mpsc;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use serde_json::json;
// 移除 rat_quick_threshold 依赖
use crate::python_api::codec::{PyQuickCodec, PyBinValue};

/// Python SSE 响应类
#[pyclass(name = "SseResponse")]
pub struct PySseResponse {
    sse: SseResponse,
    codec: PyQuickCodec,
}

#[pymethods]
impl PySseResponse {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            sse: SseResponse::new(),
            codec: PyQuickCodec::new()?,
        })
    }
    
    /// 发送 SSE 事件
    /// 
    /// Args:
    ///     event_type: 事件类型
    ///     data: 事件数据
    fn send_event(&self, event_type: &str, data: &str) -> PyResult<()> {
        self.sse.send_event(event_type, data)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("发送事件失败: {:?}", e)))?;
        Ok(())
    }
    
    /// 发送 SSE 数据
    /// 
    /// Args:
    ///     data: 要发送的数据
    fn send_data(&self, data: &str) -> PyResult<()> {
        self.sse.send_data(data)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("发送数据失败: {:?}", e)))?;
        Ok(())
    }
    
    /// 发送心跳
    fn send_heartbeat(&self) -> PyResult<()> {
        self.sse.send_heartbeat()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("发送心跳失败: {:?}", e)))?;
        Ok(())
    }
    
    /// 发送 JSON 数据
    /// 
    /// Args:
    ///     data: Python 对象，将被序列化为 JSON
    pub fn send_json(&self, py: Python, data: PyObject) -> PyResult<()> {
        // 将 Python 对象转换为 JSON
        let json_value = python_object_to_json_value(&data.as_ref(py))?;
        let json_str = serde_json::to_string(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?;
        
        self.send_data(&json_str)
    }
    
    /// 发送编码数据（使用 rat_quick_threshold）
    /// 
    /// Args:
    ///     data: Python 对象，将使用 rat_quick_threshold 编码
    fn send_encoded(&self, py: Python, data: PyObject) -> PyResult<()> {
        // 使用 rat_quick_threshold 编码数据
        let rat_module = py.import("rat_quick_threshold")?;
        let serialize_fn = rat_module.getattr("serialize")?;
        let encoded: PyObject = serialize_fn.call1((data,))?.into();
        
        // 如果编码结果是字节数据，转换为 base64 字符串发送
        if let Ok(bytes) = encoded.downcast::<PyBytes>(py) {
            let base64_data = base64::encode(bytes.as_bytes());
            self.send_data(&base64_data)
        } else {
            // 如果不是字节数据，直接转换为字符串发送
            let string_data = encoded.extract::<String>(py)?;
            self.send_data(&string_data)
        }
    }
    
    /// 获取发送器（用于异步发送）
    fn get_sender(&self) -> PySseSender {
        PySseSender {
            sender: self.sse.get_sender(),
            codec: self.codec.clone(),
        }
    }
}

/// Python SSE 发送器类
#[pyclass(name = "SseSender")]
pub struct PySseSender {
    sender: mpsc::UnboundedSender<Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
    codec: PyQuickCodec,
}

#[pymethods]
impl PySseSender {
    /// 发送原始帧数据
    /// 
    /// Args:
    ///     data: 要发送的字符串数据
    fn send_frame(&self, data: &str) -> PyResult<()> {
        let frame = Frame::data(Bytes::from(data.to_string()));
        self.sender.send(Ok(frame))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("发送帧失败: {:?}", e)))?;
        Ok(())
    }
    
    /// 发送 SSE 事件
    /// 
    /// Args:
    ///     event_type: 事件类型
    ///     data: 事件数据
    fn send_sse_event(&self, event_type: &str, data: &str) -> PyResult<()> {
        let formatted = format!("event: {}\ndata: {}\n\n", event_type, data);
        self.send_frame(&formatted)
    }
    
    /// 发送 SSE 数据
    /// 
    /// Args:
    ///     data: 要发送的数据
    pub fn send_sse_data(&self, data: &str) -> PyResult<()> {
        let formatted = format!("data: {}\n\n", data);
        self.send_frame(&formatted)
    }
    
    /// 发送心跳
    fn send_heartbeat(&self) -> PyResult<()> {
        self.send_frame(":\n\n")
    }
    
    /// 发送 JSON 数据
    /// 
    /// Args:
    ///     data: Python 对象，将被序列化为 JSON
    pub fn send_json(&self, py: Python, data: PyObject) -> PyResult<()> {
        let json_value = python_object_to_json_value(&data.as_ref(py))?;
        let json_str = serde_json::to_string(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?;
        
        self.send_sse_data(&json_str)
    }
    
    /// 发送编码数据（使用 rat_quick_threshold）
    /// 
    /// Args:
    ///     data: Python 对象，将使用 rat_quick_threshold 编码
    fn send_encoded(&self, py: Python, data: PyObject) -> PyResult<()> {
        // 使用 rat_quick_threshold 编码数据
        let rat_module = py.import("rat_quick_threshold")?;
        let serialize_fn = rat_module.getattr("serialize")?;
        let encoded: PyObject = serialize_fn.call1((data,))?.into();
        
        // 如果编码结果是字节数据，转换为 base64 字符串发送
        if let Ok(bytes) = encoded.downcast::<PyBytes>(py) {
            let base64_data = base64::encode(bytes.as_bytes());
            self.send_sse_data(&base64_data)
        } else {
            // 如果不是字节数据，直接转换为字符串发送
            let string_data = encoded.extract::<String>(py)?;
            self.send_sse_data(&string_data)
        }
    }
}

/// Python 分块响应类
#[pyclass(name = "ChunkedResponse")]
pub struct PyChunkedResponse {
    response: ChunkedResponse,
}

#[pymethods]
impl PyChunkedResponse {
    #[new]
    fn new() -> Self {
        Self {
            response: ChunkedResponse::new(),
        }
    }
    
    /// 添加数据块
    /// 
    /// Args:
    ///     chunk: 要添加的数据块
    fn add_chunk(&mut self, chunk: &str) -> PyResult<()> {
        self.response = self.response.clone().add_chunk(chunk.to_string());
        Ok(())
    }
    
    /// 设置块之间的延迟
    /// 
    /// Args:
    ///     delay_ms: 延迟时间（毫秒）
    fn with_delay(&mut self, delay_ms: u64) -> PyResult<()> {
        self.response = self.response.clone().with_delay(Duration::from_millis(delay_ms));
        Ok(())
    }
    
    /// 添加多个数据块
    /// 
    /// Args:
    ///     chunks: 数据块列表
    fn add_chunks(&mut self, py: Python, chunks: &PyList) -> PyResult<()> {
        for item in chunks {
            if let Ok(chunk) = item.extract::<String>() {
                self.add_chunk(&chunk)?;
            }
        }
        Ok(())
    }
}

/// 流式响应工具函数
#[pyfunction]
fn json_stream(py: Python, items: &PyList) -> PyResult<PyObject> {
    // 将 Python 列表转换为 JSON 流
    let mut json_items = Vec::new();
    for item in items {
        let json_value = python_object_to_json_value(item)?;
        json_items.push(json_value);
    }
    
    // 这里应该返回一个流式响应，但为了简化，我们返回一个字符串
    let json_str = serde_json::to_string(&json_items)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?;
    
    Ok(json_str.to_object(py))
}

/// 文本流工具函数
#[pyfunction]
fn text_stream(py: Python, lines: &PyList) -> PyResult<PyObject> {
    let mut text_lines = Vec::new();
    for item in lines {
        if let Ok(line) = item.extract::<String>() {
            text_lines.push(line);
        }
    }
    
    let combined_text = text_lines.join("\n");
    Ok(combined_text.to_object(py))
}

/// 注册流式响应相关函数到 Python 模块
pub fn register_streaming_functions(module: &PyModule) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(json_stream, module)?)?;
    module.add_function(wrap_pyfunction!(text_stream, module)?)?;
    Ok(())
}



/// 辅助函数：将 Python 对象转换为 JSON 值
pub fn python_object_to_json_value(obj: &PyAny) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::Value::Number(serde_json::Number::from(i)))
    } else if let Ok(f) = obj.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            Ok(serde_json::Value::Number(n))
        } else {
            Err(pyo3::exceptions::PyValueError::new_err("无效的浮点数值"))
        }
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if let Ok(list) = obj.downcast::<PyList>() {
        let mut vec = Vec::new();
        for item in list {
            vec.push(python_object_to_json_value(item)?);
        }
        Ok(serde_json::Value::Array(vec))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict {
            let key_str = key.extract::<String>()?;
            map.insert(key_str, python_object_to_json_value(value)?);
        }
        Ok(serde_json::Value::Object(map))
    } else {
        Err(pyo3::exceptions::PyValueError::new_err("不支持的 Python 对象类型"))
    }
}

/// 辅助函数：将 Python 对象转换为 PyBinValue
pub fn python_to_data_value(obj: &PyAny) -> PyResult<PyBinValue> {
    PyBinValue::from_pyobj(obj)
}

/// 注册流式响应模块
pub fn register_streaming_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    let streaming_module = PyModule::new(py, "streaming")?;
    
    streaming_module.add_class::<PySseResponse>()?;
    streaming_module.add_class::<PySseSender>()?;
    streaming_module.add_class::<PyChunkedResponse>()?;
    
    streaming_module.add_function(wrap_pyfunction!(json_stream, streaming_module)?)?;
    streaming_module.add_function(wrap_pyfunction!(text_stream, streaming_module)?)?;
    
    parent_module.add_submodule(streaming_module)?;
    Ok(())
}