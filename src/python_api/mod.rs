//! RAT Engine Python API 模块
//! 
//! 完全重构的 Python 胶水层，参考 streaming_demo.rs 的架构模式
//! 提供高性能的 HTTP 服务器和客户端功能

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use crate::utils::logger::{LogConfig, LogLevel, LogOutput, Logger};
use std::path::PathBuf;

// 子模块
pub mod server;
#[cfg(feature = "client")]
pub mod client; // 新增客户端模块
pub mod streaming;
pub mod codec;
pub mod handlers;
pub mod http;
pub mod smart_transfer; // 新增智能传输模块
pub mod congestion_control; // 新增拥塞控制模块
pub mod compression; // 新增压缩模块
pub mod cert_manager; // 证书管理模块
pub mod grpc_queue_bridge; // gRPC 队列桥接模块（统一架构）
pub mod http_queue_bridge; // HTTP 队列桥接模块（统一架构）
pub mod response_converter; // HTTP 响应转换模块
pub mod engine_builder; // 新的引擎构建器模块

// 重新导出主要类型
pub use server::{PyRouter, PyServer};
pub use client::{PyClientManager}; // 新的客户端管理器
pub use streaming::{PySseResponse, PySseSender, PyChunkedResponse};
pub use engine_builder::{PyRatEngine, PyRatEngineBuilder}; // 新的引擎构建器
pub use codec::{PyQuickCodec, PyQuickEncoder, PyQuickDecoder};
pub use handlers::{PyHandler, PyDataPipeline};
pub use http::{HttpRequest, HttpResponse, HttpMethod, ResponseType, TypedResponse};
pub use smart_transfer::{PySmartTransferRouter, PyTransferResult, PyTransferStrategy}; // 新增智能传输类型
pub use congestion_control::PyCongestionController; // 新增拥塞控制类型
pub use compression::{PyCompressionConfig, PyCompressionType}; // 新增压缩类型
pub use cert_manager::PyCertManagerConfig; // 新增证书管理类型

/// Python 服务器配置
#[pyclass(name = "ServerConfig")]
pub struct PyServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_connections: usize,
    pub timeout_seconds: u64,
}

impl PyServerConfig {
    /// 转换为服务器配置类型
    pub fn to_server_config(&self) -> crate::server::config::ServerConfig {
        use crate::server::config::ServerConfig;
        use crate::utils::logger::LogConfig;
        use std::net::SocketAddr;
        use std::str::FromStr;
        
        let addr = SocketAddr::from_str(&format!("{}:{}", self.host, self.port))
            .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], self.port)));
        
        ServerConfig::with_timeouts(
            addr,
            self.workers,
            Some(std::time::Duration::from_secs(self.timeout_seconds)),
            Some(std::time::Duration::from_secs(self.timeout_seconds))
        )
    }
}

#[pymethods]
impl PyServerConfig {
    #[new]
    #[pyo3(signature = (host = "127.0.0.1".to_string(), port = 8000, workers = num_cpus::get(), max_connections = 1000, timeout_seconds = 30))]
    fn new(
        host: String,
        port: u16,
        workers: usize,
        max_connections: usize,
        timeout_seconds: u64,
    ) -> Self {
        Self {
            host,
            port,
            workers,
            max_connections,
            timeout_seconds,
        }
    }
    
    fn __repr__(&self) -> String {
        format!(
            "ServerConfig(host='{}', port={}, workers={}, max_connections={}, timeout_seconds={})",
            self.host, self.port, self.workers, self.max_connections, self.timeout_seconds
        )
    }
}


/// Python 日志配置函数 - 极端修改版本，不进行任何初始化
#[pyfunction]
fn _configure_logging(
    _level: Option<String>,
    _enable_colors: Option<bool>,
    _enable_emoji: Option<bool>,
    _show_timestamp: Option<bool>,
    _show_module: Option<bool>,
    _log_file: Option<String>,
) -> PyResult<()> {
    // 极端修改：完全不初始化日志系统，看看会发生什么
    println!("🚨 极端修改：configure_logging 被调用但未进行任何初始化");
    Ok(())
}

/// 获取 rat_memcache 版本信息
///
/// 通过读取依赖项的 Cargo.toml 文件来获取版本信息，避免硬编码
#[pyfunction]
fn get_rat_memcache_version() -> PyResult<String> {
    // rat_memcache 的版本信息，这个会在构建时从依赖项的 Cargo.toml 中读取
    // 注意：这个版本号需要与 Cargo.toml 中的依赖项版本保持一致
    Ok("0.2.2".to_string())
}

/// 注册 Python API 模块到 PyO3
pub fn register_python_api_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    // 注册服务器相关类
    parent_module.add_class::<PyServerConfig>()?;
    parent_module.add_class::<PyRouter>()?;
    parent_module.add_class::<PyServer>()?;
    
    // 注册证书管理相关类
    parent_module.add_class::<PyCertManagerConfig>()?;
    
    // 注册客户端模块
    client::register_client_module(py, parent_module)?;
    
    // 注册流式响应相关类
    parent_module.add_class::<PySseResponse>()?;
    parent_module.add_class::<PySseSender>()?;
    parent_module.add_class::<PyChunkedResponse>()?;
    
    // 注册编解码相关类
    parent_module.add_class::<PyQuickCodec>()?;
    parent_module.add_class::<PyQuickEncoder>()?;
    parent_module.add_class::<PyQuickDecoder>()?;
    
    // 注册处理器相关类
    parent_module.add_class::<PyHandler>()?;
    parent_module.add_class::<PyDataPipeline>()?;
    
    // 注册 HTTP 相关类
    parent_module.add_class::<HttpRequest>()?;
    parent_module.add_class::<HttpResponse>()?;
    parent_module.add_class::<HttpMethod>()?;
    
    // 注册智能传输相关类
    parent_module.add_class::<PySmartTransferRouter>()?;
    parent_module.add_class::<PyTransferResult>()?;
    parent_module.add_class::<PyTransferStrategy>()?;
    
    // 注册拥塞控制相关类
    parent_module.add_class::<PyCongestionController>()?;
    
    // 注册工具函数
    streaming::register_streaming_functions(parent_module)?;
    handlers::register_handler_functions(parent_module)?;
    smart_transfer::register_smart_transfer_functions(parent_module)?; // 新增智能传输函数
    congestion_control::register_congestion_control_functions(parent_module)?; // 新增拥塞控制函数
    compression::register_compression_module(py, parent_module)?; // 新增压缩函数
    
    // 注册 gRPC 队列桥接模块
    grpc_queue_bridge::register_grpc_queue_bridge_module(parent_module)?;
    
    // 注册引擎构建器模块
    engine_builder::register_engine_builder_module(py, parent_module)?;
    
    // 同时在主模块中导出引擎构建器类，方便直接访问
    parent_module.add_class::<engine_builder::PyRatEngineBuilder>()?;
    parent_module.add_class::<engine_builder::PyRatEngine>()?;
    parent_module.add_function(wrap_pyfunction!(engine_builder::create_builder, parent_module)?)?;
    
    // 注意：configure_logging 函数已移除，现在通过 Server.configure_logging(json_string) 配置

    // 注册版本信息函数
    parent_module.add_function(wrap_pyfunction!(get_rat_memcache_version, parent_module)?)?;

    Ok(())
}