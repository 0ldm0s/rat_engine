use h2::{server::SendResponse, RecvStream};
use hyper::http::Request;
use bytes;
use crate::server::grpc_types::*;
use super::handler_traits::UnaryHandler;
use super::request_handler_core::GrpcRequestHandler;

impl GrpcRequestHandler {
    /// 处理一元请求
    pub(crate) async fn handle_unary_request(
        &self,
        request: Request<RecvStream>,
        mut respond: SendResponse<bytes::Bytes>,
        handler: &dyn UnaryHandler,
        context: GrpcContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 读取请求体
        let grpc_request = self.read_grpc_request(request).await?;
        
        // 调用处理器
        match handler.handle(grpc_request, context).await {
            Ok(response) => {
                self.send_grpc_response(respond, response).await?;
            }
            Err(error) => {
                self.send_grpc_error(respond, error).await?;
            }
        }
        
        Ok(())
    }
}
