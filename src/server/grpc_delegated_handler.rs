//! gRPC 委托处理器
//! 
//! 将 gRPC 请求完全委托给队列桥接适配器处理，不包含任何业务逻辑

use std::pin::Pin;
use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{Stream, wrappers::UnboundedReceiverStream};
use futures_util::StreamExt;
use uuid::Uuid;

use crate::error::{RatResult, RatError};
use crate::server::grpc_types::{
    GrpcRequest, GrpcResponse, GrpcStreamMessage, GrpcError,
    GrpcContext
};
use crate::server::grpc_handler::{
    UnaryHandler, ServerStreamHandler, ClientStreamHandler, BidirectionalHandler
};
use crate::server::grpc_queue_bridge_adapter::{
    QueueBridgeAdapter, ConnectionId, RequestType, RequestData, ResponseData,
    EngineToTransportMessage
};

/// 响应等待器
/// 用于等待来自 Python 引擎的响应
struct ResponseWaiter {
    adapter: Arc<QueueBridgeAdapter>,
    request_id: String,
    timeout: Duration,
}

impl ResponseWaiter {
    fn new(adapter: Arc<QueueBridgeAdapter>, request_id: String, timeout: Duration) -> Self {
        Self {
            adapter,
            request_id,
            timeout,
        }
    }

    /// 等待一元响应
    async fn wait_for_unary_response(&self) -> Result<GrpcResponse<Vec<u8>>, GrpcError> {
        let start_time = std::time::Instant::now();
        
        loop {
            // 检查超时
            if start_time.elapsed() > self.timeout {
                return Err(GrpcError::Internal("等待响应超时".to_string()));
            }
            
            // 处理来自引擎的消息
            let messages = match self.adapter.process_engine_messages().await {
                Ok(msgs) => msgs,
                Err(e) => return Err(GrpcError::Internal(format!("处理引擎消息失败: {}", e))),
            };
            
            for message in messages {
                if let EngineToTransportMessage::SendResponse { request_id, response, .. } = message {
                    if request_id == self.request_id {
                        return Ok(GrpcResponse {
                            data: response.body,
                            status: response.grpc_status.as_ref().map(|s| s.code).unwrap_or(0),
                            message: response.grpc_status.as_ref()
                                .map(|s| s.message.clone())
                                .unwrap_or_else(|| "OK".to_string()),
                            metadata: HashMap::new(),
                        });
                    }
                }
            }
            
            // 短暂等待后重试
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// 等待流响应
    async fn wait_for_stream_response(&self) -> Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let adapter = self.adapter.clone();
        let request_id = self.request_id.clone();
        let timeout = self.timeout;
        
        // 启动响应监听任务
        tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            
            loop {
                // 检查超时
                if start_time.elapsed() > timeout {
                    let _ = tx.send(Err(GrpcError::Internal("等待流响应超时".to_string())));
                    break;
                }
                
                // 处理来自引擎的消息
                if let Ok(messages) = adapter.process_engine_messages().await {
                    for message in messages {
                        match message {
                            EngineToTransportMessage::SendStreamData { request_id: msg_req_id, data, is_end, .. } => {
                                if msg_req_id == request_id {
                                    let response = GrpcStreamMessage {
                                        id: 0,
                                        stream_id: 0,
                                        sequence: 0,
                                        data,
                                        end_of_stream: is_end,
                                        metadata: HashMap::new(),
                                    };
                                    
                                    if tx.send(Ok(response)).is_err() {
                                        break;
                                    }
                                    
                                    if is_end {
                                        break;
                                    }
                                }
                            }
                            EngineToTransportMessage::SendResponse { request_id: msg_req_id, response, .. } => {
                                if msg_req_id == request_id {
                                    // 对于流请求，响应表示流结束
                                    if let Some(grpc_status) = response.grpc_status {
                                        if grpc_status.code != 0 {
                                            let _ = tx.send(Err(GrpcError::Internal(grpc_status.message)));
                                        }
                                    }
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                
                // 短暂等待后重试
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        
        let stream = UnboundedReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }
}

/// 委托一元处理器
pub struct DelegatedUnaryHandler {
    adapter: Arc<QueueBridgeAdapter>,
    timeout: Duration,
}

impl DelegatedUnaryHandler {
    pub fn new(adapter: Arc<QueueBridgeAdapter>) -> Self {
        Self {
            adapter,
            timeout: Duration::from_secs(30),
        }
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl UnaryHandler for DelegatedUnaryHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        let adapter = self.adapter.clone();
        let timeout = self.timeout;
        
        Box::pin(async move {
            let connection_id = ConnectionId::new();
            let request_id = Uuid::new_v4().to_string();
            
            // 通知连接建立
            if let Err(e) = adapter.notify_connection_established(
                connection_id.clone(),
                "grpc".to_string(),
                context.remote_addr.map(|addr| addr.to_string()).unwrap_or_default(),
                String::new(), // local_addr 不在 GrpcContext 中
            ).await {
                return Err(GrpcError::Internal(format!("连接建立失败: {}", e)));
            }
            
            // 构造请求数据
            let request_data = RequestData {
                method: Some("POST".to_string()),
                path: format!("/{}/{}", context.method.service, context.method.method),
                headers: context.headers.clone(),
                body: request.data,
                service: Some(context.method.service.clone()),
                grpc_method: Some(context.method.method.clone()),
                query_params: HashMap::new(),
            };
            
            // 通知请求接收
            if let Err(e) = adapter.notify_request_received(
                connection_id.clone(),
                request_id.clone(),
                RequestType::GrpcUnary,
                request_data,
            ).await {
                return Err(GrpcError::Internal(format!("请求接收失败: {}", e)));
            }
            
            // 等待响应
            let waiter = ResponseWaiter::new(adapter, request_id, timeout);
            waiter.wait_for_unary_response().await
        })
    }
}

/// 委托服务端流处理器
pub struct DelegatedServerStreamHandler {
    adapter: Arc<QueueBridgeAdapter>,
    timeout: Duration,
}

impl DelegatedServerStreamHandler {
    pub fn new(adapter: Arc<QueueBridgeAdapter>) -> Self {
        Self {
            adapter,
            timeout: Duration::from_secs(300), // 流请求超时时间更长
        }
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl ServerStreamHandler for DelegatedServerStreamHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        let adapter = self.adapter.clone();
        let timeout = self.timeout;
        
        Box::pin(async move {
            let connection_id = ConnectionId::new();
            let request_id = Uuid::new_v4().to_string();
            
            // 通知连接建立
            if let Err(e) = adapter.notify_connection_established(
                connection_id.clone(),
                "grpc".to_string(),
                context.remote_addr.map(|addr| addr.to_string()).unwrap_or_default(),
                String::new(), // local_addr 不在 GrpcContext 中
            ).await {
                return Err(GrpcError::Internal(format!("连接建立失败: {}", e)));
            }
            
            // 构造请求数据
            let request_data = RequestData {
                method: Some("POST".to_string()),
                path: format!("/{}/{}", context.method.service, context.method.method),
                headers: context.headers.clone(),
                body: request.data,
                service: Some(context.method.service.clone()),
                grpc_method: Some(context.method.method.clone()),
                query_params: HashMap::new(),
            };
            
            // 通知请求接收
            if let Err(e) = adapter.notify_request_received(
                connection_id.clone(),
                request_id.clone(),
                RequestType::GrpcServerStreaming,
                request_data,
            ).await {
                return Err(GrpcError::Internal(format!("请求接收失败: {}", e)));
            }
            
            // 等待流响应
            let waiter = ResponseWaiter::new(adapter, request_id, timeout);
            waiter.wait_for_stream_response().await
        })
    }
}

/// 委托客户端流处理器
pub struct DelegatedClientStreamHandler {
    adapter: Arc<QueueBridgeAdapter>,
    timeout: Duration,
}

impl DelegatedClientStreamHandler {
    pub fn new(adapter: Arc<QueueBridgeAdapter>) -> Self {
        Self {
            adapter,
            timeout: Duration::from_secs(300),
        }
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl ClientStreamHandler for DelegatedClientStreamHandler {
    fn handle(
        &self,
        mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        let adapter = self.adapter.clone();
        let timeout = self.timeout;
        
        Box::pin(async move {
            let connection_id = ConnectionId::new();
            let request_id = Uuid::new_v4().to_string();
            
            // 通知连接建立
            if let Err(e) = adapter.notify_connection_established(
                connection_id.clone(),
                "grpc".to_string(),
                context.remote_addr.map(|addr| addr.to_string()).unwrap_or_default(),
                String::new(), // local_addr 不在 GrpcContext 中
            ).await {
                return Err(GrpcError::Internal(format!("连接建立失败: {}", e)));
            }
            
            // 构造请求数据
            let request_data = RequestData {
                method: Some("POST".to_string()),
                path: format!("/{}/{}", context.method.service, context.method.method),
                headers: context.headers.clone(),
                body: Vec::new(), // 客户端流的初始请求体为空
                service: Some(context.method.service.clone()),
                grpc_method: Some(context.method.method.clone()),
                query_params: HashMap::new(),
            };
            
            // 通知请求接收
            if let Err(e) = adapter.notify_request_received(
                connection_id.clone(),
                request_id.clone(),
                RequestType::GrpcClientStreaming,
                request_data,
            ).await {
                return Err(GrpcError::Internal(format!("请求接收失败: {}", e)));
            }
            
            // 处理流数据
            let mut request_stream = request_stream;
            while let Some(stream_request) = request_stream.next().await {
                let stream_request = stream_request?;
                
                // 通知收到流数据
                if let Err(e) = adapter.notify_stream_data_received(
                    connection_id.clone(),
                    request_id.clone(),
                    stream_request.data,
                    false, // 不是结束
                ).await {
                    return Err(GrpcError::Internal(format!("流数据处理失败: {}", e)));
                }
            }
            
            // 通知流结束
            if let Err(e) = adapter.notify_stream_data_received(
                connection_id.clone(),
                request_id.clone(),
                Vec::new(),
                true, // 流结束
            ).await {
                return Err(GrpcError::Internal(format!("流结束通知失败: {}", e)));
            }
            
            // 等待最终响应
            let waiter = ResponseWaiter::new(adapter.clone(), request_id, timeout);
            match waiter.wait_for_unary_response().await {
                Ok(response) => Ok(GrpcResponse {
                    data: response.data,
                    status: response.status,
                    message: response.message,
                    metadata: response.metadata,
                }),
                Err(e) => Err(GrpcError::Internal(format!("等待响应失败: {:?}", e))),
            }
        })
    }
}

/// 委托双向流处理器
pub struct DelegatedBidirectionalHandler {
    adapter: Arc<QueueBridgeAdapter>,
    timeout: Duration,
}

impl DelegatedBidirectionalHandler {
    pub fn new(adapter: Arc<QueueBridgeAdapter>) -> Self {
        Self {
            adapter,
            timeout: Duration::from_secs(300),
        }
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl BidirectionalHandler for DelegatedBidirectionalHandler {
    fn handle(
        &self,
        mut request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        let adapter = self.adapter.clone();
        let timeout = self.timeout;
        
        Box::pin(async move {
            let connection_id = ConnectionId::new();
            let request_id = Uuid::new_v4().to_string();
            
            // 通知连接建立
            if let Err(e) = adapter.notify_connection_established(
                connection_id.clone(),
                "grpc".to_string(),
                context.remote_addr.map(|addr| addr.to_string()).unwrap_or_default(),
                String::new(), // local_addr 不在 GrpcContext 中
            ).await {
                return Err(GrpcError::Internal(format!("连接建立失败: {}", e)));
            }
            
            // 构造请求数据
            let request_data = RequestData {
                method: Some("POST".to_string()),
                path: format!("/{}/{}", context.method.service, context.method.method),
                headers: context.headers.clone(),
                body: Vec::new(), // 双向流的初始请求体为空
                service: Some(context.method.service.clone()),
                grpc_method: Some(context.method.method.clone()),
                query_params: HashMap::new(),
            };
            
            // 通知请求接收
            if let Err(e) = adapter.notify_request_received(
                connection_id.clone(),
                request_id.clone(),
                RequestType::GrpcBidirectionalStreaming,
                request_data,
            ).await {
                return Err(GrpcError::Internal(format!("请求接收失败: {}", e)));
            }
            
            // 启动输入流处理任务
            let adapter_clone = adapter.clone();
            let connection_id_clone = connection_id.clone();
            let request_id_clone = request_id.clone();
            
            tokio::spawn(async move {
                use futures_util::StreamExt;
                while let Some(stream_request) = request_stream.next().await {
                    if let Ok(stream_request) = stream_request {
                        let _ = adapter_clone.notify_stream_data_received(
                            connection_id_clone.clone(),
                            request_id_clone.clone(),
                            stream_request.data,
                            false,
                        ).await;
                    }
                }
                
                // 通知输入流结束
                let _ = adapter_clone.notify_stream_data_received(
                    connection_id_clone,
                    request_id_clone,
                    Vec::new(),
                    true,
                ).await;
            });
            
            // 等待输出流响应
            let waiter = ResponseWaiter::new(adapter, request_id, timeout);
            waiter.wait_for_stream_response().await
        })
    }
}