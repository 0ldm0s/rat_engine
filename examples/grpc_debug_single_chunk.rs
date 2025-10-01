//! 完整的 gRPC 多块传输调试示例
//! 
//! 用于调试 DownloadChunk 序列化/反序列化问题
//! 传输多个数据块，并在服务端和客户端都打印详细的调试信息

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use futures_util::{Stream, StreamExt};
use async_stream;

use rat_engine::{
    server::{
        grpc_handler::{
            GrpcConnectionManager, GrpcConnectionType, TypedServerStreamHandler
        },
        grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage, GrpcError, GrpcStatusCode, GrpcContext},
        Router, ServerConfig
    },
    utils::logger::{info, warn, error, debug},
    utils::crypto_provider,
    RatEngine, RatGrpcClient, RatGrpcClientBuilder
};

/// 下载消息类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq)]
#[repr(u8)]
pub enum DownloadMessageType {
    /// 数据块消息
    DataChunk = 0,
    /// 流结束信号
    EndOfStream = 1,
    /// 错误信息
    Error = 2,
}

/// 文件下载响应块结构体（与原版本完全一致）
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct DownloadChunk {
    /// 消息类型
    pub message_type: DownloadMessageType,
    /// 块索引（仅对数据块有效）
    pub chunk_index: u32,
    /// 总块数（仅对数据块有效）
    pub total_chunks: u32,
    /// 块数据（仅对数据块有效）
    pub data: Vec<u8>,
    /// 是否为最后一块（仅对数据块有效）
    pub is_last: bool,
    /// 错误消息（仅对错误类型有效）
    pub error_message: Option<String>,
    /// 文件ID（用于标识下载的文件）
    pub file_id: String,
    /// 文件总大小（字节数，仅在第一个数据块中有效）
    pub file_size: Option<u64>,
    /// 文件名（仅在第一个数据块中有效）
    pub filename: Option<String>,
    /// 块在文件中的偏移位置（字节偏移）
    pub offset: u64,
}

/// 文件下载请求结构体
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct DownloadRequest {
    /// 文件ID
    pub file_id: String,
}



/// 文件下载处理器（服务端流 gRPC）
#[derive(Clone)]
pub struct FileDownloadHandler {
    connection_manager: Arc<GrpcConnectionManager>,
}

impl FileDownloadHandler {
    pub fn new(connection_manager: Arc<GrpcConnectionManager>) -> Self {
        Self {
            connection_manager,
        }
    }
}

impl TypedServerStreamHandler<DownloadChunk> for FileDownloadHandler {
    fn handle_typed(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<DownloadChunk>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        // 克隆 self 以避免生命周期问题
        let handler = self.clone();
        Box::pin(async move {
            // 反序列化请求
            let download_request = match bincode::decode_from_slice::<DownloadRequest, _>(&request.data, bincode::config::standard()) {
                Ok((req, _)) => req,
                Err(e) => {
                    println!("❌ [Server] 反序列化下载请求失败: {:?}", e);
                    return Err(GrpcError::InvalidArgument(format!("反序列化下载请求失败: {:?}", e)));
                }
            };
            
            let stream = handler.handle_download_typed(download_request);
            Ok(stream)
        })
    }
}

impl FileDownloadHandler {
    fn handle_download_typed(&self, download_request: DownloadRequest) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<DownloadChunk>, GrpcError>> + Send>> {
        let connection_manager = self.connection_manager.clone();
        
        Box::pin(async_stream::stream! {
            println!("🔍 [Server] 开始处理下载请求...");

            println!("📥 [Server] 下载请求: file_id = {}", download_request.file_id);

            // 读取文件内容（简化版，直接读取固定文件）
            let file_path = format!("./file_storage/{}", download_request.file_id);
            let file_content = match fs::read(&file_path).await {
                Ok(content) => content,
                Err(e) => {
                    println!("❌ [Server] 读取文件失败: {:?}", e);
                    yield Err(GrpcError::Internal(format!("读取文件失败: {:?}", e)));
                    return;
                }
            };

            println!("📄 [Server] 文件读取成功，大小: {} 字节", file_content.len());

            // 创建连接（使用默认用户ID）
             let user_id = "client_user".to_string();
             let (conn_id, mut _rx) = connection_manager.add_connection(
                 user_id.clone(),
                 Some("download_room".to_string()),
                 GrpcConnectionType::ServerStream
             );

            // 实现完整的多块传输
            let chunk_size = 4096; // 4KB 每块
            let total_chunks = (file_content.len() + chunk_size - 1) / chunk_size; // 向上取整
            
            println!("📊 [Server] 文件将分为 {} 个数据块，每块最大 {} 字节", total_chunks, chunk_size);

            // 分块发送文件内容
            for chunk_index in 0..total_chunks {
                let start_offset = chunk_index * chunk_size;
                let end_offset = std::cmp::min(start_offset + chunk_size, file_content.len());
                let chunk_data = file_content[start_offset..end_offset].to_vec();
                let is_last = chunk_index == total_chunks - 1;
                
                let download_chunk = DownloadChunk {
                    message_type: DownloadMessageType::DataChunk,
                    chunk_index: chunk_index as u32,
                    total_chunks: total_chunks as u32,
                    data: chunk_data,
                    is_last,
                    error_message: None,
                    file_id: download_request.file_id.clone(),
                    file_size: if chunk_index == 0 { Some(file_content.len() as u64) } else { None },
                    filename: if chunk_index == 0 { Some(format!("debug_{}.txt", download_request.file_id)) } else { None },
                    offset: start_offset as u64,
                };

                // 打印服务端发送的数据的十六进制表示（前20字节）
                let hex_data = download_chunk.data.iter()
                    .take(20)
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("🔍 [Server] 块 {}/{} 前20字节 HEX: {}", chunk_index, total_chunks, hex_data);
                println!("🔍 [Server] 块 {}/{} 偏移: {}, 大小: {} 字节", chunk_index, total_chunks, start_offset, download_chunk.data.len());

                // 发送数据块
                println!("📤 [Server] 发送数据块 {}/{}", chunk_index, total_chunks);
                yield Ok(GrpcStreamMessage {
                    id: (chunk_index + 1) as u64,
                    stream_id: 1,
                    sequence: chunk_index as u64,
                    data: download_chunk,
                    end_of_stream: is_last,
                    metadata: std::collections::HashMap::new(),
                });
                
                // 添加小延迟以模拟网络传输
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            println!("✅ [Server] 多块传输完成，共发送 {} 个数据块", total_chunks);

            // 清理连接
             let connection_manager_clone = connection_manager.clone();
             tokio::spawn(async move {
                 tokio::time::sleep(Duration::from_secs(1)).await;
                 connection_manager_clone.remove_connection(&conn_id);
             });
        })
    }
}

/// 启动调试服务器
pub async fn start_debug_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 [Server] 启动调试服务器...");
    
    // 确保加密提供程序已安装
    crypto_provider::ensure_crypto_provider_installed();
    
    // 确保文件存储目录存在
    fs::create_dir_all("./file_storage").await?;
    
    // 创建测试文件 file_00000005（如果不存在）
    let test_file_path = "./file_storage/file_00000005";
    if !PathBuf::from(test_file_path).exists() {
        // 创建一个较大的测试文件（约20KB），确保会分成多个块
        let mut test_content = String::new();
        test_content.push_str("这是一个测试文件，用于调试 gRPC 多块传输功能。\n");
        test_content.push_str("这个文件包含中文字符来测试编码问题。\n");
        test_content.push_str("文件内容较大，需要分成多个数据块进行传输。\n\n");
        
        // 重复添加内容以增加文件大小（创建约1MB的文件）
        for i in 0..10000 {
            test_content.push_str(&format!("第 {} 行：这是一段测试数据，包含中文字符和英文字符 - Line {} with mixed content for testing purposes. 这行包含更多内容以增加文件大小。\n", i + 1, i + 1));
        }
        
        test_content.push_str("\n文件结束标记 - End of file marker.\n");
        
        fs::write(test_file_path, test_content.as_bytes()).await?;
        println!("📄 [Server] 创建大型测试文件: {} (大小: {} 字节)", test_file_path, test_content.len());
    }

    let addr: std::net::SocketAddr = "127.0.0.1:50051".parse().unwrap();
    
    // 创建路由器
    let mut router = Router::new();
    router.enable_h2(); // 启用 HTTP/2 with ALPN
    
    // 创建连接管理器
    let connection_manager = Arc::new(GrpcConnectionManager::new());
    
    // 创建下载处理器
    let download_handler = FileDownloadHandler::new(connection_manager);
    
    // 注册服务端流服务
    router.add_grpc_typed_server_stream("/file/Download", download_handler);
    
    println!("🔧 [Server] 注册 gRPC 服务: /file/Download");
    
    // 创建引擎并启动服务器
    let engine = RatEngine::builder()
        .worker_threads(4)
        .max_connections(10000)
        .router(router)
        .enable_development_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await?
        .build()?;
    
    engine.start(addr.ip().to_string(), addr.port()).await?;
    
    Ok(())
}

/// 运行调试客户端测试（使用真实的 gRPC 客户端代码）
async fn run_debug_client_test() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🧪 [Client] 启动调试客户端测试...");
    
    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 创建 gRPC 客户端（使用真实的客户端）
    let grpc_client = RatGrpcClientBuilder::new()
        
        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(10)?
        .http2_only()
        .user_agent("RAT-Engine-gRPC-Client/1.0")?
        .disable_compression()
        .development_mode()
        .build()?;

    // 创建下载请求
    let download_request = DownloadRequest {
        file_id: "file_00000005".to_string(),
    };

    println!("📤 [Client] 发送下载请求: file_id = {}", download_request.file_id);

    // 使用 call_server_stream_with_uri 方法
    let server_uri = "https://localhost:50051".to_string();
    match grpc_client.call_server_stream_with_uri::<DownloadRequest, DownloadChunk>(
        &server_uri,
        "file",
        "Download", 
        download_request,
        None,
    ).await {
        Ok(stream_response) => {
            let mut total_chunks = 0;
            let mut total_bytes = 0;
            let mut stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<DownloadChunk>, _>> + Send>> = stream_response.stream;
            
            // 用于存储文件信息和文件句柄
            let mut file_handle: Option<tokio::fs::File> = None;
            let mut expected_file_size: Option<u64> = None;
            let mut download_filename: Option<String> = None;
            let mut received_chunks = std::collections::HashMap::new(); // 用于按序号存储数据块
            
            println!("📥 [Client] 开始接收文件下载流...");
            
            while let Some(result) = stream.as_mut().next().await {
                match result {
                    Ok(chunk) => {
                        let receive_time = std::time::Instant::now();
                        println!("🔄 [实时] 收到流消息 - ID: {}, 序列: {}, 结束标志: {}", 
                                chunk.id, chunk.sequence, chunk.end_of_stream);
                        
                        // 监控内存使用情况
                        #[cfg(target_os = "macos")]
                        {
                            use std::process::Command;
                            if let Ok(output) = Command::new("ps")
                                .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
                                .output() {
                                if let Ok(memory_str) = String::from_utf8(output.stdout) {
                                    if let Ok(memory_kb) = memory_str.trim().parse::<u64>() {
                                        println!("🧠 [内存] 当前进程内存使用: {} KB ({:.2} MB)", memory_kb, memory_kb as f64 / 1024.0);
                                    }
                                }
                            }
                        }
                        
                        // 直接使用框架反序列化后的 DownloadChunk
                        let download_chunk = chunk.data;
                        println!("📦 [实时] 数据块详情: 类型={:?}, 文件ID={}, 索引={}, 偏移={}, 大小={} bytes, 最后块={}", 
                                download_chunk.message_type, download_chunk.file_id, 
                                download_chunk.chunk_index, download_chunk.offset, 
                                download_chunk.data.len(), download_chunk.is_last);

                        // 打印接收到的数据的十六进制表示（前20字节）
                        let hex_data = download_chunk.data.iter()
                            .take(20)
                            .map(|b| format!("{:02x}", b))
                            .collect::<Vec<_>>()
                            .join(" ");
                        println!("🔍 [Client] 接收数据前20字节 HEX: {}", hex_data);
                        
                        // 检查数据块大小是否合理
                        if download_chunk.data.len() > 10 * 1024 * 1024 { // 10MB
                            println!("⚠️ [警告] 数据块大小异常: {} bytes (超过10MB)", download_chunk.data.len());
                        }
                
                        match download_chunk.message_type {
                            DownloadMessageType::DataChunk => {
                                // 如果是第一个数据块，初始化文件
                                if download_chunk.chunk_index == 0 {
                                    if let (Some(file_size), Some(filename)) = (download_chunk.file_size, download_chunk.filename.as_ref()) {
                                        expected_file_size = Some(file_size);
                                        download_filename = Some(filename.clone());
                                        
                                        // 创建下载文件路径
                                        let download_path = format!("./downloads/{}", filename);
                                        
                                        // 确保下载目录存在
                                        if let Err(e) = tokio::fs::create_dir_all("./downloads").await {
                                            println!("❌ 创建下载目录失败: {}", e);
                                            break;
                                        }
                                        
                                        // 创建空文件并预分配空间
                                        match tokio::fs::OpenOptions::new()
                                            .create(true)
                                            .write(true)
                                            .truncate(true)
                                            .open(&download_path).await {
                                            Ok(mut file) => {
                                                // 预分配文件空间（类似网际快车）
                                                if let Err(e) = file.set_len(file_size).await {
                                                    println!("⚠️ 预分配文件空间失败: {}, 继续下载", e);
                                                } else {
                                                    println!("✅ 已预分配文件空间: {} bytes -> {}", file_size, download_path);
                                                }
                                                file_handle = Some(file);
                                            }
                                            Err(e) => {
                                                println!("❌ 创建下载文件失败: {}", e);
                                                break;
                                            }
                                        }
                                        
                                        println!("🎯 [网际快车模式] 文件初始化完成: {} ({} bytes)", filename, file_size);
                                    }
                                }
                                
                                // 实时写入数据块到指定偏移位置
                                if let Some(ref mut file) = file_handle {
                                    use tokio::io::{AsyncSeekExt, AsyncWriteExt};
                                    
                                    // 定位到指定偏移位置
                                    if let Err(e) = file.seek(std::io::SeekFrom::Start(download_chunk.offset)).await {
                                        println!("❌ 定位文件偏移失败: {}", e);
                                        break;
                                    }
                                    
                                    // 写入数据块
                                    if let Err(e) = file.write_all(&download_chunk.data).await {
                                        println!("❌ 写入数据块失败: {}", e);
                                        break;
                                    }
                                    
                                    // 强制刷新到磁盘
                                    if let Err(e) = file.flush().await {
                                        println!("⚠️ 刷新文件缓冲区失败: {}", e);
                                    }
                                    
                                    total_chunks += 1;
                                    total_bytes += download_chunk.data.len();
                                    
                                    let write_time = receive_time.elapsed();
                                    println!("💾 [实时写入] 块 {}/{} 已写入偏移 {} (大小: {} bytes, 耗时: {:?})", 
                                          download_chunk.chunk_index, download_chunk.total_chunks, 
                                          download_chunk.offset, download_chunk.data.len(), write_time);
                                    
                                    // 计算下载进度
                                    if let Some(file_size) = expected_file_size {
                                        let progress = (total_bytes as f64 / file_size as f64 * 100.0) as u32;
                                        println!("📊 [进度] {}% ({}/{} bytes)", progress, total_bytes, file_size);
                                    }
                                } else {
                                    // 如果没有文件句柄，暂存数据块
                                    received_chunks.insert(download_chunk.chunk_index, download_chunk.data.clone());
                                    total_chunks += 1;
                                    total_bytes += download_chunk.data.len();
                                    
                                    println!("📦 [暂存] 数据块 {} (索引: {}/{}, 大小: {} bytes)", 
                                          total_chunks, download_chunk.chunk_index, download_chunk.total_chunks, download_chunk.data.len());
                                }
                                
                                // 检查是否为最后一个数据块
                                if download_chunk.is_last {
                                    println!("🏁 [完成] 收到最后一个数据块，下载完成");
                                    
                                    // 确保文件完全写入磁盘
                                    if let Some(ref mut file) = file_handle {
                                        if let Err(e) = file.sync_all().await {
                                            println!("⚠️ 同步文件到磁盘失败: {}", e);
                                        } else {
                                            println!("✅ 文件已同步到磁盘");
                                        }
                                    }
                                    break;
                                }
                            }
                            DownloadMessageType::EndOfStream => {
                                // 兼容性处理：虽然新版本不再发送此消息，但保留处理逻辑
                                println!("📥 收到流结束信号（兼容模式），下载完成");
                                break;
                            }
                            DownloadMessageType::Error => {
                                let error_msg = download_chunk.error_message.unwrap_or_else(|| "未知错误".to_string());
                                println!("❌ 服务端返回错误: {}", error_msg);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        println!("❌ 文件下载流错误: {:?}", e);
                        
                        // 详细分析错误类型
                        let error_str = format!("{:?}", e);
                        if error_str.contains("capacity overflow") {
                            println!("🔍 [错误分析] 检测到 capacity overflow 错误");
                            println!("🔍 [错误分析] 这通常表示尝试分配过大的内存空间");
                            println!("🔍 [错误分析] 可能的原因:");
                            println!("   1. 反序列化时读取到错误的长度字段");
                            println!("   2. 数据损坏导致长度字段异常");
                            println!("   3. 序列化/反序列化版本不匹配");
                        } else if error_str.contains("DeserializationError") {
                            println!("🔍 [错误分析] 检测到反序列化错误");
                            println!("🔍 [错误分析] 可能的原因:");
                            println!("   1. 数据格式不匹配");
                            println!("   2. bincode 版本不兼容");
                            println!("   3. 数据传输过程中损坏");
                        } else if error_str.contains("connection") || error_str.contains("network") {
                            println!("🔍 [错误分析] 检测到网络连接错误");
                        }
                        
                        break;
                    }
                }
            }
            
            if total_chunks > 0 {
                println!("✅ [网际快车模式] 文件下载成功: {} 个数据块，共 {} 字节", total_chunks, total_bytes);
                
                if let Some(filename) = download_filename {
                    println!("📁 下载文件保存为: ./downloads/{}", filename);
                    
                    // 验证文件大小
                    if let Some(expected_size) = expected_file_size {
                        if total_bytes == expected_size as usize {
                            println!("✅ 文件大小验证通过: {} bytes", total_bytes);
                        } else {
                            println!("⚠️ 文件大小不匹配: 期望 {} bytes，实际 {} bytes", expected_size, total_bytes);
                        }
                    }
                }
            } else {
                println!("❌ 未收到任何数据块");
            }
        }
        Err(e) => {
            println!("❌ 服务端流请求失败: {:?}", e);
        }
    }

    println!("🎉 [Client] 调试测试完成");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    rat_engine::require_features!("client", "tls");

    // 日志通过RatEngineBuilder初始化

    println!("🚀 RAT Engine gRPC 多块传输调试示例");
    println!("📋 这个示例用于调试 DownloadChunk 序列化/反序列化问题");
    println!("📋 将创建一个大文件并分成多个块进行传输测试");
    println!();

    // 启动服务器任务
    let server_task = tokio::spawn(async move {
        if let Err(e) = start_debug_server().await {
            println!("❌ [Server] 服务器启动失败: {:?}", e);
        }
    });

    // 启动客户端测试任务
    let client_task = tokio::spawn(async move {
        if let Err(e) = run_debug_client_test().await {
            println!("❌ [Client] 客户端测试失败: {:?}", e);
        }
    });

    // 等待客户端测试完成
    let _ = client_task.await;
    
    // 给服务器一些时间清理
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // 停止服务器
    server_task.abort();

    println!("🎉 调试示例完成");
    Ok(())
}