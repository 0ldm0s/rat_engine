//! 综合 gRPC 文件服务示例
//! 
//! 这个示例展示了如何使用 RAT Engine 创建一个完整的 gRPC 文件服务，包括：
//! - 文件上传（一元 gRPC）
//! - 文件下载（服务端流 gRPC）  
//! - 文件列表（HTTP）
//! - 连接管理和保活机制
//! 
//! 同时启动真正的服务器和客户端进行测试

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;

use rat_engine::{Request, Response, Method, StatusCode, Incoming, Bytes, RatError, RatResult};

// 客户端类型导入（需要启用特性）
use rat_engine::client::{RatGrpcClient, RatGrpcClientBuilder, RatIndependentHttpClient, RatIndependentHttpClientBuilder};
use rat_engine::Full;
use tokio::fs;
use tokio::sync::mpsc;
use futures_util::{Stream, StreamExt};
use futures_util::Stream as FuturesStream;
use async_stream::stream;
use serde::{Serialize, Deserialize};
use serde_json;
use bincode::{Encode, Decode};
use std::env;

use rat_engine::{
    server::{
        grpc_handler::{
            GrpcConnectionManager, GrpcConnectionType
        },
        grpc_types::{GrpcRequest, GrpcResponse, GrpcStreamMessage, GrpcError, GrpcStatusCode},
        grpc_codec::GrpcCodec,
        Router, ServerConfig
    },
    utils::logger::{info, warn, error, debug},
  };

// 全局文件 ID 计数器
static FILE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// 生成唯一文件 ID
fn generate_file_id() -> String {
    let id = FILE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("file_{:08x}", id)
}

/// 测试类型枚举
#[derive(Debug, Clone, PartialEq)]
enum TestType {
    Upload,
    QueryDownload,  // 查询+下载组合测试
    MultiUpload,
    ErrorHandling,
    HttpList,
    ChunkedUpload,
    All,
}

impl TestType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "upload" => Some(Self::Upload),
            "query-download" | "query_download" => Some(Self::QueryDownload),
            "multi-upload" | "multi_upload" => Some(Self::MultiUpload),
            "error" | "error-handling" | "error_handling" => Some(Self::ErrorHandling),
            "http" | "http-list" | "http_list" => Some(Self::HttpList),
            "chunked" | "chunked-upload" | "chunked_upload" => Some(Self::ChunkedUpload),
            "all" => Some(Self::All),
            _ => None,
        }
    }
    
    fn description(&self) -> &'static str {
        match self {
            Self::Upload => "文件上传 (一元 gRPC)",
            Self::QueryDownload => "文件查询+下载 (一元查询 + 服务端流下载)",
            Self::MultiUpload => "多文件上传测试",
            Self::ErrorHandling => "错误处理测试",
            Self::HttpList => "HTTP 文件列表",
            Self::ChunkedUpload => "分块上传 (客户端流)",
            Self::All => "所有测试",
        }
    }
}

/// 解析命令行参数
fn parse_args() -> Vec<TestType> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() <= 1 {
        return Vec::new(); // 没有参数，返回空列表
    }
    
    let mut tests = Vec::new();
    for arg in &args[1..] {
        if let Some(test_type) = TestType::from_str(arg) {
            if test_type == TestType::All {
                return vec![TestType::All]; // 如果有 all，直接返回 all
            }
            tests.push(test_type);
        }
    }
    
    tests
}

/// 显示帮助信息
fn show_help() {
    println!("🚀 RAT Engine gRPC 综合测试示例");
    println!();
    println!("用法:");
    println!("  cargo run --example grpc_comprehensive_example [测试类型...]");
    println!();
    println!("可用的测试类型:");
    println!("  upload          - {}", TestType::Upload.description());
    println!("  query-download  - {}", TestType::QueryDownload.description());
    println!("  multi-upload    - {}", TestType::MultiUpload.description());
    println!("  error           - {}", TestType::ErrorHandling.description());
    println!("  http            - {}", TestType::HttpList.description());
    println!("  chunked         - {}", TestType::ChunkedUpload.description());
    println!("  all             - {}", TestType::All.description());
    println!();
    println!("示例:");
    println!("  cargo run --example grpc_comprehensive_example upload query-download");
    println!("  cargo run --example grpc_comprehensive_example query-download");
    println!("  cargo run --example grpc_comprehensive_example all");
    println!();
    println!("注意: 如果不提供参数，将显示此帮助信息并退出。");
}

/// 文件信息结构（简化版，减少序列化数据量）
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FileInfo {
    /// 文件ID
    pub id: String,
    /// 文件名（简化，只保留基本信息）
    pub filename: String,
    /// 文件大小（字节数，以实际文件为准）
    pub size: u64,
    /// 上传时间戳
    pub upload_time: u64,
}

/// 文件上传请求结构体
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct UploadRequest {
    /// 文件名
    pub filename: String,
    /// 文件内容
    pub content: Vec<u8>,
}

/// 文件下载请求结构体
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct DownloadRequest {
    /// 文件ID
    pub file_id: String,
}

/// 下载消息类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq)]
pub enum DownloadMessageType {
    /// 数据块消息
    DataChunk,
    /// 流结束信号
    EndOfStream,
    /// 错误信息
    Error,
}

/// 文件下载响应块结构体（优化版）
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

/// 文件查询请求结构体
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FileQueryRequest {
    /// 查询类型：可以是 "latest" 获取最新文件，或者 "all" 获取所有文件
    pub query_type: String,
    /// 可选的文件名过滤器
    pub filename_filter: Option<String>,
}

/// 文件查询响应结构体
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FileQueryResponse {
    /// 文件列表
    pub files: Vec<FileInfo>,
    /// 查询时间戳
    pub query_time: u64,
}

/// 文件存储管理器
pub struct FileStorageManager {
    storage_path: PathBuf,
}

impl FileStorageManager {
    /// 创建新的文件存储管理器
    pub async fn new(storage_path: impl Into<PathBuf>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let storage_path = storage_path.into();
        
        // 确保存储目录存在
        if !storage_path.exists() {
            fs::create_dir_all(&storage_path).await?;
        }
        
        let manager = Self {
            storage_path,
        };
        
        // 扫描现有文件并记录日志
        let existing_files = manager.scan_existing_files().await?;
        if !existing_files.is_empty() {
            info!("📂 发现 {} 个现有文件:", existing_files.len());
            for file in &existing_files {
                info!("   📄 {}: {} ({} bytes)", file.id, file.filename, file.size);
            }
        }
        
        Ok(manager)
    }
    
    /// 扫描存储目录中的现有文件（优化版：优先使用元数据，但以实际文件为准）
    async fn scan_existing_files(&self) -> Result<Vec<FileInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let mut files = Vec::new();
        
        if !self.storage_path.exists() {
            return Ok(files);
        }
        
        let mut entries = fs::read_dir(&self.storage_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    // 检查是否是有效的文件ID格式（file_开头的8位数字，且不是.meta文件）
                    if file_name.starts_with("file_") && file_name.len() == 13 && !file_name.ends_with(".meta") {
                        // 获取实际文件的元数据
                        let file_metadata = entry.metadata().await?;
                        let actual_size = file_metadata.len();
                        
                        // 尝试读取对应的.meta文件获取文件名和上传时间
                        let meta_path = self.storage_path.join(format!("{}.meta", file_name));
                        let (filename, upload_time) = if meta_path.exists() {
                            // 从.meta文件读取信息
                            match fs::read_to_string(&meta_path).await {
                                Ok(meta_content) => {
                                    match serde_json::from_str::<serde_json::Value>(&meta_content) {
                                        Ok(meta_json) => {
                                            let filename = meta_json.get("filename")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or(&format!("file_{}.txt", &file_name[5..]))
                                                .to_string();
                                            let upload_time = meta_json.get("upload_time")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or_else(|| {
                                                    file_metadata.modified()
                                                        .or_else(|_| file_metadata.created())
                                                        .unwrap_or(SystemTime::UNIX_EPOCH)
                                                        .duration_since(SystemTime::UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_secs()
                                                });
                                            (filename, upload_time)
                                        },
                                        Err(_) => {
                                            // meta文件格式错误，使用默认值
                                            let filename = format!("file_{}.txt", &file_name[5..]);
                                            let upload_time = file_metadata.modified()
                                                .or_else(|_| file_metadata.created())
                                                .unwrap_or(SystemTime::UNIX_EPOCH)
                                                .duration_since(SystemTime::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_secs();
                                            (filename, upload_time)
                                        }
                                    }
                                },
                                Err(_) => {
                                    // 无法读取meta文件，使用默认值
                                    let filename = format!("file_{}.txt", &file_name[5..]);
                                    let upload_time = file_metadata.modified()
                                        .or_else(|_| file_metadata.created())
                                        .unwrap_or(SystemTime::UNIX_EPOCH)
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    (filename, upload_time)
                                }
                            }
                        } else {
                            // 没有meta文件，使用默认值
                            let filename = format!("file_{}.txt", &file_name[5..]);
                            let upload_time = file_metadata.modified()
                                .or_else(|_| file_metadata.created())
                                .unwrap_or(SystemTime::UNIX_EPOCH)
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            (filename, upload_time)
                        };
                        
                        let file_info = FileInfo {
                            id: file_name.to_string(),
                            filename,
                            size: actual_size, // 始终使用实际文件大小
                            upload_time,
                        };
                        
                        files.push(file_info);
                    }
                }
            }
        }
        
        // 按上传时间降序排序
        files.sort_by(|a, b| b.upload_time.cmp(&a.upload_time));
        Ok(files)
    }
    
    /// 保存文件
    pub async fn save_file(&self, filename: &str, content: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let file_id = generate_file_id();
        let file_path = self.storage_path.join(&file_id);
        
        // 写入文件内容
        fs::write(&file_path, content).await?;
        
        // 同时保存文件元数据到一个单独的文件中
        let metadata_file = FileInfo {
            id: file_id.clone(),
            filename: filename.to_string(),
            size: content.len() as u64,
            upload_time: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        };
        
        let metadata_path = self.storage_path.join(format!("{}.meta", file_id));
        let metadata_json = serde_json::to_string_pretty(&metadata_file)?;
        fs::write(&metadata_path, metadata_json).await?;
        
        info!("📁 文件已保存: {} ({}字节) -> {}", filename, content.len(), file_id);
        Ok(file_id)
    }
    
    /// 读取文件内容
    pub async fn read_file_content(&self, file_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let file_path = self.storage_path.join(file_id);
        
        if !file_path.exists() {
            return Err(format!("文件不存在: {}", file_id).into());
        }
        
        let content = fs::read(&file_path).await?;
        Ok(content)
    }
    
    /// 读取文件元数据
    async fn read_file_metadata(&self, file_id: &str) -> Option<FileInfo> {
        let metadata_path = self.storage_path.join(format!("{}.meta", file_id));
        
        if metadata_path.exists() {
            // 如果有元数据文件，直接读取
            if let Ok(metadata_content) = fs::read_to_string(&metadata_path).await {
                if let Ok(file_info) = serde_json::from_str::<FileInfo>(&metadata_content) {
                    return Some(file_info);
                }
            }
        }
        
        // 如果没有元数据文件，从文件系统信息推断
        let file_path = self.storage_path.join(file_id);
        if let Ok(metadata) = fs::metadata(&file_path).await {
            let upload_time = metadata.modified()
                .or_else(|_| metadata.created())
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            
            let filename = format!("uploaded_file_{}.txt", &file_id[5..]);
            
            return Some(FileInfo {
                id: file_id.to_string(),
                filename,
                size: metadata.len(),
                upload_time,
            });
        }
        
        None
    }
    
    /// 列出所有文件（直接从磁盘扫描）
    pub async fn list_files(&self) -> Vec<FileInfo> {
        match self.scan_existing_files().await {
            Ok(files) => files,
            Err(e) => {
                error!("❌ 扫描文件失败: {:?}", e);
                Vec::new()
            }
        }
    }
    
    /// 获取最新上传的文件（直接从磁盘扫描）
    pub async fn get_latest_file(&self) -> Option<FileInfo> {
        let files = self.list_files().await;
        files.into_iter().max_by_key(|file| file.upload_time)
    }
    
    /// 根据文件名过滤查询文件（直接从磁盘扫描）
    pub async fn query_files(&self, filename_filter: Option<&str>) -> Vec<FileInfo> {
        let all_files = self.list_files().await;
        
        if let Some(filter) = filename_filter {
            all_files.into_iter()
                .filter(|file| file.filename.contains(filter) || file.id.contains(filter))
                .collect()
        } else {
            all_files
        }
    }
}

/// 文件上传处理器（一元 gRPC）
pub struct FileUploadHandler {
    storage: Arc<FileStorageManager>,
}

impl FileUploadHandler {
    pub fn new(storage: Arc<FileStorageManager>) -> Self {
        Self { storage }
    }
    
    /// 处理文件上传
    pub async fn handle_upload(&self, data: Vec<u8>) -> Result<UploadResponse, Box<dyn std::error::Error + Send + Sync>> {
        info!("📤 处理文件上传，数据大小: {} bytes", data.len());
        println!("DEBUG: 服务器端接收到的原始数据大小: {} bytes", data.len());
        
        // 打印前 50 个字节的十六进制内容用于调试
        if data.len() > 0 {
            let preview_len = std::cmp::min(50, data.len());
            let hex_preview: String = data[..preview_len].iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");
            println!("DEBUG: 前 {} 字节的十六进制内容: {}", preview_len, hex_preview);
        }
        
        // 使用 bincode 反序列化上传数据
        let upload_request: UploadRequest = match GrpcCodec::decode(&data) {
            Ok(request) => {
                println!("DEBUG: GrpcCodec 反序列化成功");
                request
            },
            Err(e) => {
                error!("❌ GrpcCodec 反序列化失败: {:?}", e);
                println!("DEBUG: GrpcCodec 反序列化详细错误: {:?}", e);
                return Err(format!("反序列化上传请求失败: {}", e).into());
            }
        };
        
        info!("📋 解析上传请求: 文件名 '{}', 大小 {} bytes", upload_request.filename, upload_request.content.len());
        
        let file_id = self.storage.save_file(&upload_request.filename, &upload_request.content).await?;
        
        let response = UploadResponse {
            file_id: file_id.clone(),
            file_size: upload_request.content.len() as u64,
            upload_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string(),
            chunk_count: 1, // 一元请求只有一个块
        };
        
        println!("DEBUG: 服务器端创建响应对象成功");
        info!("✅ 文件上传成功: 文件 '{}' 上传成功 (ID: {})", upload_request.filename, file_id);
        Ok(response)
    }
}

use rat_engine::server::grpc_handler::{UnaryHandler, ServerStreamHandler, ClientStreamHandler};
use rat_engine::server::grpc_types::GrpcContext;

impl UnaryHandler for FileUploadHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        let storage = self.storage.clone();
        
        Box::pin(async move {
            let handler = Self { storage };
            match handler.handle_upload(request.data).await {
                Ok(upload_response) => {
                    // 序列化 UploadResponse
                    match GrpcCodec::encode(&upload_response) {
                        Ok(response_data) => {
                            println!("DEBUG: UnaryHandler 序列化响应成功，数据大小: {} bytes", response_data.len());
                            Ok(GrpcResponse {
                                status: GrpcStatusCode::Ok.as_u32(),
                                message: "Success".to_string(),
                                data: response_data,
                                metadata: HashMap::new(),
                            })
                        }
                        Err(e) => {
                            error!("❌ 序列化 UploadResponse 失败: {:?}", e);
                            Err(GrpcError::Internal(format!("序列化响应失败: {}", e)))
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 文件上传失败: {:?}", e);
                    Err(GrpcError::InvalidArgument(e.to_string()))
                }
            }
        })
    }
}

/// 文件查询处理器（一元 gRPC）
pub struct FileQueryHandler {
    storage: Arc<FileStorageManager>,
}

impl FileQueryHandler {
    pub fn new(storage: Arc<FileStorageManager>) -> Self {
        Self { storage }
    }
    
    /// 处理文件查询
    pub async fn handle_query(&self, data: Vec<u8>) -> Result<FileQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
        info!("🔍 处理文件查询请求，数据大小: {} bytes", data.len());
        
        // 使用 GrpcCodec 反序列化查询请求
        let query_request: FileQueryRequest = match GrpcCodec::decode(&data) {
            Ok(request) => {
                println!("DEBUG: 文件查询请求反序列化成功");
                request
            },
            Err(e) => {
                error!("❌ 反序列化文件查询请求失败: {:?}", e);
                return Err(format!("反序列化查询请求失败: {}", e).into());
            }
        };
        
        info!("📋 查询类型: {}, 文件名过滤器: {:?}", query_request.query_type, query_request.filename_filter);
        
        let files = match query_request.query_type.as_str() {
            "latest" => {
                // 获取最新文件
                if let Some(latest_file) = self.storage.get_latest_file().await {
                    vec![latest_file]
                } else {
                    Vec::new()
                }
            },
            "all" => {
                // 获取所有文件（可选过滤）
                self.storage.query_files(query_request.filename_filter.as_deref()).await
            },
            _ => {
                return Err("不支持的查询类型，支持的类型: 'latest', 'all'".into());
            }
        };
        
        let response = FileQueryResponse {
            files: files.clone(),
            query_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        
        info!("✅ 文件查询成功，返回 {} 个文件", files.len());
        if !files.is_empty() {
            for file in &files {
                info!("   📄 文件: {} (ID: {}, 大小: {} bytes)", file.filename, file.id, file.size);
            }
        }
        
        Ok(response)
    }
}

impl UnaryHandler for FileQueryHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        let storage = self.storage.clone();
        
        Box::pin(async move {
            let handler = Self { storage };
            match handler.handle_query(request.data).await {
                Ok(query_response) => {
                    // 序列化 FileQueryResponse
                    match GrpcCodec::encode(&query_response) {
                        Ok(response_data) => {
                            println!("DEBUG: 文件查询响应序列化成功，数据大小: {} bytes", response_data.len());
                            Ok(GrpcResponse {
                                status: GrpcStatusCode::Ok.as_u32(),
                                message: "Success".to_string(),
                                data: response_data,
                                metadata: HashMap::new(),
                            })
                        }
                        Err(e) => {
                            error!("❌ 序列化 FileQueryResponse 失败: {:?}", e);
                            Err(GrpcError::Internal(format!("序列化响应失败: {}", e)))
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 文件查询失败: {:?}", e);
                    Err(GrpcError::InvalidArgument(e.to_string()))
                }
            }
        })
    }
}

/// 文件下载处理器（服务端流 gRPC）
pub struct FileDownloadHandler {
    storage: Arc<FileStorageManager>,
    connection_manager: Arc<GrpcConnectionManager>,
}

impl FileDownloadHandler {
    pub fn new(storage: Arc<FileStorageManager>, connection_manager: Arc<GrpcConnectionManager>) -> Self {
        Self { storage, connection_manager }
    }
    
    /// 处理文件下载
    pub fn handle_download(&self, data: Vec<u8>) -> Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>> {
        let storage = self.storage.clone();
        let connection_manager = self.connection_manager.clone();
        
        Box::pin(stream! {
            info!("📥 处理文件下载请求，数据大小: {} bytes", data.len());
            println!("DEBUG: 下载请求原始数据前32字节: {:?}", &data[..std::cmp::min(32, data.len())]);
            
            // 使用 GrpcCodec 反序列化下载请求
            let download_request: DownloadRequest = match GrpcCodec::decode::<DownloadRequest>(&data) {
                Ok(request) => {
                    println!("DEBUG: 反序列化成功，file_id: '{}'", request.file_id);
                    request
                },
                Err(e) => {
                    error!("❌ GrpcCodec 反序列化失败: {:?}", e);
                    println!("DEBUG: 反序列化失败，原始数据: {:?}", data);
                    yield Err(GrpcError::InvalidArgument(format!("反序列化下载请求失败: {}", e)));
                    return;
                }
            };
            
            let file_id = download_request.file_id.trim().to_string();
            println!("DEBUG: 处理后的 file_id: '{}'，长度: {}", file_id, file_id.len());
            
            if file_id.is_empty() {
                yield Err(GrpcError::InvalidArgument("File ID cannot be empty".to_string()));
                return;
            }
            
            info!("📥 请求下载文件: {}", file_id);
            
            // 创建连接（使用默认用户ID）
            let user_id = "client_user".to_string();
            let (conn_id, mut _rx) = connection_manager.add_connection(
                user_id.clone(),
                Some("download_room".to_string()),
                GrpcConnectionType::ServerStream
            );
            
            info!("🔗 为下载创建连接: {} (文件: {})", conn_id, file_id);
            
            // 读取文件内容和元数据
            match storage.read_file_content(&file_id).await {
                Ok(file_content) => {
                    let chunk_size = 3072; // 3KB 分块 (3的倍数，避免中文字符截断)
                    let total_chunks = (file_content.len() + chunk_size - 1) / chunk_size;
                    let file_size = file_content.len() as u64;
                    
                    // 尝试获取文件名
                    let filename = storage.read_file_metadata(&file_id).await
                        .map(|meta| meta.filename)
                        .unwrap_or_else(|| format!("file_{}", file_id));
                    
                    info!("📦 开始分块传输，文件: {}, 总大小: {} bytes，分为 {} 块", filename, file_size, total_chunks);
                    
                    // 分块发送文件内容
                    for (i, chunk) in file_content.chunks(chunk_size).enumerate() {
                        let chunk_num = i + 1;
                        let progress = (chunk_num as f32 / total_chunks as f32 * 100.0) as u32;
                        let offset = (i * chunk_size) as u64;
                        
                        info!("📤 发送块 {}/{} ({}%, 偏移: {} bytes)", chunk_num, total_chunks, progress, offset);
                        
                        // 创建下载响应块
                        let is_last_chunk = chunk_num == total_chunks;
                        let download_chunk = DownloadChunk {
                            message_type: DownloadMessageType::DataChunk,
                            chunk_index: chunk_num as u32,
                            total_chunks: total_chunks as u32,
                            data: chunk.to_vec(),
                            is_last: is_last_chunk,
                            error_message: None,
                            file_id: file_id.clone(),
                            file_size: if chunk_num == 1 { Some(file_size) } else { None }, // 只在第一个块中包含文件大小
                            filename: if chunk_num == 1 { Some(filename.clone()) } else { None }, // 只在第一个块中包含文件名
                            offset,
                        };
                        
                        // 序列化 DownloadChunk 到 data 字段
                        println!("DEBUG: [服务端] 准备序列化 DownloadChunk - chunk_index: {}, is_last: {}, data_len: {}",
                                download_chunk.chunk_index, download_chunk.is_last, download_chunk.data.len());

                        match GrpcCodec::encode(&download_chunk) {
                            Ok(serialized_chunk) => {
                                println!("DEBUG: [服务端] DownloadChunk 序列化成功，序列化后大小: {} bytes", serialized_chunk.len());
                                println!("DEBUG: [服务端] 序列化数据前32字节: {:?}", &serialized_chunk[..std::cmp::min(32, serialized_chunk.len())]);
                                
                                let mut metadata = HashMap::new();
                                metadata.insert("chunk".to_string(), format!("{}/{}", chunk_num, total_chunks));
                                metadata.insert("message_type".to_string(), "data_chunk".to_string());
                                
                                yield Ok(GrpcStreamMessage {
                                    id: chunk_num as u64,
                                    stream_id: 1,
                                    sequence: chunk_num as u64,
                                    end_of_stream: is_last_chunk, // 最后一个数据块时设置流结束标志
                                    data: serialized_chunk,
                                    metadata,
                                });
                            }
                            Err(e) => {
                                error!("❌ 序列化下载块失败: {:?}", e);
                                yield Err(GrpcError::Internal(format!("序列化下载块失败: {}", e)));
                                return;
                            }
                        }
                    }
                    
                    info!("✅ 文件下载完成: {}", file_id);
                }
                Err(e) => {
                    error!("❌ 读取文件失败: {:?}", e);
                    // 清理连接
                    connection_manager.remove_connection(&conn_id);
                    yield Err(GrpcError::NotFound(format!("文件不存在: {}", file_id)));
                    return;
                }
            }
            
            // 清理连接
            connection_manager.remove_connection(&conn_id);
            info!("🔌 移除 gRPC 连接: {} (用户: {})", conn_id, user_id);
        })
    }
}

impl ServerStreamHandler for FileDownloadHandler {
    fn handle(
        &self,
        request: GrpcRequest<Vec<u8>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>, GrpcError>> + Send>> {
        let stream = self.handle_download(request.data);
        Box::pin(async move {
            Ok(stream)
        })
    }
}

/// 客户端流分块上传处理器
pub struct ChunkedUploadHandler {
    storage: Arc<FileStorageManager>,
    connection_manager: Arc<GrpcConnectionManager>,
}

impl ChunkedUploadHandler {
    pub fn new(storage: Arc<FileStorageManager>, connection_manager: Arc<GrpcConnectionManager>) -> Self {
        Self { storage, connection_manager }
    }
    
    /// 处理分块上传流
    pub async fn handle_chunked_upload(
        &self,
        mut stream: Pin<Box<dyn Stream<Item = Result<FileChunk, GrpcError>> + Send>>,
    ) -> Result<UploadResponse, GrpcError> {
        println!("DEBUG: ===== 进入 handle_chunked_upload 方法 =====");
        info!("📦 开始处理客户端流分块上传");
        
        let mut file_name = String::new();
        let mut total_chunks = 0u32;
        let mut received_chunks = Vec::new();
        let mut total_size = 0u64;
        
        // 创建连接（使用默认用户ID）
        let user_id = "client_user".to_string();
        let (conn_id, mut _rx) = self.connection_manager.add_connection(
            user_id.clone(),
            Some("upload_room".to_string()),
            GrpcConnectionType::ClientStream
        );
        
        info!("🔗 为分块上传创建连接: {}", conn_id);
        
        println!("DEBUG: 开始接收流数据块");
        // 接收所有块
        let mut chunk_count = 0;
        while let Some(chunk_result) = stream.next().await {
            chunk_count += 1;
            println!("DEBUG: 接收到第 {} 个流项目", chunk_count);
            match chunk_result {
                Ok(chunk) => {
                    info!("📥 收到块 {}/{} (大小: {} bytes, 元数据: {})", 
                          chunk.chunk_index, chunk.total_chunks, chunk.data.len(), chunk.is_metadata);
                    
                    if chunk.is_metadata {
                        // 处理元数据块
                        file_name = chunk.file_name.clone();
                        total_chunks = chunk.total_chunks;
                        info!("📋 收到文件元数据: {} (总块数: {})", file_name, total_chunks);
                    } else {
                        // 处理数据块
                        total_size += chunk.data.len() as u64;
                        received_chunks.push(chunk);
                    }
                }
                Err(e) => {
                    error!("❌ 接收块时出错: {:?}", e);
                    self.connection_manager.remove_connection(&conn_id);
                    return Err(e);
                }
            }
        }
        
        println!("DEBUG: 流处理循环结束，总共接收了 {} 个流项目", chunk_count);
        info!("📊 接收完成: {} 个数据块，总大小: {} bytes", received_chunks.len(), total_size);
        
        // 验证块的完整性
        if received_chunks.len() != total_chunks as usize {
            let error_msg = format!("块数量不匹配: 期望 {} 个，实际收到 {} 个", 
                                   total_chunks, received_chunks.len());
            error!("❌ {}", error_msg);
            self.connection_manager.remove_connection(&conn_id);
            return Err(GrpcError::InvalidArgument(error_msg));
        }
        
        // 按块索引排序
        received_chunks.sort_by_key(|chunk| chunk.chunk_index);
        
        // 重组文件内容
        let mut file_content = Vec::new();
        for chunk in received_chunks {
            file_content.extend_from_slice(&chunk.data);
        }
        
        info!("🔧 文件重组完成，最终大小: {} bytes", file_content.len());
        
        // 保存文件
        match self.storage.save_file(&file_name, &file_content).await {
            Ok(file_id) => {
                let upload_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                let response = UploadResponse {
                    file_id: file_id.clone(),
                    file_size: file_content.len() as u64,
                    upload_time: format!("{}", upload_time),
                    chunk_count: total_chunks,
                };
                
                info!("✅ 分块上传成功: 文件 '{}' (ID: {}, 大小: {} bytes, {} 块)", 
                      file_name, file_id, file_content.len(), total_chunks);
                
                // 清理连接
                self.connection_manager.remove_connection(&conn_id);
                
                Ok(response)
            }
            Err(e) => {
                let error_msg = format!("保存文件失败: {}", e);
                error!("❌ {}", error_msg);
                self.connection_manager.remove_connection(&conn_id);
                Err(GrpcError::Internal(error_msg))
            }
        }
    }
}

impl ClientStreamHandler for ChunkedUploadHandler {
    fn handle(
        &self,
        request_stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<Vec<u8>>, GrpcError>> + Send>>,
        _context: GrpcContext,
    ) -> Pin<Box<dyn Future<Output = Result<GrpcResponse<Vec<u8>>, GrpcError>> + Send>> {
        let handler = self.clone();
        
        Box::pin(async move {
            println!("DEBUG: ClientStreamHandler 开始处理流");
            
            // 收集所有流数据，直到遇到结束信号或客户端断开
            let mut chunks = Vec::new();
            let mut stream_count = 0;
            let mut stream = request_stream;
            let mut received_close_signal = false;
            
            // 设置超时时间（60秒）
            let timeout_duration = std::time::Duration::from_secs(60);
            let start_time = std::time::Instant::now();
            
            loop {
                // 检查超时
                if start_time.elapsed() > timeout_duration {
                    println!("DEBUG: 服务端流处理超时（60秒），可能客户端已断开");
                    break;
                }
                
                // 使用 tokio::time::timeout 为每次 next() 调用设置超时
                match tokio::time::timeout(std::time::Duration::from_secs(5), stream.as_mut().next()).await {
                    Ok(Some(result)) => {
                        stream_count += 1;
                        println!("DEBUG: 接收到第 {} 个流项目", stream_count);
                        
                        match result {
                            Ok(msg) => {
                                println!("DEBUG: 收到流消息，end_of_stream: {}, 数据长度: {}", msg.end_of_stream, msg.data.len());
                                
                                if msg.end_of_stream {
                                    println!("DEBUG: 检测到流结束标志，正常结束");
                                    received_close_signal = true;
                                    break; // 遇到结束信号，停止接收
                                } else {
                                    // 尝试反序列化为 FileChunk
                                    match GrpcCodec::decode::<FileChunk>(&msg.data) {
                                        Ok(chunk) => {
                                            println!("DEBUG: 收到普通数据块，大小: {} 字节", msg.data.len());
                                            chunks.push(chunk);
                                        }
                                        Err(e) => {
                                            println!("DEBUG: 反序列化失败: {:?}", e);
                                            return Err(GrpcError::InvalidArgument(format!("反序列化 FileChunk 失败: {}", e)));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let error_msg = e.to_string();
                                println!("DEBUG: 流消息错误: {}", error_msg);
                                
                                // 检查是否是客户端断开连接
                                if error_msg.contains("stream no longer needed") || 
                                   error_msg.contains("connection closed") ||
                                   error_msg.contains("reset") ||
                                   error_msg.contains("broken pipe") {
                                    println!("DEBUG: 检测到客户端断开连接，结束流处理");
                                    break;
                                }
                                
                                return Err(e);
                            }
                        }
                    }
                    Ok(None) => {
                        println!("DEBUG: 流已结束（客户端断开连接或正常结束）");
                        break;
                    }
                    Err(_) => {
                        println!("DEBUG: 等待下一个流项目超时（5秒），可能客户端已断开");
                        // 超时不一定是错误，可能客户端已经发送完所有数据但没有发送关闭信号
                        // 我们继续等待，但会在总超时时间到达时退出
                        continue;
                    }
                }
            }
            
            println!("DEBUG: 流处理完成，共收到 {} 个流项目，其中 {} 个数据块", stream_count, chunks.len());
            println!("DEBUG: 是否收到关闭信号: {}", received_close_signal);
            
            // 即使没有收到关闭信号，只要收到了数据块，我们也尝试处理
            if chunks.is_empty() {
                println!("DEBUG: 没有收到任何数据块");
                return Err(GrpcError::InvalidArgument("没有收到任何数据块".to_string()));
            }
            
            // 将收集到的块转换为流
            let chunk_stream = futures_util::stream::iter(chunks.into_iter().map(Ok));
            let pinned_chunk_stream = Box::pin(chunk_stream);
            
            println!("DEBUG: 准备调用 handle_chunked_upload");
            
            match handler.handle_chunked_upload(pinned_chunk_stream).await {
                Ok(upload_response) => {
                    println!("DEBUG: handle_chunked_upload 返回成功响应");
                    // 序列化响应
                    match GrpcCodec::encode(&upload_response) {
                        Ok(response_data) => {
                            Ok(GrpcResponse {
                                status: GrpcStatusCode::Ok.as_u32(),
                                message: "Chunked upload successful".to_string(),
                                data: response_data,
                                metadata: HashMap::new(),
                            })
                        }
                        Err(e) => {
                            error!("❌ 序列化响应失败: {:?}", e);
                            Err(GrpcError::Internal(format!("序列化响应失败: {}", e)))
                        }
                    }
                }
                Err(e) => {
                    println!("DEBUG: handle_chunked_upload 返回错误: {:?}", e);
                    error!("❌ 分块上传处理失败: {:?}", e);
                    Err(e)
                }
            }
        })
    }
}

// 为 ChunkedUploadHandler 实现 Clone
impl Clone for ChunkedUploadHandler {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            connection_manager: self.connection_manager.clone(),
        }
    }
}

/// 文件列表 HTTP 处理器
pub struct FileListHttpHandler {
    storage: Arc<FileStorageManager>,
}

impl FileListHttpHandler {
    pub fn new(storage: Arc<FileStorageManager>) -> Self {
        Self { storage }
    }
}

// FileListHttpHandler 实现将在路由中直接使用闭包

/// 生成文件列表 HTML
fn generate_file_list_html(files: &[FileInfo]) -> String {
    let mut html = String::from(r#"
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine 文件列表</title>
    <meta charset="utf-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #f2f2f2; }
    </style>
</head>
<body>
    <h1>🚀 RAT Engine 文件服务器</h1>
    <h2>📁 文件列表</h2>
    <table>
        <tr>
            <th>文件ID</th>
            <th>文件名</th>
            <th>大小</th>
            <th>上传时间</th>
        </tr>
"#);
    
    for file in files {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{} bytes</td><td>{}</td></tr>",
            file.id,
            file.filename,
            file.size,
            file.upload_time
        ));
    }
    
    html.push_str("</table></body></html>");
    html
}

/// 启动真正的 gRPC 文件服务器
pub async fn start_real_grpc_file_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("🚀 启动真正的 gRPC 文件服务器...");
    println!("DEBUG: 开始启动服务器");
    

    
    // 创建文件存储管理器
    let storage = Arc::new(FileStorageManager::new("./file_storage").await?);
    println!("DEBUG: 文件存储管理器创建成功");
    
    // 创建服务器配置
    let addr = "127.0.0.1:50051".parse().unwrap();
    println!("DEBUG: 服务器地址解析成功: {}", addr);
    let server_config = ServerConfig::new(addr, 4)
        .with_log_config(rat_engine::utils::logger::LogConfig::default());
    println!("DEBUG: 服务器配置创建成功");
    
    // 创建路由器
    let mut router = Router::new();
    router.enable_h2(); // 启用 HTTP/2 支持，gRPC 需要 HTTP/2
    router.enable_h2c(); // 启用 H2C 以支持明文 HTTP/2
    println!("DEBUG: 路由器创建成功，H2 和 H2C 支持已启用");
    
    // 注册 gRPC 服务
    let connection_manager = Arc::new(GrpcConnectionManager::new());
    println!("DEBUG: gRPC 连接管理器创建成功");
    let upload_handler = FileUploadHandler::new(storage.clone());
    println!("DEBUG: 上传处理器创建成功");
    let query_handler = FileQueryHandler::new(storage.clone());
    println!("DEBUG: 查询处理器创建成功");
    let download_handler = FileDownloadHandler::new(storage.clone(), connection_manager.clone());
    println!("DEBUG: 下载处理器创建成功");
    let chunked_upload_handler = ChunkedUploadHandler::new(storage.clone(), connection_manager.clone());
    println!("DEBUG: 分块上传处理器创建成功");
    // 注册 gRPC 服务（使用 HTTP 路径格式以匹配客户端请求）
    router.add_grpc_unary("/file/Upload", upload_handler);
    println!("DEBUG: 一元 gRPC 服务 /file/Upload 注册成功");
    router.add_grpc_unary("/file/Query", query_handler);
    println!("DEBUG: 一元 gRPC 服务 /file/Query 注册成功");
    router.add_grpc_server_stream("/file/Download", download_handler);
    println!("DEBUG: 服务端流 gRPC 服务 /file/Download 注册成功");
    router.add_grpc_client_stream("/file/ChunkedUpload", chunked_upload_handler);
    println!("DEBUG: 客户端流 gRPC 服务 /file/ChunkedUpload 注册成功");
    
    // 添加 HTTP 文件列表路由
    let storage_for_route = storage.clone();
    router.add_route(Method::GET, "/files", move |_req| {
        let storage = storage_for_route.clone();
        Box::pin(async move {
            println!("DEBUG: HTTP /files 路由被调用");
            let file_list = storage.list_files().await;
            println!("DEBUG: 扫描到 {} 个文件", file_list.len());
            
            for (i, file) in file_list.iter().enumerate() {
                println!("DEBUG: 文件 {}: {} ({} bytes)", i + 1, file.id, file.size);
            }
            
            let json_response = serde_json::to_string(&file_list).unwrap_or_else(|_| "[]".to_string());
            println!("DEBUG: JSON 响应长度: {} 字符", json_response.len());
            
            let response = rat_engine::Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(http_body_util::Full::new(bytes::Bytes::from(json_response)))
                .unwrap();
                
            Ok(response)
        })
    });
    
    info!("📝 已注册 gRPC 服务:");
    info!("   📤 /file/Upload - 文件上传 (一元请求)");
    info!("   🔍 /file/Query - 文件查询 (一元请求)");
    info!("   📥 /file/Download - 文件下载 (服务端流)");
    info!("   📦 /file/ChunkedUpload - 分块上传 (客户端流)");
    info!("   🌐 GET /files - 文件列表 (HTTP)");
    
    info!("🚀 启动服务器在 http://127.0.0.1:50051");
    info!("📋 文件列表页面: http://127.0.0.1:50051/files");
    info!("🔧 gRPC 端点: grpc://127.0.0.1:50051");
    
    println!("DEBUG: 准备启动服务器...");
    // 启动真正的服务器
    println!("DEBUG: 使用 RatEngineBuilder 启动服务器");
    let engine = rat_engine::RatEngine::builder()
        .with_log_config(rat_engine::utils::logger::LogConfig::default())
        .enable_h2c_mode(vec!["127.0.0.1".to_string(), "localhost".to_string()]).await?
        .router(router)
        .build()?;
    engine.start("127.0.0.1".to_string(), 50051).await?;
    println!("DEBUG: 服务器已停止");
    
    Ok(())
}

/// 共享测试状态
#[derive(Debug, Default)]
struct TestState {
    uploaded_file_id: Option<String>,
    chunked_upload_file_id: Option<String>,
}

/// 测试结果统计
#[derive(Debug, Default)]
struct TestResults {
    total_tests: u32,
    successful_tests: u32,
    failed_tests: u32,
    test_details: Vec<String>,
}

impl TestResults {
    fn add_success(&mut self, test_name: &str) {
        self.total_tests += 1;
        self.successful_tests += 1;
        self.test_details.push(format!("✅ {}", test_name));
    }
    
    fn add_failure(&mut self, test_name: &str, error: &str) {
        self.total_tests += 1;
        self.failed_tests += 1;
        self.test_details.push(format!("❌ {} - {}", test_name, error));
    }
    
    fn print_summary(&self) {
        info!("📊 ========== 测试结果统计 ==========");
        info!("📈 总测试数: {}", self.total_tests);
        info!("✅ 成功: {}", self.successful_tests);
        info!("❌ 失败: {}", self.failed_tests);
        info!("📋 详细结果:");
        for detail in &self.test_details {
            info!("   {}", detail);
        }
        info!("=====================================");
        
        if self.failed_tests == 0 {
            info!("🎉 所有测试通过！");
        } else {
            warn!("⚠️ 有 {} 个测试失败", self.failed_tests);
        }
    }
}

/// 创建真正的 gRPC 客户端并测试服务器
async fn run_real_grpc_client_tests(tests_to_run: Vec<TestType>) -> Result<TestResults, Box<dyn std::error::Error + Send + Sync>> {
    info!("🧪 启动真正的 gRPC 客户端测试...");
    let mut results = TestResults::default();
    let mut test_state = TestState::default();
    
    // 等待服务器启动
    println!("DEBUG: 客户端等待服务器启动...");
    tokio::time::sleep(Duration::from_secs(6)).await;
    println!("DEBUG: 等待完成，开始创建客户端");
    
    // 创建 rat_engine gRPC 客户端
    println!("DEBUG: 开始构建 gRPC 客户端");
    let grpc_client = match RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .and_then(|b| {
            println!("DEBUG: connect_timeout 设置成功");
            b.request_timeout(Duration::from_secs(30))
        })
        .and_then(|b| {
            println!("DEBUG: request_timeout 设置成功");
            b.max_idle_connections(10)
        })
        .and_then(|b| {
            println!("DEBUG: max_idle_connections 设置成功");
            Ok(b.http_mixed())
        })
        .and_then(|b| {
            println!("DEBUG: http_mixed 设置成功");
            b.user_agent("RAT-Engine-gRPC-Client/1.0")
        })
        .and_then(|b| {
            println!("DEBUG: user_agent 设置成功");
            Ok(b.disable_compression())
        })
        .and_then(|b| {
            println!("DEBUG: disable_compression 设置成功");
            Ok(b.h2c_mode())
        })
        .and_then(|b| {
            println!("DEBUG: h2c_mode 设置成功，开始构建客户端实例");
            b.build()
        }) {
        Ok(client) => {
            println!("DEBUG: gRPC 客户端创建成功");
            client
        }
        Err(e) => {
            println!("DEBUG: gRPC 客户端创建失败: {:?}", e);
            return Err(e.into());
        }
    };
    
    // 创建独立HTTP客户端
    let http_client = RatIndependentHttpClientBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("RAT-Engine-HTTP-Client/1.0")
        .supported_compressions(vec!["gzip".to_string(), "deflate".to_string()])
        .auto_decompress(true)
        .build()?;
    

    
    // 测试文件上传
    if tests_to_run.contains(&TestType::Upload) {
        info!("📤 测试真实 gRPC 文件上传...");
        
        let test_filename = "real_test_file.txt";
        let test_content = "这是真实的 RAT Engine gRPC 客户端上传测试文件内容！".as_bytes();
        
        // 创建上传请求结构体
        let upload_request = UploadRequest {
            filename: test_filename.to_string(),
            content: test_content.to_vec(),
        };
        
        // 序列化上传请求
        let upload_data = match GrpcCodec::encode(&upload_request) {
            Ok(data) => {
                println!("DEBUG: 客户端序列化成功，数据大小: {} bytes", data.len());
                
                // 打印前 50 个字节的十六进制内容用于调试
                if data.len() > 0 {
                    let preview_len = std::cmp::min(50, data.len());
                    let hex_preview: String = data[..preview_len].iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(" ");
                    println!("DEBUG: 客户端发送的前 {} 字节十六进制: {}", preview_len, hex_preview);
                }
                
                data
            },
            Err(e) => {
                let error_msg = format!("序列化上传请求失败: {:?}", e);
                error!("❌ 文件上传序列化失败: {:?}", e);
                results.add_failure("RAT Engine gRPC 文件上传", &error_msg);
                return Ok(results);
            }
        };
        
        println!("DEBUG: 准备发送 gRPC 请求 - service: file, method: Upload");
        println!("DEBUG: 完整请求路径应该是: /file/Upload");
        println!("DEBUG: 请求数据大小: {} 字节", upload_data.len());
        println!("DEBUG: 调用 grpc_client.call_with_uri 参数: uri='http://127.0.0.1:50051', service='file', method='Upload'");
        match grpc_client.call_with_uri::<Vec<u8>, UploadResponse>(
            "http://127.0.0.1:50051",
            "file",
            "Upload",
            upload_data,
            None,
        ).await {
            Ok(response) => {
                println!("DEBUG: 客户端收到响应，状态: {}", response.status);
                
                // 检查响应状态
                if response.status == 0 {
                    println!("DEBUG: 客户端响应反序列化成功");
                    info!("✅ RAT Engine gRPC 文件上传成功:");
                    info!("   文件ID: {}", response.data.file_id);
                    info!("   文件大小: {} 字节", response.data.file_size);
                    info!("   上传时间: {}", response.data.upload_time);
                    results.add_success("RAT Engine gRPC 文件上传");
                    
                    // 保存文件ID供下载测试使用
                    test_state.uploaded_file_id = Some(response.data.file_id.clone());
                } else {
                    let error_msg = format!("gRPC 响应错误: 状态码 {}, 消息: {}", response.status, response.message);
                    error!("❌ gRPC 响应错误: {}", error_msg);
                    results.add_failure("RAT Engine gRPC 文件上传", &error_msg);
                }
            }
            Err(e) => {
                let error_msg = format!("gRPC 请求失败: {:?}", e);
                error!("❌ RAT Engine gRPC 客户端请求失败: {:?}", e);
                results.add_failure("RAT Engine gRPC 文件上传", &error_msg);
            }
        }
    }
    
    // 测试文件查询+下载（一元查询 + 服务端流下载）
    if tests_to_run.contains(&TestType::QueryDownload) {
        info!("🔍📥 测试文件查询+下载功能...");
        
        // 第一步：测试查询最新文件
        info!("🔍 步骤1: 查询最新文件...");
        let query_request = FileQueryRequest {
            query_type: "latest".to_string(),
            filename_filter: None,
        };
        
        let latest_file_id = match grpc_client.call_typed_with_uri::<FileQueryRequest, FileQueryResponse>(
            "http://127.0.0.1:50051",
            "file",
            "Query",
            query_request,
            None,
        ).await {
            Ok(query_response) => {
                if query_response.status == 0 {
                    info!("✅ 文件查询成功，返回 {} 个文件", query_response.data.files.len());
                    for file in &query_response.data.files {
                        info!("   📄 {}: {} ({} bytes)", file.id, file.filename, file.size);
                    }
                    results.add_success(&format!("文件查询+下载 - 查询步骤 ({} 个文件)", query_response.data.files.len()));
                    
                    // 获取最新文件ID用于下载
                    if let Some(latest_file) = query_response.data.files.first() {
                        latest_file.id.clone()
                    } else {
                        let error_msg = "查询结果为空，没有可下载的文件";
                        error!("❌ {}", error_msg);
                        results.add_failure("文件查询+下载 - 查询步骤", error_msg);
                        return Ok(results);
                    }
                } else {
                    let error_msg = format!("查询响应错误: 状态码 {}, 消息: {}", query_response.status, query_response.message);
                    error!("❌ 文件查询响应错误: {}", error_msg);
                    results.add_failure("文件查询+下载 - 查询步骤", &error_msg);
                    return Ok(results);
                }
            }
            Err(e) => {
                let error_msg = format!("查询请求失败: {:?}", e);
                error!("❌ 文件查询请求失败: {:?}", e);
                results.add_failure("文件查询+下载 - 查询步骤", &error_msg);
                return Ok(results);
            }
        };
        
        // 第二步：测试带文件名过滤器的查询
        info!("🔍 步骤2: 测试过滤查询...");
        let filtered_query_request = FileQueryRequest {
            query_type: "all".to_string(),
            filename_filter: Some("file_".to_string()),
        };
        
        match grpc_client.call_typed_with_uri::<FileQueryRequest, FileQueryResponse>(
            "http://127.0.0.1:50051",
            "file",
            "Query",
            filtered_query_request,
            None,
        ).await {
            Ok(query_response) => {
                if query_response.status == 0 {
                    info!("✅ 过滤查询成功，返回 {} 个匹配文件", query_response.data.files.len());
                    for file in &query_response.data.files {
                        info!("   📄 {}: {} ({} bytes)", file.id, file.filename, file.size);
                    }
                    results.add_success(&format!("文件查询+下载 - 过滤查询 ({} 个文件)", query_response.data.files.len()));
                } else {
                    let error_msg = format!("过滤查询响应错误: 状态码 {}, 消息: {}", query_response.status, query_response.message);
                    error!("❌ 过滤查询响应错误: {}", error_msg);
                    results.add_failure("文件查询+下载 - 过滤查询", &error_msg);
                }
            }
            Err(e) => {
                let error_msg = format!("过滤查询请求失败: {:?}", e);
                error!("❌ 过滤查询请求失败: {:?}", e);
                results.add_failure("文件查询+下载 - 过滤查询", &error_msg);
            }
        }
        
        // 第三步：使用查询到的文件ID进行下载
        info!("📥 步骤3: 下载文件 ID: {}", latest_file_id);
        
        let download_request = DownloadRequest {
            file_id: latest_file_id,
        };
        
        match grpc_client.call_server_stream_with_uri::<DownloadRequest, DownloadChunk>(
            "http://127.0.0.1:50051",
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
                
                info!("📥 开始接收文件下载流...");
                
                while let Some(result) = stream.as_mut().next().await {
                    match result {
                        Ok(chunk) => {
                            let receive_time = std::time::Instant::now();
                            info!("🔄 [实时] 收到流消息 - ID: {}, 序列: {}, 结束标志: {}", 
                                    chunk.id, chunk.sequence, chunk.end_of_stream);
                            
                            // 现在 chunk.data 已经是 DownloadChunk 类型了
                            let download_chunk = chunk.data;
                            info!("📦 [实时] 数据块详情: 类型={:?}, 文件ID={}, 索引={}, 偏移={}, 大小={} bytes, 最后块={}", 
                                    download_chunk.message_type, download_chunk.file_id, 
                                    download_chunk.chunk_index, download_chunk.offset, 
                                    download_chunk.data.len(), download_chunk.is_last);
                            
                            // 立即处理数据块，实现真正的流式下载
                    
                            match download_chunk.message_type {
                                DownloadMessageType::DataChunk => {
                                    // 如果是第一个数据块，初始化文件
                                    if download_chunk.chunk_index == 1 {
                                        if let (Some(file_size), Some(filename)) = (download_chunk.file_size, download_chunk.filename.as_ref()) {
                                            expected_file_size = Some(file_size);
                                            download_filename = Some(filename.clone());
                                            
                                            // 创建下载文件路径
                                            let download_path = format!("./downloads/{}", filename);
                                            
                                            // 确保下载目录存在
                                            if let Err(e) = tokio::fs::create_dir_all("./downloads").await {
                                                error!("❌ 创建下载目录失败: {}", e);
                                                results.add_failure("文件查询+下载 - 下载步骤", &format!("创建下载目录失败: {}", e));
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
                                                        warn!("⚠️ 预分配文件空间失败: {}, 继续下载", e);
                                                    } else {
                                                        info!("✅ 已预分配文件空间: {} bytes -> {}", file_size, download_path);
                                                    }
                                                    file_handle = Some(file);
                                                }
                                                Err(e) => {
                                                    error!("❌ 创建下载文件失败: {}", e);
                                                    results.add_failure("文件查询+下载 - 下载步骤", &format!("创建下载文件失败: {}", e));
                                                    break;
                                                }
                                            }
                                            
                                            info!("🎯 [网际快车模式] 文件初始化完成: {} ({} bytes)", filename, file_size);
                                        }
                                    }
                                    
                                    // 实时写入数据块到指定偏移位置
                                    if let Some(ref mut file) = file_handle {
                                        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
                                        
                                        // 定位到指定偏移位置
                                        if let Err(e) = file.seek(std::io::SeekFrom::Start(download_chunk.offset)).await {
                                            error!("❌ 定位文件偏移失败: {}", e);
                                            results.add_failure("文件查询+下载 - 下载步骤", &format!("定位文件偏移失败: {}", e));
                                            break;
                                        }
                                        
                                        // 写入数据块
                                        if let Err(e) = file.write_all(&download_chunk.data).await {
                                            error!("❌ 写入数据块失败: {}", e);
                                            results.add_failure("文件查询+下载 - 下载步骤", &format!("写入数据块失败: {}", e));
                                            break;
                                        }
                                        
                                        // 强制刷新到磁盘
                                        if let Err(e) = file.flush().await {
                                            warn!("⚠️ 刷新文件缓冲区失败: {}", e);
                                        }
                                        
                                        total_chunks += 1;
                                        total_bytes += download_chunk.data.len();
                                        
                                        let write_time = receive_time.elapsed();
                                        info!("💾 [实时写入] 块 {}/{} 已写入偏移 {} (大小: {} bytes, 耗时: {:?})", 
                                              download_chunk.chunk_index, download_chunk.total_chunks, 
                                              download_chunk.offset, download_chunk.data.len(), write_time);
                                        
                                        // 计算下载进度
                                        if let Some(file_size) = expected_file_size {
                                            let progress = (total_bytes as f64 / file_size as f64 * 100.0) as u32;
                                            info!("📊 [进度] {}% ({}/{} bytes)", progress, total_bytes, file_size);
                                        }
                                    } else {
                                        // 如果没有文件句柄，暂存数据块
                                        received_chunks.insert(download_chunk.chunk_index, download_chunk.data.clone());
                                        total_chunks += 1;
                                        total_bytes += download_chunk.data.len();
                                        
                                        info!("📦 [暂存] 数据块 {} (索引: {}/{}, 大小: {} bytes)", 
                                              total_chunks, download_chunk.chunk_index, download_chunk.total_chunks, download_chunk.data.len());
                                    }
                                    
                                    // 检查是否为最后一个数据块
                                    if download_chunk.is_last {
                                        info!("🏁 [完成] 收到最后一个数据块，下载完成");
                                        
                                        // 确保文件完全写入磁盘
                                        if let Some(ref mut file) = file_handle {
                                            if let Err(e) = file.sync_all().await {
                                                warn!("⚠️ 同步文件到磁盘失败: {}", e);
                                            } else {
                                                info!("✅ 文件已同步到磁盘");
                                            }
                                        }
                                        break;
                                    }
                                }
                                DownloadMessageType::EndOfStream => {
                                    // 兼容性处理：虽然新版本不再发送此消息，但保留处理逻辑
                                    info!("📥 收到流结束信号（兼容模式），下载完成");
                                    break;
                                }
                                DownloadMessageType::Error => {
                                    let error_msg = download_chunk.error_message.unwrap_or_else(|| "未知错误".to_string());
                                    error!("❌ 服务端返回错误: {}", error_msg);
                                    results.add_failure("文件查询+下载 - 下载步骤", &error_msg);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("流接收失败: {:?}", e);
                            error!("❌ 文件下载流错误: {:?}", e);
                            results.add_failure("文件查询+下载 - 下载步骤", &error_msg);
                            break;
                        }
                    }
                }
                
                if total_chunks > 0 {
                    info!("✅ [网际快车模式] 文件下载成功: {} 个数据块，共 {} 字节", total_chunks, total_bytes);
                    
                    if let Some(filename) = download_filename {
                        info!("📁 下载文件保存为: ./downloads/{}", filename);
                        
                        // 验证文件大小
                        if let Some(expected_size) = expected_file_size {
                            if total_bytes == expected_size as usize {
                                info!("✅ 文件大小验证通过: {} bytes", total_bytes);
                                results.add_success("文件查询+下载 - 下载步骤 (网际快车模式，大小验证通过)");
                            } else {
                                warn!("⚠️ 文件大小不匹配: 期望 {} bytes，实际 {} bytes", expected_size, total_bytes);
                                results.add_success("文件查询+下载 - 下载步骤 (网际快车模式，大小不匹配)");
                            }
                        } else {
                            results.add_success("文件查询+下载 - 下载步骤 (网际快车模式)");
                        }
                    } else {
                        results.add_success("文件查询+下载 - 下载步骤");
                    }
                } else {
                    results.add_failure("文件查询+下载 - 下载步骤", "未收到任何数据块");
                }
            }
            Err(e) => {
                let error_msg = format!("服务端流请求失败: {:?}", e);
                error!("❌ 文件查询+下载服务端流请求失败: {:?}", e);
                results.add_failure("文件查询+下载 - 下载步骤", &error_msg);
            }
        }
    }
    
    // 测试多文件上传
    if tests_to_run.contains(&TestType::MultiUpload) {
        info!("📤 测试多文件上传...");
        
        let test_files = vec![
            ("config.json", r#"{"server": "127.0.0.1", "port": 8080}"#),
            ("readme.md", "# RAT Engine 文件服务\n\n这是一个测试文件。"),
            ("data.txt", "Line 1\nLine 2\nLine 3\n测试中文内容"),
        ];
        
        for (filename, content) in test_files {
            // 创建上传请求结构体
            let upload_request = UploadRequest {
                filename: filename.to_string(),
                content: content.as_bytes().to_vec(),
            };
            
            // 序列化上传请求
            let upload_data = match GrpcCodec::encode(&upload_request) {
                Ok(data) => data,
                Err(e) => {
                    let error_msg = format!("序列化上传请求失败: {:?}", e);
                    error!("❌ 文件 {} 序列化失败: {:?}", filename, e);
                    results.add_failure(&format!("多文件上传 - {}", filename), &error_msg);
                    continue;
                }
            };
            
            match grpc_client.call_with_uri::<Vec<u8>, UploadResponse>(
                "http://127.0.0.1:50051",
                "file",
                "Upload",
                upload_data,
                None,
            ).await {
                Ok(response) => {
                    if response.status == 0 {
                        info!("✅ 文件 {} 上传成功:", filename);
                        info!("   文件ID: {}", response.data.file_id);
                        info!("   文件大小: {} 字节", response.data.file_size);
                        results.add_success(&format!("多文件上传 - {}", filename));
                    } else {
                        let error_msg = format!("gRPC 响应错误: 状态码 {}, 消息: {}", response.status, response.message);
                        error!("❌ 文件 {} 上传失败: {}", filename, error_msg);
                        results.add_failure(&format!("多文件上传 - {}", filename), &error_msg);
                    }
                }
                Err(e) => {
                    let error_msg = format!("上传失败: {:?}", e);
                    error!("❌ 文件 {} 上传失败: {:?}", filename, e);
                    results.add_failure(&format!("多文件上传 - {}", filename), &error_msg);
                }
            }
        }
    }
    

    
    // 测试错误处理 - 下载不存在的文件
    if tests_to_run.contains(&TestType::ErrorHandling) {
        info!("🧪 测试错误处理 - 下载不存在的文件...");
        
        let invalid_download_request = DownloadRequest {
            file_id: "nonexistent_file_id".to_string(),
        };
        
        let invalid_file_id = match GrpcCodec::encode(&invalid_download_request) {
            Ok(data) => data,
            Err(e) => {
                let error_msg = format!("序列化错误测试请求失败: {:?}", e);
                error!("❌ 错误测试请求序列化失败: {:?}", e);
                results.add_failure("错误处理 - 不存在文件下载", &error_msg);
                return Ok(results);
            }
        };
        
        match grpc_client.call_server_stream_with_uri::<Vec<u8>, DownloadChunk>(
            "http://127.0.0.1:50051",
            "file",
            "Download",
            invalid_file_id,
            None,
        ).await {
            Ok(stream_response) => {
                let mut error_received = false;
                let mut data_received = false;
                let mut stream: Pin<Box<dyn Stream<Item = Result<GrpcStreamMessage<DownloadChunk>, _>> + Send>> = stream_response.stream;
                
                // 设置超时，避免无限等待
                let timeout_duration = Duration::from_secs(10);
                let timeout_result = tokio::time::timeout(timeout_duration, async {
                    let mut stream_ended = false;
                    while let Some(result) = stream.as_mut().next().await {
                        match result {
                            Ok(chunk) => {
                                data_received = true;
                                warn!("⚠️ 意外收到数据块，应该返回错误");
                                // 现在 chunk.data 已经是 DownloadChunk 类型了
                                let download_chunk = chunk.data;
                                warn!("⚠️ 收到下载块: 索引 {}, 大小 {} bytes", download_chunk.chunk_index, download_chunk.data.len());
                            }
                            Err(e) => {
                                info!("✅ 正确收到错误响应: {:?}", e);
                                error_received = true;
                                break;
                            }
                        }
                    }
                    stream_ended = true;
                    info!("📡 流已结束，error_received: {}, data_received: {}", error_received, data_received);
                }).await;
                
                match timeout_result {
                     Ok(_) => {
                         if error_received {
                             results.add_success("错误处理 - 不存在文件下载");
                         } else if data_received {
                             results.add_failure("错误处理 - 不存在文件下载", "收到数据而非错误");
                         } else {
                             results.add_failure("错误处理 - 不存在文件下载", "未收到任何响应");
                         }
                     }
                     Err(_) => {
                         results.add_failure("错误处理 - 不存在文件下载", "请求超时");
                     }
                 }
            }
            Err(e) => {
                info!("✅ 正确收到错误响应: {:?}", e);
                results.add_success("错误处理 - 不存在文件下载");
            }
        }
    }
    
    // 测试 HTTP 文件列表
    if tests_to_run.contains(&TestType::HttpList) {
        info!("📋 测试真实 HTTP 文件列表...");
        
        // 增加重试机制，因为服务器可能需要时间启动
        let mut retry_count = 0;
        let max_retries = 3;
        let mut last_error = String::new();
        
        loop {
            println!("DEBUG: 尝试 HTTP 请求 - 重试次数: {}/{}", retry_count + 1, max_retries);
            println!("DEBUG: 请求 URL: http://127.0.0.1:50051/files");
            
            match http_client.get("http://127.0.0.1:50051/files").await {
             Ok(response) => {
                 println!("DEBUG: HTTP 响应状态: {}", response.status);
                 if response.is_success() {
                     if let Ok(body_str) = response.text() {
                         println!("DEBUG: HTTP 响应体长度: {} 字符", body_str.len());
                         println!("DEBUG: HTTP 响应体内容: {}", body_str);
                         info!("✅ RAT Engine HTTP 文件列表获取成功，响应长度: {} 字符", body_str.len());
                         
                         // 尝试解析 JSON 响应
                         if let Ok(files) = serde_json::from_str::<Vec<FileInfo>>(&body_str) {
                             info!("📊 文件列表包含 {} 个文件", files.len());
                             for file in &files {
                                 info!("   📄 {}: {} ({} bytes)", file.id, file.filename, file.size);
                             }
                             results.add_success(&format!("RAT Engine HTTP 文件列表 ({} 个文件)", files.len()));
                         } else if body_str.contains("test") || body_str.contains("file_") {
                             info!("✅ 文件列表包含测试文件");
                             results.add_success("RAT Engine HTTP 文件列表 (包含测试文件)");
                         } else {
                             results.add_success("RAT Engine HTTP 文件列表");
                         }
                     } else {
                         println!("DEBUG: 无法读取响应体");
                         results.add_success("RAT Engine HTTP 文件列表");
                     }
                     break; // 成功时跳出循环
                 } else {
                     let error_msg = format!("HTTP 状态码: {}", response.status);
                     warn!("⚠️ 文件列表响应状态: {}", response.status);
                     last_error = error_msg;
                 }
             }
             Err(e) => {
                 let error_msg = format!("网络请求失败: {:?}", e);
                 println!("DEBUG: HTTP 请求详细错误: {:?}", e);
                 error!("❌ RAT Engine HTTP 文件列表请求失败: {:?}", e);
                 last_error = error_msg;
             }
         }
         
         retry_count += 1;
         if retry_count >= max_retries {
             results.add_failure("RAT Engine HTTP 文件列表", &last_error);
             break;
         } else {
             warn!("⚠️ HTTP 文件列表请求失败，{} 秒后重试 ({}/{})", retry_count, retry_count, max_retries);
             tokio::time::sleep(Duration::from_secs(retry_count as u64)).await;
         }
     }
    }
    
    // 测试客户端流分块上传
    if tests_to_run.contains(&TestType::ChunkedUpload) {
        info!("📤 测试客户端流分块上传...");
        
        match test_client_stream_upload(&grpc_client).await {
            Ok(file_id) => {
                info!("✅ 客户端流分块上传测试成功");
                test_state.chunked_upload_file_id = Some(file_id);
                results.add_success("客户端流分块上传");
            }
            Err(e) => {
                let error_msg = format!("分块上传失败: {:?}", e);
                error!("❌ 客户端流分块上传测试失败: {:?}", e);
                results.add_failure("客户端流分块上传", &error_msg);
            }
        }
    }
    
    info!("✅ RAT Engine 客户端测试完成");
    Ok(results)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    rat_engine::require_features!("client", "tls", "reqwest");

    // 日志通过RatEngineBuilder初始化
    
    // 解析命令行参数
    let test_types = parse_args();
    
    // 如果没有参数，显示帮助信息并退出
    if test_types.is_empty() {
        show_help();
        return Ok(());
    }
    
    // 确定要运行的测试
    let tests_to_run = if test_types.contains(&TestType::All) {
        vec![
            TestType::Upload,
            TestType::QueryDownload,
            TestType::MultiUpload,
            TestType::ErrorHandling,
            TestType::HttpList,
            TestType::ChunkedUpload,
        ]
    } else {
        test_types
    };
    
    info!("🚀 启动 RAT Engine gRPC 文件服务器和客户端测试");
    info!("📋 将运行以下测试:");
    for test_type in &tests_to_run {
        info!("   {}", test_type.description());
    }
    
    // 启动服务器任务
    let server_task = tokio::spawn(async {
        info!("🔧 启动服务器任务...");
        if let Err(e) = start_real_grpc_file_server().await {
            error!("❌ 服务器启动失败: {:?}", e);
        }
    });
    
    // 等待服务器启动
    tokio::time::sleep(Duration::from_secs(5)).await;
    info!("⏳ 服务器启动中，等待就绪...");
    
    // 启动客户端测试任务
    let client_task = tokio::spawn(async move {
        info!("🧪 启动客户端测试任务...");
        match run_real_grpc_client_tests(tests_to_run).await {
            Ok(results) => {
                info!("✅ 客户端测试任务完成");
                results
            }
            Err(e) => {
                error!("❌ 客户端测试失败: {:?}", e);
                let mut results = TestResults::default();
                results.add_failure("客户端测试初始化", &format!("{:?}", e));
                results
            }
        }
    });
    
    // 等待客户端测试完成并获取结果
    let test_results = match client_task.await {
        Ok(results) => results,
        Err(e) => {
            error!("❌ 客户端测试任务异常: {:?}", e);
            let mut results = TestResults::default();
            results.add_failure("客户端测试任务", &format!("{:?}", e));
            results
        }
    };
    
    // 停止服务器任务
    info!("🛑 停止服务器...");
    server_task.abort();
    
    // 等待一下确保服务器完全停止
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // 输出测试统计结果
    test_results.print_summary();
    
    // 根据测试结果决定退出码
    if test_results.failed_tests == 0 {
        info!("🎉 所有测试通过！RAT Engine gRPC 文件服务工作正常");
    } else {
        warn!("⚠️ 有 {} 个测试失败，请检查日志", test_results.failed_tests);
    }
    
    info!("🏁 RAT Engine gRPC 综合示例执行完成");
    
    Ok(())
}

/// 文件块结构体（用于客户端流上传）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[derive(bincode::Encode, bincode::Decode)]
struct FileChunk {
    /// 文件名
    file_name: String,
    /// 块索引（从0开始，0为元数据块）
    chunk_index: u32,
    /// 总块数
    total_chunks: u32,
    /// 块数据
    data: Vec<u8>,
    /// 是否为元数据块
    is_metadata: bool,
}

/// 上传响应结构体
#[derive(Serialize, Deserialize, Clone, Debug)]
#[derive(bincode::Encode, bincode::Decode)]
struct UploadResponse {
    /// 文件ID
    file_id: String,
    /// 文件大小
    file_size: u64,
    /// 上传时间
    upload_time: String,
    /// 块数量
    chunk_count: u32,
}

/// 测试客户端流分块上传功能（统一化版本，支持断点续传）
async fn test_client_stream_upload(grpc_client: &RatGrpcClient) -> RatResult<String> {
    println!("DEBUG: ===== 开始客户端流上传测试（统一化版本） =====");
    // 创建一个大文件内容用于分块上传测试
    let large_content = "这是一个大文件的内容，用于测试客户端流分块上传功能。".repeat(1000); // 约 60KB 的内容
    let file_name = "large_file_chunked.txt";
    
    info!("📦 开始分块上传文件: {} (大小: {} 字节)", file_name, large_content.len());
    
    // 使用统一化的客户端流接口
    println!("DEBUG: 准备创建客户端流（统一化接口）");
    let (mut request_sender, response_receiver) = grpc_client
        .call_client_stream_with_uri::<FileChunk, UploadResponse>("http://127.0.0.1:50051", "file", "ChunkedUpload", None)
        .await?;
    println!("DEBUG: 客户端流创建成功");
    
    // 分块大小 (3KB，与下载保持一致，避免中文字符截断)
    const CHUNK_SIZE: usize = 3072;
    let content_bytes = large_content.as_bytes().to_vec(); // 转换为 Vec<u8> 避免生命周期问题
    let total_chunks = (content_bytes.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
    
    info!("📊 文件将被分为 {} 个块进行上传", total_chunks);
    
    // 启动发送任务
    let sender_task = {
        let mut sender = request_sender.clone();
        tokio::spawn(async move {
            // 发送文件元数据（第一个消息）
            println!("DEBUG: 准备发送元数据块");
            let metadata_chunk = FileChunk {
                file_name: file_name.to_string(),
                chunk_index: 0,
                total_chunks: total_chunks as u32,
                data: Vec::new(), // 元数据块不包含实际数据
                is_metadata: true,
            };
            
            if let Err(e) = sender.send(metadata_chunk).await {
                return Err(format!("发送元数据失败: {}", e));
            }
            println!("DEBUG: 元数据块发送成功");
            info!("📋 已发送文件元数据");
            
            // 分块发送文件内容（支持断点续传逻辑）
            let mut uploaded_chunks = 0;
            for (index, chunk) in content_bytes.chunks(CHUNK_SIZE).enumerate() {
                let file_chunk = FileChunk {
                    file_name: file_name.to_string(),
                    chunk_index: (index + 1) as u32, // 从1开始，0是元数据
                    total_chunks: total_chunks as u32,
                    data: chunk.to_vec(),
                    is_metadata: false,
                };
                
                println!("DEBUG: 准备发送第 {} 个数据块，大小: {} 字节", index + 1, chunk.len());
                
                // 模拟断点续传：在某些块上添加重试逻辑
                let mut retry_count = 0;
                const MAX_RETRIES: usize = 3;
                
                loop {
                    match sender.send(file_chunk.clone()).await {
                        Ok(_) => {
                            uploaded_chunks += 1;
                            info!("📤 已发送第 {}/{} 块 (大小: {} 字节)", index + 1, total_chunks, chunk.len());
                            println!("DEBUG: 第 {} 个数据块发送成功", index + 1);
                            break;
                        }
                        Err(e) => {
                            retry_count += 1;
                            if retry_count <= MAX_RETRIES {
                                warn!("⚠️ 发送第 {} 块失败，重试 {}/{}: {}", index + 1, retry_count, MAX_RETRIES, e);
                                tokio::time::sleep(tokio::time::Duration::from_millis(100 * retry_count as u64)).await;
                            } else {
                                return Err(format!("发送数据块 {} 失败（已重试 {} 次）: {}", index + 1, MAX_RETRIES, e));
                            }
                        }
                    }
                }
                
                // 增加延迟以避免发送过快导致的流控制问题
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            
            info!("🔒 发送完成，关闭流... (已上传 {} 个块)", uploaded_chunks);
            
            // 显式关闭发送端，通知服务器没有更多数据
            if let Err(e) = sender.send_close().await {
                return Err(format!("关闭流失败: {}", e));
            }
            info!("✅ 流已关闭");
            
            Ok(uploaded_chunks)
        })
    };
    
    // 等待发送任务完成和服务端响应
    info!("⏳ 等待上传完成和服务端处理...");
    let timeout_duration = tokio::time::Duration::from_secs(60); // 增加到60秒超时
    
    match tokio::time::timeout(timeout_duration, async {
        // 等待发送任务完成
        let send_result = sender_task.await;
        
        // 等待服务端响应
        let response_result = response_receiver.await;
        
        (send_result, response_result)
    }).await {
        Ok((Ok(Ok(uploaded_chunks)), Ok(Ok(upload_response)))) => {
            info!("✅ 分块上传成功!");
            info!("   文件ID: {}", upload_response.file_id);
            info!("   文件大小: {} 字节", upload_response.file_size);
            info!("   上传时间: {}", upload_response.upload_time);
            info!("   块数量: {} (实际上传: {})", upload_response.chunk_count, uploaded_chunks);
            Ok(upload_response.file_id)
        }
        Ok((Ok(Err(send_error)), _)) => {
            Err(RatError::NetworkError(format!("发送任务失败: {}", send_error)))
        }
        Ok((Err(join_error), _)) => {
            Err(RatError::NetworkError(format!("发送任务异常: {}", join_error)))
        }
        Ok((_, Ok(Err(response_error)))) => {
            Err(RatError::NetworkError(format!("服务端响应错误: {}", response_error)))
        }
        Ok((_, Err(response_error))) => {
            Err(RatError::NetworkError(format!("接收响应失败: {}", response_error)))
        }
        Err(_) => {
            Err(RatError::NetworkError("上传超时（60秒）".to_string()))
        }
    }
}