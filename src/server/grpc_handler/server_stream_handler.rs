    use std::pin::Pin;
use h2::{server::SendResponse, RecvStream};
use hyper::http::{Request, Response, StatusCode, HeaderMap, HeaderValue};
use bytes;
use futures_util::StreamExt;
use crate::server::grpc_types::*;
use crate::utils::logger::{info, warn, error, debug};
use super::handler_traits::ServerStreamHandler;
use super::request_handler_core::GrpcRequestHandler;

impl GrpcRequestHandler {
    /// 处理服务端流请求
    pub(crate) async fn handle_server_stream_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn ServerStreamHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 读取请求体
        let grpc_request = self.read_grpc_request(request).await?;
        
        // 调用处理器
        match handler.handle(grpc_request, context).await {
            Ok(mut stream) => {
                // 发送响应头
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/grpc")
                    .header("grpc-encoding", "identity")
                    .body(())?;
                
                let mut send_stream = match respond.send_response(response, false) {
                    Ok(stream) => stream,
                    Err(e) => {
                        // 如果发送响应头失败，可能是连接已关闭
                        if e.to_string().contains("inactive stream") || e.to_string().contains("closed") {
                            info!("ℹ️ [服务端] 客户端连接已关闭，无法发送响应头");
                            return Ok(());
                        }
                        return Err(Box::new(e));
                    }
                };
                
                let mut stream_closed = false;
                let mut error_sent = false;
                
                // 发送流数据
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(message) => {
                            let data = match self.encode_grpc_message(&message) {
                                Ok(data) => data,
                                Err(e) => {
                                    error!("❌ 编码 gRPC 消息失败: {}", e);
                                    break;
                                }
                            };
                            
                            // 发送数据时检查连接状态
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
                                    error!("❌ 发送数据失败: {}", error_msg);
                                    break;
                                }
                            }
                        }
                        Err(error) => {
                            // 尝试发送错误，但如果连接已关闭则忽略
                            let _ = self.send_grpc_error_to_stream(&mut send_stream, error).await;
                            error_sent = true;
                            break;
                        }
                    }
                }
                
                // 只有在流未关闭且未发送错误时才发送正常的 gRPC 状态
                if !stream_closed && !error_sent {
                    let _ = self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await;
                }
            }
            Err(error) => {
                // 对于服务端流，当处理器返回错误时，先发送正常的响应头，然后通过 trailers 发送错误
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/grpc")
                    .header("grpc-encoding", "identity")
                    .body(())?;
                
                match respond.send_response(response, false) {
                    Ok(mut send_stream) => {
                        // 通过 trailers 发送错误状态
                        let _ = self.send_grpc_error_to_stream(&mut send_stream, error).await;
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("inactive stream") || 
                           error_msg.contains("closed") || 
                           error_msg.contains("broken pipe") ||
                           error_msg.contains("connection reset") {
                            info!("ℹ️ [服务端] 客户端连接已关闭，错误响应发送被忽略");
                        } else {
                            error!("❌ 发送服务端流响应头失败: {}", error_msg);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}
