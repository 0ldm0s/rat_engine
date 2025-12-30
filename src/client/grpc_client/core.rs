//! gRPC 客户端核心模块
//!
//! 专注于 TLS/SSL 配置，支持 mTLS（rustls 版本）

use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;

use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::Full;
use hyper::body::Bytes;

#[cfg(feature = "compression")]
use crate::compression::CompressionConfig;
#[cfg(not(feature = "compression"))]
use crate::client::grpc_builder::CompressionConfig;
use crate::client::connection_pool::ClientConnectionPool;
use crate::client::grpc_builder::MtlsClientConfig;
use crate::client::grpc_client_delegated::ClientBidirectionalManager;
use super::GrpcCompressionMode;

/// RAT Engine gRPC+Bincode 客户端
///
/// 提供基于 hyper 和 bincode 2.x 的高性能 gRPC 客户端实现，支持：
/// - 连接池管理和复用
/// - 自动保活机制
/// - 超时控制
/// - Bincode 2.x 序列化/反序列化
/// - LZ4 压缩（可选）
/// - 自动重试（可选）
/// - 请求/响应日志
/// - H2C (HTTP/2 over cleartext) 支持
#[derive(Debug)]
pub struct RatGrpcClient {
    /// 底层 hyper 客户端
    pub client: Client<HttpConnector, Full<Bytes>>,
    /// base_uri: 服务器基础 URI（已移除，现在在每次请求时传入）
    // base_uri: Uri, // 已移除
    /// 连接超时时间
    pub connect_timeout: Duration,
    /// 请求超时时间
    pub request_timeout: Duration,
    /// 最大空闲连接数
    pub max_idle_connections: usize,
    /// 用户代理字符串
    pub user_agent: String,
    /// 压缩配置
    pub compression_config: CompressionConfig,
    /// 是否启用压缩
    pub enable_compression: bool,
    /// 是否启用自动重试
    pub enable_retry: bool,
    /// 最大重试次数
    pub max_retries: u32,
    /// 客户端连接池
    pub connection_pool: Arc<ClientConnectionPool>,
    /// 压缩模式
    pub compression_mode: GrpcCompressionMode,
    /// 请求 ID 计数器
    pub request_id_counter: std::sync::atomic::AtomicU64,
    /// 流 ID 计数器
    pub stream_id_counter: std::sync::atomic::AtomicU64,
    /// 委托模式双向流管理器
    pub delegated_manager: Arc<ClientBidirectionalManager>,
    /// 是否启用开发模式（跳过证书验证）
    pub h2c_mode: bool,
    /// mTLS 客户端配置
    pub mtls_config: Option<MtlsClientConfig>,
    /// DNS解析映射表（域名 -> 预解析IP）
    pub dns_mapping: Option<std::collections::HashMap<String, String>>,
    /// 是否启用 h2c-over-TLS 模式（Xray-core 风格：TLS 通道内传输 h2c）
    pub h2c_over_tls: bool,
}

impl Clone for RatGrpcClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            connect_timeout: self.connect_timeout,
            request_timeout: self.request_timeout,
            max_idle_connections: self.max_idle_connections,
            user_agent: self.user_agent.clone(),
            compression_config: self.compression_config.clone(),
            enable_compression: self.enable_compression,
            enable_retry: self.enable_retry,
            max_retries: self.max_retries,
            connection_pool: self.connection_pool.clone(),
            compression_mode: self.compression_mode,
            request_id_counter: std::sync::atomic::AtomicU64::new(0),
            stream_id_counter: std::sync::atomic::AtomicU64::new(0),
            delegated_manager: self.delegated_manager.clone(),
            h2c_mode: self.h2c_mode,
            mtls_config: self.mtls_config.clone(),
            dns_mapping: self.dns_mapping.clone(),
            h2c_over_tls: self.h2c_over_tls,
        }
    }
}
