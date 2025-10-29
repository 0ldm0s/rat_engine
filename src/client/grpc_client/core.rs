//! gRPC 客户端核心模块

use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;

use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::Full;
use hyper::body::Bytes;

use crate::compression::CompressionConfig;
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
    pub development_mode: bool,
    /// mTLS 客户端配置
    pub mtls_config: Option<crate::client::grpc_builder::MtlsClientConfig>,
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
            development_mode: self.development_mode,
            mtls_config: self.mtls_config.as_ref().map(|config| {
                crate::client::grpc_builder::MtlsClientConfig {
                    client_cert_chain: config.client_cert_chain.clone(),
                    client_private_key: config.client_private_key.clone(),
                    ca_certs: config.ca_certs.clone(),
                    skip_server_verification: config.skip_server_verification,
                    server_name: config.server_name.clone(),
                    client_cert_path: config.client_cert_path.clone(),
                    client_key_path: config.client_key_path.clone(),
                    ca_cert_path: config.ca_cert_path.clone(),
                }
            }),
        }
    }
}
