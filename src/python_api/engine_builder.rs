//! RAT Engine 构建器 Python 绑定
//! 
//! 提供与主库一致的 RatEngineBuilder 模式的 Python 接口

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::runtime::Runtime;
use crate::utils::logger::LogConfig;
use crate::engine::{RatEngine, RatEngineBuilder, ActualRatEngine};
use crate::server::Router;
use crate::server::config::SpaConfig;
use crate::python_api::server::PyRouter as PyRouterInner;

/// Python RAT Engine 构建器
/// 
/// 提供与主库完全一致的 Builder 模式 API
#[pyclass(name = "RatEngineBuilder")]
pub struct PyRatEngineBuilder {
    builder: RatEngineBuilder,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PyRatEngineBuilder {
    /// 创建新的构建器
    #[new]
    fn new() -> PyResult<Self> {
        let runtime = Arc::new(Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("创建运行时失败: {}", e))
        })?);
        
        Ok(Self {
            builder: RatEngine::builder(),
            runtime,
        })
    }
    
    /// 设置工作线程数
    fn worker_threads(&mut self, threads: usize) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.worker_threads(threads);
        Ok(())
    }
    
    /// 设置最大连接数
    fn max_connections(&mut self, connections: usize) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.max_connections(connections);
        Ok(())
    }
    
    /// 设置缓冲区大小
    fn buffer_size(&mut self, size: usize) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.buffer_size(size);
        Ok(())
    }
    
    /// 设置超时时间（秒）
    fn timeout(&mut self, timeout_secs: u64) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.timeout(std::time::Duration::from_secs(timeout_secs));
        Ok(())
    }
    
    /// 启用 Keep-Alive
    fn keepalive(&mut self, enabled: bool) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.keepalive(enabled);
        Ok(())
    }
    
    /// 启用 TCP_NODELAY
    fn tcp_nodelay(&mut self, enabled: bool) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.tcp_nodelay(enabled);
        Ok(())
    }
    
        
    /// 启用拥塞控制
    fn congestion_control(&mut self, enabled: bool, algorithm: String) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.congestion_control(enabled, algorithm);
        Ok(())
    }
    
    /// 配置 SPA 支持
    fn spa_config(&mut self, index_file: String) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.spa_config(index_file);
        Ok(())
    }
    
    /// 启用开发模式（自动生成自签名证书）
    /// 
    /// # 参数
    /// - hostnames: 主机名列表，如 ["localhost", "127.0.0.1"]
    fn enable_development_mode(&mut self, hostnames: Vec<String>) -> PyResult<()> {
        let runtime = self.runtime.clone();
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        
        let result = runtime.block_on(async move {
            builder.enable_development_mode(hostnames).await
        });
        
        match result {
            Ok(b) => {
                self.builder = b;
                Ok(())
            }
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("启用开发模式失败: {}", e)))
        }
    }
    
    /// 配置手动证书文件
    /// 
    /// # 参数
    /// - cert_path: 证书文件路径
    /// - key_path: 私钥文件路径  
    /// - ca_path: CA 证书文件路径（可选）
    fn with_certificate_files(&mut self, cert_path: String, key_path: String, ca_path: Option<String>) -> PyResult<()> {
        let runtime = self.runtime.clone();
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        
        let result = runtime.block_on(async move {
            builder.with_certificate_files(cert_path, key_path, ca_path).await
        });
        
        match result {
            Ok(b) => {
                self.builder = b;
                Ok(())
            }
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("配置证书文件失败: {}", e)))
        }
    }
    
    /// 配置 ACME 自动证书管理
    /// 
    /// # 参数
    /// - domain: 域名
    /// - email: ACME 账户邮箱
    /// - cloudflare_token: Cloudflare API 令牌
    /// - cert_dir: 证书存储目录
    /// - renewal_days: 续期天数阈值
    /// - production: 是否使用生产环境（false 为沙盒环境）
    fn cert_manager_acme(
        &mut self, 
        domain: String, 
        email: String, 
        cloudflare_token: String, 
        cert_dir: String, 
        renewal_days: u32, 
        production: bool
    ) -> PyResult<()> {
        let runtime = self.runtime.clone();
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        
        let result = runtime.block_on(async move {
            builder.cert_manager_acme(domain, email, cloudflare_token, cert_dir, renewal_days, production).await
        });
        
        match result {
            Ok(b) => {
                self.builder = b;
                Ok(())
            }
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("配置 ACME 证书失败: {}", e)))
        }
    }
    
    /// 配置路由
    /// 
    /// # 参数
    /// - router: PyRouter 实例
    fn with_router(&mut self, router: &PyRouterInner) -> PyResult<()> {
        // 从 PyRouter 中获取内部的 Router
        let router_ptr = router.get_inner_router()?;
        let inner_router = unsafe {
            std::ptr::read(router_ptr as *const Router)
        };
        
        // 使用 with_router 闭包方法
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        self.builder = builder.with_router(|_r| inner_router);
        Ok(())
    }
    
    /// 构建并启动服务器
    /// 
    /// # 参数  
    /// - host: 监听主机地址
    /// - port: 监听端口
    /// 
    /// # 返回
    /// - PyRatEngine: 已启动的引擎实例
    fn build_and_start(&mut self, host: String, port: u16) -> PyResult<PyRatEngine> {
        let runtime = self.runtime.clone();
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        
        let result = runtime.block_on(async move {
            builder.build_and_start(host, port).await
        });
        
        match result {
            Ok(engine) => Ok(PyRatEngine {
                engine: Arc::new(engine),
                runtime,
            }),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("启动服务器失败: {}", e)))
        }
    }
    
    /// 仅构建引擎（不启动）
    fn build(&mut self) -> PyResult<PyRatEngine> {
        let builder = std::mem::replace(&mut self.builder, RatEngine::builder());
        match builder.build() {
            Ok(engine) => Ok(PyRatEngine {
                engine: Arc::new(engine),
                runtime: self.runtime.clone(),
            }),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("构建引擎失败: {}", e)))
        }
    }
}

/// Python RAT Engine 实例
#[pyclass(name = "RatEngine")]
pub struct PyRatEngine {
    engine: Arc<ActualRatEngine>,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PyRatEngine {
    /// 启动服务器
    /// 
    /// # 参数
    /// - host: 监听主机地址
    /// - port: 监听端口
    fn start(&self, host: String, port: u16) -> PyResult<()> {
        let engine = self.engine.clone();
        let runtime = self.runtime.clone();
        
        let result = runtime.block_on(async move {
            engine.start(host, port).await
        });
        
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("启动服务器失败: {}", e)))
        }
    }
    
    /// 获取工作线程数
    #[getter]
    fn get_workers(&self) -> PyResult<usize> {
        Ok(self.engine.get_workers())
    }
    
    /// 获取最大连接数
    #[getter]
    fn get_max_connections(&self) -> PyResult<usize> {
        Ok(self.engine.get_max_connections())
    }
    
    /// 获取监听主机
    #[getter]
    fn get_host(&self) -> PyResult<String> {
        Ok(self.engine.get_host().to_string())
    }
    
    /// 获取监听端口
    #[getter]
    fn get_port(&self) -> PyResult<u16> {
        Ok(self.engine.get_port())
    }
}

/// 注册引擎构建器模块
pub fn register_engine_builder_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    let engine_module = PyModule::new(py, "engine")?;
    
    engine_module.add_class::<PyRatEngineBuilder>()?;
    engine_module.add_class::<PyRatEngine>()?;
    
    // 添加便捷函数
    engine_module.add_function(wrap_pyfunction!(create_builder, engine_module)?)?;
    
    parent_module.add_submodule(engine_module)?;
    Ok(())
}

/// 创建构建器的便捷函数
#[pyfunction]
pub fn create_builder() -> PyResult<PyRatEngineBuilder> {
    PyRatEngineBuilder::new()
}