  //! gRPC 服务端流模块

use std::pin::Pin;
use std::collections::HashMap;
use futures_util::{Stream, StreamExt};
use serde::{Serialize, Deserialize};
use async_stream::stream;

use crate::server::grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage};
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::{info, warn, debug, error};
use crate::error::{RatError, RatResult};
use crate::client::grpc_client::RatGrpcClient;
use hyper::{Request, Uri, Method};
use hyper::header::{HeaderMap, HeaderValue, CONTENT_TYPE, USER_AGENT, ACCEPT_ENCODING, CONTENT_ENCODING};
use http_body_util::Full;
use hyper::body::Bytes;
use h2::RecvStream;
use super::GrpcStreamResponse;
use super::GrpcStreamSender;

impl RatGrpcClient {
    ///
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据
    /// * `metadata` - 可选的元数据
    ///
    /// # 返回
    /// 返回服务端流响应
    /// 
    /// # 弃用警告
    /// 此方法已弃用，请使用 `call_server_stream_with_uri` 方法
    #[deprecated(note = "请使用 call_server_stream_with_uri 方法")]
    pub async fn call_server_stream<T, R>(
        &self, 
        service: &str, 
        method: &str, 
        request_data: T, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<GrpcStreamResponse<R>>
    where
        T: Serialize + Send + Sync + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        let stream_id = self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // 统一化处理：先序列化强类型数据为 Vec<u8>，然后包装到 GrpcRequest 中
        // 这样服务端就能接收到 GrpcRequest<Vec<u8>> 格式的数据，与 call_typed 保持一致
        let serialized_data = GrpcCodec::encode(&request_data)
            .map_err(|e| RatError::SerializationError(rat_embed_lang::tf("serialize_request_failed", &[("msg", &e.to_string())])))?;
        
        // 构建 gRPC 请求（使用序列化后的数据）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: serialized_data, // 使用序列化后的 Vec<u8> 数据
            metadata: metadata.unwrap_or_default(),
        };

        // 使用统一的编解码器编码并创建帧
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(rat_embed_lang::tf("encode_grpc_request_failed", &[("msg", &e.to_string())])))?;

        // 服务端流直接使用 gRPC 消息格式，不进行额外的 HTTP 压缩
        let compressed_data = Bytes::from(grpc_message);
        let content_encoding: Option<&'static str> = None;

        // 构建 HTTP 请求
        // 弃用方法 - 请使用 call_server_stream_with_uri
        let base_uri_str = "https://localhost:8080".trim_end_matches('/').to_string();
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", base_uri_str, path);
        let uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_uri", &[("msg", &e.to_string())])))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_user_agent_msg", &[("msg", &e.to_string())])));
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(self.compression_mode.accept_encoding()));
        headers.insert("grpc-stream-type", HeaderValue::from_static("server-stream"));
        
        if let Some(encoding) = content_encoding {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }

        let request = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Full::new(compressed_data))
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_request_failed", &[("msg", &e.to_string())])))?;

        // 添加头部
        let (mut parts, body) = request.into_parts();
        parts.headers = headers;
        let request = Request::from_parts(parts, body);

        // 发送 H2 流请求并获取流响应
        let h2_response = self.send_h2_request_stream(request).await?;
        let recv_stream = h2_response.into_body();
        let stream = self.create_server_stream(recv_stream);

        Ok(GrpcStreamResponse {
            stream_id,
            stream,
        })
    }

    /// 调用泛型服务端流 gRPC 方法（支持框架层统一序列化）
    /// 
    /// 类似于 call_typed，但用于服务端流调用
    /// 自动处理请求数据的 GrpcRequest 包装，保持与一元调用的一致性
    /// 
    /// # 参数
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据（强类型）
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回服务端流响应（强类型）
    pub async fn call_typed_server_stream<T, R>(
        &self, 
        service: &str, 
        method: &str, 
        request_data: T, 
        metadata: Option<HashMap<String, String>>
    ) -> RatResult<GrpcStreamResponse<R>>
    where
        T: Serialize + bincode::Encode + Send + Sync,
        R: bincode::Decode<()> + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        // 直接调用原始方法，使用强类型数据，让 call_server_stream 处理 GrpcRequest 包装
        self.call_server_stream::<T, R>(service, method, request_data, metadata).await
    }

    /// 调用服务端流 gRPC 方法（带 URI 参数）
    /// 
    /// 支持自定义服务器地址和协议，避免硬编码
    /// 
    /// # 参数
    /// * `uri` - 服务器 URI (例如: "https://localhost:8080")
    /// * `service` - 服务名称
    /// * `method` - 方法名称
    /// * `request_data` - 请求数据
    /// * `metadata` - 可选的元数据
    /// 
    /// # 返回
    /// 返回服务端流响应
    pub async fn call_server_stream_with_uri<T, R>(
        &self,
        uri: &str,
        service: &str,
        method: &str,
        request_data: T,
        metadata: Option<HashMap<String, String>>,
    ) -> RatResult<GrpcStreamResponse<R>>
    where
        T: Serialize + Send + Sync + bincode::Encode,
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        let stream_id = self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let request_id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // 统一化处理：先序列化强类型数据为 Vec<u8>，然后包装到 GrpcRequest 中
        // 这样服务端就能接收到 GrpcRequest<Vec<u8>> 格式的数据，与 call_typed 保持一致
        let serialized_data = GrpcCodec::encode(&request_data)
            .map_err(|e| RatError::SerializationError(rat_embed_lang::tf("serialize_request_failed", &[("msg", &e.to_string())])))?;
        
        // 构建 gRPC 请求（使用序列化后的数据）
        let grpc_request = GrpcRequest {
            id: request_id,
            method: format!("{}/{}", service, method),
            data: serialized_data, // 使用序列化后的 Vec<u8> 数据
            metadata: metadata.unwrap_or_default(),
        };

        // 使用统一的编解码器编码并创建帧
        let grpc_message = GrpcCodec::encode_frame(&grpc_request)
            .map_err(|e| RatError::SerializationError(rat_embed_lang::tf("encode_grpc_request_failed", &[("msg", &e.to_string())])))?;

        // 服务端流直接使用 gRPC 消息格式，不进行额外的 HTTP 压缩
        let compressed_data = Bytes::from(grpc_message);
        let content_encoding: Option<&'static str> = None;

        // 构建 HTTP 请求
        let base_uri_str = uri.trim_end_matches('/').to_string();
        let path = format!("/{}/{}", service, method);
        let full_uri = format!("{}{}", base_uri_str, path);
        let request_uri = full_uri
            .parse::<Uri>()
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_uri", &[("msg", &e.to_string())])))?;

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc+bincode"));
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("invalid_user_agent_msg", &[("msg", &e.to_string())])));
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(self.compression_mode.accept_encoding()));
        headers.insert("grpc-stream-type", HeaderValue::from_static("server-stream"));
        
        if let Some(encoding) = content_encoding {
            headers.insert(CONTENT_ENCODING, HeaderValue::from_static(encoding));
        }

        let request = Request::builder()
            .method(Method::POST)
            .uri(request_uri)
            .body(Full::new(compressed_data))
            .map_err(|e| RatError::RequestError(rat_embed_lang::tf("build_request_failed", &[("msg", &e.to_string())])))?;

        // 添加头部
        let (mut parts, body) = request.into_parts();
        parts.headers = headers;
        let request = Request::from_parts(parts, body);

        // 发送 H2 流请求并获取流响应
        let h2_response = self.send_h2_request_stream(request).await?;
        let recv_stream = h2_response.into_body();
        let stream = self.create_server_stream(recv_stream);

        Ok(GrpcStreamResponse {
            stream_id,
            stream,
        })
    }


    /// 创建服务端流 - 直接使用 H2 RecvStream
    fn create_server_stream<R>(&self, mut recv_stream: RecvStream) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<R>, RatError>> + Send>>
    where
        R: for<'de> Deserialize<'de> + Send + Sync + 'static + bincode::Decode<()>,
    {
        // 创建流来处理响应体
        let stream = async_stream::stream! {
            let mut buffer = Vec::new();
            let mut stream_ended = false;
            
            // 接收响应流数据
            while let Some(chunk_result) = recv_stream.data().await {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.extend_from_slice(&chunk);
                        // 释放流控制窗口
                        let _ = recv_stream.flow_control().release_capacity(chunk.len());
                        
                        // 尝试解析完整的 gRPC 消息
                        while buffer.len() >= 5 {
                            let compression_flag = buffer[0];
                            let message_length = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
                            
                            println!("DEBUG: [客户端] 解析 gRPC 消息头 - 压缩标志: {}, 消息长度: {}, 缓冲区总长度: {}", 
                                    compression_flag, message_length, buffer.len());
                            
                            if buffer.len() >= 5 + message_length {
                                let message_data = &buffer[5..5 + message_length];
                                
                                println!("DEBUG: [客户端] 提取消息数据，长度: {}, 前32字节: {:?}", 
                                        message_data.len(), 
                                        &message_data[..std::cmp::min(32, message_data.len())]);
                                
                                // 检查压缩标志
                                if compression_flag != 0 {
                                    yield Err(RatError::DeserializationError("不支持压缩的 gRPC 消息".to_string()));
                                    stream_ended = true;
                                    break;
                                }
                                
                                // 优化反序列化策略：先尝试直接反序列化为目标类型 R
                                // 如果失败，再尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                
                                // 策略1：如果 R 是 Vec<u8>，直接尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                if std::any::TypeId::of::<R>() == std::any::TypeId::of::<Vec<u8>>() {
                                    match GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(message_data) {
                                        Ok(stream_message) => {
                                            // 安全的类型转换
                                            let typed_message = GrpcStreamMessage {
                                                id: stream_message.id,
                                                stream_id: stream_message.stream_id,
                                                sequence: stream_message.sequence,
                                                end_of_stream: stream_message.end_of_stream,
                                                data: unsafe { std::mem::transmute_copy(&stream_message.data) },
                                                metadata: stream_message.metadata,
                                            };
                                            yield Ok(typed_message);
                                            
                                            // 如果是流结束标志，退出循环
                                            if stream_message.end_of_stream {
                                                stream_ended = true;
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            println!("DEBUG: [客户端] 反序列化 GrpcStreamMessage<Vec<u8>> 失败: {}", e);
                                            yield Err(RatError::DeserializationError(rat_embed_lang::tf("deserialize_grpc_stream_message_failed", &[("msg", &e.to_string())])));
                                            stream_ended = true;
                                            break;
                                        }
                                    }
                                } else {
                                    // 策略2：对于其他类型，先尝试直接反序列化为目标类型
                                    match GrpcCodec::decode::<R>(message_data) {
                                        Ok(data) => {
                                            println!("DEBUG: [客户端] 直接反序列化为目标类型成功！");
                                            // 创建一个简化的流消息结构
                                            let typed_message = GrpcStreamMessage {
                                                id: 0, // 简化处理
                                                stream_id: 0,
                                                sequence: 0,
                                                end_of_stream: false, // 由上层逻辑判断
                                                data,
                                                metadata: std::collections::HashMap::new(),
                                            };
                                            yield Ok(typed_message);
                                        }
                                        Err(_) => {
                                            // 如果直接反序列化失败，尝试反序列化为 GrpcStreamMessage<Vec<u8>>
                                            println!("DEBUG: [客户端] 直接反序列化失败，尝试 GrpcStreamMessage 包装格式");
                                            match GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(message_data) {
                                                Ok(stream_message) => {
                                                    // 尝试反序列化 data 字段为目标类型 R
                                                    println!("DEBUG: [客户端] 尝试反序列化 data 字段，数据长度: {}, 前32字节: {:?}", 
                                                            stream_message.data.len(), 
                                                            &stream_message.data[..std::cmp::min(32, stream_message.data.len())]);
                                                    match GrpcCodec::decode::<R>(&stream_message.data) {
                                                        Ok(data) => {
                                                            println!("DEBUG: [客户端] 反序列化成功！");
                                                            let typed_message = GrpcStreamMessage {
                                                                id: stream_message.id,
                                                                stream_id: stream_message.stream_id,
                                                                sequence: stream_message.sequence,
                                                                end_of_stream: stream_message.end_of_stream,
                                                                data,
                                                                metadata: stream_message.metadata,
                                                            };
                                                            yield Ok(typed_message);
                                                            
                                                            // 如果是流结束标志，退出循环
                                                            if stream_message.end_of_stream {
                                                                stream_ended = true;
                                                                break;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            println!("DEBUG: [客户端] 反序列化 data 字段失败: {}", e);
                                                            yield Err(RatError::DeserializationError(rat_embed_lang::tf("deserialize_data_field_failed", &[("msg", &e.to_string())])));
                                                            stream_ended = true;
                                                            break;
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    println!("DEBUG: [客户端] 反序列化 GrpcStreamMessage 失败: {}", e);
                                                    yield Err(RatError::DeserializationError(rat_embed_lang::tf("deserialize_grpc_stream_message_failed", &[("msg", &e.to_string())])));
                                                    stream_ended = true;
                                                    break;
                                                }
                                            }
                                        }
                                     }
                                 }
                                
                                // 移除已处理的数据
                                buffer.drain(0..5 + message_length);
                            } else {
                                // 数据不完整，等待更多数据
                                break;
                            }
                        }
                        
                        if stream_ended {
                            break;
                        }
                    }
                    Err(e) => {
                        yield Err(RatError::NetworkError(rat_embed_lang::tf("receive_stream_data_error", &[("msg", &e.to_string())])));
                        stream_ended = true;
                        break;
                    }
                }
            }
            
            // 检查 trailers 以获取 gRPC 状态
            if let Ok(trailers) = recv_stream.trailers().await {
                if let Some(trailers) = trailers {
                    if let Some(grpc_status) = trailers.get("grpc-status") {
                        if let Ok(status_str) = grpc_status.to_str() {
                            if let Ok(status_code) = status_str.parse::<u32>() {
                                if status_code != 0 {
                                    let grpc_message = trailers.get("grpc-message")
                                        .and_then(|v| v.to_str().ok())
                                        .unwrap_or("Unknown error");
                                    yield Err(RatError::Other(rat_embed_lang::tf("grpc_error_with_status", &[("status", &status_code.to_string()), ("message", &grpc_message)])));
                                }
                            }
                        }
                    }
                }
            }
        };

        Box::pin(stream)
    }
}
