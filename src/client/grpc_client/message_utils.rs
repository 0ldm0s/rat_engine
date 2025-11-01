    //! gRPC 消息处理工具模块

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use hyper::{StatusCode, HeaderMap, Request, Response};
use hyper::header::{HeaderValue, CONTENT_TYPE};
use hyper::body::Bytes;
use serde::{Serialize, Deserialize};
use bincode;

use crate::error::{RatError, RatResult};
use crate::compression::{CompressionType, CompressionConfig};
use crate::server::grpc_types::{GrpcResponse, GrpcStreamMessage};
use crate::server::grpc_codec::GrpcCodec;
use crate::client::grpc_client_delegated::ClientStreamContext;
use crate::utils::logger::{info, debug, error};
use http_body_util::{Full, BodyExt};
use super::GrpcCompressionMode;
use crate::client::grpc_client::RatGrpcClient;

impl RatGrpcClient {
    /// 构建标准 gRPC 消息格式
    ///
    /// gRPC 消息格式：[压缩标志(1字节)][长度(4字节)][数据]
    fn build_grpc_message(&self, data: &[u8]) -> Vec<u8> {
        let mut message = Vec::with_capacity(5 + data.len());
        
        // 压缩标志（0 = 不压缩）
        message.push(0);
        
        // 消息长度（大端序）
        let length = data.len() as u32;
        let length_bytes = length.to_be_bytes();
        message.extend_from_slice(&length_bytes);
        
        // 消息数据
        message.extend_from_slice(data);
        

        
        message
    }

    /// 解析标准 gRPC 消息格式
    /// 
    /// 从 gRPC 消息格式中提取实际数据：[压缩标志(1字节)][长度(4字节)][数据]
    fn parse_grpc_message(&self, data: &[u8]) -> RatResult<Vec<u8>> {
        if data.len() < 5 {
            return Err(RatError::DecodingError("gRPC 消息太短".to_string()));
        }
        
        let compressed = data[0] != 0;
        let length = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
        
        // 添加详细的调试日志
        eprintln!("=== DEBUG: [客户端] 解析 gRPC 消息: 总长度={} bytes, 压缩标志={}, 声明长度={} bytes ===", 
                 data.len(), compressed, length);
        eprintln!("=== DEBUG: [客户端] 消息头部字节: {:?} ===", &data[..std::cmp::min(10, data.len())]);
        println!("DEBUG: [客户端] 解析 gRPC 消息: 总长度={} bytes, 压缩标志={}, 声明长度={} bytes", 
                 data.len(), compressed, length);
        println!("DEBUG: [客户端] 消息头部字节: {:?}", &data[..std::cmp::min(10, data.len())]);
        info!("🔍 [客户端] 解析 gRPC 消息: 总长度={} bytes, 压缩标志={}, 声明长度={} bytes", 
                         data.len(), compressed, length);
        info!("🔍 [客户端] 消息头部字节: {:?}", &data[..std::cmp::min(10, data.len())]);
        
        // 添加合理的长度限制，防止容量溢出（最大 100MB）
        const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
        if length > MAX_MESSAGE_SIZE {
            error!("❌ [客户端] gRPC 消息长度异常: {} 字节 > {} 字节", length, MAX_MESSAGE_SIZE);
            return Err(RatError::DecodingError(format!(
                "gRPC 消息长度过大: {} 字节，最大允许: {} 字节", 
                length, MAX_MESSAGE_SIZE
            )));
        }
        
        if data.len() < 5 + length {
            error!("❌ [客户端] gRPC 消息长度不匹配: 期望 {} 字节，实际 {} 字节", 5 + length, data.len());
            return Err(RatError::DecodingError(format!(
                "gRPC 消息长度不匹配: 期望 {} 字节，实际 {} 字节", 
                5 + length, data.len()
            )));
        }
        
        if compressed {
            return Err(RatError::DecodingError("不支持压缩的 gRPC 消息".to_string()));
        }
        
        info!("✅ [客户端] gRPC 消息解析成功，提取数据长度: {} bytes", length);
        Ok(data[5..5 + length].to_vec())
    }

    /// 压缩数据
    fn compress_data(&self, data: Bytes) -> RatResult<(Bytes, Option<&'static str>)> {
        match self.compression_mode {
            GrpcCompressionMode::Disabled => Ok((data, None)),
            GrpcCompressionMode::Lz4 => {
                #[cfg(feature = "compression")]
                {
                    let compressed = lz4_flex::block::compress(&data);
                    Ok((Bytes::from(compressed), Some("lz4")))
                }
                #[cfg(not(feature = "compression"))]
                {
                    Err(RatError::Other("LZ4 压缩功能未启用".to_string()))
                }
            }
        }
    }

    /// 解压缩数据
    fn decompress_data(&self, data: Bytes, encoding: Option<&HeaderValue>) -> RatResult<Bytes> {
        let encoding = match encoding {
            Some(value) => match value.to_str() {
                Ok(s) => s,
                Err(_) => return Ok(data), // 无法解析编码，返回原始数据
            },
            None => return Ok(data), // 没有编码头，返回原始数据
        };

        match encoding.to_lowercase().as_str() {
            "lz4" => {
                #[cfg(feature = "compression")]
                {
                    let decompressed = lz4_flex::block::decompress(&data, data.len() * 4)
                        .map_err(|e| RatError::DecodingError(rat_embed_lang::tf("lz4_decompress_failed", &[("msg", &e.to_string())])))?;
                    Ok(Bytes::from(decompressed))
                }
                #[cfg(not(feature = "compression"))]
                {
                    Err(RatError::DecodingError("LZ4 压缩功能未启用".to_string()))
                }
            },
            "identity" | "" => Ok(data),
            _ => Ok(data), // 未知编码，返回原始数据
        }
    }

    /// 发送 gRPC 请求 - 统一使用 h2 依赖
    /// 
    /// gRPC 本身就不支持 HTTP/1.1，所以统一使用 h2 crate 处理 HTTP/2 和 H2C
    /// 直接返回响应数据，不再考虑 Hyper 兼容性
    pub async fn send_request(&self, request: Request<Full<Bytes>>) -> RatResult<(StatusCode, HeaderMap, Bytes)> {
        // gRPC 统一使用 h2 依赖，根据 URI scheme 决定是否使用 TLS
        let response = self.send_h2_request(request).await?;
        
        // 直接提取响应数据
        let (parts, body) = response.into_parts();
        let body_bytes = body.collect().await
            .map_err(|e| RatError::NetworkError(rat_embed_lang::tf("read_response_failed", &[("msg", &e.to_string())])))?
            .to_bytes();
        
        Ok((parts.status, parts.headers, body_bytes))
    }
    /// 解析 gRPC 响应
    pub fn parse_grpc_response<R>(&self, status: StatusCode, headers: HeaderMap, body_bytes: Bytes) -> RatResult<GrpcResponse<R>>
    where
        R: for<'de> Deserialize<'de> + bincode::Decode<()>,
    {
        // 检查 HTTP 状态码
        if !status.is_success() {
            return Err(RatError::NetworkError(rat_embed_lang::tf("grpc_http_error", &[("msg", &status.to_string())])));
        }

        // 检查 Content-Type
        if let Some(content_type) = headers.get(CONTENT_TYPE) {
            if !content_type.to_str().unwrap_or("").starts_with("application/grpc") {
                return Err(RatError::DecodingError("无效的 gRPC Content-Type".to_string()));
            }
        }

        // 从响应头中提取 gRPC 状态和消息
        let grpc_status = headers
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0); // 默认为成功状态

        let grpc_message = headers
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // 提取元数据（所有非标准 gRPC 头部）
        let mut metadata = std::collections::HashMap::new();
        for (name, value) in &headers {
            let name_str = name.as_str();
            // 跳过标准 HTTP 和 gRPC 头部
            if !name_str.starts_with(":")
                && name_str != "content-type"
                && name_str != "grpc-status"
                && name_str != "grpc-message"
                && name_str != "grpc-encoding"
                && name_str != "user-agent"
            {
                if let Ok(value_str) = value.to_str() {
                    metadata.insert(name_str.to_string(), value_str.to_string());
                }
            }
        }

        // 使用统一的编解码器解析帧并反序列化
        let message_data = GrpcCodec::parse_frame(&body_bytes)
            .map_err(|e| RatError::DecodingError(rat_embed_lang::tf("parse_grpc_frame_failed", &[("msg", &e.to_string())])))?;

        // 添加反序列化前的调试信息
        eprintln!("=== DEBUG: [客户端] 准备反序列化响应数据，数据大小: {} bytes ===", message_data.len());
        eprintln!("=== DEBUG: [客户端] 反序列化数据前32字节: {:?} ===", &message_data[..std::cmp::min(32, message_data.len())]);
        println!("DEBUG: [客户端] 准备反序列化响应数据，数据大小: {} bytes", message_data.len());
        println!("DEBUG: [客户端] 反序列化数据前32字节: {:?}", &message_data[..std::cmp::min(32, message_data.len())]);
        info!("🔍 [客户端] 准备反序列化响应数据，数据大小: {} bytes", message_data.len());
        info!("🔍 [客户端] 反序列化数据前32字节: {:?}", &message_data[..std::cmp::min(32, message_data.len())]);

        eprintln!("=== DEBUG: [客户端] 开始使用 GrpcCodec 反序列化 ===");
        // 直接反序列化为最终的 R 类型，因为服务端现在发送完整的 GrpcResponse 结构
        let response_data: R = GrpcCodec::decode(message_data)
            .map_err(|e| {
                eprintln!("=== DEBUG: [客户端] GrpcCodec 反序列化最终数据类型失败: {} ===", e);
                println!("DEBUG: [客户端] GrpcCodec 反序列化最终数据类型失败: {}", e);
                error!("❌ [客户端] GrpcCodec 反序列化最终数据类型失败: {}", e);
                RatError::DeserializationError(rat_embed_lang::tf("deserialize_data_type_failed", &[("msg", &e.to_string())]))
            })?;
        eprintln!("=== DEBUG: [客户端] 最终数据类型反序列化成功 ===");

        // 构建默认的 GrpcResponse 结构，因为我们只收到了实际数据
        let grpc_response = GrpcResponse {
            status: 0, // OK
            message: "Success".to_string(),
            data: response_data,
            metadata: std::collections::HashMap::new(),
        };

        Ok(grpc_response)
    }

    /// 获取下一个请求 ID
    pub fn next_request_id(&self) -> u64 {
        self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
    /// 获取委托模式流的上下文
    /// 
    /// # 参数
    /// * `stream_id` - 流ID
    /// 
    /// # 返回
    /// 返回流上下文，用户可以通过此上下文发送消息
    pub async fn get_stream_context(&self, stream_id: u64) -> Option<ClientStreamContext> {
        self.delegated_manager.get_stream_context(stream_id).await
    }

    /// 获取下一个流 ID
    pub fn next_stream_id(&self) -> u64 {
        self.stream_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}
