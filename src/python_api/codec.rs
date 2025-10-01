//! Python 编码解码模块
//! 
//! 提供基本的编码解码功能

use std::sync::Arc;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyBool, PyFloat, PyInt};
use pyo3::exceptions::{PyValueError, PyRuntimeError};
use std::collections::HashMap;
use crate::utils::logger::{info, error, debug};

/// Python 可序列化的值类型
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, bincode::Encode, bincode::Decode)]
pub enum PyBinValue {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<PyBinValue>),
    Dict(HashMap<String, PyBinValue>),
}

impl PyBinValue {
    /// 从 Python 对象创建 PyBinValue
    pub fn from_pyobj(obj: &PyAny) -> PyResult<Self> {
        if obj.is_none() {
            Ok(PyBinValue::None)
        } else if obj.is_instance_of::<PyBool>() {
            Ok(PyBinValue::Bool(obj.extract::<bool>()?))
        } else if obj.is_instance_of::<PyInt>() {
            Ok(PyBinValue::Int(obj.extract::<i64>()?))
        } else if obj.is_instance_of::<PyFloat>() {
            Ok(PyBinValue::Float(obj.extract::<f64>()?))
        } else if obj.is_instance_of::<PyString>() {
            Ok(PyBinValue::String(obj.extract::<String>()?))
        } else if obj.is_instance_of::<PyBytes>() {
            Ok(PyBinValue::Bytes(obj.extract::<Vec<u8>>()?))
        } else if obj.is_instance_of::<PyList>() {
            let list = obj.extract::<Vec<&PyAny>>()?;
            let mut result = Vec::new();
            for item in list {
                result.push(PyBinValue::from_pyobj(item)?);
            }
            Ok(PyBinValue::List(result))
        } else if obj.is_instance_of::<PyDict>() {
            let dict = obj.extract::<HashMap<String, &PyAny>>()?;
            let mut result = HashMap::new();
            for (key, value) in dict {
                result.insert(key, PyBinValue::from_pyobj(value)?);
            }
            Ok(PyBinValue::Dict(result))
        } else {
            Err(PyValueError::new_err("Unsupported Python type"))
        }
    }

    /// 转换为 Python 对象
    pub fn to_pyobj(&self, py: Python) -> PyResult<PyObject> {
        match self {
            PyBinValue::None => Ok(py.None()),
            PyBinValue::Bool(b) => Ok(b.to_object(py)),
            PyBinValue::Int(i) => Ok(i.to_object(py)),
            PyBinValue::Float(f) => Ok(f.to_object(py)),
            PyBinValue::String(s) => Ok(s.to_object(py)),
            PyBinValue::Bytes(b) => Ok(PyBytes::new(py, b).to_object(py)),
            PyBinValue::List(l) => {
                let list = PyList::empty(py);
                for item in l {
                    list.append(item.to_pyobj(py)?)?;
                }
                Ok(list.to_object(py))
            }
            PyBinValue::Dict(d) => {
                let dict = PyDict::new(py);
                for (key, value) in d {
                    dict.set_item(key, value.to_pyobj(py)?)?;
                }
                Ok(dict.to_object(py))
            }
        }
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        match bincode::encode_to_vec(self, bincode::config::standard()) {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("序列化失败: {}", e);
                Vec::new()
            }
        }
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        bincode::decode_from_slice(bytes, bincode::config::standard())
            .map(|(value, _)| value)
            .map_err(|e| e.into())
    }
}

/// Python 编码器
#[pyclass(name = "QuickEncoder")]
#[derive(Clone)]
pub struct PyQuickEncoder {
    // 简化的编码器，不需要复杂的状态
}

#[pymethods]
impl PyQuickEncoder {
    #[new]
    fn new() -> Self {
        Self {}
    }

    fn encode(&self, py: Python, obj: &PyAny) -> PyResult<PyObject> {
        let value = PyBinValue::from_pyobj(obj)?;
        let bytes = value.to_bytes();
        Ok(PyBytes::new(py, &bytes).to_object(py))
    }
}

/// Python 解码器
#[pyclass(name = "QuickDecoder")]
#[derive(Clone)]
pub struct PyQuickDecoder {
    // 简化的解码器，不需要复杂的状态
}

#[pymethods]
impl PyQuickDecoder {
    #[new]
    fn new() -> Self {
        Self {}
    }

    fn decode(&self, py: Python, data: &[u8]) -> PyResult<PyObject> {
        match PyBinValue::from_bytes(data) {
            Ok(value) => value.to_pyobj(py),
            Err(e) => Err(PyRuntimeError::new_err(format!("解码失败: {}", e))),
        }
    }
}

/// 编解码器
#[pyclass(name = "QuickCodec")]
#[derive(Clone)]
pub struct PyQuickCodec {
    encoder: PyQuickEncoder,
    decoder: PyQuickDecoder,
}

#[pymethods]
impl PyQuickCodec {
    #[new]
    pub fn new() -> PyResult<Self> {
        Ok(Self {
            encoder: PyQuickEncoder::new(),
            decoder: PyQuickDecoder::new(),
        })
    }

    fn encode(&self, py: Python, obj: &PyAny) -> PyResult<PyObject> {
        self.encoder.encode(py, obj)
    }

    fn decode(&self, py: Python, data: &[u8]) -> PyResult<PyObject> {
        self.decoder.decode(py, data)
    }
}

/// 注册编解码相关函数
pub fn register_codec_functions(module: &PyModule) -> PyResult<()> {
    module.add_class::<PyQuickEncoder>()?;
    module.add_class::<PyQuickDecoder>()?;
    module.add_class::<PyQuickCodec>()?;
    Ok(())
}