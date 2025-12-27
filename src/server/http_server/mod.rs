//! HTTP 专用服务器模块
//!
//! 此模块包含 HTTP 连接处理的完整独立副本
//! 允许代码重复以确保稳定性

pub mod http_connection;
pub mod h2_request_handler;
pub mod streaming;
pub mod global_sse_manager;

pub use http_connection::handle_http_dedicated_connection;
pub use http_connection::handle_http1_connection;
pub use http_connection::handle_http1_connection_with_stream;
pub use http_connection::handle_tls_connection;
pub use http_connection::handle_h2_tls_connection;
