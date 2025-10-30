use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::pin::Pin;
use std::time::Instant;
use futures_util::Stream;
use h2::server::SendResponse;
use tokio::sync::broadcast;
use bytes;
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use serde::Serialize;

pub enum GrpcTask {
    /// 一元请求任务
    UnaryRequest {
        method: String,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
        respond: Option<SendResponse<bytes::Bytes>>,
    },
    /// 服务端流请求任务
    ServerStreamRequest {
        method: String,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
        respond: Option<SendResponse<bytes::Bytes>>,
    },
    /// 双向流数据任务
    BidirectionalData {
        method: String,
        request_stream: Option<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>>,
        context: GrpcContext,
        respond: Option<SendResponse<bytes::Bytes>>,
    },
}

impl std::fmt::Debug for GrpcTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrpcTask::UnaryRequest { method, .. } => {
                write!(f, "GrpcTask::UnaryRequest {{ method: {:?} }}", method)
            }
            GrpcTask::ServerStreamRequest { method, .. } => {
                write!(f, "GrpcTask::ServerStreamRequest {{ method: {:?} }}", method)
            }
            GrpcTask::BidirectionalData { method, .. } => {
                write!(f, "GrpcTask::BidirectionalData {{ method: {:?} }}", method)
            }
        }
    }
}

// 为 GrpcTask 实现 Send 和 Sync，确保可以在线程间安全传递
unsafe impl Send for GrpcTask {}
unsafe impl Sync for GrpcTask {}

/// gRPC 连接类型
#[derive(Debug, Clone, PartialEq)]
pub enum GrpcConnectionType {
    /// 客户端流连接（客户端向服务端流式发送数据）
    ClientStream,
    /// 服务端流连接（单向推送）
    ServerStream,
    /// 双向流连接（双向通信）
    BidirectionalStream,
}

/// gRPC 连接信息
#[derive(Debug, Clone)]
pub struct GrpcConnection {
    /// 连接ID
    pub connection_id: String,
    /// 用户ID
    pub user_id: String,
    /// 房间ID（可选）
    /// 
    /// 用于标识连接所属的逻辑房间或频道。当连接加入特定房间时，
    /// 该字段包含房间的唯一标识符，用于：
    /// - 房间内消息广播
    /// - 房间成员管理
    /// - 房间级别的权限控制
    /// 
    /// 如果连接未加入任何房间，则为 None
    pub room_id: Option<String>,
    /// 连接类型
    pub connection_type: GrpcConnectionType,
    /// 连接时间
    pub connected_at: Instant,
    /// 最后活跃时间
    pub last_active: Instant,
    /// 广播发送器
    pub broadcast_tx: broadcast::Sender<Vec<u8>>,
}
