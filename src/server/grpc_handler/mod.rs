// gRPC handler 模块
//
// 这个模块将原来庞大的 grpc_handler.rs 文件拆分为多个逻辑清晰的子模块

// 通用导入 - 所有子模块都需要
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::future::Future;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, Ordering};
use futures_util::{Stream, StreamExt, stream::StreamExt as _};
use h2::{server::SendResponse, RecvStream, Reason};
use hyper::http::{Request, Response, StatusCode, HeaderMap, HeaderValue};
use pin_project_lite::pin_project;
use tokio::sync::{mpsc, broadcast};
use bytes;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{info, warn, error, debug};
use crate::engine::work_stealing::WorkStealingQueue;
use serde::Serialize;

pub mod types;
pub mod connection_manager;
pub mod service_registry;
pub mod handler_traits;
pub mod request_handler_core;
pub mod unary_request_handler;
pub mod server_stream_handler;
pub mod client_stream_handler;
pub mod bidirectional_request_handler;
pub mod request_utils;
pub mod request_stream;
pub mod service_registry_default;

// 重新导出所有公共API，保持与原模块的兼容性

// 数据类型
pub use types::{
    GrpcTask,
    GrpcConnectionType,
    GrpcConnection,
};

// 连接管理
pub use connection_manager::{
    GrpcConnectionManager,
};

// 服务注册表
pub use service_registry::{
    GrpcServiceRegistry,
};

// 处理器接口
pub use handler_traits::{
    UnaryHandler,
    ServerStreamHandler,
    TypedServerStreamHandler,
    TypedServerStreamAdapter,
    ClientStreamHandler,
    BidirectionalHandler,
};

// 请求处理器核心
pub use request_handler_core::{
    GrpcRequestHandler,
};

// 工具方法
pub use request_utils::{
    // 注意：这些原本是 GrpcRequestHandler 的私有方法，
    // 现在可能需要调整为 pub(crate) 或提供公共接口
};

// 请求流
pub use request_stream::{
    GrpcRequestStream,
};

// 默认实现
pub use service_registry_default::{
    // GrpcServiceRegistry 的 Default 实现
};