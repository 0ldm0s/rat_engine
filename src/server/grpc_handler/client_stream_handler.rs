    use h2::{server::SendResponse, RecvStream};
use hyper::http::{Request, Response, StatusCode};
use bytes;
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use super::handler_traits::ClientStreamHandler;
use super::request_handler_core::GrpcRequestHandler;

impl GrpcRequestHandler {
    /// 处理客户端流请求
    pub(crate) async fn handle_client_stream_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn ClientStreamHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 对于客户端流，需要先发送响应头让客户端知道连接已建立
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "identity")
            .body(())?;
        
        let mut send_stream = respond.send_response(response, false)?;
         // 创建请求流
        let request_stream = self.create_grpc_request_stream(request);
        
        // 调用处理器
        match handler.handle(request_stream, context).await {
            Ok(response) => {
                          // 直接发送 GrpcResponse 数据，不包装成 GrpcStreamMessage
                let data = GrpcCodec::encode_frame(&response)
                    .map_err(|e| GrpcError::Internal(format!("编码 gRPC 响应失败: {}", e)))?;
                send_stream.send_data(data.into(), false)?;
                // 发送 gRPC 状态
                self.send_grpc_status(&mut send_stream, GrpcStatusCode::Ok, "").await?;
            }
            Err(error) => {
                         self.send_grpc_error_to_stream(&mut send_stream, error).await?;
            }
        }
        
        Ok(())
    }
}
