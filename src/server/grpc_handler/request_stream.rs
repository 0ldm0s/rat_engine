use std::pin::Pin;
use std::task::{Context, Poll};
use std::collections::HashMap;
use futures_util::{Stream, StreamExt};
use h2::RecvStream;
use pin_project_lite::pin_project;
use crate::server::grpc_types::*;
use crate::server::grpc_codec::GrpcCodec;
use crate::utils::logger::debug;

/// gRPC 请求流
pin_project! {
    pub struct GrpcRequestStream {
        #[pin]
        body: RecvStream,
        buffer: Vec<u8>,
        sequence: u64,
    }
}

impl GrpcRequestStream {
    pub(crate) fn new(body: RecvStream) -> Self {
        debug!("🔍 [DEBUG] GrpcRequestStream::new 开始");
        let stream = Self {
            body,
            buffer: Vec::new(),
            sequence: 0,
        };
        debug!("🔍 [DEBUG] GrpcRequestStream::new 完成");
        stream
    }
}

impl Stream for GrpcRequestStream {
    type Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>;
    
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        
        // 尝试从缓冲区解析完整的 gRPC 消息
        if this.buffer.len() >= 5 {
            let length = u32::from_be_bytes([
                this.buffer[1],
                this.buffer[2], 
                this.buffer[3],
                this.buffer[4]
            ]) as usize;
            
            // 添加合理的长度限制，防止容量溢出（最大 100MB）
            const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
            if length > MAX_MESSAGE_SIZE {
                return Poll::Ready(Some(Err(GrpcError::Internal(format!(
                    "gRPC 消息长度过大: {} 字节，最大允许: {} 字节", 
                    length, MAX_MESSAGE_SIZE
                )))));
            }
            
            if this.buffer.len() >= 5 + length {
                // 有完整的消息
                let compressed = this.buffer[0] != 0;
                if compressed {
                    return Poll::Ready(Some(Err(GrpcError::Unimplemented("不支持压缩的 gRPC 消息".to_string()))));
                }
                
                let data = this.buffer[5..5 + length].to_vec();
                this.buffer.drain(..5 + length);
                
                let current_sequence = *this.sequence;
                *this.sequence += 1;
                
                // 尝试解析为 GrpcStreamMessage<Vec<u8>>（关闭信号）
                if let Ok(stream_message) = GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(&data) {
                    // 这是一个关闭信号或其他流消息
                    let msg = stream_message;
                    println!("DEBUG: 收到流消息，end_of_stream: {}, 数据长度: {}", msg.end_of_stream, msg.data.len());
                    if msg.end_of_stream {
                        // 收到关闭信号，结束流
                        println!("DEBUG: 收到关闭信号，正常结束流");
                        return Poll::Ready(None);
                    } else {
                        return Poll::Ready(Some(Ok(msg)));
                    }
                } else {
                    // 这是普通数据（如序列化的 FileChunk）
                    println!("DEBUG: 收到普通数据块，大小: {} 字节", data.len());
                    return Poll::Ready(Some(Ok(GrpcStreamMessage { 
                        id: current_sequence,
                        stream_id: 1,
                        sequence: current_sequence,
                        data: data,
                        end_of_stream: false,
                        metadata: HashMap::new(),
                    })));
                }
            }
        }
        
        // 读取更多数据
        match this.body.poll_data(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                // 释放流控制容量
                if let Err(e) = this.body.flow_control().release_capacity(chunk.len()) {
                    println!("DEBUG: 释放流控制容量失败: {}", e);
                    return Poll::Ready(Some(Err(GrpcError::Internal(format!("释放流控制容量失败: {}", e)))));
                }
                
                this.buffer.extend_from_slice(&chunk);
                println!("DEBUG: 接收到 {} 字节数据，缓冲区总大小: {} 字节", chunk.len(), this.buffer.len());
                
                // 立即尝试解析消息，而不是返回 Pending
                // 这避免了无限循环问题
                if this.buffer.len() >= 5 {
                    let length = u32::from_be_bytes([
                        this.buffer[1],
                        this.buffer[2], 
                        this.buffer[3],
                        this.buffer[4]
                    ]) as usize;
                    
                    // 添加合理的长度限制，防止容量溢出（最大 100MB）
                    const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;
                    if length > MAX_MESSAGE_SIZE {
                        return Poll::Ready(Some(Err(GrpcError::Internal(format!(
                            "gRPC 消息长度过大: {} 字节，最大允许: {} 字节", 
                            length, MAX_MESSAGE_SIZE
                        )))));
                    }
                    
                    if this.buffer.len() >= 5 + length {
                        // 有完整的消息，立即处理
                        let compressed = this.buffer[0] != 0;
                        if compressed {
                            return Poll::Ready(Some(Err(GrpcError::Unimplemented("不支持压缩的 gRPC 消息".to_string()))));
                        }
                        
                        let data = this.buffer[5..5 + length].to_vec();
                        this.buffer.drain(..5 + length);
                        
                        let current_sequence = *this.sequence;
                        *this.sequence += 1;
                        
                        // 尝试解析为 GrpcStreamMessage<Vec<u8>>（关闭信号）
                        if let Ok(stream_message) = GrpcCodec::decode::<GrpcStreamMessage<Vec<u8>>>(&data) {
                            // 这是一个关闭信号或其他流消息
                            let msg = stream_message;
                            println!("DEBUG: 收到流消息，end_of_stream: {}, 数据长度: {}", msg.end_of_stream, msg.data.len());
                            if msg.end_of_stream {
                                // 收到关闭信号，结束流
                                println!("DEBUG: 收到关闭信号，正常结束流");
                                return Poll::Ready(None);
                            } else {
                                return Poll::Ready(Some(Ok(msg)));
                            }
                        } else {
                            // 这是普通数据（如序列化的 FileChunk）
                            println!("DEBUG: 收到普通数据块，大小: {} 字节", data.len());
                            return Poll::Ready(Some(Ok(GrpcStreamMessage { 
                                id: current_sequence,
                                stream_id: 1,
                                sequence: current_sequence,
                                data: data,
                                end_of_stream: false,
                                metadata: HashMap::new(),
                            })));
                        }
                    }
                }
                
                // 数据不完整，继续等待
                Poll::Pending
            }
            Poll::Ready(Some(Err(e))) => {
                let error_msg = e.to_string();
                println!("DEBUG: 读取流数据失败: {}", error_msg);
                
                // 检查是否是客户端断开连接
                if error_msg.contains("stream no longer needed") || 
                   error_msg.contains("connection closed") ||
                   error_msg.contains("reset") ||
                   error_msg.contains("broken pipe") {
                    println!("DEBUG: 检测到客户端断开连接，正常结束流");
                    return Poll::Ready(None);
                }
                
                Poll::Ready(Some(Err(GrpcError::Internal(format!("读取流数据失败: {}", e)))))
            }
            Poll::Ready(None) => {
                println!("DEBUG: 流已结束（客户端断开连接）");
                if this.buffer.is_empty() {
                    println!("DEBUG: 缓冲区为空，正常结束流");
                    Poll::Ready(None)
                } else {
                    println!("DEBUG: 缓冲区中还有 {} 字节未处理数据，但客户端已断开", this.buffer.len());
                    // 客户端断开时，如果缓冲区中有数据，我们仍然正常结束流，而不是报错
                    // 这是一种容错处理，避免因网络问题导致的数据丢失被误判为错误
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
