//! 协议检测工具模块
//!
//! 提供协议类型检测功能，用于混合模式下的协议自动识别

/// 检测数据是否为 gRPC 请求
///
/// 检测方法：
/// 1. 检查是否为 TLS ClientHello (0x16)
/// 2. 检查 HTTP 头中的 content-type
/// 3. 检查 gRPC 二进制帧格式
pub fn is_grpc_request(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }

    // 方法1: 检查是否为 TLS ClientHello (0x16 = TLS record)
    // 如果是 TLS，需要 TLS 握手后才能判断内容
    if data[0] == 0x16 {
        // TLS 连接，返回 false 让 TLS 处理器接管
        return false;
    }

    // 方法2: 检查是否为 HTTP/1.1 请求中的 gRPC
    let data_str = String::from_utf8_lossy(data);
    if data_str.contains("content-type:") || data_str.contains("Content-Type:") {
        return data_str.contains("application/grpc");
    }

    // 方法3: 检查是否为 gRPC 二进制帧格式 (compressed-flag + 4-byte length)
    // gRPC 消息格式: [1 byte compressed flag] [4 bytes message length] [message data]
    if data.len() >= 5 {
        // 检查前5字节是否可能是 gRPC 帧头
        let compressed_flag = data[0];
        if compressed_flag == 0 || compressed_flag == 1 {
            let length = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
            // 合理的 gRPC 消息长度（通常 < 16MB）
            if length < 16 * 1024 * 1024 && data.len() >= 5 + length as usize {
                return true;
            }
        }
    }

    false
}
