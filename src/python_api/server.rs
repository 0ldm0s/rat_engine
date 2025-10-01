//! Python 服务器模块
//! 
//! 提供与 streaming_demo.rs 一致的路由器和服务器管理功能

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyFunction};
use std::sync::Arc;
use hyper::{Request, Method, StatusCode, Response};
use hyper::body::{Incoming, Bytes};
use http_body_util::{Full, BodyExt};
use futures_util::TryFutureExt;
use crate::server::{
    Router,
    streaming::{SseResponse, ChunkedResponse, StreamingResponse, StreamingBody},
    config::ServerConfig,
    http_request::HttpRequest
};
// 移除已废弃的 ServerConfigData 导入
use std::net::SocketAddr;
use std::str::FromStr;
use hyper::http::Error as HttpError;
use tokio::time::{sleep, Duration};
use serde_json::json;
use std::pin::Pin;
use std::future::Future;
// 移除 rat_quick_threshold 依赖，使用原生实现
use crate::python_api::codec::{PyQuickCodec, PyBinValue};
use crate::python_api::streaming::{PySseResponse, PyChunkedResponse};
use super::PyServerConfig;
use super::congestion_control::PyCongestionController;
use crate::utils::logger::{info, warn, debug, error};
use crate::python_api::grpc_queue_bridge::PyGrpcMainThread;
use crate::python_api::http_queue_bridge::{PyHttpMainThread, PyHttpHandler};
use crate::python_api::cert_manager::PyCertManagerConfig;
use std::collections::HashMap;
use regex::Regex;
use pyo3::Python;

/// Python 路由器类
#[pyclass(name = "Router")]
pub struct PyRouter {
    router: Router,
    codec: PyQuickCodec,
    http_main_thread: Option<Arc<PyHttpMainThread>>,
}

impl PyRouter {
    fn execute_python_handler(
        path: String,
        req: Request<()>,
        handler: PyObject,
        codec: PyQuickCodec,
        body_bytes: Vec<u8>
    ) -> Result<ChunkedResponse, PyErr> {
        Python::with_gil(|py| {
            // 从请求URI路径中重新提取参数（临时方案，直到主库支持参数传递）
            let path_params = extract_path_params(&path, req.uri().path());
            
            let request_data_dict = prepare_request_data(
                py, 
                &req, 
                &codec, 
                &body_bytes, 
                Some(&path_params)
            )?;
            
            let request_data = request_data_dict.to_object(py);
            
            let args = pyo3::types::PyTuple::new(py, &[request_data]);
            let result = handler.call(py, args, None)?;
            
            handle_python_chunked_response(py, result, &codec)
        })
    }
}

#[pymethods]
impl PyRouter {
    #[new]
    fn new() -> PyResult<Self> {
        // 严格装饰器模式检查 - 确保只有 RatApp 可以创建 PyRouter 实例
        Python::with_gil(|py| {
            let traceback = py.import("traceback")?;
            let stack = traceback.call_method0("extract_stack")?;
            let stack_list: Vec<&pyo3::types::PyAny> = stack.extract()?;
            
            let mut ratapp_found = false;
            
            // 检查调用栈中是否有 RatApp 的 __init__ 方法
            for frame in stack_list {
                let filename: String = frame.getattr("filename")?.extract()?;
                let name: String = frame.getattr("name")?.extract()?;
                
                // 检查是否来自 web_app.py 的 RatApp.__init__ 方法
                if filename.contains("web_app.py") && name == "__init__" {
                    ratapp_found = true;
                    break;
                }
            }
            
            if !ratapp_found {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "🚫 严格装饰器模式：PyRouter 只能通过 RatApp 创建！\n"
                    .to_owned() + 
                    "请使用 RatApp 类来创建应用实例，严禁直接实例化 PyRouter！\n" +
                    "正确用法：app = RatApp('my_app')"
                ));
            }
            
            Ok(())
        })?;
        
        Ok(Self {
            router: Router::new(),
            codec: PyQuickCodec::new()?,
            http_main_thread: None,
        })
    }
    
    /// 获取内部 Router 的引用（用于 Builder 模式）
    /// 
    /// # 注意
    /// 这是一个内部方法，仅供 RatEngineBuilder 使用
    /// 
    /// # 返回值
    /// - usize: Router 的内存地址（用于 unsafe 操作）
    pub fn get_inner_router(&self) -> PyResult<usize> {
        Ok(&self.router as *const Router as usize)
    }
    
    /// 启用压缩功能
    /// 
    /// 参数:
    ///     min_size: 压缩配置对象或最小压缩大小（字节），默认为 1024
    ///     level: 压缩级别（1-9），默认为 6
    ///     enable_gzip: 是否启用 Gzip 压缩，默认为 True
    ///     enable_deflate: 是否启用 Deflate 压缩，默认为 True
    ///     enable_brotli: 是否启用 Brotli 压缩，默认为 True
    ///     enable_zstd: 是否启用 Zstd 压缩，默认为 True
    ///     enable_lz4: 是否启用 LZ4 压缩，默认为 False
    ///     excluded_content_types: 排除的内容类型列表，默认为常见的已压缩类型
    ///     excluded_extensions: 排除的文件扩展名列表，默认为常见的已压缩扩展名
    /// 
    /// 返回:
    ///     self: 返回自身，支持链式调用
    #[pyo3(signature = (
        *, 
        min_size=1024, 
        level=6, 
        enable_gzip=true, 
        enable_deflate=true, 
        enable_brotli=true, 
        enable_zstd=true, 
        enable_lz4=false, 
        excluded_content_types=None, 
        excluded_extensions=None
    ))]
    fn enable_compression(
        &mut self,
        min_size: usize,
        level: Option<u32>,
        enable_gzip: Option<bool>,
        enable_deflate: Option<bool>,
        enable_brotli: Option<bool>,
        enable_zstd: Option<bool>,
        enable_lz4: Option<bool>,
        excluded_content_types: Option<Vec<String>>,
        excluded_extensions: Option<Vec<String>>,
    ) -> PyResult<()> {
        let min_size_value = min_size;
        let level = level.unwrap_or(6);
        let enable_gzip = enable_gzip.unwrap_or(true);
        let enable_deflate = enable_deflate.unwrap_or(true);
        let enable_brotli = enable_brotli.unwrap_or(true);
        let enable_zstd = enable_zstd.unwrap_or(true);
        let enable_lz4 = enable_lz4.unwrap_or(false);
        
        let mut enabled_algorithms = Vec::new();
        if enable_gzip { enabled_algorithms.push(crate::compression::CompressionType::Gzip); }
        if enable_deflate { enabled_algorithms.push(crate::compression::CompressionType::Deflate); }
        if enable_brotli { enabled_algorithms.push(crate::compression::CompressionType::Brotli); }
        if enable_zstd { enabled_algorithms.push(crate::compression::CompressionType::Zstd); }
        if enable_lz4 { enabled_algorithms.push(crate::compression::CompressionType::Lz4); }

        let config = crate::compression::CompressionConfig {
            enabled_algorithms,
            min_size: min_size_value,
            level,
            excluded_content_types: excluded_content_types.unwrap_or_default().into_iter().collect(),
            excluded_extensions: excluded_extensions.unwrap_or_default().into_iter().collect(),
            enable_smart_compression: false,
        };
          
        // 启用压缩
        self.router.enable_compression(config);
        
        Ok(())
    }
    
    /// 禁用压缩功能
    /// 
    /// 返回:
    ///     None
    fn disable_compression(&mut self) -> PyResult<()> {
        // 注意：Router 没有 disable_compression 方法
        // 压缩功能一旦启用就无法禁用，这是设计决定
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "压缩功能一旦启用就无法禁用，这是框架的设计决定"
        ))
    }
    
    /// 启用 H2C (HTTP/2 over cleartext) 支持
    /// 
    /// 返回:
    ///     None
    fn enable_h2c(&mut self) -> PyResult<()> {
        self.router.enable_h2c();
        Ok(())
    }
    
    /// 启用 HTTP/2 支持
    /// 
    /// 返回:
    ///     None
    fn enable_h2(&mut self) -> PyResult<()> {
        self.router.enable_h2();
        Ok(())
    }
    
    /// 检查是否启用了 H2C
    /// 
    /// 返回:
    ///     bool: 是否启用了 H2C
    fn is_h2c_enabled(&self) -> bool {
        self.router.is_h2c_enabled()
    }
    
    /// 检查是否启用了 HTTP/2
    /// 
    /// 返回:
    ///     bool: 是否启用了 HTTP/2
    fn is_h2_enabled(&self) -> bool {
        self.router.is_h2_enabled()
    }
    
    /// 启用缓存功能
    ///
    /// 参数:
    ///     config_json: 缓存配置的JSON字符串，包含完整的缓存配置
    ///
    /// JSON配置格式示例：
    /// {
    ///   "l1": {
    ///     "max_memory": 67108864,
    ///     "max_entries": 1000,
    ///     "eviction_strategy": "Lru"
    ///   },
    ///   "l2": {
    ///     "enable_l2_cache": true,
    ///     "data_dir": "./cache_l2",
    ///     "clear_on_startup": false,
    ///     "max_disk_size": 1073741824,
    ///     "write_buffer_size": 67108864,
    ///     "max_write_buffer_number": 3,
    ///     "block_cache_size": 33554432,
    ///     "background_threads": 2,
    ///     "enable_lz4": true,
    ///     "compression_threshold": 128,
    ///     "compression_max_threshold": 1048576,
    ///     "compression_level": 6,
    ///     "cache_size_mb": 512,
    ///     "max_file_size_mb": 1024,
    ///     "smart_flush_enabled": true,
    ///     "smart_flush_base_interval_ms": 100,
    ///     "smart_flush_min_interval_ms": 20,
    ///     "smart_flush_max_interval_ms": 500,
    ///     "smart_flush_write_rate_threshold": 10000,
    ///     "smart_flush_accumulated_bytes_threshold": 4194304,
    ///     "cache_warmup_strategy": "Recent",
    ///     "l2_write_strategy": "write_through",
    ///     "l2_write_threshold": 1024,
    ///     "l2_write_ttl_threshold": 300
    ///   },
    ///   "ttl": {
    ///     "expire_seconds": 60,
    ///     "cleanup_interval": 300,
    ///     "max_cleanup_entries": 1000,
    ///     "lazy_expiration": true,
    ///     "active_expiration": true
    ///   },
    ///   "performance": {
    ///     "worker_threads": 4,
    ///     "enable_concurrency": true,
    ///     "read_write_separation": true,
    ///     "batch_size": 100,
    ///     "enable_warmup": true,
    ///     "large_value_threshold": 512
    ///   }
    /// }
    ///
    /// 返回:
    ///     None
    fn enable_cache(&mut self, config_json: String) -> PyResult<()> {
        // 启用缓存功能

        // 创建新的 tokio 运行时执行异步操作
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("创建运行时失败: {}", e)))?;

        let mut router = &mut self.router;

        rt.block_on(async {
            // 解析JSON配置
            let config: serde_json::Value = serde_json::from_str(&config_json)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON解析失败: {}", e)))?;

            // 提取并构建L1配置
            let l1_config = config.get("l1")
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少l1配置"))?;

            let l1 = crate::cache::L1Config {
                max_memory: l1_config.get("max_memory")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少max_memory"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("max_memory必须是数字"))? as usize,
                max_entries: l1_config.get("max_entries")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少max_entries"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("max_entries必须是数字"))? as usize,
                eviction_strategy: match l1_config.get("eviction_strategy")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Lru") {
                    "Lru" => crate::cache::EvictionStrategy::Lru,
                    "Lfu" => crate::cache::EvictionStrategy::Lfu,
                    "Fifo" => crate::cache::EvictionStrategy::Fifo,
                    _ => return Err(pyo3::exceptions::PyValueError::new_err("不支持的eviction_strategy")),
                },
            };
            // L1配置构建完成

            // 提取并构建TTL配置
            let ttl_config = config.get("ttl")
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少ttl配置"))?;

            let ttl = crate::cache::TtlConfig {
                expire_seconds: ttl_config.get("expire_seconds")
                    .and_then(|v| v.as_u64()),
                cleanup_interval: ttl_config.get("cleanup_interval")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少cleanup_interval"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("cleanup_interval必须是数字"))?,
                max_cleanup_entries: ttl_config.get("max_cleanup_entries")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少max_cleanup_entries"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("max_cleanup_entries必须是数字"))? as usize,
                lazy_expiration: ttl_config.get("lazy_expiration")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少lazy_expiration"))?
                    .as_bool()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("lazy_expiration必须是布尔值"))?,
                active_expiration: ttl_config.get("active_expiration")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少active_expiration"))?
                    .as_bool()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("active_expiration必须是布尔值"))?,
            };

            // 提取并构建性能配置
            let perf_config = config.get("performance")
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少performance配置"))?;

            let performance = crate::cache::PerformanceConfig {
                worker_threads: perf_config.get("worker_threads")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少worker_threads"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("worker_threads必须是数字"))? as usize,
                enable_concurrency: perf_config.get("enable_concurrency")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少enable_concurrency"))?
                    .as_bool()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("enable_concurrency必须是布尔值"))?,
                read_write_separation: perf_config.get("read_write_separation")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少read_write_separation"))?
                    .as_bool()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("read_write_separation必须是布尔值"))?,
                batch_size: perf_config.get("batch_size")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少batch_size"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("batch_size必须是数字"))? as usize,
                enable_warmup: perf_config.get("enable_warmup")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少enable_warmup"))?
                    .as_bool()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("enable_warmup必须是布尔值"))?,
                large_value_threshold: perf_config.get("large_value_threshold")
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("缺少large_value_threshold"))?
                    .as_u64()
                    .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("large_value_threshold必须是数字"))? as usize,
            };
            // 性能配置构建完成

  
            // 保存TTL配置用于后续使用
            let expire_seconds = ttl.expire_seconds;

            // 构建缓存
            let mut builder = crate::cache::CacheBuilder::new()
                .with_l1_config(l1)
                .with_ttl_config(ttl)
                .with_performance_config(performance);

            // 如果有L2配置，添加L2配置
            if let Some(l2_config) = config.get("l2") {

                let l2 = crate::cache::L2Config {
                    enable_l2_cache: l2_config.get("enable_l2_cache")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    data_dir: l2_config.get("data_dir")
                        .and_then(|v| v.as_str())
                        .map(|s| std::path::PathBuf::from(s)),
                    clear_on_startup: l2_config.get("clear_on_startup")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    max_disk_size: l2_config.get("max_disk_size")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1024 * 1024 * 1024),
                    write_buffer_size: l2_config.get("write_buffer_size")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(64 * 1024 * 1024) as usize,
                    max_write_buffer_number: l2_config.get("max_write_buffer_number")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(3) as i32,
                    block_cache_size: l2_config.get("block_cache_size")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(32 * 1024 * 1024) as usize,
                    background_threads: l2_config.get("background_threads")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(2) as i32,
                    enable_lz4: l2_config.get("enable_lz4")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    compression_threshold: l2_config.get("compression_threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(128) as usize,
                    compression_max_threshold: l2_config.get("compression_max_threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1024 * 1024) as usize,
                    compression_level: l2_config.get("compression_level")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(6) as i32,
                    cache_size_mb: l2_config.get("cache_size_mb")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(512) as usize,
                    max_file_size_mb: l2_config.get("max_file_size_mb")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1024) as usize,
                    smart_flush_enabled: l2_config.get("smart_flush_enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    smart_flush_base_interval_ms: l2_config.get("smart_flush_base_interval_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(100) as usize,
                    smart_flush_min_interval_ms: l2_config.get("smart_flush_min_interval_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(20) as usize,
                    smart_flush_max_interval_ms: l2_config.get("smart_flush_max_interval_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(500) as usize,
                    smart_flush_write_rate_threshold: l2_config.get("smart_flush_write_rate_threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(10000) as usize,
                    smart_flush_accumulated_bytes_threshold: l2_config.get("smart_flush_accumulated_bytes_threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(4 * 1024 * 1024) as usize,
                    cache_warmup_strategy: match l2_config.get("cache_warmup_strategy")
                        .and_then(|v| v.as_str())
                        .unwrap_or("None") {
                        "None" => crate::cache::CacheWarmupStrategy::None,
                        "Recent" => crate::cache::CacheWarmupStrategy::Recent,
                        "Frequent" => crate::cache::CacheWarmupStrategy::Frequent,
                        "Full" => crate::cache::CacheWarmupStrategy::Full,
                        _ => return Err(pyo3::exceptions::PyValueError::new_err("不支持的cache_warmup_strategy")),
                    },
                    zstd_compression_level: l2_config.get("zstd_compression_level")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as i32),
                    l2_write_strategy: l2_config.get("l2_write_strategy")
                        .and_then(|v| v.as_str())
                        .unwrap_or("write_through")
                        .to_string(),
                    l2_write_threshold: l2_config.get("l2_write_threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1024) as usize,
                    l2_write_ttl_threshold: l2_config.get("l2_write_ttl_threshold")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(300),
                };
                builder = builder.with_l2_config(l2);
            } else {
            }

            // 构建缓存实例
            let cache = match builder.build().await {
                Ok(cache) => cache,
                Err(e) => {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        format!("创建缓存失败: {}", e)
                    ));
                }
            };

            // 创建多版本缓存管理器配置
            let version_manager_config = config.get("version_manager");

            let version_config = crate::server::cache_version_manager::CacheVersionManagerConfig {
                enable_precompression: version_manager_config
                    .and_then(|v| v.get("enable_precompression"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                supported_encodings: version_manager_config
                    .and_then(|v| v.get("supported_encodings"))
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_else(|| vec![
                        "br".to_string(),
                        "gzip".to_string(),
                        "deflate".to_string(),
                        "identity".to_string(),
                    ]),
                precompression_threshold: version_manager_config
                    .and_then(|v| v.get("precompression_threshold"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1024) as usize,
                enable_stats: version_manager_config
                    .and_then(|v| v.get("enable_stats"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                enable_smart_precompression: version_manager_config
                    .and_then(|v| v.get("enable_smart_precompression"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            };


            // 创建多版本缓存管理器
            let version_manager = crate::server::cache_version_manager::CacheVersionManager::with_cache_and_config(
                cache.clone(),
                version_config.clone(),
                expire_seconds
            );

            // 创建缓存中间件
            let cache_middleware = std::sync::Arc::new(
                crate::server::cache_middleware_impl::CacheMiddlewareImpl::new_multi_version(version_manager)
            );

            // 启用路由器的缓存功能
            router.enable_cache(cache_middleware);


            Ok(())
        })
    }

  
    /// 禁用缓存功能
    /// 
    /// 返回:
    ///     None
    fn disable_cache(&mut self) -> PyResult<()> {
        // 注意：Router 没有 disable_cache 方法
        // 缓存功能一旦启用就无法禁用，这是设计决定
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "缓存功能一旦启用就无法禁用，这是框架的设计决定"
        ))
    }
    
    /// 启用多版本缓存管理器
    /// 
    /// 参数:
    ///     max_encoding_versions: 最大编码版本数，默认为 5
    ///     enable_precompression: 是否启用预压缩，默认为 true
    ///     hot_encoding_threshold: 热点编码阈值（使用率），默认为 0.1
    ///     store_original_data: 是否存储原始数据，默认为 true
    ///     cleanup_age_threshold: 清理策略的年龄阈值（秒），默认为 3600
    ///     cleanup_idle_threshold: 清理策略的空闲时间阈值（秒），默认为 1800
    /// 
    /// 返回:
    ///     None
    #[pyo3(signature = (
        max_encoding_versions=5,
        enable_precompression=true,
        hot_encoding_threshold=0.1,
        store_original_data=true,
        cleanup_age_threshold=3600,
        cleanup_idle_threshold=1800
    ))]
    fn enable_version_manager(
        &mut self,
        max_encoding_versions: usize,
        enable_precompression: bool,
        hot_encoding_threshold: f64,
        store_original_data: bool,
        cleanup_age_threshold: u64,
        cleanup_idle_threshold: u64,
    ) -> PyResult<()> {
        // 创建多版本缓存管理器配置
        let config = crate::server::cache_version_manager::CacheVersionManagerConfig {
            enable_precompression,
            supported_encodings: vec![
                "lz4".to_string(),
                "zstd".to_string(),
                "br".to_string(),
                "gzip".to_string(),
                "deflate".to_string(),
                "identity".to_string(),
            ],
            precompression_threshold: 1024,
            enable_stats: false,
            enable_smart_precompression: true,
        };

            // 多版本缓存管理器配置完成
        
        Ok(())
    }
    
    /// 初始化 HTTP 队列桥接适配器
    pub fn initialize_http_queue_bridge(&mut self, callback: PyObject) -> PyResult<()> {
        let mut main_thread = PyHttpMainThread::new();
        main_thread.initialize_queue_bridge(None)?;
        main_thread.set_callback(callback)?;
        main_thread.start()?;
        
        self.http_main_thread = Some(Arc::new(main_thread));
        info!("✅ [PyRouter] HTTP 队列桥接适配器初始化成功");
        Ok(())
    }
    
    /// 添加 SSE 路由
    /// 
    /// Args:
    ///     method: HTTP 方法 ("GET", "POST", etc.)
    ///     path: 路由路径
    ///     handler: Python 处理函数，接收请求数据，返回 SSE 响应
    fn add_sse_route(&mut self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let method = match method.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("不支持的 HTTP 方法")),
        };
        
        let path = path.to_string();
        let path_for_closure = path.clone();
        let codec = self.codec.clone();
        
        // 创建一个自定义的流式处理器，能够接收路径参数
        let streaming_handler = move |req: HttpRequest, path_params: HashMap<String, String>| -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> {
            let handler = handler.clone();
            let codec = codec.clone();
            let value = path_for_closure.clone();
            
            Box::pin(async move {
                // 创建 SSE 响应
                let sse = SseResponse::new();
                
                // 在后台任务中调用 Python 处理函数
                let sender = sse.get_sender();
                let codec_clone = codec.clone();
                
                tokio::spawn(async move {
                    // 使用 spawn_blocking 来处理可能阻塞的 Python 代码
                    let sender_clone = sender.clone();
                    let codec_clone2 = codec_clone.clone();
                    
                    tokio::task::spawn_blocking(move || {
                        Python::with_gil(|py| {
                            // 准备请求数据
                            let request_data_dict = prepare_request_data_from_http_request(py, &req, &codec_clone2, Some(&path_params))?;
                            let request_data = request_data_dict.to_object(py);
                            
                            // 准备参数
                             let mut args_vec: Vec<PyObject> = vec![request_data];
                             
                             // 添加路径参数（按顺序）
                             for (_, value) in &path_params {
                                 args_vec.push(value.into_py(py));
                             }
                            
                            // 调用 Python 处理函数
                            let args = pyo3::types::PyTuple::new(py, &args_vec);
                            let result = handler.call(py, args, None);
                                
                            match result {
                                Ok(response) => {
                                    // 处理 Python 函数返回的响应
                                    if let Err(e) = handle_python_sse_response(py, response, &sender_clone, &codec_clone2) {
                                        eprintln!("处理 SSE 响应时出错: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("调用 Python 处理函数时出错: {:?}", e);
                                    let error_msg = format!("event: error\ndata: {}\n\n", e);
                                    let _ = sender_clone.send(Ok(hyper::body::Frame::data(
                                        hyper::body::Bytes::from(error_msg)
                                    )));
                                }
                            }
                            
                            Ok::<(), PyErr>(())
                        }).unwrap_or_else(|e| {
                            eprintln!("Python GIL 错误: {:?}", e);
                        });
                    }).await.unwrap_or_else(|e| {
                        eprintln!("spawn_blocking 错误: {:?}", e);
                    });
                });
                
                // 使用 SseResponse 的 build 方法构建响应
                sse.build()
            })
        };
        
        // 使用流式路由注册，这样可以避免重复的路径参数提取
        self.router.add_streaming_route(method, &path, streaming_handler);
        
        Ok(())
    }
    
    /// 添加分块传输路由
    /// 
    /// # 参数
    /// - `method`: HTTP 方法 (GET/POST/PUT/DELETE等)
    /// - `path`: 路由路径
    /// - `handler`: Python处理函数
    /// 
    /// # 返回值
    /// - PyResult<()>: 成功返回Ok(()), 失败返回PyErr
    fn add_chunked_route(&mut self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let method = match method.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("不支持的 HTTP 方法")),
        };
        
        let path = path.to_string();
        let path_for_closure = path.clone();
        let codec = self.codec.clone();
        
        // 创建一个自定义的流式处理器，能够接收路径参数
        let streaming_handler = move |req: HttpRequest, path_params: HashMap<String, String>| -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> {
            let handler = handler.clone();
            let codec = codec.clone();
            let value = path_for_closure.clone();
            
            Box::pin(async move {
                // 调用 Python 处理器，传递主库提供的路径参数
                match PyRouter::execute_python_chunked_handler(value, req, handler, codec, path_params) {
                    Ok(chunked_response) => {
                        // 使用 ChunkedResponse 的 build 方法构建响应
                        chunked_response.build()
                    }
                    Err(_) => {
                        let error_response = ChunkedResponse::new().add_chunk("Internal Server Error".to_string());
                        error_response.build()
                    }
                }
            })
        };
        
        // 使用流式路由注册，这样可以避免重复的路径参数提取
        self.router.add_streaming_route(method, &path, streaming_handler);
        
        Ok(())
    }
    
    /// 添加普通路由 - 队列桥接模式
    /// 
    /// 使用队列桥接模式而不是传统的同步调用，彻底避免 GIL 死锁问题
    fn add_route(&mut self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let method = match method.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("不支持的 HTTP 方法")),
        };
        
        let path_pattern = path.to_string();
        
        // 检查是否已初始化队列桥接适配器
        let main_thread = self.http_main_thread.as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(
                "HTTP 队列桥接适配器未初始化，请先调用 initialize_http_queue_bridge()"
            ))?;
        
        // 创建队列桥接模式的处理器
        let http_handler = PyHttpHandler::new(handler, main_thread.clone(), self.codec.clone());
        
        let handler_fn = move |req: HttpRequest| -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> {
            info!("🌉 [HTTP队列桥接] 开始处理请求: {} {}", req.method, req.uri.path());
            http_handler.handle_request(req)
        };
        
        // 使用普通路由注册
        self.router.add_route(method.clone(), &path_pattern, handler_fn);
        
        info!("✅ [PyRouter] 队列桥接路由注册成功: {} {}", method, path_pattern);
        Ok(())
    }
    
    /// 添加 gRPC 一元处理器
    #[pyo3(signature = (path, handler, main_thread))]
    fn add_grpc_unary(&mut self, path: &str, handler: PyObject, main_thread: &PyGrpcMainThread) -> PyResult<()> {
        let grpc_handler = crate::python_api::grpc_queue_bridge::PyGrpcUnaryHandler::new(
            handler, 
            Arc::new(main_thread.clone_for_handler())
        );
        self.router.add_grpc_unary(path, grpc_handler);
        Ok(())
    }

    /// 添加 gRPC 服务端流处理器
    #[pyo3(signature = (path, handler, main_thread))]
    fn add_grpc_server_stream(&mut self, path: &str, handler: PyObject, main_thread: &PyGrpcMainThread) -> PyResult<()> {
        let grpc_handler = crate::python_api::grpc_queue_bridge::PyGrpcServerStreamHandler::new(
            handler, 
            Arc::new(main_thread.clone_for_handler())
        );
        self.router.add_grpc_server_stream(path, grpc_handler);
        Ok(())
    }

    /// 添加 gRPC 客户端流处理器
    #[pyo3(signature = (path, handler, main_thread))]
    fn add_grpc_client_stream(&mut self, path: &str, handler: PyObject, main_thread: &PyGrpcMainThread) -> PyResult<()> {
        let grpc_handler = crate::python_api::grpc_queue_bridge::PyGrpcClientStreamHandler::new(
            handler, 
            Arc::new(main_thread.clone_for_handler())
        );
        self.router.add_grpc_client_stream(path, grpc_handler);
        Ok(())
    }

    /// 添加 gRPC 双向流处理器
    #[pyo3(signature = (path, handler, main_thread))]
    fn add_grpc_bidirectional(&mut self, path: &str, handler: PyObject, main_thread: &PyGrpcMainThread) -> PyResult<()> {
        let grpc_handler = crate::python_api::grpc_queue_bridge::PyGrpcBidirectionalHandler::new(
            handler, 
            Arc::new(main_thread.clone_for_handler())
        );
        self.router.add_grpc_bidirectional(path, grpc_handler);
        Ok(())
    }

    /// 启用开发模式（自动生成自签名证书）
    /// 
    /// 此方法将：
    /// - 自动生成自签名证书
    /// - 启用 H2 和 H2C 协议支持
    /// - 配置 TLS 用于 HTTPS 访问
    /// 
    /// **警告：此模式仅用于开发和测试环境！**
    /// 
    /// # 参数
    /// - `hostnames`: 可选的主机名列表，如果未提供则使用默认值 ["localhost", "127.0.0.1"]
    /// 
    /// # 返回值
    /// - PyResult<()>: 成功返回 Ok(()), 失败返回 PyErr
    #[pyo3(signature = (hostnames=None))]
    fn enable_development_mode(&mut self, hostnames: Option<Vec<String>>) -> PyResult<()> {
        let hostnames = hostnames.unwrap_or_else(|| vec!["localhost".to_string(), "127.0.0.1".to_string()]);
        
        // 创建新的 tokio 运行时执行异步操作
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("创建运行时失败: {}", e)))?;
        
        // 创建证书管理器并配置开发模式
        let cert_config = crate::server::cert_manager::CertManagerConfig {
            development_mode: true,
            cert_path: None,
            key_path: None,
            ca_path: None,
            validity_days: 365,
            hostnames,
            acme_enabled: false,
            acme_production: false,
            acme_email: None,
            cloudflare_api_token: None,
            acme_renewal_days: 30,
            acme_cert_dir: None,
            mtls_enabled: false,
            client_cert_path: None,
            client_key_path: None,
            client_ca_path: None,
            mtls_mode: None,
            auto_generate_client_cert: false,
            client_cert_subject: None,
            auto_refresh_enabled: true,
            refresh_check_interval: 3600,
            force_cert_rotation: false,
            mtls_whitelist_paths: Vec::new(),
        };
        
        rt.block_on(async {
            // 确保加密提供程序已安装
            crate::utils::crypto_provider::ensure_crypto_provider_installed();
            
            let cert_manager = Arc::new(std::sync::RwLock::new(
                crate::server::cert_manager::CertificateManager::new(cert_config)
            ));
            
            // 初始化证书管理器，这会生成开发模式证书
            {
                let mut cert_manager_write = cert_manager.write()
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("获取证书管理器写锁失败: {}", e)))?;
                cert_manager_write.initialize().await
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("证书管理器初始化失败: {}", e)))?;
            }
            
            self.router.set_cert_manager(cert_manager);
            Ok::<(), pyo3::PyErr>(())
        })?;
        Ok(())
    }
    
    /// 配置 ACME 自动证书管理
    /// 
    /// 此方法将：
    /// - 启用 ACME 自动证书申请和续期
    /// - 配置 Let's Encrypt 或其他 ACME 提供商
    /// - 支持 DNS-01 挑战（通过 Cloudflare）
    /// - 自动处理证书续期（默认30天内到期时续期）
    /// 
    /// # 参数
    /// - `domains`: 需要申请证书的域名列表
    /// - `cert_config`: 证书管理器配置
    /// 
    /// # 返回值
    /// - PyResult<()>: 成功返回 Ok(()), 失败返回 PyErr
    /// 
    /// # 示例
    /// ```python
    /// # 创建 ACME 配置
    /// cert_config = CertManagerConfig.acme_config(
    ///     email="admin@example.com",
    ///     production=False,  # 使用测试环境
    ///     cloudflare_token="your_cloudflare_token"
    /// )
    /// 
    /// # 配置 ACME 证书
    /// router.configure_acme_certs(["example.com", "www.example.com"], cert_config)
    /// ```
    fn configure_acme_certs(&mut self, domains: Vec<String>, cert_config: &PyCertManagerConfig) -> PyResult<()> {
        // 验证配置
        if !cert_config.acme_enabled {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "证书配置未启用 ACME，请使用 CertManagerConfig.acme_config() 创建配置"
            ));
        }
        
        if !cert_config.is_valid() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "证书配置无效，请检查必要参数是否已设置"
            ));
        }
        
        if domains.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "域名列表不能为空"
            ));
        }
        
        // 转换为 Rust 配置
        let rust_config = cert_config.to_cert_manager_config()?;
        
        // 创建新的 tokio 运行时执行异步操作
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("创建运行时失败: {}", e)))?;
        
        rt.block_on(async {
            // 确保加密提供程序已安装
            crate::utils::crypto_provider::ensure_crypto_provider_installed();
            
            // 从配置中提取参数
            let email = cert_config.acme_email.clone()
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("ACME 配置缺少邮箱地址"))?;
            let cloudflare_token = cert_config.cloudflare_api_token.clone();
            let server_url = if cert_config.acme_production { Some("https://acme-v02.api.letsencrypt.org/directory".to_string()) } else { None };
            let cert_dir = Some(cert_config.acme_cert_dir.clone());
            let renewal_days = Some(cert_config.acme_renewal_days as u32);
            
            // 创建 ACME 证书管理器配置
            let mut cert_config = crate::server::cert_manager::CertManagerConfig {
                development_mode: false,
                cert_path: None,
                key_path: None,
                ca_path: None,
                validity_days: 90,
                hostnames: domains.clone(),
                acme_enabled: true,
                acme_production: cert_config.acme_production,
                acme_email: Some(email.clone()),
                cloudflare_api_token: cloudflare_token.clone(),
                acme_renewal_days: cert_config.acme_renewal_days as u32,
                acme_cert_dir: Some(cert_config.acme_cert_dir.clone()),
                mtls_enabled: false,
                client_cert_path: None,
                client_key_path: None,
                client_ca_path: None,
                mtls_mode: None,
                auto_generate_client_cert: false,
                client_cert_subject: None,
                auto_refresh_enabled: true,
                refresh_check_interval: 3600,
                force_cert_rotation: false,
                mtls_whitelist_paths: Vec::new(),
            };
            
            // 所有字段已经在上面设置完成
            
            let cert_manager = Arc::new(std::sync::RwLock::new(
                crate::server::cert_manager::CertificateManager::new(cert_config)
            ));
            
            self.router.set_cert_manager(cert_manager);
            Ok(())
        })
    }
    
    /// 配置 mTLS 双向认证
    /// 
    /// 此方法将：
    /// - 启用 mTLS 双向认证功能
    /// - 支持自签名模式和 ACME 混合模式
    /// - 自动生成或使用现有的客户端证书
    /// - 配置客户端证书验证
    /// 
    /// # 参数
    /// - `cert_config`: 证书管理器配置，必须启用 mTLS 并包含相关配置
    /// 
    /// # 返回值
    /// - PyResult<()>: 成功返回 Ok(()), 失败返回 PyErr
    /// 
    /// # 示例
    /// ```python
    /// # 创建 mTLS 自签名配置
    /// cert_config = CertManagerConfig.mtls_self_signed_config(
    ///     auto_generate_client_cert=True,
    ///     client_cert_subject="CN=Client,O=Example Corp"
    /// )
    /// 
    /// # 配置 mTLS
    /// router.configure_mtls(cert_config)
    /// 
    /// # 或者使用 ACME 混合模式
    /// cert_config = CertManagerConfig.mtls_acme_mixed_config(
    ///     email="admin@example.com",
    ///     domains=["api.example.com"],
    ///     auto_generate_client_cert=True
    /// )
    /// router.configure_mtls(cert_config)
    /// ```
    fn configure_mtls(&mut self, cert_config: &PyCertManagerConfig) -> PyResult<()> {
        // 验证配置
        if !cert_config.mtls_enabled {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "证书配置未启用 mTLS，请使用 CertManagerConfig.mtls_*_config() 创建配置"
            ));
        }
        
        if !cert_config.is_valid() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "mTLS 配置无效，请检查必要参数是否已设置"
            ));
        }
        
        // 转换为 Rust 配置
        let rust_config = cert_config.to_cert_manager_config()?;
        
        // 创建新的 tokio 运行时执行异步操作
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("创建运行时失败: {}", e)))?;
        
        rt.block_on(async {
            // 确保 CryptoProvider 已安装
            crate::utils::crypto_provider::ensure_crypto_provider_installed();
            
            // 直接创建和设置证书管理器，避免调用私有方法
            use crate::server::cert_manager::CertificateManager;
            use std::sync::{Arc, RwLock};
            
            let mut cert_manager = CertificateManager::new(rust_config);
            cert_manager.initialize().await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("初始化证书管理器失败: {}", e)))?;
            
            self.router.set_cert_manager(Arc::new(RwLock::new(cert_manager)));
            Ok(())
        })
    }
    
    /// 配置生产环境证书
    /// 
    /// 此方法将：
    /// - 使用预先准备的证书文件
    /// - 配置 TLS 用于 HTTPS 访问
    /// - 启用 H2 协议支持
    /// 
    /// # 参数
    /// - `cert_config`: 证书管理器配置，必须包含证书文件和私钥文件路径
    /// 
    /// # 返回值
    /// - PyResult<()>: 成功返回 Ok(()), 失败返回 PyErr
    /// 
    /// # 示例
    /// ```python
    /// # 创建生产环境证书配置
    /// cert_config = CertManagerConfig.production_config(
    ///     cert_file="/path/to/cert.pem",
    ///     key_file="/path/to/key.pem"
    /// )
    /// 
    /// # 配置生产环境证书
    /// router.configure_production_certs(cert_config)
    /// ```
    fn configure_production_certs(&mut self, cert_config: &PyCertManagerConfig) -> PyResult<()> {
        // 验证配置
        if cert_config.acme_enabled {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "证书配置启用了 ACME，请使用 configure_acme_certs() 方法"
            ));
        }
        
        if cert_config.cert_file.is_none() || cert_config.key_file.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "生产环境证书配置必须提供 cert_file 和 key_file"
            ));
        }
        
        if !cert_config.is_valid() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "证书配置无效，请检查证书文件和私钥文件路径"
            ));
        }
        
        // 转换为 Rust 配置
        let rust_config = cert_config.to_cert_manager_config();
        
        // 创建新的 tokio 运行时执行异步操作
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("创建运行时失败: {}", e)))?;
        
        rt.block_on(async {
            // 确保加密提供程序已安装
            crate::utils::crypto_provider::ensure_crypto_provider_installed();
            
            // 从配置中提取参数
            let cert_path = cert_config.cert_file.clone()
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("生产环境配置缺少证书文件路径"))?;
            let key_path = cert_config.key_file.clone()
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("生产环境配置缺少私钥文件路径"))?;
            let ca_path = None; // 暂时不支持 CA 路径
            let hostnames = vec![]; // 暂时使用空的主机名列表
            
            // 创建证书管理器并配置生产环境证书
            let cert_config = crate::server::cert_manager::CertManagerConfig {
                development_mode: false,
                cert_path: Some(cert_path),
                key_path: Some(key_path),
                ca_path: ca_path,
                validity_days: 365,
                hostnames: hostnames,
                acme_enabled: false,
                acme_production: false,
                acme_email: None,
                cloudflare_api_token: None,
                acme_renewal_days: 30,
                acme_cert_dir: None,
                mtls_enabled: false,
                client_cert_path: None,
                client_key_path: None,
                client_ca_path: None,
                mtls_mode: None,
                auto_generate_client_cert: false,
                client_cert_subject: None,
                auto_refresh_enabled: true,
                refresh_check_interval: 3600,
                force_cert_rotation: false,
                mtls_whitelist_paths: Vec::new(),
            };
            
            let cert_manager = Arc::new(std::sync::RwLock::new(
                crate::server::cert_manager::CertificateManager::new(cert_config)
            ));
            
            self.router.set_cert_manager(cert_manager);
            Ok(())
        })
    }

    /// 启用 SPA (单页应用) 支持
    /// 
    /// # 参数
    /// * `fallback_path` - SPA 回退路径，通常是 index.html
    /// 
    /// # 示例
    /// ```python
    /// router.enable_spa("/index.html")
    /// ```
    fn enable_spa(&mut self, fallback_path: &str) -> PyResult<()> {
        self.router = self.router.clone().enable_spa(fallback_path);
        Ok(())
    }

    /// 禁用 SPA (单页应用) 支持
    /// 
    /// # 示例
    /// ```python
    /// router.disable_spa()
    /// ```
    fn disable_spa(&mut self) -> PyResult<()> {
        self.router = self.router.clone().disable_spa();
        Ok(())
    }

    /// 配置 SPA (单页应用) 设置
    /// 
    /// # 参数
    /// * `enabled` - 是否启用 SPA 支持
    /// * `fallback_path` - SPA 回退路径，当启用时必须提供
    /// 
    /// # 示例
    /// ```python
    /// router.configure_spa(True, "/index.html")
    /// router.configure_spa(False, None)
    /// ```
    #[pyo3(signature = (enabled, fallback_path=None))]
    fn configure_spa(&mut self, enabled: bool, fallback_path: Option<&str>) -> PyResult<()> {
        if enabled {
            let fallback = fallback_path
                .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(
                    "启用 SPA 时必须提供 fallback_path 参数"
                ))?;
            self.router = self.router.clone().enable_spa(fallback);
        } else {
            self.router = self.router.clone().disable_spa();
        }
        Ok(())
    }

    }

impl PyRouter {
    /// 执行 Python 分块处理器
    fn execute_python_chunked_handler(
        path_pattern: String,
        req: HttpRequest,
        handler: PyObject,
        codec: PyQuickCodec,
        path_params: HashMap<String, String>
    ) -> Result<ChunkedResponse, Box<dyn std::error::Error + Send + Sync>> {
        let result = Python::with_gil(|py| -> Result<ChunkedResponse, pyo3::PyErr> {
            // 准备请求数据
            let request_data = prepare_request_data_from_http_request(py, &req, &codec, Some(&path_params))?;
            
            // 准备参数
            let mut args_vec: Vec<PyObject> = vec![request_data.to_object(py)];
            
            // 添加路径参数（按顺序）
            for (_, value) in &path_params {
                args_vec.push(value.into_py(py));
            }
            
            // 调用 Python 处理函数
            let args = pyo3::types::PyTuple::new(py, &args_vec);
            let result = handler.call(py, args, None)?;
            
            // 处理 Python 函数返回的响应
            handle_python_chunked_response(py, result, &codec)
        });
        
        result.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })
    }

    /// 执行 Python 处理器（带路径参数）
    fn execute_python_handler_with_params(
        path_pattern: String,
        req: Request<()>,
        handler: PyObject,
        codec: PyQuickCodec,
        body_bytes: Vec<u8>,
        path_params: HashMap<String, String>
    ) -> Result<ChunkedResponse, Box<dyn std::error::Error + Send + Sync>> {
        // 🔧 [调试信息] 分块响应处理调试 - 如需调试分块响应处理问题，可取消注释以下行
        // println!("🔍 [RUST-DEBUG] execute_python_handler_with_params 被调用");
        // println!("🔍 [RUST-DEBUG] path_params: {:?}", path_params);
        info!("开始执行 Python handler，路径参数: {:?}", path_params);
        
        let result = Python::with_gil(|py| -> Result<ChunkedResponse, pyo3::PyErr> {
            // 准备请求数据，传递主库提供的路径参数
            let request_data = prepare_request_data(py, &req, &codec, &body_bytes, Some(&path_params))?;
            
            // 准备参数
            let mut args_vec: Vec<PyObject> = vec![request_data.to_object(py)];
            
            // 添加路径参数（按顺序）
            for (_, value) in &path_params {
                args_vec.push(value.into_py(py));
            }
            
            // 🔧 [调试信息] 路径参数转换调试 - 如需调试路径参数转换问题，可取消注释以下行
            // println!("🔍 [RUST-DEBUG] 转换后的路径参数数量: {}", path_params.len());
            info!("转换后的路径参数数量: {}", path_params.len());
            
            // 调用 Python 处理函数
            let args = pyo3::types::PyTuple::new(py, &args_vec);
            let result = handler.call(py, args, None)?;
            
            // 🔧 [调试信息] Python handler 成功调试 - 如需调试 handler 调用成功情况，可取消注释以下行
            // println!("🔍 [RUST-DEBUG] Python handler 调用成功");
            info!("Python handler 调用成功");
            
            // 处理 Python 函数返回的响应
            let mut chunked_response = ChunkedResponse::new();
            
            if let Ok(response_str) = result.extract::<String>(py) {
                chunked_response = chunked_response.add_chunk(response_str);
            } else if let Ok(response_dict) = result.downcast::<pyo3::types::PyDict>(py) {
                // 处理字典格式的响应
                if let Ok(body) = response_dict.get_item("body") {
                    if let Some(body) = body {
                        if let Ok(body_str) = body.extract::<String>() {
                            chunked_response = chunked_response.add_chunk(body_str);
                        }
                    }
                }
            }
            
            Ok(chunked_response)
        });
        
        result.map_err(|e| {
            // 🔧 [调试信息] Python handler 错误调试 - 如需调试 handler 调用失败情况，可取消注释以下行
            // println!("🔍 [RUST-DEBUG] Python handler 调用失败: {}", e);
            error!("Python handler 调用失败: {}", e);
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })
    }
}

impl Clone for PyRouter {
    fn clone(&self) -> Self {
        Self {
            router: self.router.clone(),
            codec: self.codec.clone(),
            http_main_thread: self.http_main_thread.clone(),
        }
    }
}

/// Python 服务器类
#[pyclass(name = "Server")]
pub struct PyServer {
    config: crate::server::ServerConfig,
    log_config_json: Option<String>,
}

#[pymethods]
impl PyServer {
    #[new]
    fn new(config: &PyServerConfig) -> PyResult<Self> {
        let server_config = config.to_server_config();
        
        Ok(Self {
            config: server_config,
            log_config_json: None,
        })
    }
    
    /// 配置日志系统
    ///
    /// # 参数
    /// - `config_json`: 日志配置的JSON字符串
    ///
    /// # 示例
    /// ```python
    /// log_config = {
    ///     "level": "debug",
    ///     "enable_colors": True,
    ///     "enable_emoji": True,
    ///     "show_timestamp": True,
    ///     "show_module": True,
    ///     "log_file": None
    /// }
    /// server.configure_logging(json.dumps(log_config))
    /// ```
    fn configure_logging(&mut self, config_json: String) -> PyResult<()> {
        // 存储日志配置JSON，供后续run方法使用
        self.log_config_json = Some(config_json);
        Ok(())
    }

    /// 启动服务器（非阻塞模式）
    /// 为了避免 Python 主线程被阻塞导致程序假死，只支持非阻塞模式
    #[pyo3(signature = (router, host="127.0.0.1", port=8000))]
    fn run(&self, router: PyRouter, host: &str, port: u16) -> PyResult<()> {
        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("无效的地址: {}", e)))?;
        
        // 克隆配置数据以避免生命周期问题
        let workers = self.config.workers;
        let host_string = host.to_string();
        let host_display = host_string.clone(); // 用于显示

        // 解析日志配置，用户可以通过JSON完全控制日志行为
        let log_config = if let Some(ref config_json) = self.log_config_json {
            // 解析JSON配置
            let config_value: serde_json::Value = serde_json::from_str(config_json)
                .unwrap_or_else(|_| {
                    // 如果解析失败，使用默认配置
                    serde_json::json!({
                        "enabled": true,
                        "level": "info",
                        "enable_colors": true,
                        "enable_emoji": true,
                        "show_timestamp": true,
                        "show_module": true,
                        "log_file": serde_json::Value::Null
                    })
                });

            crate::utils::logger::LogConfig {
                enabled: config_value.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                level: match config_value.get("level").and_then(|v| v.as_str()).unwrap_or("info") {
                    "error" => crate::utils::logger::LogLevel::Error,
                    "warn" => crate::utils::logger::LogLevel::Warn,
                    "info" => crate::utils::logger::LogLevel::Info,
                    "debug" => crate::utils::logger::LogLevel::Debug,
                    "trace" => crate::utils::logger::LogLevel::Trace,
                    _ => crate::utils::logger::LogLevel::Info,
                },
                output: if let Some(log_file) = config_value.get("log_file").and_then(|v| v.as_str()) {
                    if log_file.is_empty() || log_file == "null" {
                        crate::utils::logger::LogOutput::Terminal
                    } else {
                        crate::utils::logger::LogOutput::File {
                            log_dir: std::path::PathBuf::from(log_file).parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf(),
                            max_file_size: 10 * 1024 * 1024, // 10MB
                            max_compressed_files: 5,
                        }
                    }
                } else {
                    crate::utils::logger::LogOutput::Terminal
                },
                use_colors: config_value.get("enable_colors").and_then(|v| v.as_bool()).unwrap_or(true),
                use_emoji: config_value.get("enable_emoji").and_then(|v| v.as_bool()).unwrap_or(true),
                show_timestamp: config_value.get("show_timestamp").and_then(|v| v.as_bool()).unwrap_or(true),
                show_module: config_value.get("show_module").and_then(|v| v.as_bool()).unwrap_or(true),
            }
        } else {
            // 用户没有配置日志，使用强制的默认配置
            crate::utils::logger::LogConfig {
                enabled: true,
                level: crate::utils::logger::LogLevel::Info,
                output: crate::utils::logger::LogOutput::Terminal,
                use_colors: true,
                use_emoji: true,
                show_timestamp: true,
                show_module: true,
            }
        };

        // 非阻塞模式：在后台线程中运行服务器
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[ERROR] 创建 Tokio 运行时失败: {}", e);
                    return;
                }
            };
            
            rt.block_on(async {
                // 从 PyRouter 中获取内部的 Router
                let router_ptr = router.get_inner_router().unwrap();
                let cloned_router = unsafe {
                    &*(router_ptr as *const crate::server::Router)
                }.clone();
                
                // 使用新的 RatEngine::builder() API，添加 ACME 支持
                let mut builder = crate::engine::RatEngine::builder()
                    .worker_threads(workers)
                    .max_connections(workers * 1000); // 基于 worker 数量计算最大连接数

                // 如果启用日志，直接使用 Logger::init 初始化日志系统，绕过构造器
                if log_config.enabled {
                    if let Err(e) = crate::utils::logger::Logger::init(log_config.clone()) {
                        if !e.to_string().contains("already initialized") {
                            eprintln!("[ERROR] 日志系统初始化失败: {}", e);
                        }
                    } else {
                    }
                } else {
                }
                
                // 检查路由器是否有证书管理器配置
                if let Some(cert_config) = cloned_router.get_cert_manager_config() {
                    if cert_config.development_mode {
                        // 开发模式配置
                        match builder.enable_development_mode(cert_config.hostnames).await {
                            Ok(b) => builder = b,
                            Err(e) => {
                                eprintln!("[ERROR] 开发模式配置失败: {}", e);
                                return;
                            }
                        }
                    } else if cert_config.acme_enabled {
                        // ACME 模式配置
                        if let (Some(email), Some(cloudflare_token), Some(cert_dir)) = (
                            &cert_config.acme_email,
                            &cert_config.cloudflare_api_token,
                            &cert_config.acme_cert_dir
                        ) {
                            // 从 hostnames 中获取域名
                            let domain = cert_config.hostnames.first()
                                .cloned()
                                .unwrap_or_else(|| host_string.clone());
                            
                            match builder.cert_manager_acme(
                                domain,
                                email.clone(),
                                cloudflare_token.clone(),
                                cert_dir.clone(),
                                cert_config.acme_renewal_days,
                                cert_config.acme_production
                            ).await {
                                Ok(b) => builder = b,
                                Err(e) => {
                                    eprintln!("[ERROR] ACME 证书管理器配置失败: {}", e);
                                    return;
                                }
                            }
                        }
                    }
                }
                
                let engine = match builder
                    .router(cloned_router)
                    .build_and_start(host_string, port).await {
                    Ok(engine) => engine,
                    Err(e) => {
                        eprintln!("[ERROR] 构建引擎失败: {}", e);
                        return;
                    }
                };
                
                // 服务器现在正在运行
            });
        });
        
        // 给服务器一些时间启动
        std::thread::sleep(std::time::Duration::from_millis(500));
        
        Ok(())
    }
}

/// 提取路径参数
fn extract_path_params(pattern: &str, path: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    
    // 如果模式不包含参数，直接返回空的 HashMap
    if !pattern.contains('<') {
        info!("路径模式 '{}' 不包含参数", pattern);
        return params;
    }
    
    info!("开始提取路径参数 - 模式: '{}', 路径: '{}'", pattern, path);
    
    // 编译路径模式为正则表达式
    let mut param_names = Vec::new();
    let mut regex_pattern = pattern.to_string();
    
    // 提取参数名并替换为正则表达式捕获组
    let param_regex = Regex::new(r"<([^>]+)>").unwrap();
    regex_pattern = param_regex.replace_all(&regex_pattern, |caps: &regex::Captures| {
        let param_def = &caps[1];
        if param_def.contains(':') {
            let parts: Vec<&str> = param_def.split(':').collect();
            if parts.len() == 2 {
                let param_type = parts[0];
                let param_name = parts[1];
                param_names.push(param_name.to_string());
                
                match param_type {
                    "int" => r"(\d+)",
                    "float" => r"([\d.]+)",
                    "path" => r"(.+)",
                    _ => r"([^/]+)",
                }
            } else {
                param_names.push(param_def.to_string());
                r"([^/]+)"
            }
        } else {
            param_names.push(param_def.to_string());
            r"([^/]+)"
        }
    }).to_string();
    
    // 转义点号并添加锚点
    regex_pattern = regex_pattern.replace(".", "\\.");
    regex_pattern = format!("^{}$", regex_pattern);
    
    info!("生成的正则表达式: '{}'", regex_pattern);
    info!("参数名列表: {:?}", param_names);
    
    // 尝试匹配并提取参数
    if let Ok(regex) = Regex::new(&regex_pattern) {
        if let Some(captures) = regex.captures(path) {
            info!("正则匹配成功，捕获组数量: {}", captures.len());
            for (i, param_name) in param_names.iter().enumerate() {
                if let Some(capture) = captures.get(i + 1) {
                    let raw_value = capture.as_str();
                    // URL解码参数值
                    let decoded_value = urlencoding::decode(raw_value)
                        .unwrap_or_else(|_| raw_value.into())
                        .to_string();
                    
                    info!("参数 '{}': 原始值='{}', 解码后='{}'", param_name, raw_value, decoded_value);
                    params.insert(param_name.clone(), decoded_value);
                }
            }
        } else {
            warn!("正则表达式匹配失败 - 模式: '{}', 路径: '{}'", regex_pattern, path);
        }
    } else {
        error!("正则表达式编译失败: '{}'", regex_pattern);
    }
    
    info!("最终提取的参数: {:?}", params);
    params
}

/// 从 HttpRequest 准备请求数据供 Python 处理函数使用
fn prepare_request_data_from_http_request<'a>(
    py: Python<'a>,
    req: &HttpRequest,
    codec: &PyQuickCodec,
    path_params: Option<&HashMap<String, String>>
) -> PyResult<&'a pyo3::types::PyDict> {
    let dict = pyo3::types::PyDict::new(py);
    
    // 添加请求方法
    dict.set_item("method", req.method.as_str())?;
    
    // 添加请求路径
    dict.set_item("path", req.path())?;
    
    // 添加查询参数
    if let Some(query) = req.query() {
        dict.set_item("query", query)?;
    } else {
        dict.set_item("query", "")?;
    }
    
    // 添加请求头
    let headers_dict = pyo3::types::PyDict::new(py);
    for (name, value) in &req.headers {
        if let Ok(value_str) = value.to_str() {
            headers_dict.set_item(name.as_str(), value_str)?;
        }
    }
    dict.set_item("headers", headers_dict)?;
    
    // 添加路径参数
    if let Some(params) = path_params {
        let path_params_dict = pyo3::types::PyDict::new(py);
        for (key, value) in params {
            path_params_dict.set_item(key, value)?;
        }
        dict.set_item("path_params", path_params_dict)?;
    } else {
        let empty_dict = pyo3::types::PyDict::new(py);
        dict.set_item("path_params", empty_dict)?;
    }
    
    // 添加请求体
    let py_bytes = pyo3::types::PyBytes::new(py, &req.body);
    dict.set_item("body", py_bytes)?;
    
    Ok(dict)
}

/// 准备请求数据供 Python 处理函数使用
fn prepare_request_data<'a, T>(py: Python<'a>, req: &Request<T>, codec: &PyQuickCodec, body_bytes: &[u8], path_params: Option<&std::collections::HashMap<String, String>>) -> PyResult<&'a PyDict> {
    
    let dict = pyo3::types::PyDict::new(py);
    
    // 添加请求方法
    dict.set_item("method", req.method().as_str())?;
    
    // 添加请求路径
    dict.set_item("path", req.uri().path())?;
    
    // 添加查询参数
    if let Some(query) = req.uri().query() {
        dict.set_item("query", query)?;
    } else {
        dict.set_item("query", "")?;
    }
    
    // 添加请求头
    let headers_dict = pyo3::types::PyDict::new(py);
    for (name, value) in req.headers() {
        if let Ok(value_str) = value.to_str() {
            headers_dict.set_item(name.as_str(), value_str)?;
        }
    }
    dict.set_item("headers", headers_dict)?;
    
    // 添加路径参数
    if let Some(params) = path_params {
        // println!("🔍 [DEBUG-RUST] 提取到的路径参数: {:?}", params);
        let path_params_dict = pyo3::types::PyDict::new(py);
        for (key, value) in params {
            path_params_dict.set_item(key, value)?;
        }
        dict.set_item("path_params", path_params_dict)?;
    } else {
        // println!("🔍 [DEBUG-RUST] 没有路径参数传递");
        let empty_dict = pyo3::types::PyDict::new(py);
        dict.set_item("path_params", empty_dict)?;
    }
    

    
    // 添加请求体
    let py_bytes = pyo3::types::PyBytes::new(py, body_bytes);
    dict.set_item("body", py_bytes)?;
    
    Ok(dict)
}

/// 处理 Python 函数返回的 SSE 响应
fn handle_python_sse_response(
    py: Python,
    response: PyObject,
    sender: &tokio::sync::mpsc::UnboundedSender<Result<hyper::body::Frame<hyper::body::Bytes>, Box<dyn std::error::Error + Send + Sync>>>,
    codec: &PyQuickCodec
) -> PyResult<()> {
    // 首先检查是否为生成器
    if response.as_ref(py).hasattr("__iter__")? && response.as_ref(py).hasattr("__next__")? {
        // 处理生成器
        let iterator = response.call_method0(py, "__iter__")?;
        let mut count = 0;
        
        loop {
             // 释放 GIL 让其他任务有机会执行
             py.allow_threads(|| {
                 std::thread::sleep(std::time::Duration::from_millis(1));
             });
             
             match iterator.call_method0(py, "__next__") {
                 Ok(item) => {
                     count += 1;
                     
                     if let Ok(string_data) = item.extract::<String>(py) {
                         // 检查字符串是否已经是 SSE 格式
                         if string_data.starts_with("data: ") {
                             // 已经是 SSE 格式，直接发送
                             let formatted = if string_data.ends_with("\n\n") {
                                 string_data
                             } else {
                                 format!("{}\n\n", string_data)
                             };
                             let _ = sender.send(Ok(hyper::body::Frame::data(
                                 hyper::body::Bytes::from(formatted)
                             )));
                         } else {
                             // 不是 SSE 格式，需要包装
                             let formatted = format!("data: {}\n\n", string_data);
                             let _ = sender.send(Ok(hyper::body::Frame::data(
                                 hyper::body::Bytes::from(formatted)
                             )));
                         }
                     }
                     
                     // 防止无限循环，设置最大迭代次数
                     if count > 10000 {
                         break;
                     }
                 }
                 Err(e) => {
                     // 检查是否为 StopIteration 异常
                     if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) {
                         break;
                     } else {
                         return Err(e);
                     }
                 }
             }
         }
    } else if let Ok(string_data) = response.extract::<String>(py) {
        let formatted = format!("data: {}\n\n\n", string_data);
        let _ = sender.send(Ok(hyper::body::Frame::data(
            hyper::body::Bytes::from(formatted)
        )));
    } else if let Ok(dict) = response.downcast::<pyo3::types::PyDict>(py) {
        // 处理字典类型的响应
        let json_value = crate::python_api::streaming::python_object_to_json_value(dict.as_ref())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("转换失败: {}", e)))?;
        let json_str = serde_json::to_string(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?;
        let formatted = format!("data: {}\n\n\n", json_str);
        let _ = sender.send(Ok(hyper::body::Frame::data(
            hyper::body::Bytes::from(formatted)
        )));
    }
    
    Ok(())
}

/// 处理 Python 函数返回的分块响应
fn handle_python_chunked_response(
    py: Python,
    response: PyObject,
    codec: &PyQuickCodec
) -> PyResult<ChunkedResponse> {
    let mut chunked_response = ChunkedResponse::new();
    
    if let Ok(string_data) = response.extract::<String>(py) {
        chunked_response = chunked_response.add_chunk(string_data);
    } else if let Ok(list) = response.downcast::<pyo3::types::PyList>(py) {
        for item in list {
            if let Ok(chunk) = item.extract::<String>() {
                chunked_response = chunked_response.add_chunk(chunk);
            }
        }
    }
    
    Ok(chunked_response)
}

/// 处理 Python 函数返回的普通响应
fn handle_python_response(
    py: Python,
    response: PyObject,
    codec: &PyQuickCodec
) -> PyResult<hyper::Response<http_body_util::Full<hyper::body::Bytes>>> {
    // 首先尝试提取 TypedResponse 对象
    if let Ok(typed_response) = response.extract::<crate::python_api::http::core::TypedResponse>(py) {
        use crate::python_api::http::core::ResponseType;
        
        match typed_response.response_type {
            ResponseType::HTML => {
                if let Ok(content) = typed_response.content.extract::<String>(py) {
                    return Ok(hyper::Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "text/html; charset=utf-8")
                        .body(http_body_util::Full::new(hyper::body::Bytes::from(content)))
                        .unwrap());
                }
            },
            ResponseType::JSON => {
                // 尝试序列化为 JSON
                let json_str = if let Ok(string_content) = typed_response.content.extract::<String>(py) {
                    string_content
                } else {
                    // 如果不是字符串，尝试转换为 JSON
                    let json_value = crate::python_api::streaming::python_object_to_json_value(typed_response.content.as_ref(py))
                        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 转换失败: {}", e)))?;
                    serde_json::to_string(&json_value)
                        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?
                };
                
                return Ok(hyper::Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json; charset=utf-8")
                    .body(http_body_util::Full::new(hyper::body::Bytes::from(json_str)))
                    .unwrap());
            },
            ResponseType::TEXT => {
                if let Ok(content) = typed_response.content.extract::<String>(py) {
                    return Ok(hyper::Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "text/plain; charset=utf-8")
                        .body(http_body_util::Full::new(hyper::body::Bytes::from(content)))
                        .unwrap());
                }
            },
            _ => {
                // 对于其他类型，尝试转换为字符串
                if let Ok(content) = typed_response.content.extract::<String>(py) {
                    return Ok(hyper::Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "text/html; charset=utf-8")
                        .body(http_body_util::Full::new(hyper::body::Bytes::from(content)))
                        .unwrap());
                }
            }
        }
    }
    
    // 然后尝试提取 HttpResponse 对象
    if let Ok(http_response) = response.extract::<crate::python_api::HttpResponse>(py) {
        
        let mut builder = hyper::Response::builder().status(http_response.status);
        
        // 设置响应头
        for (key, value) in &http_response.headers {
            builder = builder.header(key, value);
        }
        
        return Ok(builder
            .body(http_body_util::Full::new(hyper::body::Bytes::from(http_response.body)))
            .unwrap());
    }
    
    // 尝试提取字符串
    if let Ok(string_data) = response.extract::<String>(py) {
        return Ok(hyper::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(http_body_util::Full::new(hyper::body::Bytes::from(string_data)))
            .unwrap());
    }
    
    // 尝试提取字典
    if let Ok(dict) = response.downcast::<pyo3::types::PyDict>(py) {
        let json_value = crate::python_api::streaming::python_object_to_json_value(dict.as_ref())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("转换失败: {}", e)))?;
        let json_str = serde_json::to_string(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("JSON 序列化失败: {}", e)))?;
        
        return Ok(hyper::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json; charset=utf-8")
            .body(http_body_util::Full::new(hyper::body::Bytes::from(json_str)))
            .unwrap());
    }
    Err(pyo3::exceptions::PyValueError::new_err("不支持的响应类型"))
}

/// 注册服务器模块
pub fn register_server_module(py: Python, parent_module: &PyModule) -> PyResult<()> {
    let server_module = PyModule::new(py, "server")?;
    server_module.add_class::<PyRouter>()?;
    server_module.add_class::<PyServer>()?;
    server_module.add_class::<PyHttpMainThread>()?;
    parent_module.add_submodule(server_module)?;
    Ok(())
}