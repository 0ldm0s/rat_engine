//! 下载元数据管理器
//! 
//! 支持断点续传功能，记录下载进度和已接收的数据块信息

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 下载元数据结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadata {
    /// 文件ID
    pub file_id: String,
    /// 文件名
    pub filename: String,
    /// 文件总大小
    pub total_size: u64,
    /// 总块数
    pub total_chunks: u32,
    /// 块大小
    pub chunk_size: usize,
    /// 已接收的块索引集合
    pub received_chunks: HashMap<u32, ChunkInfo>,
    /// 下载开始时间
    pub start_time: u64,
    /// 最后更新时间
    pub last_update_time: u64,
    /// 下载状态
    pub status: DownloadStatus,
    /// 下载文件路径
    pub download_path: PathBuf,
    /// 元数据文件路径
    pub metadata_path: PathBuf,
}

/// 数据块信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// 块索引
    pub chunk_index: u32,
    /// 块偏移位置
    pub offset: u64,
    /// 块大小
    pub size: usize,
    /// 接收时间
    pub received_time: u64,
    /// 是否已验证
    pub verified: bool,
}

/// 下载状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    /// 初始化中
    Initializing,
    /// 下载中
    Downloading,
    /// 已暂停
    Paused,
    /// 已完成
    Completed,
    /// 出错
    Error(String),
}

/// 下载元数据管理器
#[derive(Debug)]
pub struct DownloadMetadataManager {
    /// 元数据存储目录
    metadata_dir: PathBuf,
    /// 下载目录
    download_dir: PathBuf,
}

impl DownloadMetadataManager {
    /// 创建新的下载元数据管理器
    pub fn new<P: AsRef<Path>>(metadata_dir: P, download_dir: P) -> Self {
        Self {
            metadata_dir: metadata_dir.as_ref().to_path_buf(),
            download_dir: download_dir.as_ref().to_path_buf(),
        }
    }

    /// 初始化目录
    pub async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(&self.metadata_dir).await?;
        fs::create_dir_all(&self.download_dir).await?;
        Ok(())
    }

    /// 创建新的下载任务
    pub async fn create_download(
        &self,
        file_id: &str,
        filename: &str,
        total_size: u64,
        total_chunks: u32,
        chunk_size: usize,
    ) -> Result<DownloadMetadata, Box<dyn std::error::Error + Send + Sync>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        
        let download_path = self.download_dir.join(filename);
        let metadata_path = self.metadata_dir.join(format!("{}.metadata", file_id));

        let metadata = DownloadMetadata {
            file_id: file_id.to_string(),
            filename: filename.to_string(),
            total_size,
            total_chunks,
            chunk_size,
            received_chunks: HashMap::new(),
            start_time: now,
            last_update_time: now,
            status: DownloadStatus::Initializing,
            download_path,
            metadata_path: metadata_path.clone(),
        };

        // 保存元数据到文件
        self.save_metadata(&metadata).await?;

        // 预分配下载文件空间（网际快车模式）
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&metadata.download_path).await 
        {
            if let Err(e) = file.set_len(total_size).await {
                eprintln!("⚠️ 预分配文件空间失败: {}, 继续下载", e);
            } else {
                println!("✅ 已预分配文件空间: {} bytes -> {:?}", total_size, metadata.download_path);
            }
        }

        Ok(metadata)
    }

    /// 加载现有的下载任务
    pub async fn load_download(&self, file_id: &str) -> Result<Option<DownloadMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        let metadata_path = self.metadata_dir.join(format!("{}.metadata", file_id));
        
        if !metadata_path.exists() {
            return Ok(None);
        }

        let mut file = fs::File::open(&metadata_path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        
        let metadata: DownloadMetadata = serde_json::from_str(&contents)?;
        Ok(Some(metadata))
    }

    /// 记录接收到的数据块
    pub async fn record_chunk(
        &self,
        metadata: &mut DownloadMetadata,
        chunk_index: u32,
        offset: u64,
        size: usize,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        
        let chunk_info = ChunkInfo {
            chunk_index,
            offset,
            size,
            received_time: now,
            verified: true, // 简化版本，假设接收到的块都是正确的
        };

        metadata.received_chunks.insert(chunk_index, chunk_info);
        metadata.last_update_time = now;
        metadata.status = DownloadStatus::Downloading;

        // 检查是否下载完成
        if metadata.received_chunks.len() == metadata.total_chunks as usize {
            metadata.status = DownloadStatus::Completed;
            println!("🎉 文件下载完成: {}", metadata.filename);
        }

        // 保存更新后的元数据
        self.save_metadata(metadata).await?;

        Ok(())
    }

    /// 获取缺失的数据块列表
    pub fn get_missing_chunks(&self, metadata: &DownloadMetadata) -> Vec<u32> {
        let mut missing_chunks = Vec::new();
        
        for chunk_index in 0..metadata.total_chunks {
            if !metadata.received_chunks.contains_key(&chunk_index) {
                missing_chunks.push(chunk_index);
            }
        }

        missing_chunks
    }

    /// 计算下载进度
    pub fn calculate_progress(&self, metadata: &DownloadMetadata) -> f64 {
        if metadata.total_chunks == 0 {
            return 0.0;
        }
        
        let received_count = metadata.received_chunks.len() as f64;
        let total_count = metadata.total_chunks as f64;
        
        (received_count / total_count) * 100.0
    }

    /// 获取已下载的字节数
    pub fn get_downloaded_bytes(&self, metadata: &DownloadMetadata) -> u64 {
        metadata.received_chunks.values()
            .map(|chunk| chunk.size as u64)
            .sum()
    }

    /// 保存元数据到文件
    async fn save_metadata(&self, metadata: &DownloadMetadata) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json_data = serde_json::to_string_pretty(metadata)?;
        let mut file = fs::File::create(&metadata.metadata_path).await?;
        file.write_all(json_data.as_bytes()).await?;
        file.flush().await?;
        Ok(())
    }

    /// 删除下载任务（包括文件和元数据）
    pub async fn delete_download(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let metadata_path = self.metadata_dir.join(format!("{}.metadata", file_id));
        
        // 先加载元数据获取下载文件路径
        if let Ok(Some(metadata)) = self.load_download(file_id).await {
            // 删除下载文件
            if metadata.download_path.exists() {
                fs::remove_file(&metadata.download_path).await?;
                println!("🗑️ 已删除下载文件: {:?}", metadata.download_path);
            }
        }

        // 删除元数据文件
        if metadata_path.exists() {
            fs::remove_file(&metadata_path).await?;
            println!("🗑️ 已删除元数据文件: {:?}", metadata_path);
        }

        Ok(())
    }

    /// 列出所有下载任务
    pub async fn list_downloads(&self) -> Result<Vec<DownloadMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        let mut downloads = Vec::new();
        let mut dir = fs::read_dir(&self.metadata_dir).await?;

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("metadata") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(Some(metadata)) = self.load_download(file_stem).await {
                        downloads.push(metadata);
                    }
                }
            }
        }

        Ok(downloads)
    }

    /// 暂停下载
    pub async fn pause_download(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Ok(Some(mut metadata)) = self.load_download(file_id).await {
            metadata.status = DownloadStatus::Paused;
            metadata.last_update_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            self.save_metadata(&metadata).await?;
            println!("⏸️ 下载已暂停: {}", metadata.filename);
        }
        Ok(())
    }

    /// 恢复下载
    pub async fn resume_download(&self, file_id: &str) -> Result<Option<DownloadMetadata>, Box<dyn std::error::Error + Send + Sync>> {
        if let Ok(Some(mut metadata)) = self.load_download(file_id).await {
            if metadata.status == DownloadStatus::Paused {
                metadata.status = DownloadStatus::Downloading;
                metadata.last_update_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                self.save_metadata(&metadata).await?;
                println!("▶️ 下载已恢复: {}", metadata.filename);
                return Ok(Some(metadata));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[cfg(feature = "http-client")]
    #[tokio::test]
    async fn test_download_metadata_manager() {

        let temp_dir = TempDir::new().unwrap();
        let metadata_dir = temp_dir.path().join("metadata");
        let download_dir = temp_dir.path().join("downloads");

        let manager = DownloadMetadataManager::new(&metadata_dir, &download_dir);
        manager.initialize().await.unwrap();

        // 创建下载任务
        let mut metadata = manager.create_download(
            "test_file_123",
            "test.txt",
            1024,
            4,
            256,
        ).await.unwrap();

        assert_eq!(metadata.file_id, "test_file_123");
        assert_eq!(metadata.filename, "test.txt");
        assert_eq!(metadata.total_size, 1024);
        assert_eq!(metadata.total_chunks, 4);

        // 记录数据块
        manager.record_chunk(&mut metadata, 0, 0, 256).await.unwrap();
        manager.record_chunk(&mut metadata, 1, 256, 256).await.unwrap();

        // 检查进度
        let progress = manager.calculate_progress(&metadata);
        assert_eq!(progress, 50.0); // 2/4 = 50%

        // 检查缺失的块
        let missing = manager.get_missing_chunks(&metadata);
        assert_eq!(missing, vec![2, 3]);

        // 加载下载任务
        let loaded = manager.load_download("test_file_123").await.unwrap().unwrap();
        assert_eq!(loaded.received_chunks.len(), 2);
    }
}