//! RAT Engine 客户端模块
//! 
//! 提供基于 hyper 的高性能 HTTP 和 gRPC 客户端实现，与服务端保持技术栈一致性
//! 
//! ## 透明化设计理念
//! 
//! 本模块采用透明化设计，对最终用户隐藏底层 hyper 实现细节：
//! 
//! ### 优势
//! - **统一版本控制**: 避免用户项目与 rat_engine 的 hyper 版本冲突
//! - **简化依赖**: 用户无需显式引入 hyper 相关依赖
//! - **API 稳定性**: 即使底层 hyper 版本升级，用户代码无需修改
//! - **生态一致性**: 确保整个 rat_engine 生态使用相同的 HTTP 实现
//! 
//! ### HTTP 客户端使用方式
//! ```rust
//! use rat_engine::{RatHttpClient, RatHttpClientBuilder};
//! use std::time::Duration;
//! 
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 用户无需关心底层 hyper 实现
//! let client = RatHttpClientBuilder::new()
//!     .connect_timeout(Duration::from_secs(10))
//!     .request_timeout(Duration::from_secs(30))
//!     .enable_compression() // 支持 lz4 > zstd > gzip 自动协商
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//! 
//! ### gRPC+Bincode 客户端使用方式
//! ```rust
//! use rat_engine::{RatGrpcClient, RatGrpcClientBuilder, GrpcCompressionMode};
//! use std::time::Duration;
//! 
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 基于 bincode 2.x 的 gRPC 客户端
//! let grpc_client = RatGrpcClientBuilder::new()
//!     // base_uri 方法已弃用，现在在每次调用时传入完整 URI
//!     .connect_timeout(Duration::from_secs(10))?
//!     .request_timeout(Duration::from_secs(30))?
//!     .disable_compression() // 默认禁用压缩
//!     .build()?;
//! # Ok(())
//! # }
//! ```

pub mod builder;
pub mod http_client;
pub mod http_client_delegated;
pub mod grpc_client;
pub mod grpc_builder;
pub mod grpc_client_delegated;
pub mod download_metadata;
pub mod types;
pub mod connection_pool;

#[cfg(feature = "reqwest")]
pub mod independent_http_client;

// #[cfg(any(feature = "client", feature = "http-client"))]
// pub use builder::RatHttpClientBuilder;  // 已移除HTTP客户端，只保留gRPC客户端
// #[cfg(any(feature = "client", feature = "http-client"))]
// pub use http_client::{RatHttpClient, RatHttpResponse};  // 已移除HTTP客户端，只保留gRPC客户端
// #[cfg(any(feature = "client", feature = "http-client"))]
// pub use http_client_delegated::{HttpRequestHandler, HttpRequestManager};  // 已移除HTTP客户端，只保留gRPC客户端

#[cfg(any(feature = "client", feature = "grpc-client"))]
pub use grpc_client::{
    RatGrpcClient, GrpcRequest, GrpcResponse, GrpcCompressionMode,
    GrpcStreamMessage, GrpcStreamResponse, GrpcBidirectionalStream,
};
#[cfg(any(feature = "client", feature = "grpc-client"))]
pub use grpc_client_delegated::{
    ClientBidirectionalHandler, ClientStreamContext, ClientStreamSender,
    ClientBidirectionalManager,
};
#[cfg(any(feature = "client", feature = "grpc-client"))]
pub use grpc_builder::RatGrpcClientBuilder;

pub use types::*;

#[cfg(feature = "reqwest")]
pub use independent_http_client::{
    RatIndependentHttpClient, RatIndependentHttpResponse, SseStream, SseEvent,
    CompressionTestResult, RatIndependentHttpClientBuilder,
};