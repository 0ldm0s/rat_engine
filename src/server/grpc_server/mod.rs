//! gRPC 专用服务器模块
//!
//! 此模块包含 gRPC 连接处理的完整独立副本
//! 允许代码重复以确保稳定性

pub mod grpc_connection;
pub mod h2_request_handler;
pub mod streaming;
pub mod global_sse_manager;

pub use grpc_connection::handle_grpc_tls_connection;

// 内部函数（供外部调用，接受已建立的 TLS 连接）
pub use grpc_connection::handle_grpc_h2_connection_internal;
