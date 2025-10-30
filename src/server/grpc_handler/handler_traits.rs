use std::pin::Pin;
use futures_util::{Stream, StreamExt};
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use serde::Serialize;

pub trait UnaryHandler: Send + Sync {
    /// 处理一元请求
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>>;
}

/// 服务端流处理器特征（原始版本，用于向后兼容）
pub trait ServerStreamHandler: Send + Sync {
    /// 处理服务端流请求
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>>;
}

/// 泛型服务端流处理器特征（支持框架层统一序列化）
pub trait TypedServerStreamHandler<T>: Send + Sync 
where
    T: Serialize + bincode::Encode + Send + Sync + 'static,
{
    /// 处理服务端流请求，返回强类型的流
    fn handle_typed(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<T>, GrpcError>> + Send>>, GrpcError>> + Send>>;
}

/// 泛型服务端流处理器适配器
pub struct TypedServerStreamAdapter<T, H> {
    handler: H,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, H> TypedServerStreamAdapter<T, H> {
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// 为泛型处理器适配器实现原始处理器接口（自动序列化适配器）
impl<T, H> ServerStreamHandler for TypedServerStreamAdapter<T, H>
where
    T: Serialize + bincode::Encode + Send + Sync + 'static,
    H: TypedServerStreamHandler<T> + Clone + 'static,
{
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        // 克隆处理器以避免生命周期问题
        let handler = self.handler.clone();
        Box::pin(async move {
            // 调用强类型处理器
            let typed_stream = handler.handle_typed(request, context).await?;
            
            // 创建序列化适配器流
            let serialized_stream = typed_stream.map(|item| {
                match item {
                    Ok(typed_message) => {
                        // 序列化 data 字段
                        match GrpcCodec::encode(&typed_message.data) {
                            Ok(serialized_data) => Ok(GrpcStreamMessage {
                                id: typed_message.id,
                                stream_id: typed_message.stream_id,
                                sequence: typed_message.sequence,
                                end_of_stream: typed_message.end_of_stream,
                                data: serialized_data,
                                metadata: typed_message.metadata,
                            }),
                            Err(e) => Err(GrpcError::Internal(format!("序列化数据失败: {}", e))),
                        }
                    }
                    Err(e) => Err(e),
                }
            });
            
            Ok(Box::pin(serialized_stream) as Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>)
        })
    }
}

/// 客户端流处理器特征
pub trait ClientStreamHandler: Send + Sync {
    /// 处理客户端流请求
    fn handle(
        &self,
        request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>>;
}

/// 双向流处理器特征
pub trait BidirectionalHandler: Send + Sync {
    /// 处理双向流请求
    fn handle(
        &self,
        request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>>;
}
