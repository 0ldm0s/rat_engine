//! gRPC 类型定义和处理器
//! 
//! 提供基于 HTTP/2 的 gRPC 服务支持，使用 bincode 序列化
//! 支持一元请求、服务端流和双向流三种模式

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use bytes::Bytes;
use h2::{RecvStream, SendStream};
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use serde::{Serialize, Deserialize};
use tokio::sync::mpsc;
use futures_util::{Stream, StreamExt};
use pin_project_lite::pin_project;

// ============================================================================
// 一元请求/响应消息类型
// ============================================================================

/// gRPC 一元请求消息
/// 
/// 用于单次请求-响应模式的 gRPC 调用
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct GrpcUnaryRequest<T> {
    /// 请求 ID
    pub id: u64,
    /// 方法名
    pub method: String,
    /// 请求数据
    pub data: T,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// gRPC 一元响应消息
/// 
/// 用于单次请求-响应模式的 gRPC 调用
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct GrpcUnaryResponse<T> {
    /// 响应状态码
    pub status: u32,
    /// 状态消息
    pub message: String,
    /// 响应数据
    pub data: T,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

// ============================================================================
// 流式消息类型
// ============================================================================

/// gRPC 流式请求消息
/// 
/// 用于客户端流和双向流模式的 gRPC 调用
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct GrpcStreamRequest<T> {
    /// 消息 ID
    pub id: u64,
    /// 流 ID（用于标识特定的流）
    pub stream_id: u64,
    /// 消息序号（用于排序）
    pub sequence: u64,
    /// 请求数据
    pub data: T,
    /// 是否为流结束标记
    pub end_of_stream: bool,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// gRPC 流式响应消息
/// 
/// 用于服务端流和双向流模式的 gRPC 调用
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct GrpcStreamResponse<T> {
    /// 消息 ID
    pub id: u64,
    /// 流 ID（用于标识特定的流）
    pub stream_id: u64,
    /// 消息序号（用于排序）
    pub sequence: u64,
    /// 响应数据
    pub data: T,
    /// 是否为流结束标记
    pub end_of_stream: bool,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

// ============================================================================
// 兼容性类型别名（逐步迁移用）
// ============================================================================

/// 兼容性别名：GrpcRequest -> GrpcUnaryRequest
#[deprecated(note = "请使用 GrpcUnaryRequest 替代")]
pub type GrpcRequest<T> = GrpcUnaryRequest<T>;

/// 兼容性别名：GrpcResponse -> GrpcUnaryResponse
#[deprecated(note = "请使用 GrpcUnaryResponse 替代")]
pub type GrpcResponse<T> = GrpcUnaryResponse<T>;

/// 兼容性别名：GrpcStreamMessage -> GrpcStreamResponse
#[deprecated(note = "请使用 GrpcStreamResponse 替代")]
pub type GrpcStreamMessage<T> = GrpcStreamResponse<T>;

// ============================================================================
// 一元消息实现
// ============================================================================

impl<T> GrpcUnaryRequest<T> {
    /// 创建新的一元请求
    pub fn new(id: u64, method: impl Into<String>, data: T) -> Self {
        Self {
            id,
            method: method.into(),
            data,
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// 添加单个元数据项
    pub fn with_metadata_item(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

impl<T> GrpcUnaryResponse<T> {
    /// 创建成功响应
    pub fn success(data: T) -> Self {
        Self {
            status: 0, // gRPC OK
            message: "OK".to_string(),
            data,
            metadata: HashMap::new(),
        }
    }

    /// 创建错误响应
    pub fn error(status: u32, message: impl Into<String>) -> Self 
    where 
        T: Default,
    {
        Self {
            status,
            message: message.into(),
            data: T::default(),
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// 检查是否成功
    pub fn is_success(&self) -> bool {
        self.status == 0
    }
}

// ============================================================================
// 流式消息实现
// ============================================================================

impl<T> GrpcStreamRequest<T> {
    /// 创建新的流请求消息
    pub fn new(id: u64, stream_id: u64, sequence: u64, data: T) -> Self {
        Self {
            id,
            stream_id,
            sequence,
            data,
            end_of_stream: false,
            metadata: HashMap::new(),
        }
    }

    /// 创建流结束消息
    pub fn end_of_stream(id: u64, stream_id: u64, sequence: u64) -> Self 
    where 
        T: Default,
    {
        Self {
            id,
            stream_id,
            sequence,
            data: T::default(),
            end_of_stream: true,
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }
}

impl<T> GrpcStreamResponse<T> {
    /// 创建新的流响应消息
    pub fn new(id: u64, stream_id: u64, sequence: u64, data: T) -> Self {
        Self {
            id,
            stream_id,
            sequence,
            data,
            end_of_stream: false,
            metadata: HashMap::new(),
        }
    }

    /// 创建流结束消息
    pub fn end_of_stream(id: u64, stream_id: u64, sequence: u64) -> Self 
    where 
        T: Default,
    {
        Self {
            id,
            stream_id,
            sequence,
            data: T::default(),
            end_of_stream: true,
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// 检查是否为最后一条消息（兼容旧的 is_last 字段）
    pub fn is_last(&self) -> bool {
        self.end_of_stream
    }

    /// 设置为最后一条消息（兼容旧的 is_last 字段）
    pub fn set_last(&mut self, is_last: bool) {
        self.end_of_stream = is_last;
    }
}

// ============================================================================
// 兼容性实现（为了向后兼容）
// ============================================================================

// GrpcStreamMessage 是 GrpcStreamResponse 的类型别名，所以不需要重复实现方法

/// gRPC 状态码枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum GrpcStatusCode {
    /// 成功
    Ok = 0,
    /// 取消
    Cancelled = 1,
    /// 未知错误
    Unknown = 2,
    /// 无效参数
    InvalidArgument = 3,
    /// 超时
    DeadlineExceeded = 4,
    /// 未找到
    NotFound = 5,
    /// 已存在
    AlreadyExists = 6,
    /// 权限不足
    PermissionDenied = 7,
    /// 资源耗尽
    ResourceExhausted = 8,
    /// 前置条件失败
    FailedPrecondition = 9,
    /// 中止
    Aborted = 10,
    /// 超出范围
    OutOfRange = 11,
    /// 未实现
    Unimplemented = 12,
    /// 内部错误
    Internal = 13,
    /// 不可用
    Unavailable = 14,
    /// 数据丢失
    DataLoss = 15,
    /// 未认证
    Unauthenticated = 16,
}

impl GrpcStatusCode {
    /// 转换为 u32
    pub fn as_u32(self) -> u32 {
        self as u32
    }
    
    /// 从 u32 转换
    pub fn from_u32(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Ok),
            1 => Some(Self::Cancelled),
            2 => Some(Self::Unknown),
            3 => Some(Self::InvalidArgument),
            4 => Some(Self::DeadlineExceeded),
            5 => Some(Self::NotFound),
            6 => Some(Self::AlreadyExists),
            7 => Some(Self::PermissionDenied),
            8 => Some(Self::ResourceExhausted),
            9 => Some(Self::FailedPrecondition),
            10 => Some(Self::Aborted),
            11 => Some(Self::OutOfRange),
            12 => Some(Self::Unimplemented),
            13 => Some(Self::Internal),
            14 => Some(Self::Unavailable),
            15 => Some(Self::DataLoss),
            16 => Some(Self::Unauthenticated),
            _ => None,
        }
    }
}

/// gRPC 错误类型
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub enum GrpcError {
    /// 无效参数
    InvalidArgument(String),
    /// 未找到
    NotFound(String),
    /// 内部错误
    Internal(String),
    /// 未实现
    Unimplemented(String),
    /// 权限不足
    PermissionDenied(String),
    /// 资源耗尽
    ResourceExhausted(String),
    /// 超时
    DeadlineExceeded(String),
    /// 取消
    Cancelled(String),
    /// 未认证
    Unauthenticated(String),
    /// 自定义错误
    Custom { code: GrpcStatusCode, message: String, details: Option<String> },
}

impl GrpcError {
    /// 创建新的 gRPC 错误
    pub fn new(code: GrpcStatusCode, message: impl Into<String>) -> Self {
        let message = message.into();
        match code {
            GrpcStatusCode::InvalidArgument => Self::InvalidArgument(message),
            GrpcStatusCode::NotFound => Self::NotFound(message),
            GrpcStatusCode::Internal => Self::Internal(message),
            GrpcStatusCode::Unimplemented => Self::Unimplemented(message),
            GrpcStatusCode::PermissionDenied => Self::PermissionDenied(message),
            GrpcStatusCode::ResourceExhausted => Self::ResourceExhausted(message),
            GrpcStatusCode::DeadlineExceeded => Self::DeadlineExceeded(message),
            GrpcStatusCode::Cancelled => Self::Cancelled(message),
            GrpcStatusCode::Unauthenticated => Self::Unauthenticated(message),
            _ => Self::Custom { code, message, details: None },
        }
    }
    
    /// 添加错误详情
    pub fn with_details(self, details: impl Into<String>) -> Self {
        match self {
            Self::Custom { code, message, .. } => Self::Custom { 
                code, 
                message, 
                details: Some(details.into()) 
            },
            other => other, // 其他类型不支持详情
        }
    }
    
    /// 获取状态码
    pub fn status_code(&self) -> GrpcStatusCode {
        match self {
            Self::InvalidArgument(_) => GrpcStatusCode::InvalidArgument,
            Self::NotFound(_) => GrpcStatusCode::NotFound,
            Self::Internal(_) => GrpcStatusCode::Internal,
            Self::Unimplemented(_) => GrpcStatusCode::Unimplemented,
            Self::PermissionDenied(_) => GrpcStatusCode::PermissionDenied,
            Self::ResourceExhausted(_) => GrpcStatusCode::ResourceExhausted,
            Self::DeadlineExceeded(_) => GrpcStatusCode::DeadlineExceeded,
            Self::Cancelled(_) => GrpcStatusCode::Cancelled,
            Self::Unauthenticated(_) => GrpcStatusCode::Unauthenticated,
            Self::Custom { code, .. } => *code,
        }
    }
    
    /// 获取错误消息
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidArgument(msg) => msg,
            Self::NotFound(msg) => msg,
            Self::Internal(msg) => msg,
            Self::Unimplemented(msg) => msg,
            Self::PermissionDenied(msg) => msg,
            Self::ResourceExhausted(msg) => msg,
            Self::DeadlineExceeded(msg) => msg,
            Self::Cancelled(msg) => msg,
            Self::Unauthenticated(msg) => msg,
            Self::Custom { message, .. } => message,
        }
    }
}

impl std::fmt::Display for GrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for GrpcError {}

/// gRPC 结果类型
pub type GrpcResult<T> = Result<T, GrpcError>;

// ============================================================================
// 处理器类型定义
// ============================================================================

/// 一元 gRPC 处理器类型
/// 
/// 处理单个请求并返回单个响应
pub type UnaryGrpcHandler<Req, Resp> = Arc<
    dyn Fn(GrpcUnaryRequest<Req>) -> Pin<Box<dyn Future<Output = GrpcResult<GrpcUnaryResponse<Resp>>> + Send>>
        + Send
        + Sync,
>;

/// 服务端流 gRPC 处理器类型
/// 
/// 处理单个请求并返回响应流
pub type ServerStreamingGrpcHandler<Req, Resp> = Arc<
    dyn Fn(
        GrpcUnaryRequest<Req>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = GrpcResult<
                        Pin<Box<dyn Stream<Item = GrpcResult<GrpcStreamResponse<Resp>>> + Send>>,
                    >,
                > + Send,
        >,
    > + Send
        + Sync,
>;

/// 双向流 gRPC 处理器类型
/// 
/// 处理请求流并返回响应流
pub type BidirectionalStreamingGrpcHandler<Req, Resp> = Arc<
    dyn Fn(
        Pin<Box<dyn Stream<Item = GrpcResult<GrpcStreamRequest<Req>>> + Send>>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = GrpcResult<
                        Pin<Box<dyn Stream<Item = GrpcResult<GrpcStreamResponse<Resp>>> + Send>>,
                    >,
                > + Send,
        >,
    > + Send
        + Sync,
>;

// ============================================================================
// 兼容性处理器类型别名
// ============================================================================

/// 兼容性别名：使用旧的类型名称
#[deprecated(note = "请使用 UnaryGrpcHandler 替代")]
pub type LegacyUnaryGrpcHandler<Req, Resp> = Arc<
    dyn Fn(GrpcRequest<Req>) -> Pin<Box<dyn Future<Output = GrpcResult<GrpcResponse<Resp>>> + Send>>
        + Send
        + Sync,
>;

/// gRPC 方法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcMethodType {
    /// 一元请求
    Unary,
    /// 服务端流
    ServerStreaming,
    /// 双向流
    BidirectionalStreaming,
}

/// gRPC 方法描述符
#[derive(Debug, Clone)]
pub struct GrpcMethodDescriptor {
    /// 服务名
    pub service: String,
    /// 方法名
    pub method: String,
    /// 方法类型
    pub method_type: GrpcMethodType,
    /// 完整路径
    pub path: String,
}

impl GrpcMethodDescriptor {
    /// 创建新的方法描述符
    pub fn new(service: impl Into<String>, method: impl Into<String>, method_type: GrpcMethodType) -> Self {
        let service = service.into();
        let method = method.into();
        let path = format!("/{}/{}", service, method);
        
        Self {
            service,
            method,
            method_type,
            path,
        }
    }
    
    /// 从路径解析方法描述符
    pub fn from_path(path: &str, method_type: GrpcMethodType) -> Option<Self> {
        let path = path.strip_prefix('/').unwrap_or(path);
        let parts: Vec<&str> = path.split('/').collect();
        
        if parts.len() == 2 {
            Some(Self::new(parts[0], parts[1], method_type))
        } else {
            None
        }
    }
}

/// gRPC 上下文信息
#[derive(Debug, Clone)]
pub struct GrpcContext {
    /// 远程地址
    pub remote_addr: Option<std::net::SocketAddr>,
    /// 请求头
    pub headers: HashMap<String, String>,
    /// 方法描述符
    pub method: GrpcMethodDescriptor,
}

pin_project! {
    /// gRPC 流包装器
    pub struct GrpcStream<T> {
        #[pin]
        inner: mpsc::Receiver<GrpcResult<T>>,
    }
}

impl<T> GrpcStream<T> {
    /// 创建新的 gRPC 流
    pub fn new(receiver: mpsc::Receiver<GrpcResult<T>>) -> Self {
        Self { inner: receiver }
    }
}

impl<T> Stream for GrpcStream<T> {
    type Item = GrpcResult<T>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.project();
        match this.inner.poll_recv(cx) {
            std::task::Poll::Ready(Some(item)) => std::task::Poll::Ready(Some(item)),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}



/// gRPC 状态码常量
pub mod status_codes {
    /// 成功
    pub const OK: u32 = 0;
    /// 取消
    pub const CANCELLED: u32 = 1;
    /// 未知错误
    pub const UNKNOWN: u32 = 2;
    /// 无效参数
    pub const INVALID_ARGUMENT: u32 = 3;
    /// 超时
    pub const DEADLINE_EXCEEDED: u32 = 4;
    /// 未找到
    pub const NOT_FOUND: u32 = 5;
    /// 已存在
    pub const ALREADY_EXISTS: u32 = 6;
    /// 权限不足
    pub const PERMISSION_DENIED: u32 = 7;
    /// 资源耗尽
    pub const RESOURCE_EXHAUSTED: u32 = 8;
    /// 前置条件失败
    pub const FAILED_PRECONDITION: u32 = 9;
    /// 中止
    pub const ABORTED: u32 = 10;
    /// 超出范围
    pub const OUT_OF_RANGE: u32 = 11;
    /// 未实现
    pub const UNIMPLEMENTED: u32 = 12;
    /// 内部错误
    pub const INTERNAL: u32 = 13;
    /// 不可用
    pub const UNAVAILABLE: u32 = 14;
    /// 数据丢失
    pub const DATA_LOSS: u32 = 15;
    /// 未认证
    pub const UNAUTHENTICATED: u32 = 16;
}