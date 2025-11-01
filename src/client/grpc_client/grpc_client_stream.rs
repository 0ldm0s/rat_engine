  //! gRPC 客户端流模块

use std::pin::Pin;
use std::sync::Arc;
use std::collections::HashMap;
use futures_util::{Stream, StreamExt};
use serde::{Serialize, Deserialize};

use crate::server::grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage};
use crate::server::grpc_codec::GrpcCodec;
use crate::client::grpc_client_delegated::ClientStreamContext;
use crate::utils::logger::{info, warn, debug, error};
use crate::error::{RatError, RatResult};
use crate::client::grpc_client::RatGrpcClient;
use hyper::{Request, Uri, Method};
use hyper::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT};
use http_body_util::Full;
use hyper::body::Bytes;
use tokio::sync::mpsc;
use super::GrpcStreamResponse;
use super::GrpcStreamSender;

impl RatGrpcClient {
    /// 创建客户端流连接（统一化版本，用于分块上传等场景）
    /// 
    /// 复用双向流的底层机制，但只使用发送端，适合大文件分块上传
    /// 使用 GrpcCodec 统一编码解码器，确保与服务端的一致性
    /// 
    /// # 参数
    /// * `uri` - 服务器 URI
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回客户端流发送端和强类型响应数据的接收器
    pub async fn call_client_stream_with_uri<S, R>(
        &self, 
        uri: &str,
        service: &str, 
        method: &str, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<(GrpcStreamSender<S>, tokio::sync::oneshot::Receiver<RatResult<R>>)>
    where
        S: Serialize + Send + Sync + 'static + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        let base_uri: Uri = uri.parse()
            .map_err(|e| RatError::ConfigError(rat_embed_lang::tf("invalid_uri", &[("msg", &e.to_string())])))?;
        
        // 从连接池获取连接
        let connection = self.connection_pool.get_connection(&base_uri).await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("get_connection_failed", &[("msg", &e.to_string())])))?;
        let mut send_request = connection.send_request.clone();

        // 构建请求路径
        let path = format!("/{}/{}", service, method);

        // 创建客户端流请求（复用双向流的请求构建方式）
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(CONTENT_TYPE, "application/grpc")
            .header("grpc-stream-type", "client-stream")
            .header(USER_AGENT, &self.user_agent)
            .body(())
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_client_stream_request_failed", &[("msg", &e.to_string())])))?;

        // 发送请求并获取响应流（复用双向流的发送方式）
        let (response, send_stream) = send_request.send_request(request, false)
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("send_client_stream_request_failed", &[("msg", &e.to_string())])))?;

        // 等待响应头
        let response = response.await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("receive_client_stream_response_failed", &[("msg", &e.to_string())])))?;

        let receive_stream = response.into_body();

        // 创建发送通道
        let (send_tx, send_rx) = mpsc::unbounded_channel::<Bytes>();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        // 启动发送任务（复用双向流的发送逻辑）
        let connection_id = connection.connection_id.clone();
        let connection_pool = self.connection_pool.clone();
        let send_task = {
            let mut send_stream = send_stream;
            tokio::spawn(async move {
                let mut send_rx = send_rx;
                let mut message_sent = false;
                
                while let Some(data) = send_rx.recv().await {
                    message_sent = true;
                    
                    // 构建 gRPC 消息帧
                    let frame = GrpcCodec::create_frame(&data);
                    
                    if let Err(e) = send_stream.send_data(Bytes::from(frame), false) {
                        error!("客户端流发送数据失败: {}", e);
                        break;
                    }
                }
                
                // 注意：结束信号已经通过 send_close() 方法发送，这里不需要重复发送
                // 只需要关闭底层的 H2 流
                if message_sent {
                    if let Err(e) = send_stream.send_data(Bytes::new(), true) {
                        if e.to_string().contains("inactive stream") {
                            info!("ℹ️ [客户端流] 流已关闭，H2 结束信号发送被忽略");
                        } else {
                            error!("❌ [客户端流] 发送 H2 结束信号失败: {}", e);
                        }
                    } else {
                        info!("✅ [客户端流] H2 流已结束");
                    }
                }
                
                // 释放连接回连接池
                connection_pool.release_connection(&connection_id);
                info!("客户端流发送完成，连接已释放");
            })
        };

        // 启动响应接收任务（统一化版本，使用GrpcCodec解码）
        let recv_task = {
            let mut receive_stream = receive_stream;
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                
                // 接收响应数据
                while let Some(chunk_result) = receive_stream.data().await {
                    match chunk_result {
                        Ok(chunk) => buffer.extend_from_slice(&chunk),
                        Err(e) => {
                            let _ = response_tx.send(Err(RatError::NetworkError(rat_embed_lang::tf("receive_response_data_failed", &[("msg", &e.to_string())]))));
                            return;
                        }
                    }
                }
                
                // 使用 GrpcCodec 统一解码响应数据
                if buffer.is_empty() {
                    let _ = response_tx.send(Err(RatError::NetworkError("接收到空响应".to_string())));
                    return;
                }
                
                // 解析 gRPC 响应帧
                match GrpcCodec::decode_frame::<GrpcResponse<Vec<u8>>>(&buffer) {
                    Ok(grpc_response) => {
                        // 解码业务数据
                        match GrpcCodec::decode::<R>(&grpc_response.data) {
                            Ok(response_data) => {
                                let _ = response_tx.send(Ok(response_data));
                            }
                            Err(e) => {
                                let _ = response_tx.send(Err(RatError::SerializationError(rat_embed_lang::tf("decode_response_data_failed", &[("msg", &e.to_string())]))));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = response_tx.send(Err(RatError::SerializationError(rat_embed_lang::tf("decode_grpc_response_frame_failed", &[("msg", &e.to_string())]))));
                    }
                }
            })
        };

        Ok((GrpcStreamSender::new(send_tx), response_rx))
    }
}
