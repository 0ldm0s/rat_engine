    use std::pin::Pin;
use h2::{server::SendResponse, RecvStream};
use hyper::http::{Request, Response, StatusCode, HeaderMap, HeaderValue};
use bytes;
use futures_util::StreamExt;
use crate::server::grpc_types::*;
use crate::utils::logger::{debug, info};
use super::handler_traits::BidirectionalHandler;
use super::request_handler_core::GrpcRequestHandler;

impl GrpcRequestHandler {
    /// 处理双向流请求
    pub(crate) async fn handle_bidirectional_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn BidirectionalHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("🔍 [DEBUG] handle_bidirectional_request 开始");
        
        // 创建请求流
        debug!("🔍 [DEBUG] 准备创建请求流");
        let request_stream = self.create_grpc_request_stream(request);
        debug!("🔍 [DEBUG] 请求流创建完成");
        
        // 调用处理器
        debug!("🔍 [DEBUG] 准备调用双向流处理器");
        match handler.handle(request_stream, context).await {
            Ok(mut response_stream) => {
                debug!("🔍 [DEBUG] 双向流处理器调用成功，准备发送响应头");
                
                // 发送响应头
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/grpc")
                    .header("grpc-encoding", "identity")
                    .body(())?;
                
                debug!("🔍 [DEBUG] 响应头构建完成，准备发送");
                let mut send_stream = respond.send_response(response, false)?;
                debug!("🔍 [DEBUG] 响应头发送成功，开始处理响应流");
                
                let mut stream_closed = false;
                
                // 发送流数据
                while let Some(result) = response_stream.next().await {
                    debug!("🔍 [DEBUG] 收到响应流数据");
                    match result {
                        Ok(message) => {
                            debug!("🔍 [DEBUG] 编码响应消息");
                            let data = self.encode_grpc_message(&message)?;
                            debug!("🔍 [DEBUG] 发送响应数据");
                            if let Err(e) = send_stream.send_data(data.into(), false) {
                                let error_msg = e.to_string();
                                if error_msg.contains("inactive stream") || 
                                   error_msg.contains("closed") || 
                                   error_msg.contains("broken pipe") ||
                                   error_msg.contains("connection reset") {
                                    info!("ℹ️ [服务端] 客户端连接已关闭，停止发送数据");
                                    stream_closed = true;
                                    break;
                                } else {
                                    return Err(e.into());
                                }
                            }
                        }
                        Err(error) => {
                            debug!("🔍 [DEBUG] 响应流出现错误: {:?}", error);
                            self.send_grpc_error_to_stream(&mut send_stream, error).await?;
                            break;
                        }
                    }
                }
                
                debug!("🔍 [DEBUG] 响应流处理完成");
                
                // 只有在流未关闭时才发送 gRPC 状态
                if !stream_closed {
                    debug!("🔍 [DEBUG] 发送 gRPC 状态");
                    self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await?;
                }
                
                debug!("🔍 [DEBUG] handle_bidirectional_request 成功完成");
            }
            Err(error) => {
                debug!("🔍 [DEBUG] 双向流处理器调用失败: {:?}", error);
                self.send_grpc_error(respond, error).await?;
            }
        }
        
        Ok(())
    }
}
