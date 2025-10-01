//! gRPC 编解码模块
//! 
//! 提供统一的 bincode 序列化/反序列化接口，确保类型安全和错误处理一致性

use serde::{Serialize, Deserialize};
use crate::error::RatError;

/// gRPC 编解码器
/// 
/// 统一封装所有 bincode 操作，提供类型安全的序列化/反序列化接口
pub struct GrpcCodec;

impl GrpcCodec {
    /// 序列化消息为字节数组
    /// 
    /// # 参数
    /// - `message`: 要序列化的消息，必须实现 Serialize 和 bincode::Encode
    /// 
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 序列化后的字节数组
    /// - `Err(RatError)`: 序列化失败错误
    pub fn encode<T>(message: &T) -> Result<Vec<u8>, RatError>
    where
        T: Serialize + bincode::Encode,
    {
        bincode::encode_to_vec(message, bincode::config::standard())
            .map_err(|e| RatError::SerializationError(format!("gRPC 消息序列化失败: {}", e)))
    }

    /// 从字节数组反序列化消息
    /// 
    /// # 参数
    /// - `data`: 要反序列化的字节数组
    /// 
    /// # 返回值
    /// - `Ok(T)`: 反序列化后的消息
    /// - `Err(RatError)`: 反序列化失败错误
    pub fn decode<T>(data: &[u8]) -> Result<T, RatError>
    where
        T: for<'de> Deserialize<'de> + bincode::Decode<()>,
    {
        let (message, _) = bincode::decode_from_slice(data, bincode::config::standard())
            .map_err(|e| RatError::DeserializationError(format!("gRPC 消息反序列化失败: {}", e)))?;
        Ok(message)
    }

    /// 创建 gRPC 消息帧
    /// 
    /// 按照 gRPC 协议格式创建消息帧：[压缩标志(1字节)][长度(4字节)][数据]
    /// 
    /// # 参数
    /// - `data`: 消息数据
    /// 
    /// # 返回值
    /// - 完整的 gRPC 消息帧
    pub fn create_frame(data: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(5 + data.len());
        // 压缩标志（0 = 未压缩）
        frame.push(0);
        // 消息长度（4 字节，大端序）
        frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
        // 消息数据
        frame.extend_from_slice(data);
        frame
    }

    /// 解析 gRPC 消息帧
    /// 
    /// 从 gRPC 协议帧中提取消息数据
    /// 
    /// # 参数
    /// - `frame`: 完整的 gRPC 消息帧
    /// 
    /// # 返回值
    /// - `Ok(&[u8])`: 提取的消息数据
    /// - `Err(RatError)`: 帧格式错误
    pub fn parse_frame(frame: &[u8]) -> Result<&[u8], RatError> {
        if frame.len() < 5 {
            return Err(RatError::InvalidArgument("gRPC 帧长度不足".to_string()));
        }

        let _compressed = frame[0];
        let length = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;

        if frame.len() < 5 + length {
            return Err(RatError::InvalidArgument("gRPC 帧数据不完整".to_string()));
        }

        Ok(&frame[5..5 + length])
    }

    /// 编码并创建完整的 gRPC 消息帧
    /// 
    /// 将消息序列化并包装成 gRPC 协议帧的一站式方法
    /// 
    /// # 参数
    /// - `message`: 要编码的消息
    /// 
    /// # 返回值
    /// - `Ok(Vec<u8>)`: 完整的 gRPC 消息帧
    /// - `Err(RatError)`: 编码失败错误
    pub fn encode_frame<T>(message: &T) -> Result<Vec<u8>, RatError>
    where
        T: Serialize + bincode::Encode,
    {
        let data = Self::encode(message)?;
        Ok(Self::create_frame(&data))
    }

    /// 解析帧并反序列化消息
    /// 
    /// 从 gRPC 协议帧中提取并反序列化消息的一站式方法
    /// 
    /// # 参数
    /// - `frame`: 完整的 gRPC 消息帧
    /// 
    /// # 返回值
    /// - `Ok(T)`: 反序列化后的消息
    /// - `Err(RatError)`: 解析或反序列化失败错误
    pub fn decode_frame<T>(frame: &[u8]) -> Result<T, RatError>
    where
        T: for<'de> Deserialize<'de> + bincode::Decode<()>,
    {
        let data = Self::parse_frame(frame)?;
        Self::decode(data)
    }

    /// 尝试从缓冲区解析完整的 gRPC 消息帧（用于流式处理）
    /// 
    /// # 参数
    /// - `buffer`: 数据缓冲区
    /// 
    /// # 返回值
    /// - `Some((data, consumed))`: 成功解析出消息数据和消耗的字节数
    /// - `None`: 缓冲区中没有完整的消息帧
    pub fn try_parse_frame(buffer: &[u8]) -> Option<(&[u8], usize)> {
        if buffer.len() < 5 {
            return None;
        }

        let _compressed = buffer[0];
        let length = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;

        if buffer.len() < 5 + length {
            return None;
        }

        let data = &buffer[5..5 + length];
        let consumed = 5 + length;
        Some((data, consumed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
    struct TestMessage {
        id: u64,
        content: String,
    }

    #[test]
    fn test_encode_decode() {
        let message = TestMessage {
            id: 123,
            content: "测试消息".to_string(),
        };

        // 测试编码
        let encoded = GrpcCodec::encode(&message).unwrap();
        assert!(!encoded.is_empty());

        // 测试解码
        let decoded: TestMessage = GrpcCodec::decode(&encoded).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn test_frame_operations() {
        let message = TestMessage {
            id: 456,
            content: "帧测试".to_string(),
        };

        // 测试编码帧
        let frame = GrpcCodec::encode_frame(&message).unwrap();
        assert!(frame.len() >= 5); // 至少包含帧头

        // 测试解码帧
        let decoded: TestMessage = GrpcCodec::decode_frame(&frame).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn test_create_parse_frame() {
        let data = b"hello world";
        
        // 创建帧
        let frame = GrpcCodec::create_frame(data);
        assert_eq!(frame.len(), 5 + data.len());
        assert_eq!(frame[0], 0); // 压缩标志
        
        // 解析帧
        let parsed_data = GrpcCodec::parse_frame(&frame).unwrap();
        assert_eq!(parsed_data, data);
    }

    #[test]
    fn test_invalid_frame() {
        // 测试帧长度不足
        let short_frame = vec![0, 0, 0];
        assert!(GrpcCodec::parse_frame(&short_frame).is_err());

        // 测试数据不完整
        let incomplete_frame = vec![0, 0, 0, 0, 10]; // 声明长度为10但没有数据
        assert!(GrpcCodec::parse_frame(&incomplete_frame).is_err());
    }
}