use std::sync::{Arc, RwLock};
use h2::{server::SendResponse, RecvStream};
use hyper::http::Request;
use bytes;
use crate::server::grpc_types::*;
use crate::utils::logger::{info, warn, debug, error};
use super::service_registry::GrpcServiceRegistry;
use super::types::*;

pub struct GrpcRequestHandler {
    registry: Arc<RwLock<GrpcServiceRegistry>>,
}

impl GrpcRequestHandler {
    /// 创建新的请求处理器
    pub fn new(registry: Arc<RwLock<GrpcServiceRegistry>>) -> Self {
        Self { registry }
    }
    
    /// 处理 gRPC 请求（集成无锁队列和向下委托）
    pub async fn handle_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let method = self.extract_grpc_method(&request)?;
        let context = self.create_grpc_context(&request);
        
        debug!("🔄 处理 gRPC 请求: {}", method);
        
        // 检查是否启用无锁模式
        let lockfree_enabled = {
            let registry = self.registry.read().unwrap();
            registry.lockfree_enabled
        };
        
        if lockfree_enabled {
            // 无锁模式：向下委托任务
            debug!("🚀 使用无锁模式处理 gRPC 请求");
            self.handle_request_lockfree(request, respond, method, context).await
        } else {
            // 传统模式：直接处理
            debug!("🔄 使用传统模式处理 gRPC 请求");
            self.handle_request_traditional(request, respond, method, context).await
        }
    }
    
    /// 无锁模式处理请求
    async fn handle_request_lockfree(
        &self,
        request: Request<RecvStream>,
        respond: SendResponse<bytes::Bytes>,
        method: String,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 获取处理器类型，避免长时间持有锁
        let handler_type = {
            let registry = self.registry.read().unwrap();
            if registry.get_unary_handler(&method).is_some() {
                Some("unary")
            } else if registry.get_server_stream_handler(&method).is_some() {
                Some("server_stream")
            } else if registry.get_bidirectional_handler(&method).is_some() {
                Some("bidirectional")
            } else {
                None
            }
        };
        
        match handler_type {
            Some("unary") => {
                // 读取请求体
                let grpc_request = self.read_grpc_request(request).await?;
                
                // 创建任务并委托
                let task = GrpcTask::UnaryRequest {
                    method: method.clone(),
                    request: grpc_request,
                    context,
                    respond: Some(respond),
                };
                
                let registry = self.registry.read().unwrap();
                registry.delegate_task(task);
                debug!("📤 一元请求已委托到无锁队列: {}", method);
            },
            Some("server_stream") => {
                // 读取请求体
                let grpc_request = self.read_grpc_request(request).await?;
                
                // 创建任务并委托
                let task = GrpcTask::ServerStreamRequest {
                    method: method.clone(),
                    request: grpc_request,
                    context,
                    respond: Some(respond),
                };
                
                let registry = self.registry.read().unwrap();
                registry.delegate_task(task);
                debug!("📤 服务端流请求已委托到无锁队列: {}", method);
            },
            Some("bidirectional") => {
                // 创建请求流
                let request_stream = self.create_grpc_request_stream(request);
                
                // 创建任务并委托
                let task = GrpcTask::BidirectionalData {
                    method: method.clone(),
                    request_stream: Some(request_stream),
                    context,
                    respond: Some(respond),
                };
                
                let registry = self.registry.read().unwrap();
                registry.delegate_task(task);
                debug!("📤 双向流请求已委托到无锁队列: {}", method);
            },
            _ => {
                // 方法未找到
                warn!("❌ gRPC 方法未找到: {}", method);
                self.send_grpc_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await?;
            }
        }
        
        Ok(())
    }
    
    /// 传统模式处理请求
    async fn handle_request_traditional(
        &self,
        request: Request<RecvStream>,
        respond: SendResponse<bytes::Bytes>,
        method: String,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("🚨🚨🚨 [服务端] 传统模式处理请求，方法: {}", method);
        
        // 获取处理器类型，避免长时间持有锁
        let handler_type = {
            let registry = self.registry.read().unwrap();
            debug!("🚨🚨🚨 [服务端] 检查一元处理器: {}", registry.get_unary_handler(&method).is_some());
            debug!("🚨🚨🚨 [服务端] 检查服务端流处理器: {}", registry.get_server_stream_handler(&method).is_some());
            debug!("🚨🚨🚨 [服务端] 检查客户端流处理器: {}", registry.get_client_stream_handler(&method).is_some());
            debug!("🚨🚨🚨 [服务端] 检查双向流处理器: {}", registry.get_bidirectional_handler(&method).is_some());
            
            if registry.get_unary_handler(&method).is_some() {
                Some("unary")
            } else if registry.get_server_stream_handler(&method).is_some() {
                Some("server_stream")
            } else if registry.get_client_stream_handler(&method).is_some() {
                Some("client_stream")
            } else if registry.get_bidirectional_handler(&method).is_some() {
                Some("bidirectional")
            } else {
                None
            }
        };
        
        debug!("🚨🚨🚨 [服务端] 处理器类型: {:?}", handler_type);
        
        match handler_type {
            Some("unary") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_unary_handler(&method).unwrap()
                };
                self.handle_unary_request(request, respond, &*handler, context).await
            },
            Some("server_stream") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_server_stream_handler(&method).unwrap()
                };
                self.handle_server_stream_request(request, respond, &*handler, context).await
            },
            Some("client_stream") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_client_stream_handler(&method).unwrap()
                };
                self.handle_client_stream_request(request, respond, &*handler, context).await
            },
            Some("bidirectional") => {
                let handler = {
                    let registry = self.registry.read().unwrap();
                    registry.get_bidirectional_handler(&method).unwrap()
                };
                self.handle_bidirectional_request(request, respond, &*handler, context).await
            },
            _ => {
                // 方法未找到
                warn!("❌ gRPC 方法未找到: {}", method);
                self.send_grpc_error(respond, GrpcError::Unimplemented(format!("方法未实现: {}", method))).await
            }
        }
    }
}
