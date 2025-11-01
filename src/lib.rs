//! RAT Engine - 高性能 Web 框架
//! 
//! RAT Engine 是一个基于 Rust 的高性能 Web 框架，提供：
//! - 高性能工作窃取调度器
//! - 零拷贝网络 I/O
//! - 内存池管理
//! - 原子性能监控
//! - 内容压缩支持
//! - Python 集成支持
//! - Flask 风格的 API

#[macro_use]
extern crate lazy_static;

// 核心模块
pub mod engine;
pub mod server;
#[cfg(any(feature = "client", feature = "http-client", feature = "grpc-client", feature = "reqwest"))]
pub mod client;
pub mod utils;
pub mod error;
pub mod error_i18n;
#[cfg(feature = "compression")]
pub mod compression;
pub mod cache;

// 公共模块
pub mod common;

// 在库加载时确保 CryptoProvider 只安装一次
lazy_static! {
    static ref _CRYPTO_PROVIDER_INIT: () = {
        utils::crypto_provider::ensure_crypto_provider_installed();
        // 初始化错误信息多语言支持
        error_i18n::init_error_translations();
    };
}

// Python API 模块（暂时禁用，等主库迁移完成后再处理）
#[cfg(feature = "python")]
pub mod python_api;

// 导出核心类型
pub use server::{ServerConfig, Router, WorkerPool};
#[allow(deprecated)]
pub use server::run_server_with_router;
pub use engine::RatEngine;

// 重新导出 hyper 常用类型，让用户无需直接引入 hyper
pub use hyper::{Method, Response, StatusCode, Version, HeaderMap, Error};
pub use hyper::body::{Bytes, Incoming, Frame};
pub use hyper::http::Uri;
pub use hyper::Request;
pub use http_body_util::{Full, Empty, BodyExt};
// 导出客户端相关类型
#[cfg(feature = "client")]
pub use client::{
    // RatHttpClient, RatHttpClientBuilder,  // 已移除HTTP客户端，只保留gRPC客户端
    RatGrpcClient, RatGrpcClientBuilder,
    GrpcRequest, GrpcResponse, GrpcCompressionMode,
    GrpcStreamResponse,
    // RatHttpResponse, HttpMethod, HttpStatusCode, HttpHeaders, HttpRequestBuilder,  // 已移除HTTP客户端，只保留gRPC客户端
    download_metadata::{DownloadMetadataManager, DownloadMetadata, ChunkInfo, DownloadStatus},
    connection_pool::ClientConnectionPool,
};

// 导出独立HTTP客户端类型
#[cfg(feature = "reqwest")]
pub use client::independent_http_client::{
    RatIndependentHttpClient, RatIndependentHttpClientBuilder, RatIndependentHttpResponse,
    SseStream, SseEvent, CompressionTestResult,
};
// 导出统一的 GrpcStreamMessage
pub use server::grpc_types::GrpcStreamMessage;
pub use utils::sys_info::SystemInfo;
pub use utils::logger::{Logger, LogLevel, LogConfig};
pub use utils::logger::{error, warn, info, debug, trace};
pub use error::{RatError, RatResult, CacheError};

// 导出性能优化函数
pub use server::performance::optimize_for_throughput;

// 导出缓存相关类型
#[cfg(feature = "cache")]
pub use cache::{CacheBuilder, Cache, RatMemCache};

// 导出压缩相关类型
#[cfg(feature = "compression")]
pub use compression::{CompressionType, CompressionConfig, Compressor};
#[cfg(feature = "compression")]
pub use server::compression_middleware::CompressionMiddleware;
#[cfg(feature = "compression")]
pub use server::compression_middleware_impl;

// 重新导出智能传输相关类型
pub use engine::smart_transfer::SmartTransferManager;

// 引入构建时生成的版本信息
include!(concat!(env!("OUT_DIR"), "/version_info.rs"));

// 常量定义
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 8080;
pub const MIN_WORKERS: usize = 1;
pub const MAX_WORKERS: usize = 1024;
pub const DEFAULT_TIMEOUT: u64 = 30;

/// 获取版本信息
pub fn version_info() -> &'static VersionInfo {
    &VERSION_INFO
}

// Python 模块定义
#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[pymodule]
fn rat_engine(py: Python, m: &PyModule) -> PyResult<()> {
    // 确保 OpenSSL 已初始化（Python 模块初始化时）
    utils::crypto_provider::ensure_crypto_provider_installed();
    
    // 注意：日志系统现在由用户手动初始化，不再在模块加载时自动初始化
    
    // 注册 Python API 模块
    python_api::register_python_api_module(py, m)?;
    
    // 添加版本信息
    m.add("__version__", VERSION)?;
    
    Ok(())
}