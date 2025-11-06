//! RAT Engine 大文件分块上传示例
//!
//! 特性：
//! - 大文件分块上传（支持GB级文件）
//! - Base64编码传输
//! - SSE实时进度推送
//! - 文件完整性验证（SHA-256）
//! - 现代化Web UI
//! - 断点续传支持
//! - 连接池管理

use rat_engine::{RatEngine, Method, Response, StatusCode};
use rat_engine::server::{
    Router,
    http_request::HttpRequest,
    global_sse_manager::get_global_sse_manager,
};
use http_body_util::{Full, StreamBody};
use hyper::body::Bytes;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use std::pin::Pin;
use tokio::sync::RwLock;
use tokio::fs as async_fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use futures_util::stream::{self, Stream};

// 配置常量
const SERVER_HOST: &str = "127.0.0.1";
const SERVER_PORT: u16 = 8088;
const UPLOAD_DIR: &str = "uploads";
const CHUNK_SIZE: usize = 64 * 1024; // 64KB 分块（适合Base64传输）
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2GB 最大文件大小

// 上传会话状态
#[derive(Debug, Clone)]
struct UploadSession {
    id: String,
    filename: String,
    file_size: u64,
    file_hash: Option<String>,
    total_chunks: u32,
    received_chunks: HashMap<u32, usize>,
    temp_file_path: String,
    created_at: SystemTime,
    completed: bool,
    progress: f64,
}

impl UploadSession {
    fn new(id: String, filename: String, file_size: u64, file_hash: Option<String>) -> Self {
        let total_chunks = (file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64;
        let temp_file_path = format!("{}/{}.tmp", UPLOAD_DIR, id);
        Self {
            id: id.clone(),
            filename,
            file_size,
            file_hash,
            total_chunks: total_chunks as u32,
            received_chunks: HashMap::new(),
            temp_file_path,
            created_at: SystemTime::now(),
            completed: false,
            progress: 0.0,
        }
    }

    fn update_progress(&mut self) {
        self.progress = (self.received_chunks.len() as f64 / self.total_chunks as f64) * 100.0;
    }
}

// 全局状态
struct AppState {
    sessions: RwLock<HashMap<String, UploadSession>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

// 请求数据结构
#[derive(Deserialize)]
struct InitRequest {
    filename: String,
    file_size: u64,
    file_hash: Option<String>,
}

#[derive(Deserialize)]
struct ChunkRequest {
    session_id: String,
    chunk_index: u32,
    chunk_data: String, // Base64编码
}

// 响应数据结构
#[derive(Serialize)]
struct InitResponse {
    session_id: String,
    chunk_size: usize,
    total_chunks: u32,
}

#[derive(Serialize)]
struct ChunkResponse {
    success: bool,
    progress: f64,
    completed: bool,
}

#[derive(Serialize)]
struct StatusResponse {
    session_id: String,
    filename: String,
    file_size: u64,
    progress: f64,
    received_chunks: usize,
    total_chunks: u32,
    completed: bool,
    created_at: u64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
enum ProgressMessage {
    Init {
        session_id: String,
        filename: String,
        progress: f64,
        completed: bool,
    },
    Progress {
        progress: f64,
        chunk_index: u32,
        received_chunks: usize,
        total_chunks: u32,
    },
    Completed {
        session_id: String,
        filename: String,
        file_size: u64,
        download_url: String,
        progress: f64,
    },
    Heartbeat {
        timestamp: u64,
    },
    Error {
        message: String,
    },
}

// HTML模板
const UPLOAD_PAGE_TEMPLATE: &str = include_str!("chunked_upload_template.html");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 创建上传目录
    async_fs::create_dir_all(UPLOAD_DIR).await?;
    println!("📁 上传目录: {}/", std::path::Path::new(UPLOAD_DIR).display());

    // 创建应用状态
    let state = Arc::new(AppState::new());

    // 创建并启动引擎
    let engine = RatEngine::builder()
        .worker_threads(4)
        .with_router(|mut router| {
            // 主页 - 文件上传界面
            router.add_route(Method::GET, "/", move |_req| {
                Box::pin(async {
                    Ok(render_upload_page())
                })
            });

            // 初始化上传
            let state_clone = state.clone();
            router.add_route(Method::POST, "/api/init", move |req| {
                let state = state_clone.clone();
                Box::pin(async move {
                    Ok(handle_init_upload(req, state).await)
                })
            });

            // 上传分块
            let state_clone = state.clone();
            router.add_route(Method::POST, "/api/chunk", move |req| {
                let state = state_clone.clone();
                Box::pin(async move {
                    Ok(handle_upload_chunk(req, state).await)
                })
            });

            // SSE进度推送
            let state_clone = state.clone();
            router.add_streaming_route(Method::GET, "/api/progress/<session_id>", move |req, params| {
                let state = state_clone.clone();
                Box::pin(async move {
                    handle_progress_stream(req, params, state).await
                })
            });

            // 获取上传状态
            let state_clone = state.clone();
            router.add_route(Method::GET, "/api/status/<session_id>", move |req| {
                let state = state_clone.clone();
                Box::pin(async move {
                    Ok(handle_get_status(req, state).await)
                })
            });

            // 下载文件
            let state_clone = state.clone();
            router.add_route(Method::GET, "/api/download/<session_id>", move |req| {
                let state = state_clone.clone();
                Box::pin(async move {
                    Ok(handle_download_file(req, state).await)
                })
            });

            router
        })
        .build()?;

    println!("🚀 RAT Engine 大文件分块上传服务器启动成功！");
    println!("🌐 访问地址: http://{}:{}/", SERVER_HOST, SERVER_PORT);
    println!("📋 功能特性:");
    println!("   • 大文件分块上传（最大2GB）");
    println!("   • 实时进度推送（SSE）");
    println!("   • 文件完整性验证");
    println!("   • 断点续传支持");
    println!("   • 现代化Web界面");

    engine.start(SERVER_HOST.to_string(), SERVER_PORT).await?;
    Ok(())
}

fn render_upload_page() -> Response<Full<Bytes>> {
    let html = UPLOAD_PAGE_TEMPLATE
        .replace("{{CHUNK_SIZE}}", &CHUNK_SIZE.to_string())
        .replace("{{MAX_FILE_SIZE}}", &MAX_FILE_SIZE.to_string())
        .replace("{{MAX_FILE_SIZE_MB}}", &(MAX_FILE_SIZE / 1024 / 1024).to_string())
        .replace("{{SERVER_HOST}}", SERVER_HOST)
        .replace("{{SERVER_PORT}}", &SERVER_PORT.to_string());

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(html)))
        .unwrap()
}

async fn handle_init_upload(
    req: rat_engine::server::http_request::HttpRequest,
    state: Arc<AppState>,
) -> Response<Full<Bytes>> {
    // 读取请求体
    let body_str = match String::from_utf8(req.body.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            return json_response(&ErrorResponse {
                error: "无效的请求数据".to_string(),
            }, StatusCode::BAD_REQUEST);
        }
    };

    // 解析JSON
    let init_req: InitRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(_) => {
            return json_response(&ErrorResponse {
                error: "JSON解析失败".to_string(),
            }, StatusCode::BAD_REQUEST);
        }
    };

    // 验证文件信息
    if init_req.filename.is_empty() || init_req.file_size == 0 {
        return json_response(&ErrorResponse {
            error: "无效的文件信息".to_string(),
        }, StatusCode::BAD_REQUEST);
    }

    if init_req.file_size > MAX_FILE_SIZE {
        return json_response(&ErrorResponse {
            error: format!("文件过大，最大支持{}GB", MAX_FILE_SIZE / 1024 / 1024 / 1024),
        }, StatusCode::PAYLOAD_TOO_LARGE);
    }

    // 生成会话ID
    let session_id = Uuid::new_v4().to_string().replace('-', "")[..16].to_string();

    // 创建上传会话
    let session = UploadSession::new(
        session_id.clone(),
        init_req.filename.clone(),
        init_req.file_size,
        init_req.file_hash.clone(),
    );

    // 创建临时文件
    if let Err(_) = async_fs::File::create(&session.temp_file_path).await {
        return json_response(&ErrorResponse {
            error: "创建临时文件失败".to_string(),
        }, StatusCode::INTERNAL_SERVER_ERROR);
    }

  // 保存会话
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(session_id.clone(), session);
    }

    let response = InitResponse {
        session_id: session_id.clone(),
        chunk_size: CHUNK_SIZE,
        total_chunks: ((init_req.file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64) as u32,
    };

    println!("📋 初始化上传会话: {} ({})", session_id, init_req.filename);

    json_response(&response, StatusCode::OK)
}

async fn handle_upload_chunk(
    req: rat_engine::server::http_request::HttpRequest,
    state: Arc<AppState>,
) -> Response<Full<Bytes>> {
    // 读取请求体
    let body_str = match String::from_utf8(req.body.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            return json_response(&ErrorResponse {
                error: "无效的请求数据".to_string(),
            }, StatusCode::BAD_REQUEST);
        }
    };

    // 解析JSON
    let chunk_req: ChunkRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(_) => {
            return json_response(&ErrorResponse {
                error: "JSON解析失败".to_string(),
            }, StatusCode::BAD_REQUEST);
        }
    };

    // 获取会话
    let mut session = {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(&chunk_req.session_id) {
            session.clone()
        } else {
            return json_response(&ErrorResponse {
                error: "无效的会话ID".to_string(),
            }, StatusCode::NOT_FOUND);
        }
    };

    // 验证分块索引
    if chunk_req.chunk_index >= session.total_chunks {
        return json_response(&ErrorResponse {
            error: "无效的分块索引".to_string(),
        }, StatusCode::BAD_REQUEST);
    }

    // 解码Base64数据
    let chunk_data = match general_purpose::STANDARD.decode(&chunk_req.chunk_data) {
        Ok(data) => data,
        Err(_) => {
            return json_response(&ErrorResponse {
                error: "Base64解码失败".to_string(),
            }, StatusCode::BAD_REQUEST);
        }
    };

    // 写入分块数据
    if let Err(e) = write_chunk_to_file(&session.temp_file_path, chunk_req.chunk_index, &chunk_data).await {
        return json_response(&ErrorResponse {
            error: "写入分块失败".to_string(),
        }, StatusCode::INTERNAL_SERVER_ERROR);
    }

    // 更新会话状态
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&chunk_req.session_id) {
            session.received_chunks.insert(chunk_req.chunk_index, chunk_data.len());
            session.update_progress();

            // 广播进度
            broadcast_progress(
                &chunk_req.session_id,
                ProgressMessage::Progress {
                    progress: session.progress,
                    chunk_index: chunk_req.chunk_index,
                    received_chunks: session.received_chunks.len(),
                    total_chunks: session.total_chunks,
                },
            );

            // 检查是否完成
            println!("🔍 检查完成状态: {}/{}",
                session.received_chunks.len(),
                session.total_chunks
            );
            if session.received_chunks.len() == session.total_chunks as usize {
                println!("🚀 上传完成，触发完成流程");
                complete_upload(&chunk_req.session_id, session, &state).await;
            } else {
                println!("📤 上传进行中，等待更多分块");
            }

            let response = ChunkResponse {
                success: true,
                progress: session.progress,
                completed: session.completed,
            };

            return json_response(&response, StatusCode::OK);
        }
    }

    json_response(&ErrorResponse {
        error: "会话状态更新失败".to_string(),
    }, StatusCode::INTERNAL_SERVER_ERROR)
}

async fn handle_progress_stream(
    req: HttpRequest,
    params: HashMap<String, String>,
    state: Arc<AppState>,
) -> Result<Response<StreamBody<Pin<Box<dyn Stream<Item = Result<rat_engine::Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>> + Send + Sync>>>>, rat_engine::Error> {
    // 从路径参数获取session_id
    let session_id = match params.get("session_id") {
        Some(id) => id.clone(),
        None => {
            // 创建临时错误连接
            let temp_uuid = format!("error-{}", Uuid::new_v4().to_string()[..8].to_string());
            let sse_manager = get_global_sse_manager();
            let response = sse_manager.register_connection(temp_uuid.clone())?;

            let error_message = serde_json::json!({
                "type": "error",
                "message": "缺少会话ID"
            });
            let _ = sse_manager.send_data(&temp_uuid, &error_message.to_string());
            let _ = sse_manager.send_data(&temp_uuid, "DISCONNECT_EVENT");

            return Ok(response);
        }
    };

    // 验证会话存在
    let session = {
        let sessions = state.sessions.read().await;
        if let Some(session) = sessions.get(&session_id) {
            session.clone()
        } else {
            // 创建临时错误连接
            let temp_uuid = format!("error-{}", Uuid::new_v4().to_string()[..8].to_string());
            let sse_manager = get_global_sse_manager();
            let response = sse_manager.register_connection(temp_uuid.clone())?;

            let error_message = serde_json::json!({
                "type": "error",
                "message": "会话不存在"
            });
            let _ = sse_manager.send_data(&temp_uuid, &error_message.to_string());
            let _ = sse_manager.send_data(&temp_uuid, "DISCONNECT_EVENT");

            return Ok(response);
        }
    };

    // 注册SSE连接
    let sse_manager = get_global_sse_manager();
    let response = sse_manager.register_connection(session_id.clone())?;

    // 发送初始状态
    let init_message = serde_json::json!({
        "type": "init",
        "session_id": session_id,
        "filename": session.filename,
        "progress": session.progress,
        "completed": session.completed
    });
    let _ = sse_manager.send_data(&session_id, &init_message.to_string());

    // 如果已完成，发送完成消息
    if session.completed {
        let completed_message = serde_json::json!({
            "type": "completed",
            "session_id": session_id,
            "filename": session.filename,
            "file_size": session.file_size,
            "download_url": format!("/api/download/{}", session_id),
            "progress": 100.0
        });
        let _ = sse_manager.send_data(&session_id, &completed_message.to_string());
        let _ = sse_manager.send_data(&session_id, "DISCONNECT_EVENT");
    }

    Ok(response)
}

async fn handle_get_status(
    req: rat_engine::server::http_request::HttpRequest,
    state: Arc<AppState>,
) -> Response<Full<Bytes>> {
    // 提取session_id
    let session_id = req.param("session_id").unwrap_or("");

    let sessions = state.sessions.read().await;
    if let Some(session) = sessions.get(session_id) {
        let response = StatusResponse {
            session_id: session_id.to_string(),
            filename: session.filename.clone(),
            file_size: session.file_size,
            progress: session.progress,
            received_chunks: session.received_chunks.len(),
            total_chunks: session.total_chunks,
            completed: session.completed,
            created_at: session.created_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        json_response(&response, StatusCode::OK)
    } else {
        json_response(&ErrorResponse {
            error: "会话不存在".to_string(),
        }, StatusCode::NOT_FOUND)
    }
}

async fn handle_download_file(
    req: rat_engine::server::http_request::HttpRequest,
    state: Arc<AppState>,
) -> Response<Full<Bytes>> {
    // 提取session_id
    let session_id = req.param("session_id").unwrap_or("");

    let sessions = state.sessions.read().await;
    if let Some(session) = sessions.get(session_id) {
        if !session.completed {
            return json_response(&ErrorResponse {
                error: "文件未完成上传".to_string(),
            }, StatusCode::BAD_REQUEST);
        }

        let final_file_path = format!("{}/{}", UPLOAD_DIR, session.filename);
        if let Ok(content) = async_fs::read(&final_file_path).await {
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/octet-stream")
                .header("Content-Disposition", format!("attachment; filename=\"{}\"", session.filename))
                .header("Content-Length", content.len().to_string())
                .body(Full::new(Bytes::from(content)))
                .unwrap();
            response
        } else {
            json_response(&ErrorResponse {
                error: "文件不存在".to_string(),
            }, StatusCode::NOT_FOUND)
        }
    } else {
        json_response(&ErrorResponse {
            error: "会话不存在".to_string(),
        }, StatusCode::NOT_FOUND)
    }
}

// 辅助函数
async fn write_chunk_to_file(
    file_path: &str,
    chunk_index: u32,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncSeekExt;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(file_path)
        .await?;

    let offset = chunk_index as u64 * CHUNK_SIZE as u64;
    file.seek(tokio::io::SeekFrom::Start(offset)).await?;
    file.write_all(data).await?;
    file.flush().await?;

    Ok(())
}

fn broadcast_progress(
    session_id: &str,
    message: ProgressMessage,
) {
    let sse_manager = get_global_sse_manager();

    // 使用serde_json的json!宏来确保正确格式
    let msg = match message {
        ProgressMessage::Progress { progress, chunk_index, received_chunks, total_chunks } => {
            serde_json::json!({
                "type": "Progress",
                "progress": progress,
                "chunk_index": chunk_index,
                "received_chunks": received_chunks,
                "total_chunks": total_chunks
            }).to_string()
        },
        _ => {
            println!("🔍 发送非Progress消息: {:?}", message);
            serde_json::to_string(&message).unwrap()
        },
    };

    println!("🔍 发送SSE消息 [{}]: {}", session_id, msg);
    if let Err(e) = sse_manager.send_data(session_id, &msg) {
        eprintln!("❌ 发送SSE消息失败: {}", e);
    } else {
        println!("✅ SSE消息发送成功");
    }
}

async fn complete_upload(session_id: &str, session: &UploadSession, state: &Arc<AppState>) {
    println!("🔍 开始完成上传流程: {}", session_id);
    println!("🔍 会话状态: 接收 {}/{}, 进度: {}%",
        session.received_chunks.len(),
        session.total_chunks,
        session.progress
    );

    // 重命名临时文件
    let final_file_path = format!("{}/{}", UPLOAD_DIR, session.filename);
    if let Ok(_) = async_fs::rename(&session.temp_file_path, &final_file_path).await {
        // 验证文件哈希
        if let Some(expected_hash) = &session.file_hash {
            if let Ok(actual_hash) = calculate_file_hash(&final_file_path).await {
                if actual_hash.to_lowercase() != expected_hash.to_lowercase() {
                    eprintln!("⚠️ 文件哈希验证失败: {} (期望: {})", actual_hash, expected_hash);
                } else {
                    println!("✅ 文件哈希验证通过: {}", session.filename);
                }
            }
        }

        // 更新会话状态
        {
            let mut sessions = state.sessions.write().await;
            if let Some(s) = sessions.get_mut(session_id) {
                s.completed = true;
                s.progress = 100.0;
            }
        }

        // 广播完成消息
        broadcast_progress(
            session_id,
            ProgressMessage::Completed {
                session_id: session_id.to_string(),
                filename: session.filename.clone(),
                file_size: session.file_size,
                download_url: format!("/api/download/{}", session_id),
                progress: 100.0,
            },
        );

        // 延迟断开SSE连接，确保客户端能接收到完成消息
        let sse_manager = get_global_sse_manager();
        let session_id_clone = session_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = sse_manager.send_data(&session_id_clone, "DISCONNECT_EVENT");
        });

        println!("✅ 文件上传完成: {} ({})", session.filename, format_bytes(session.file_size));

        // 延迟清理会话
        let state_clone = state.clone();
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            cleanup_session(&session_id, &state_clone).await;
        });
    }
}

async fn calculate_file_hash(file_path: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut file = async_fs::File::open(file_path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

async fn cleanup_session(session_id: &str, state: &Arc<AppState>) {
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.remove(session_id) {
        println!("🧹 清理会话: {} ({})", session_id, session.filename);
    }
}

fn json_response<T: Serialize>(data: &T, status: StatusCode) -> Response<Full<Bytes>> {
    let json = serde_json::to_string(data).unwrap();
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .unwrap()
}


fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

