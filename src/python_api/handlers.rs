//! Python 处理函数模块
//! 
//! 提供常用的处理函数和工具，参考 streaming_demo.rs 的模式

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyFunction};
use std::sync::Arc;
use hyper::{Request, Method, StatusCode};
use hyper::body::Incoming;
use crate::server::streaming::{SseResponse, ChunkedResponse};
use tokio::time::{sleep, Duration};
use serde_json::json;
// 移除 rat_quick_threshold 依赖
use crate::python_api::codec::PyQuickCodec;
use crate::python_api::streaming::{PySseResponse, PySseSender};

/// Python 处理函数包装器
#[pyclass(name = "Handler")]
pub struct PyHandler {
    handler_func: PyObject,
    codec: PyQuickCodec,
}

#[pymethods]
impl PyHandler {
    #[new]
    fn new(handler_func: PyObject) -> PyResult<Self> {
        Ok(Self {
            handler_func,
            codec: PyQuickCodec::new()?,
        })
    }
    
    /// 调用处理函数
    /// 
    /// Args:
    ///     request_data: 请求数据
    /// 
    /// Returns:
    ///     处理结果
    fn call(&self, py: Python, request_data: PyObject) -> PyResult<PyObject> {
        self.handler_func.call1(py, (request_data,))
    }
    
    /// 异步调用处理函数（用于 SSE）
    /// 
    /// Args:
    ///     request_data: 请求数据
    ///     sender: SSE 发送器
    fn call_async(&self, py: Python, request_data: PyObject, sender: &PySseSender) -> PyResult<()> {
        // 在后台任务中调用处理函数
        let handler = self.handler_func.clone();
        let codec = self.codec.clone();
        
        // 这里应该在异步上下文中执行，但为了简化，我们直接调用
        let result = handler.call1(py, (request_data,))?;
        
        // 处理返回结果
        if let Ok(string_data) = result.extract::<String>(py) {
            sender.send_sse_data(&string_data)?;
        } else if let Ok(_dict) = result.downcast::<PyDict>(py) {
            sender.send_json(py, result)?;
        }
        
        Ok(())
    }
}

/// 创建简单的文本响应处理函数
#[pyfunction]
fn create_text_handler(text: String) -> PyResult<PyHandler> {
    Python::with_gil(|py| {
        let handler_code = format!(
            "lambda request: '{}'",
            text.replace("'", "\\'")
        );
        
        let handler_func = py.eval(&handler_code, None, None)?
            .to_object(py);
        
        PyHandler::new(handler_func)
    })
}

/// 创建 JSON 响应处理函数
#[pyfunction]
fn create_json_handler(py: Python, data: PyObject) -> PyResult<PyHandler> {
    let json_str = if let Ok(dict) = data.downcast::<PyDict>(py) {
        let json_value = crate::python_api::streaming::python_object_to_json_value(dict.as_ref())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("转换失败: {}", e)))?;
        serde_json::to_string(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?
    } else {
        return Err(pyo3::exceptions::PyValueError::new_err("数据必须是字典类型"));
    };
    
    let handler_code = format!(
        "lambda request: {}",
        json_str
    );
    
    let handler_func = py.eval(&handler_code, None, None)?
        .to_object(py);
    
    Ok(PyHandler::new(handler_func)?)
}

/// 创建 SSE 计数器处理函数
#[pyfunction]
fn create_counter_handler(py: Python, max_count: i32, interval_ms: u64) -> PyResult<PyHandler> {
    let handler_code = format!(
        r#"
import time
import json

def counter_handler(request):
    results = []
    for i in range(1, {} + 1):
        data = {{
            "timestamp": time.time(),
            "counter": i,
            "message": f"Update #{{i}}"
        }}
        results.append(json.dumps(data))
        if i < {}:
            time.sleep({} / 1000.0)
    return results

counter_handler
"#,
        max_count, max_count, interval_ms
    );
    
    let handler_func = py.eval(&handler_code, None, None)?
        .to_object(py);
    
    Ok(PyHandler::new(handler_func)?)
}

/// 创建实时日志处理函数
#[pyfunction]
fn create_log_handler(py: Python, log_count: i32, interval_ms: u64) -> PyResult<PyHandler> {
    let handler_code = format!(
        r#"
import time
import random

def log_handler(request):
    log_levels = ["INFO", "WARN", "ERROR", "DEBUG"]
    messages = [
        "用户登录成功",
        "数据库连接建立",
        "处理用户请求",
        "缓存更新完成",
        "定时任务执行",
        "系统健康检查"
    ]
    
    results = []
    for i in range({}):
        level = random.choice(log_levels)
        message = random.choice(messages)
        timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
        
        log_entry = f"[{{timestamp}}] {{level}} - {{message}}"
        results.append(log_entry)
        
        if i < {} - 1:
            time.sleep({} / 1000.0)
    
    return results

log_handler
"#,
        log_count, log_count, interval_ms
    );
    
    let handler_func = py.eval(&handler_code, None, None)?
        .to_object(py);
    
    Ok(PyHandler::new(handler_func)?)
}

/// 创建文件上传处理函数
#[pyfunction]
fn create_upload_handler(py: Python, upload_dir: String) -> PyResult<PyHandler> {
    let handler_code = format!(
        r#"
import os
import json
import base64

def upload_handler(request):
    upload_dir = "{}"
    
    # 确保上传目录存在
    os.makedirs(upload_dir, exist_ok=True)
    
    # 处理上传的文件数据
    if "file_data" in request and "filename" in request:
        file_data = base64.b64decode(request["file_data"])
        filename = request["filename"]
        
        file_path = os.path.join(upload_dir, filename)
        with open(file_path, "wb") as f:
            f.write(file_data)
        
        return {{
            "status": "success",
            "message": f"文件 {{filename}} 上传成功",
            "file_path": file_path,
            "file_size": len(file_data)
        }}
    else:
        return {{
            "status": "error",
            "message": "缺少文件数据或文件名"
        }}

upload_handler
"#,
        upload_dir.replace("\\", "\\\\")
    );
    
    let handler_func = py.eval(&handler_code, None, None)?
        .to_object(py);
    
    Ok(PyHandler::new(handler_func)?)
}

/// 创建数据处理管道
#[pyclass(name = "DataPipeline")]
pub struct PyDataPipeline {
    processors: Vec<PyObject>,
    codec: PyQuickCodec,
}

#[pymethods]
impl PyDataPipeline {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            processors: Vec::new(),
            codec: PyQuickCodec::new()?,
        })
    }
    
    /// 添加处理器
    /// 
    /// Args:
    ///     processor: 处理函数，接收数据并返回处理后的数据
    fn add_processor(&mut self, processor: PyObject) {
        self.processors.push(processor);
    }
    
    /// 处理数据
    /// 
    /// Args:
    ///     data: 要处理的数据
    /// 
    /// Returns:
    ///     处理后的数据
    fn process(&self, py: Python, data: PyObject) -> PyResult<PyObject> {
        let mut current_data = data;
        
        for processor in &self.processors {
            current_data = processor.call1(py, (current_data,))?;
        }
        
        Ok(current_data)
    }
    
    /// 批量处理数据
    /// 
    /// Args:
    ///     data_list: 要处理的数据列表
    /// 
    /// Returns:
    ///     处理后的数据列表
    fn process_batch(&self, py: Python, data_list: &PyList) -> PyResult<PyObject> {
        let py_list = PyList::empty(py);
        
        for item in data_list {
            let processed = self.process(py, item.to_object(py))?;
            py_list.append(processed)?;
        }
        
        Ok(py_list.to_object(py))
    }
    
    /// 清空处理器
    fn clear(&mut self) {
        self.processors.clear();
    }
    
    /// 获取处理器数量
    fn len(&self) -> usize {
        self.processors.len()
    }
}

/// 创建数据验证器
#[pyfunction]
fn create_validator(py: Python, schema: PyObject) -> PyResult<PyObject> {
    let validator_code = r#"
def create_validator(schema):
    def validator(data):
        # 简单的数据验证逻辑
        if isinstance(schema, dict):
            if not isinstance(data, dict):
                raise ValueError("数据必须是字典类型")
            
            for key, expected_type in schema.items():
                if key not in data:
                    raise ValueError(f"缺少必需字段: {key}")
                
                if expected_type == "str" and not isinstance(data[key], str):
                    raise ValueError(f"字段 {key} 必须是字符串类型")
                elif expected_type == "int" and not isinstance(data[key], int):
                    raise ValueError(f"字段 {key} 必须是整数类型")
                elif expected_type == "float" and not isinstance(data[key], (int, float)):
                    raise ValueError(f"字段 {key} 必须是数字类型")
                elif expected_type == "bool" and not isinstance(data[key], bool):
                    raise ValueError(f"字段 {key} 必须是布尔类型")
                elif expected_type == "list" and not isinstance(data[key], list):
                    raise ValueError(f"字段 {key} 必须是列表类型")
        
        return True
    
    return validator

create_validator
"#;
    
    let create_validator_func = py.eval(validator_code, None, None)?;
    Ok(create_validator_func.call1((schema,))?.to_object(py))
}

/// 注册处理器相关函数到 Python 模块
pub fn register_handler_functions(module: &PyModule) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(create_text_handler, module)?)?;
    module.add_function(wrap_pyfunction!(create_json_handler, module)?)?;
    module.add_function(wrap_pyfunction!(create_counter_handler, module)?)?;
    module.add_function(wrap_pyfunction!(create_log_handler, module)?)?;
    module.add_function(wrap_pyfunction!(create_upload_handler, module)?)?;
    module.add_function(wrap_pyfunction!(create_validator, module)?)?;
    Ok(())
}

/// 注册处理函数模块
pub fn register_handlers_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    let handlers_module = PyModule::new(py, "handlers")?;
    
    handlers_module.add_class::<PyHandler>()?;
    handlers_module.add_class::<PyDataPipeline>()?;
    
    handlers_module.add_function(wrap_pyfunction!(create_text_handler, handlers_module)?)?;
    handlers_module.add_function(wrap_pyfunction!(create_json_handler, handlers_module)?)?;
    handlers_module.add_function(wrap_pyfunction!(create_counter_handler, handlers_module)?)?;
    handlers_module.add_function(wrap_pyfunction!(create_log_handler, handlers_module)?)?;
    handlers_module.add_function(wrap_pyfunction!(create_upload_handler, handlers_module)?)?;
    handlers_module.add_function(wrap_pyfunction!(create_validator, handlers_module)?)?;
    
    parent_module.add_submodule(handlers_module)?;
    Ok(())
}