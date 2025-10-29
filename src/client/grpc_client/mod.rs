//! RAT Engine gRPC 客户端模块
//!
//! 这个模块包含了gRPC客户端的所有功能，拆分为多个子模块以提高可维护性。

// 重新导出所有公共类型和功能，保持向后兼容性
pub use core::*;
pub use grpc_compression::*;
pub use grpc_stream::*;
pub use grpc_security::*;
pub use grpc_calls::*;
pub use grpc_bidirectional_stream::*;
pub use grpc_client_stream::*;
pub use grpc_server_stream::*;
pub use message_utils::*;
pub use http_connection::*;

// 重新导出 gRPC 类型以保持向后兼容性
pub use crate::server::grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage};

// Python集成模块只在启用python特性时导出
#[cfg(feature = "python")]
pub use grpc_python::*;

// 子模块声明 - core模块必须在最前面
mod core;
mod grpc_compression;
mod grpc_stream;
mod grpc_security;
mod grpc_calls;
mod grpc_bidirectional_stream;
mod grpc_client_stream;
mod grpc_server_stream;
mod message_utils;
mod http_connection;

#[cfg(feature = "python")]
mod grpc_python;

// 使用 pub use 来重新导出所有类型，避免循环导入问题
pub use core::RatGrpcClient;
pub use grpc_compression::GrpcCompressionMode;
pub use grpc_stream::{GrpcStreamResponse, GrpcStreamSender, GrpcStreamReceiver};