    use std::collections::HashMap;
use std::pin::Pin;
use futures_util::Stream;
use std::sync::Arc;
use h2::{server::SendResponse, RecvStream};
use hyper::http::{Request, Response, StatusCode, HeaderMap, HeaderValue};
use bytes;
use pin_project_lite::pin_project;
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{debug, info, error};
use super::request_handler_core::GrpcRequestHandler;
use super::request_stream::GrpcRequestStream;

impl GrpcRequestHandler {
    /// 提取 gRPC 方法名
    pub(crate) fn extract_grpc_method(&self, request: &Request<RecvStream>) -> Result<String, GrpcError> {
        let path = request.uri().path();
        debug!("🚨🚨🚨 [服务端] 提取 gRPC 方法路径: {}", path);
        if path.starts_with('/') {
            // 保留完整路径，包括前导斜杠，以匹配注册时的方法名
            debug!("🚨🚨🚨 [服务端] 返回方法名: {}", path);
            Ok(path.to_string())
        } else {
            Err(GrpcError::InvalidArgument("无效的 gRPC 方法路径".to_string()))
        }
    }
    
    /// 创建 gRPC 上下文
    pub(crate) fn create_grpc_context(&self, request: &Request<RecvStream>) -> GrpcContext {
        let mut metadata = HashMap::new();
        
        // 提取请求头作为元数据
        for (name, value) in request.headers() {
            if let Ok(value_str) = value.to_str() {
                metadata.insert(name.to_string(), value_str.to_string());
            }
        }
        
        // 从请求扩展中获取远程地址（如果可用）
        let remote_addr = request.extensions()
            .get::<std::net::SocketAddr>()
            .copied();
        
        GrpcContext {
            remote_addr,
            headers: metadata,
            method: GrpcMethodDescriptor::from_path(request.uri().path(), GrpcMethodType::Unary)
                .unwrap_or_else(|| GrpcMethodDescriptor::new("unknown", "unknown", GrpcMethodType::Unary)),
        }
    }
    
    /// 读取 gRPC 请求
    pub(crate) async fn read_grpc_request(&self, request: Request<RecvStream>) -> Result<GrpcRequest<Vec<u8>>, GrpcError> {
        // 先创建上下文以获取方法信息
        let context = self.create_grpc_context(&request);
        
        let mut body = request.into_body();
        let mut data = Vec::new();
        
        while let Some(chunk) = body.data().await {
            match chunk {
                Ok(bytes) => {
                    // 释放流控制容量
                    if let Err(e) = body.flow_control().release_capacity(bytes.len()) {
                        return Err(GrpcError::Internal(format!("释放流控制容量失败: {}", e)));
                    }
                    data.extend_from_slice(&bytes);
                }
                Err(e) => {
                    return Err(GrpcError::Internal(format!("读取请求体失败: {}", e)));
                }
            }
        }
        
        self.decode_grpc_request(&data, &context)
    }
    
    /// 解码 gRPC 请求
    fn decode_grpc_request(&self, data: &[u8], context: &GrpcContext) -> Result<GrpcRequest<Vec<u8>>, GrpcError> {
        // 使用统一的编解码器解析帧
        let payload = GrpcCodec::parse_frame(data)
            .map_err(|e| GrpcError::InvalidArgument(format!("解析 gRPC 帧失败: {}", e)))?;
        
        // 尝试反序列化为 GrpcRequest 结构体（客户端发送的是完整的 GrpcRequest）
        match GrpcCodec::decode::<GrpcRequest<Vec<u8>>>(&payload) {
            Ok(grpc_request) => {
                // 成功反序列化，直接返回
                Ok(grpc_request)
            },
            Err(_) => {
                // 反序列化失败，可能是原始数据，直接使用
                let request = GrpcRequest {
                    id: 0, // 默认 ID
                    method: context.method.method.clone(),
                    data: payload.to_vec(),
                    metadata: context.headers.clone(),
                };
                Ok(request)
            }
        }
    }
    
    /// 创建 gRPC 请求流
    pub(crate) fn create_grpc_request_stream(
        &self,
        request: Request<RecvStream>,
    ) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>> {
        debug!("🔍 [DEBUG] create_grpc_request_stream 开始");
        let body = request.into_body();
        debug!("🔍 [DEBUG] 获取请求体成功");
        let stream = GrpcRequestStream::new(body);
        debug!("🔍 [DEBUG] 创建 GrpcRequestStream 成功");
        let boxed_stream = Box::pin(stream);
        debug!("🔍 [DEBUG] 包装为 Pin<Box> 成功");
        boxed_stream
    }
    
    /// 编码 gRPC 消息
    pub(crate) fn encode_grpc_message(&self, message: &GrpcStreamMessage<Vec<u8>>) -> Result<Vec<u8>, GrpcError> {
        // 使用统一的编解码器编码并创建帧
        GrpcCodec::encode_frame(message)
            .map_err(|e| GrpcError::Internal(format!("编码 gRPC 流消息失败: {}", e)))
    }
    
    /// 发送 gRPC 响应
    pub(crate) async fn send_grpc_response(
        &self,
        mut respond: SendResponse<bytes::Bytes>,
        response: GrpcResponse<Vec<u8>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 直接使用 response.data，不再序列化整个 GrpcResponse 结构体
        // 因为 response.data 已经包含了序列化后的实际响应数据
        let response_data = response.data;
        
        // 构建 gRPC 消息格式（5字节头部 + 数据）
        let mut data = Vec::new();
        
        // 压缩标志（0 = 不压缩）
        data.push(0);
        
        // 消息长度
        let length = response_data.len() as u32;
        data.extend_from_slice(&length.to_be_bytes());
        
        // 消息数据
        data.extend_from_slice(&response_data);
        
        let http_response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "identity")
            .header("grpc-status", response.status.to_string())
            .body(())?;
        
        let mut send_stream = respond.send_response(http_response, false)?;
        
        // 容错处理：如果流已经关闭，不记录为错误
        if let Err(e) = send_stream.send_data(data.into(), false) {
            if e.to_string().contains("inactive stream") {
                info!("ℹ️ [服务端] 流已关闭，一元响应数据发送被忽略");
                return Ok(());
            } else {
                return Err(Box::new(e));
            }
        }
        
        // 发送 gRPC 状态
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&response.status.to_string())?);
        if !response.message.is_empty() {
            trailers.insert("grpc-message", HeaderValue::from_str(&response.message)?);
        }
        
        // 容错处理：如果流已经关闭，不记录为错误
        if let Err(e) = send_stream.send_trailers(trailers) {
            if e.to_string().contains("inactive stream") {
                info!("ℹ️ [服务端] 流已关闭，一元响应状态发送被忽略");
            } else {
                return Err(Box::new(e));
            }
        }
        
        Ok(())
    }
    
    /// 发送 gRPC 错误
    pub(crate) async fn send_grpc_error(
        &self,
        mut respond: SendResponse<bytes::Bytes>,
        error: GrpcError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let http_response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-status", error.status_code().as_u32().to_string())
            .header("grpc-message", error.message())
            .body(())?;
        
        if let Err(e) = respond.send_response(http_response, true) {
            let error_msg = e.to_string();
            if error_msg.contains("inactive stream") || 
               error_msg.contains("closed") || 
               error_msg.contains("broken pipe") ||
               error_msg.contains("connection reset") {
                info!("ℹ️ [服务端] 客户端连接已关闭，gRPC 错误响应发送被忽略");
            } else {
                error!("❌ 发送 gRPC 错误响应失败: {}", error_msg);
                return Err(e.into());
            }
        }
        
        Ok(())
    }
    
    /// 发送 gRPC 错误到流
    pub(crate) async fn send_grpc_error_to_stream(
        &self,
        send_stream: &mut h2::SendStream<bytes::Bytes>,
        error: GrpcError,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&error.status_code().as_u32().to_string())?);
        trailers.insert("grpc-message", HeaderValue::from_str(&error.message())?);
        
        match send_stream.send_trailers(trailers) {
            Ok(_) => Ok(()),
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("inactive stream") || 
                   error_msg.contains("closed") || 
                   error_msg.contains("broken pipe") ||
                   error_msg.contains("connection reset") {
                    info!("ℹ️ [服务端] 客户端连接已关闭，gRPC 错误发送被忽略");
                    Ok(())
                } else {
                    Err(Box::new(e))
                }
            }
        }
    }
    
    /// 发送 gRPC 状态
    pub(crate) async fn send_grpc_status(
        &self,
        send_stream: &mut h2::SendStream<bytes::Bytes>,
        status: GrpcStatusCode,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", HeaderValue::from_str(&status.as_u32().to_string())?);
        if !message.is_empty() {
            trailers.insert("grpc-message", HeaderValue::from_str(message)?);
        }
        
        if let Err(e) = send_stream.send_trailers(trailers) {
            let error_msg = e.to_string();
            if error_msg.contains("inactive stream") || 
               error_msg.contains("closed") || 
               error_msg.contains("broken pipe") ||
               error_msg.contains("connection reset") {
                info!("ℹ️ [服务端] 客户端连接已关闭，gRPC 状态发送被忽略");
            } else {
                return Err(Box::new(e));
            }
        }
        
        Ok(())
    }
}
