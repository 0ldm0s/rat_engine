//! 支持断点续传的网际快车模式下载示例
//! 
//! 展示如何使用下载元数据管理器实现断点续传功能
//! 自动启动服务器和客户端进行完整测试

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
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
    RatGrpcClient, RatGrpcClientBuilder,
    DownloadMetadataManager, DownloadMetadata, DownloadStatus,
    RatEngine
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

/// 文件下载响应块结构体
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
    /// 可选的断点续传信息：已接收的块索引列表
    pub received_chunks: Option<Vec<u32>>,
}

/// 支持断点续传的文件下载处理器
#[derive(Clone)]
pub struct ResumableFileDownloadHandler {
    connection_manager: Arc<GrpcConnectionManager>,
}

impl ResumableFileDownloadHandler {
    pub fn new(connection_manager: Arc<GrpcConnectionManager>) -> Self {
        Self {
            connection_manager,
        }
    }
}

impl TypedServerStreamHandler<DownloadChunk> for ResumableFileDownloadHandler {
    fn handle_typed(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<DownloadChunk>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        let handler = self.clone();
        Box::pin(async move {
            // 反序列化请求
            let download_request = match bincode::decode_from_slice::<DownloadRequest, _>(&request.data, bincode::config::standard()) {
                Ok((req, _)) => req,
                Err(e) => {
                    error!("❌ [Server] 反序列化下载请求失败: {:?}", e);
                    return Err(GrpcError::InvalidArgument(format!("反序列化下载请求失败: {:?}", e)));
                }
            };
            
            let stream = handler.handle_resumable_download(download_request);
            Ok(stream)
        })
    }
}

impl ResumableFileDownloadHandler {
    fn handle_resumable_download(&self, download_request: DownloadRequest) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<DownloadChunk>, GrpcError>> + Send>> {
        let connection_manager = self.connection_manager.clone();
        
        Box::pin(async_stream::stream! {
            info!("🔍 [Server] 开始处理断点续传下载请求...");
            info!("📥 [Server] 下载请求: file_id = {}", download_request.file_id);

            // 读取文件内容
            let file_path = format!("./file_storage/{}", download_request.file_id);
            let file_content = match fs::read(&file_path).await {
                Ok(content) => content,
                Err(e) => {
                    error!("❌ [Server] 读取文件失败: {:?}", e);
                    yield Err(GrpcError::Internal(format!("读取文件失败: {:?}", e)));
                    return;
                }
            };

            info!("📄 [Server] 文件读取成功，大小: {} 字节", file_content.len());

            // 创建连接
            let user_id = "client_user".to_string();
            let (conn_id, mut _rx) = connection_manager.add_connection(
                user_id.clone(),
                Some("resumable_download_room".to_string()),
                GrpcConnectionType::ServerStream
            );

            // 分块配置
            let chunk_size = 4096; // 4KB 每块
            let total_chunks = (file_content.len() + chunk_size - 1) / chunk_size;
            
            info!("📊 [Server] 文件将分为 {} 个数据块，每块最大 {} 字节", total_chunks, chunk_size);

            // 确定需要发送的块（支持断点续传）
            let mut chunks_to_send: Vec<usize> = (0..total_chunks).collect();
            
            if let Some(received_chunks) = &download_request.received_chunks {
                info!("🔄 [Server] 检测到断点续传请求，已接收块: {:?}", received_chunks);
                
                // 过滤掉已接收的块
                chunks_to_send.retain(|&chunk_index| {
                    !received_chunks.contains(&(chunk_index as u32))
                });
                
                info!("📋 [Server] 需要发送的块: {} 个（跳过 {} 个已接收的块）", 
                      chunks_to_send.len(), received_chunks.len());
            }

            // 发送需要的数据块
            for (send_index, &chunk_index) in chunks_to_send.iter().enumerate() {
                let start_offset = chunk_index * chunk_size;
                let end_offset = std::cmp::min(start_offset + chunk_size, file_content.len());
                let chunk_data = file_content[start_offset..end_offset].to_vec();
                let is_last = send_index == chunks_to_send.len() - 1;
                
                let download_chunk = DownloadChunk {
                    message_type: DownloadMessageType::DataChunk,
                    chunk_index: chunk_index as u32,
                    total_chunks: total_chunks as u32,
                    data: chunk_data,
                    is_last,
                    error_message: None,
                    file_id: download_request.file_id.clone(),
                    file_size: if send_index == 0 { Some(file_content.len() as u64) } else { None },
                    filename: if send_index == 0 { Some(format!("resumable_{}.txt", download_request.file_id)) } else { None },
                    offset: start_offset as u64,
                };

                info!("📤 [Server] 发送数据块 {}/{} (索引: {}, 偏移: {}, 大小: {} 字节)", 
                      send_index + 1, chunks_to_send.len(), chunk_index, start_offset, download_chunk.data.len());

                yield Ok(GrpcStreamMessage {
                    id: (send_index + 1) as u64,
                    stream_id: 1,
                    sequence: send_index as u64,
                    data: download_chunk,
                    end_of_stream: is_last,
                    metadata: HashMap::new(),
                });
                
                // 添加小延迟以模拟网络传输
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            info!("✅ [Server] 断点续传下载完成，共发送 {} 个数据块", chunks_to_send.len());

            // 清理连接
            connection_manager.remove_connection(&conn_id);
        })
    }
}

/// 支持断点续传的下载客户端
pub struct ResumableDownloadClient {
    grpc_client: RatGrpcClient,
    metadata_manager: DownloadMetadataManager,
    server_uri: String,
}

impl ResumableDownloadClient {
    pub fn new(grpc_client: RatGrpcClient, metadata_manager: DownloadMetadataManager, server_uri: String) -> Self {
        Self {
            grpc_client,
            metadata_manager,
            server_uri,
        }
    }

    /// 开始新的下载任务
    pub async fn start_download(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 开始新的下载任务: {}", file_id);

        // 检查是否已存在下载任务
        if let Some(existing_metadata) = self.metadata_manager.load_download(file_id).await? {
            match existing_metadata.status {
                DownloadStatus::Completed => {
                    info!("✅ 文件已下载完成: {}", existing_metadata.filename);
                    return Ok(());
                }
                DownloadStatus::Downloading | DownloadStatus::Paused => {
                    info!("🔄 检测到未完成的下载任务，将进行断点续传");
                    return self.resume_download(file_id).await;
                }
                _ => {
                    info!("🗑️ 清理异常状态的下载任务");
                    self.metadata_manager.delete_download(file_id).await?;
                }
            }
        }

        // 开始新的下载
        self.download_file(file_id, None).await
    }

    /// 恢复下载任务
    pub async fn resume_download(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("▶️ 恢复下载任务: {}", file_id);

        if let Some(metadata) = self.metadata_manager.resume_download(file_id).await? {
            let received_chunks: Vec<u32> = metadata.received_chunks.keys().cloned().collect();
            info!("📋 已接收 {} 个数据块，继续下载剩余部分", received_chunks.len());
            
            self.download_file(file_id, Some(received_chunks)).await
        } else {
            Err("找不到可恢复的下载任务".into())
        }
    }

    /// 执行文件下载
    async fn download_file(&self, file_id: &str, received_chunks: Option<Vec<u32>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let download_request = DownloadRequest {
            file_id: file_id.to_string(),
            received_chunks,
        };

        // 发送下载请求
        let stream_response = self.grpc_client.call_server_stream_with_uri::<DownloadRequest, DownloadChunk>(
            &self.server_uri,
            "file",
            "ResumableDownload",
            download_request,
            None,
        ).await?;

        let mut stream = stream_response.stream;
        let mut metadata: Option<DownloadMetadata> = None;

        info!("📥 开始接收文件下载流...");

        while let Some(result) = stream.as_mut().next().await {
            match result {
                Ok(chunk_msg) => {
                    let download_chunk = chunk_msg.data;
                    
                    // 如果是第一个数据块，初始化元数据
                    if metadata.is_none() {
                        if let (Some(file_size), Some(filename)) = (download_chunk.file_size, download_chunk.filename.as_ref()) {
                            let chunk_size = 4096; // 与服务端保持一致
                            let total_chunks = (file_size as usize + chunk_size - 1) / chunk_size;
                            
                            metadata = Some(self.metadata_manager.create_download(
                                file_id,
                                filename,
                                file_size,
                                total_chunks as u32,
                                chunk_size,
                            ).await?);
                            
                            info!("📋 下载任务初始化完成: {} ({} bytes, {} 块)", filename, file_size, total_chunks);
                        }
                    }

                    if let Some(ref mut meta) = metadata {
                        // 写入数据块到指定偏移位置
                        if let Ok(mut file) = fs::OpenOptions::new()
                            .write(true)
                            .open(&meta.download_path).await 
                        {
                            // 定位到指定偏移位置
                            file.seek(std::io::SeekFrom::Start(download_chunk.offset)).await?;
                            
                            // 写入数据块
                            file.write_all(&download_chunk.data).await?;
                            file.flush().await?;
                            
                            info!("💾 写入数据块 {} (偏移: {}, 大小: {} bytes)", 
                                  download_chunk.chunk_index, download_chunk.offset, download_chunk.data.len());

                            // 记录到元数据
                            self.metadata_manager.record_chunk(
                                meta,
                                download_chunk.chunk_index,
                                download_chunk.offset,
                                download_chunk.data.len(),
                            ).await?;

                            // 显示进度
                            let progress = self.metadata_manager.calculate_progress(meta);
                            let downloaded_bytes = self.metadata_manager.get_downloaded_bytes(meta);
                            info!("📊 下载进度: {:.1}% ({}/{} bytes)", 
                                  progress, downloaded_bytes, meta.total_size);
                        }
                    }

                    if download_chunk.is_last {
                        info!("🎉 文件下载完成！");
                        break;
                    }
                }
                Err(e) => {
                    // 检查是否是正常的连接关闭
                    let error_msg = e.to_string();
                    if error_msg.contains("stream closed") || error_msg.contains("broken pipe") {
                        info!("📡 连接已正常关闭");
                        break;
                    } else {
                        error!("❌ 接收数据块失败: {:?}", e);
                        return Err(e.into());
                    }
                }
            }
        }

        // 确保流正确关闭
        drop(stream);
        
        Ok(())
    }

    /// 暂停下载
    pub async fn pause_download(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.metadata_manager.pause_download(file_id).await
    }

    /// 列出所有下载任务
    pub async fn list_downloads(&self) -> Result<Vec<DownloadMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        self.metadata_manager.list_downloads().await
    }

    /// 删除下载任务
    pub async fn delete_download(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.metadata_manager.delete_download(file_id).await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    rat_engine::require_features!("client", "tls");

    // 初始化 CryptoProvider
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    // 日志通过RatEngineBuilder初始化

    println!("🚀 启动断点续传下载完整测试");

    // 启动服务器任务
    let server_handle = tokio::spawn(async {
        if let Err(e) = run_server().await {
            eprintln!("❌ 服务器启动失败: {}", e);
        }
    });

    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 执行完整的下载测试
    let test_result = run_complete_download_test().await;

    // 显示测试结果
    match test_result {
        Ok(_) => {
            println!("✅ 断点续传下载测试完成");
        }
        Err(e) => {
            eprintln!("❌ 断点续传下载测试失败: {}", e);
        }
    }

    // 测试完成，关闭服务器
    println!("🛑 测试完成，正在关闭服务器...");
    server_handle.abort();

    // 等待一下确保服务器关闭
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("✅ 服务器已关闭，测试结束");

    Ok(())
}
/// 运行完整的下载测试流程
async fn run_complete_download_test() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file_id = "test_file_001";
    
    println!("📋 开始完整的断点续传测试流程");
    
    // 1. 清理之前的下载
    println!("🧹 清理之前的下载任务...");
    let _ = run_client_delete(file_id).await; // 忽略错误，可能没有之前的下载
    
    // 2. 开始新的下载（模拟中断）
    println!("📥 开始新的下载任务...");
    let download_result = tokio::time::timeout(
        Duration::from_secs(3), // 3秒后超时，模拟下载中断
        run_client_download(file_id)
    ).await;
    
    let mut download_completed = false;
    match download_result {
        Ok(Ok(_)) => {
            println!("✅ 初次下载已完成");
            download_completed = true;
        }
        Ok(Err(e)) => {
            println!("⚠️ 下载过程中出现错误: {}", e);
        }
        Err(_) => {
            println!("⏰ 下载超时（模拟中断），准备测试断点续传");
        }
    }
    
    // 3. 等待一下
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // 4. 列出下载任务，检查状态
    println!("📋 检查下载任务状态...");
    let _ = run_client_list().await;
    
    // 5. 根据下载状态决定是否需要恢复
    if !download_completed {
        println!("▶️ 尝试恢复下载...");
        let resume_result = tokio::time::timeout(
            Duration::from_secs(10),
            run_client_resume(file_id)
        ).await;
        
        match resume_result {
            Ok(Ok(_)) => {
                println!("✅ 断点续传下载完成");
            }
            Ok(Err(e)) => {
                // 如果恢复失败，检查是否是因为已经完成
                if e.to_string().contains("找不到可恢复的下载任务") {
                    println!("ℹ️ 下载任务可能已经完成，验证文件状态...");
                    let _ = run_client_list().await;
                } else {
                    println!("❌ 断点续传失败: {}", e);
                    return Err(e);
                }
            }
            Err(_) => {
                println!("⏰ 断点续传超时");
                return Err("断点续传超时".into());
            }
        }
    } else {
        println!("ℹ️ 初次下载已完成，无需断点续传");
    }
    
    // 6. 最终验证下载状态
    println!("📋 最终下载任务状态:");
    let _ = run_client_list().await;
    
    println!("🎉 断点续传功能测试完成！");
    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 初始化日志系统
    let log_config = rat_engine::utils::logger::LogConfig::default();
    rat_engine::utils::logger::Logger::init(log_config).expect("Failed to initialize logger");

    info!("🚀 启动支持断点续传的文件下载服务器...");

    // 创建文件存储目录
    fs::create_dir_all("./file_storage").await?;
    
    // 创建测试文件
    let test_content = "这是一个测试文件，用于演示断点续传功能。".repeat(1000); // 约 60KB
    fs::write("./file_storage/test_file_001", test_content).await?;
    info!("📄 创建测试文件: test_file_001");

    // 创建连接管理器
    let connection_manager = Arc::new(GrpcConnectionManager::new());

    // 配置服务器
    let addr: std::net::SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let config = ServerConfig::with_timeouts(
        addr,
        4,
        Some(Duration::from_secs(30)), // connection_timeout
        Some(Duration::from_secs(30))  // request_timeout
    );

    // 创建路由器并注册处理器
    let mut router = Router::new();
    
    // 启用 HTTP/2 支持（需要 TLS）
    router.enable_h2(); // 启用 HTTP/2 with ALPN
    
    router.add_grpc_typed_server_stream(
        "/file/ResumableDownload",
        ResumableFileDownloadHandler::new(connection_manager.clone())
    );

    // 框架会自动输出服务器地址和协议信息
    
    // 启动服务器
    let engine = RatEngine::builder()
        .router(router)
        .enable_h2c_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await
        .map_err(|e| format!("启用开发模式失败: {}", e))?
        .build()?;
    
    engine.start("127.0.0.1".to_string(), 8080).await
}

async fn run_client_download(file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 初始化日志系统
    let log_config = rat_engine::utils::logger::LogConfig::default();
    rat_engine::utils::logger::Logger::init(log_config).expect("Failed to initialize logger");

    info!("📥 开始下载文件: {}", file_id);

    // 创建客户端
    let grpc_client = RatGrpcClientBuilder::new()

        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(10)?
        .http_mixed()
        .user_agent("ResumableDownloadClient/1.0")?
        .disable_compression()
        .h2c_mode()
        .build()?;

    // 创建元数据管理器
    let metadata_manager = DownloadMetadataManager::new("./download_metadata", "./downloads");
    metadata_manager.initialize().await?;

    // 创建下载客户端
    let server_uri = "https://127.0.0.1:8080".to_string();
    let download_client = ResumableDownloadClient::new(grpc_client, metadata_manager, server_uri);

    // 开始下载
    download_client.start_download(file_id).await?;

    Ok(())
}

async fn run_client_resume(file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 初始化日志系统
    let log_config = rat_engine::utils::logger::LogConfig::default();
    rat_engine::utils::logger::Logger::init(log_config).expect("Failed to initialize logger");

    info!("▶️ 恢复下载文件: {}", file_id);

    // 创建客户端
    let grpc_client = RatGrpcClientBuilder::new()

        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(10)?
        .http_mixed()
        .user_agent("ResumableDownloadClient/1.0")?
        .disable_compression()
        .h2c_mode()
        .build()?;

    // 创建元数据管理器
    let metadata_manager = DownloadMetadataManager::new("./download_metadata", "./downloads");
    metadata_manager.initialize().await?;

    // 创建下载客户端
    let server_uri = "https://127.0.0.1:8080".to_string();
    let download_client = ResumableDownloadClient::new(grpc_client, metadata_manager, server_uri);

    // 恢复下载
    download_client.resume_download(file_id).await?;

    Ok(())
}

async fn run_client_list() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("📋 列出所有下载任务");

    // 创建元数据管理器
    let metadata_manager = DownloadMetadataManager::new("./download_metadata", "./downloads");
    metadata_manager.initialize().await?;

    let downloads = metadata_manager.list_downloads().await?;

    if downloads.is_empty() {
        println!("📭 没有找到下载任务");
    } else {
        println!("📋 下载任务列表:");
        for download in downloads {
            let progress = metadata_manager.calculate_progress(&download);
            let downloaded_bytes = metadata_manager.get_downloaded_bytes(&download);
            
            println!("  📄 {} ({})", download.filename, download.file_id);
            println!("     状态: {:?}", download.status);
            println!("     进度: {:.1}% ({}/{} bytes)", progress, downloaded_bytes, download.total_size);
            println!("     路径: {:?}", download.download_path);
            println!();
        }
    }

    Ok(())
}

async fn run_client_delete(file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("🗑️ 删除下载任务: {}", file_id);

    // 创建元数据管理器
    let metadata_manager = DownloadMetadataManager::new("./download_metadata", "./downloads");
    metadata_manager.initialize().await?;

    metadata_manager.delete_download(file_id).await?;
    println!("✅ 下载任务已删除: {}", file_id);

    Ok(())
}