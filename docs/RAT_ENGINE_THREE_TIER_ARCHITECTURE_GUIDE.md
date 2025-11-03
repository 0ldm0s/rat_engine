# RAT Engine三层架构开发规范指导手册

## 1. 架构概述

基于RAT Engine框架的三层架构，通过明确的职责分离实现高可维护性的后端应用开发。

### 1.1 核心依赖

- **rat_engine**: 自研HTTP服务器框架（非axum/hyper）
- **rat_quickdb**: 自研ODM框架，支持PostgreSQL、MySQL、SQLite、MongoDB
- **tokio**: 异步运行时
- **serde**: JSON序列化/反序列化
- **chrono**: 时间处理
- **uuid**: 唯一ID生成
- **tracing**: 日志记录

### 1.2 三层职责划分

```
┌─────────────────────┐
│   表示层 (Routes)    │  ← rat_engine::Router, HttpRequest, Response
├─────────────────────┤
│    业务层 (Services) │  ← 业务逻辑、验证、数据编排
├─────────────────────┤
│   数据层 (Models)    │  ← rat_quickdb::define_model! 宏定义
└─────────────────────┘
```

## 2. 表示层 (Routes)

### 2.1 核心依赖导入

```rust
use rat_engine::{Response, StatusCode, Method, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use rat_logger::{info, warn, error};
use serde_json::json;
use serde::{Deserialize, Serialize};
use std::error::Error;
```

### 2.2 路由处理器模板

```rust
//! 功能模块路由处理

use rat_engine::{Response, StatusCode, Method, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use rat_logger::{info, warn, error};
use serde_json::json;

use ai_chat_backend::public::services::BusinessService;
use ai_chat_backend::public::utils::response::ApiResponse;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// 请求参数结构
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateResourceParams {
    pub name: String,           // 必填字段
    pub description: Option<String>, // 可选字段
    pub category_id: String,    // 关联ID
}

/// 资源创建处理器 (/api/resources)
pub async fn handle_create_resource(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    // 1. 用户身份验证
    let user_id = match extract_user_id_from_request(&req)? {
        Some(id) => id,
        None => return ApiResponse::unauthorized("未授权访问").to_http_response(),
    };

    // 2. 请求体解析
    let body_str = req.body_as_string().unwrap_or_default();
    let params: CreateResourceParams = match serde_json::from_str(&body_str) {
        Ok(data) => data,
        Err(_) => return ApiResponse::validation_error("请求格式错误，请检查输入").to_http_response(),
    };

    // 3. 参数验证
    if params.name.trim().is_empty() {
        return ApiResponse::validation_error("资源名称不能为空").to_http_response();
    }

    if params.name.len() > 100 {
        return ApiResponse::validation_error("资源名称不能超过100个字符").to_http_response();
    }

    // 4. 调用业务层
    match BusinessService::create_resource(user_id, params).await {
        Ok(result) => {
            info!("资源创建成功");
            ApiResponse::success(result).to_http_response()
        }
        Err(e) => {
            error!("资源创建失败: {}", e);
            match e {
                ai_chat_backend::common::models::AppError::Validation(msg, details) => {
                    ApiResponse::validation_error_with_details(msg, details).to_http_response()
                }
                ai_chat_backend::common::models::AppError::Conflict(msg) => {
                    ApiResponse::conflict(msg).to_http_response()
                }
                ai_chat_backend::common::models::AppError::Auth(msg) => {
                    ApiResponse::authentication_error(msg).to_http_response()
                }
                ai_chat_backend::common::models::AppError::Permission(msg) => {
                    ApiResponse::authorization_error(msg).to_http_response()
                }
                ai_chat_backend::common::models::AppError::UserNotFound => {
                    ApiResponse::not_found_error("用户不存在").to_http_response()
                }
                _ => {
                    ApiResponse::internal_error(&format!("创建资源失败: {}", e)).to_http_response()
                }
            }
        }
    }
}

/// 获取资源列表处理器 (/api/resources)
pub async fn handle_get_resources(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    let user_id = match extract_user_id_from_request(&req)? {
        Some(id) => id,
        None => return ApiResponse::unauthorized("未授权访问").to_http_response(),
    };

    // 从查询参数提取分页信息
    let query_params = req.query_params();
    let page: u32 = query_params.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    let limit: u32 = query_params.get("limit").and_then(|l| l.parse().ok()).unwrap_or(20);

    match BusinessService::get_user_resources(user_id, page, limit).await {
        Ok(result) => ApiResponse::success(result).to_http_response(),
        Err(e) => {
            error!("获取资源列表失败: {}", e);
            ApiResponse::internal_error(&format!("获取资源列表失败: {}", e)).to_http_response()
        }
    }
}

/// 从请求中提取用户ID（JWT认证）
fn extract_user_id_from_request(req: &HttpRequest) -> Result<Option<String>, Box<dyn Error>> {
    use ai_chat_backend::public::services::auth::PublicAuthService;

    // 从Authorization头获取token
    let auth_header = req.headers.get("authorization").and_then(|h| h.to_str().ok());

    let token = if let Some(auth_str) = auth_header {
        if auth_str.starts_with("Bearer ") {
            auth_str.strip_prefix("Bearer ").unwrap_or("")
        } else {
            auth_str
        }
    } else {
        return Ok(None);
    };

    // 验证token并提取用户ID
    match PublicAuthService::decode_token(token) {
        Ok(claims) => Ok(Some(claims.sub)),
        Err(_) => Ok(None),
    }
}
```

### 2.3 路由注册

```rust
// 在 main.rs 中注册路由
use rat_engine::Router;

fn setup_routes(router: &mut Router) {
    // 基础路由
    router.add_route(Method::GET, "/health", |_req| {
        Box::pin(async {
            ApiResponse::success(json!({
                "status": "healthy",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).to_http_response()
        })
    });

    // 业务路由
    router.add_route(Method::POST, "/api/resources", |req| {
        Box::pin(resource::handle_create_resource(req))
    });
    router.add_route(Method::GET, "/api/resources", |req| {
        Box::pin(resource::handle_get_resources(req))
    });
    router.add_route(Method::GET, "/api/resources/<str:id>", |req| {
        Box::pin(resource::handle_get_resource(req))
    });
    router.add_route(Method::PUT, "/api/resources/<str:id>", |req| {
        Box::pin(resource::handle_update_resource(req))
    });
    router.add_route(Method::DELETE, "/api/resources/<str:id>", |req| {
        Box::pin(resource::handle_delete_resource(req))
    });
}
```

### 2.4 统一响应格式

```rust
// src/public/utils/response.rs
use rat_engine::{Response, Full, Bytes};
use serde_json::{json, Value};

pub struct ApiResponse;

impl ApiResponse {
    pub fn success<T: serde::Serialize>(data: T) -> Self {
        Self {
            status_code: 200,
            data: Some(json!(data)),
            message: "操作成功".to_string(),
            success: true,
        }
    }

    pub fn validation_error(message: &str) -> Self {
        Self {
            status_code: 400,
            data: None,
            message: message.to_string(),
            success: false,
        }
    }

    pub fn validation_error_with_details(message: String, details: Vec<String>) -> Self {
        Self {
            status_code: 400,
            data: Some(json!({ "details": details })),
            message,
            success: false,
        }
    }

    pub fn authentication_error(message: &str) -> Self {
        Self {
            status_code: 401,
            data: None,
            message: message.to_string(),
            success: false,
        }
    }

    pub fn authorization_error(message: &str) -> Self {
        Self {
            status_code: 403,
            data: None,
            message: message.to_string(),
            success: false,
        }
    }

    pub fn not_found_error(message: &str) -> Self {
        Self {
            status_code: 404,
            data: None,
            message: message.to_string(),
            success: false,
        }
    }

    pub fn conflict(message: &str) -> Self {
        Self {
            status_code: 409,
            data: None,
            message: message.to_string(),
            success: false,
        }
    }

    pub fn internal_error(message: &str) -> Self {
        Self {
            status_code: 500,
            data: None,
            message: message.to_string(),
            success: false,
        }
    }

    pub fn to_http_response(self) -> Response<Full<Bytes>> {
        let response_body = json!({
            "code": self.status_code,
            "data": self.data,
            "message": self.message,
            "success": self.success
        });

        Response::builder()
            .status(StatusCode::from_u16(self.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(response_body.to_string())))
            .unwrap()
    }
}

impl ApiResponse {
    status_code: u16,
    data: Option<Value>,
    message: String,
    success: bool,
}
```

## 3. SSE (Server-Sent Events) 实现

### 3.1 SSE连接管理

```rust
// src/public/services/user_connection_manager.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 用户连接信息
#[derive(Debug, Clone)]
pub struct UserConnection {
    pub connection_id: String,
    pub user_id: String,
    pub client_info: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// 用户连接管理器
pub struct UserConnectionManager {
    connections: Arc<RwLock<HashMap<String, UserConnection>>>, // connection_id -> UserConnection
    user_connections: Arc<RwLock<HashMap<String, Vec<String>>>>, // user_id -> Vec<connection_id>
}

impl UserConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            user_connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册用户连接
    pub async fn register_connection(&self, connection_id: String, user_id: String, client_info: String) {
        let connection = UserConnection {
            connection_id: connection_id.clone(),
            user_id: user_id.clone(),
            client_info,
            connected_at: chrono::Utc::now(),
        };

        // 添加到连接映射
        self.connections.write().await.insert(connection_id.clone(), connection);

        // 添加到用户连接列表
        let mut user_conns = self.user_connections.write().await;
        user_conns.entry(user_id.clone()).or_insert_with(Vec::new).push(connection_id.clone());

        info!("用户连接已注册 - 用户ID: {}, 连接ID: {}", user_id, connection_id);
    }

    /// 移除连接
    pub async fn remove_connection(&self, connection_id: &str) {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(connection_id) {
            let mut user_conns = self.user_connections.write().await;
            if let Some(conn_list) = user_conns.get_mut(&connection.user_id) {
                conn_list.retain(|id| id != connection_id);
                if conn_list.is_empty() {
                    user_conns.remove(&connection.user_id);
                }
            }
            info!("用户连接已移除 - 用户ID: {}, 连接ID: {}", connection.user_id, connection_id);
        }
    }

    /// 获取用户的所有连接
    pub async fn get_user_connections(&self, user_id: &str) -> Option<Vec<String>> {
        self.user_connections.read().await.get(user_id).cloned()
    }

    /// 获取活跃连接数
    pub async fn get_active_connections_count(&self) -> usize {
        self.connections.read().await.len()
    }
}

/// 全局SSE管理器
pub struct GlobalSseManager {
    connection_manager: Arc<UserConnectionManager>,
}

impl GlobalSseManager {
    pub fn new() -> Self {
        Self {
            connection_manager: Arc::new(UserConnectionManager::new()),
        }
    }

    pub fn get_connection_manager(&self) -> Arc<UserConnectionManager> {
        Arc::clone(&self.connection_manager)
    }
}

// 全局实例
lazy_static::lazy_static! {
    pub static ref GLOBAL_SSE_MANAGER: GlobalSseManager = GlobalSseManager::new();
}
```

### 3.2 SSE路由实现

```rust
// src/public/routes/notification.rs
use rat_engine::{Response, StatusCode, Method, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use rat_logger::{info, warn, error};
use serde_json::json;
use std::time::Duration;

use ai_chat_backend::public::services::sse_manager::SseConnectionManager;
use ai_chat_backend::public::services::user_connection_manager::GLOBAL_SSE_MANAGER;

/// SSE通知流处理器 (/api/notifications/stream)
pub async fn handle_sse_stream(req: HttpRequest) -> Result<Response<Full<Bytes>>, rat_engine::Error> {
    // 从URL参数或请求头获取认证信息
    let (user_id, connection_id) = match extract_sse_auth(&req) {
        Ok(Some((uid, cid))) => (uid, cid),
        Ok(None) => return ApiResponse::unauthorized("认证信息缺失").to_http_response(),
        Err(_) => return ApiResponse::authentication_error("认证失败").to_http_response(),
    };

    info!("SSE连接请求 - 用户ID: {}, 连接ID: {}", user_id, connection_id);

    // 验证连接ID格式
    if uuid::Uuid::parse_str(&connection_id).is_err() {
        return ApiResponse::validation_error("无效的连接ID格式").to_http_response();
    }

    // 验证用户是否存在
    if UserDetails::find_by_id(&user_id).await?.is_none() {
        return ApiResponse::not_found_error("用户不存在").to_http_response();
    }

    // 建立SSE连接
    match SseConnectionManager::establish_connection(user_id.clone(), connection_id.clone()).await {
        Ok(response) => {
            info!("SSE连接建立成功 - 用户ID: {}, 连接ID: {}", user_id, connection_id);
            Ok(response)
        }
        Err(e) => {
            error!("SSE连接建立失败: {}", e);
            ApiResponse::internal_error(&format!("SSE连接失败: {}", e)).to_http_response()
        }
    }
}

/// 提取SSE认证信息
fn extract_sse_auth(req: &HttpRequest) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    use ai_chat_backend::public::services::auth::PublicAuthService;

    // 优先从URL参数获取（浏览器兼容）
    let query_params = req.query_params();

    if let (Some(token), Some(conn_id)) = (
        query_params.get("authorization"),
        query_params.get("x_connection_id")
    ) {
        // 处理URL编码的Bearer token
        let clean_token = if token.starts_with("Bearer%20") {
            token.strip_prefix("Bearer%20").unwrap_or("")
        } else if token.starts_with("Bearer ") {
            token.strip_prefix("Bearer ").unwrap_or("")
        } else {
            token
        };

        match PublicAuthService::decode_token(clean_token) {
            Ok(claims) => Ok(Some((claims.sub, conn_id.to_string()))),
            Err(_) => Ok(None),
        }
    } else {
        // 从请求头获取
        let auth_header = req.headers.get("authorization").and_then(|h| h.to_str().ok());
        let conn_header = req.headers.get("x-connection-id").and_then(|h| h.to_str().ok());

        if let (Some(auth_str), Some(conn_str)) = (auth_header, conn_header) {
            let token = if auth_str.starts_with("Bearer ") {
                auth_str.strip_prefix("Bearer ").unwrap_or("")
            } else {
                auth_str
            };

            match PublicAuthService::decode_token(token) {
                Ok(claims) => Ok(Some((claims.sub, conn_str.to_string()))),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
}
```

### 3.3 SSE连接管理

```rust
// src/public/services/sse_manager.rs
use rat_engine::{Response, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use rat_logger::{info, warn, error};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

use ai_chat_backend::public::services::user_connection_manager::GLOBAL_SSE_MANAGER;

/// SSE连接管理器
pub struct SseConnectionManager {
    connections: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<String>>>>,
}

impl SseConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 建立SSE连接
    pub async fn establish_connection(user_id: String, connection_id: String) -> Result<Response<Full<Bytes>>, Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        // 注册连接
        let connection_manager = GLOBAL_SSE_MANAGER.get_connection_manager();
        connection_manager.register_connection(
            connection_id.clone(),
            user_id.clone(),
            "SSE客户端".to_string(),
        ).await;

        // 保存发送器
        self.connections.write().await.insert(connection_id.clone(), tx);

        // 获取客户端信息
        let client_info = "curl/8.16.0".to_string(); // 可以从请求头获取实际客户端信息

        // 创建SSE响应
        let connection_id_clone = connection_id.clone();
        let user_id_clone = user_id.clone();

        // 发送连接确认消息
        let welcome_message = json!({
            "type": "connected",
            "message": "通知连接已建立",
            "connection_id": connection_id_clone,
            "user_id": user_id_clone,
            "client_info": client_info,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }).to_string();

        // 构建SSE响应体
        let mut response_body = format!("data: {}\n\n", welcome_message);

        // 启动心跳任务
        let connection_id_heartbeat = connection_id.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let heartbeat = json!({
                    "type": "heartbeat",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }).to_string();

                if let Some(conn_manager) = Self::get_instance().connections.read().await.get(&connection_id_heartbeat) {
                    if conn_manager.send(format!("data: {}\n\n", heartbeat)).is_err() {
                        warn!("心跳发送失败，连接可能已断开: {}", connection_id_heartbeat);
                        // 清理连接
                        Self::cleanup_connection(&connection_id_heartbeat).await;
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        // 构建HTTP响应
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive")
            .header("access-control-allow-origin", "*")
            .header("access-control-allow-headers", "Cache-Control")
            .body(Full::new(Bytes::from(response_body)))
            .unwrap();

        Ok(response)
    }

    /// 发送数据到指定连接
    pub async fn send_to_connection(connection_id: &str, data: &str) -> Result<(), Box<dyn std::error::Error>> {
        let connections = self.connections.read().await;
        if let Some(sender) = connections.get(connection_id) {
            let message = format!("data: {}\n\n", data);
            sender.send(message).map_err(|e| {
                warn!("发送SSE消息失败: {}", e);
                Box::new(e) as Box<dyn std::error::Error>
            })?;
            Ok(())
        } else {
            warn!("SSE连接不存在: {}", connection_id);
            Err("连接不存在".into())
        }
    }

    /// 清理连接
    async fn cleanup_connection(connection_id: &str) {
        // 从SSE管理器移除
        Self::get_instance().connections.write().await.remove(connection_id);

        // 从用户连接管理器移除
        let connection_manager = GLOBAL_SSE_MANAGER.get_connection_manager();
        connection_manager.remove_connection(connection_id).await;

        info!("SSE连接已清理: {}", connection_id);
    }

    fn get_instance() -> Arc<Self> {
        lazy_static::lazy_static! {
            static ref INSTANCE: Arc<SseConnectionManager> = Arc::new(SseConnectionManager::new());
        }
        INSTANCE.clone()
    }

    /// 发送通知给用户的所有连接
    pub async fn send_notification_to_user(user_id: &str, notification: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
        let connection_manager = GLOBAL_SSE_MANAGER.get_connection_manager();

        if let Some(connection_ids) = connection_manager.get_user_connections(user_id).await {
            let notification_str = notification.to_string();

            for conn_id in connection_ids {
                if let Err(e) = Self::get_instance().send_to_connection(&conn_id, &notification_str).await {
                    warn!("发送通知到连接失败 {}: {}", conn_id, e);
                }
            }
        } else {
            warn!("用户 {} 没有活跃连接", user_id);
        }

        Ok(())
    }
}
```

### 3.4 SSE心跳保活

```rust
// src/public/services/sse_heartbeat.rs
use std::time::Duration;
use tokio::time;
use tracing::{info, error, warn};

use crate::public::services::sse_manager::SseConnectionManager;
use crate::public::services::user_connection_manager::GLOBAL_SSE_MANAGER;

/// 启动SSE心跳保活任务
pub async fn start_sse_heartbeat_task() {
    info!("💓 SSE心跳保活任务已启动");
    let mut interval = time::interval(Duration::from_secs(30));

    loop {
        interval.tick().await;
        if let Err(e) = send_heartbeat_to_all_connections().await {
            error!("发送SSE心跳失败: {}", e);
        }
    }
}

/// 向所有连接发送心跳
async fn send_heartbeat_to_all_connections() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::json;

    let heartbeat_data = json!({
        "type": "heartbeat",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "message": "keep-alive"
    }).to_string();

    let sse_manager = SseConnectionManager::get_instance();
    let connections = sse_manager.connections.read().await;

    let mut failed_connections = Vec::new();

    for (connection_id, sender) in connections.iter() {
        let message = format!("data: {}\n\n", heartbeat_data);
        if sender.send(message).is_err() {
            warn!("心跳发送失败，连接可能已断开: {}", connection_id);
            failed_connections.push(connection_id.clone());
        }
    }

    drop(connections);

    // 清理失败的连接
    for failed_id in failed_connections {
        SseConnectionManager::cleanup_connection(&failed_id).await;
    }

    Ok(())
}
```

## 4. 业务层 (Services)

### 4.1 服务层模板

```rust
//! 业务服务模块

use crate::common::models::{AppResult, AppError, ResourceModel, UserDetails};
use serde_json::json;
use tracing::{info, warn, error};

/// 业务服务结构
pub struct BusinessService;

impl BusinessService {
    /// 创建资源
    pub async fn create_resource(
        user_id: String,
        params: CreateResourceParams,
    ) -> AppResult<serde_json::Value> {
        info!("开始创建资源，用户: {}", user_id);

        // 1. 验证用户权限
        let user = UserDetails::find_by_id(&user_id).await?
            .ok_or_else(|| AppError::UserNotFound)?;

        if !Self::can_create_resource(&user) {
            return Err(AppError::forbidden("用户无权限创建资源".to_string()));
        }

        // 2. 检查业务规则
        if Self::resource_exists_for_user(&user_id, &params.name).await? {
            return Err(AppError::conflict("资源名称已存在".to_string()));
        }

        // 3. 创建主要资源
        let mut resource = ResourceModel::new(
            user_id.clone(),
            params.name,
            params.description,
            params.category_id,
        );

        // 4. 数据验证
        resource.validate_business_constraints()?;

        // 5. 保存到数据库
        resource.create().await?;

        // 6. 发送实时通知
        if let Err(e) = Self::send_notification_to_user(&user_id, &json!({
            "type": "resource_created",
            "resource_id": resource.id,
            "message": "资源创建成功",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })).await {
            warn!("发送创建通知失败: {}", e);
        }

        // 7. 返回结果
        let result = json!({
            "resource": {
                "id": resource.id,
                "name": resource.name,
                "created_at": resource.created_at.to_rfc3339(),
            }
        });

        info!("资源创建成功: {}", resource.id);
        Ok(result)
    }

    /// 获取用户资源列表
    pub async fn get_user_resources(
        user_id: String,
        page: u32,
        limit: u32,
    ) -> AppResult<serde_json::Value> {
        let offset = (page - 1) * limit;

        // 查询资源
        let resources = ResourceModel::find_by_user_id_with_pagination(
            &user_id,
            offset as usize,
            limit as usize,
        ).await?;

        // 获取总数
        let total = ResourceModel::count_by_user_id(&user_id).await?;

        // 转换为响应格式
        let resource_list: Vec<serde_json::Value> = resources
            .into_iter()
            .map(|r| json!({
                "id": r.id,
                "name": r.name,
                "status": r.status,
                "created_at": r.created_at.to_rfc3339(),
            }))
            .collect();

        Ok(json!({
            "resources": resource_list,
            "pagination": {
                "page": page,
                "limit": limit,
                "total": total,
                "total_pages": (total as f64 / limit as f64).ceil() as u32,
            }
        }))
    }

    /// 权限检查
    fn can_create_resource(user: &UserDetails) -> bool {
        // 实现权限检查逻辑
        user.status == "active"
    }

    /// 检查资源是否存在
    async fn resource_exists_for_user(user_id: &str, name: &str) -> AppResult<bool> {
        match ResourceModel::find_by_user_and_name(user_id, name).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// 发送通知给用户
    async fn send_notification_to_user(
        user_id: &str,
        notification: &serde_json::Value,
    ) -> AppResult<()> {
        use crate::public::services::sse_manager::SseConnectionManager;

        SseConnectionManager::send_notification_to_user(user_id, notification).await
            .map_err(|e| AppError::internal(format!("发送通知失败: {}", e)))?;

        Ok(())
    }
}
```

## 5. 数据层 (Models)

### 5.1 数据库兼容性说明

rat_quickdb 提供统一的API接口，支持多种数据库后端：
- **PostgreSQL**: 生产环境首选
- **MySQL**: 传统企业应用
- **SQLite**: 开发测试和轻量级应用
- **MongoDB**: NoSQL文档数据库

所有数据库使用相同的API接口，rat_quickdb负责处理底层的差异，开发者无需修改代码即可切换数据库。

### 5.2 模型定义宏

```rust
//! 资源数据模型

use rat_quickdb::define_model;
use crate::common::models::AppError;

define_model! {
    struct ResourceModel {
        id: String,
        user_id: String,
        name: String,
        description: Option<String>,
        category_id: String,
        status: ResourceStatus,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    }
    collection = "resources",
    fields = {
        id: string_field(None, None, None).unique(),
        user_id: string_field(None, None, None).required(),
        name: string_field(None, None, None).required(),
        description: string_field(None, None, None).optional(),
        category_id: string_field(None, None, None).required(),
        status: string_field(None, None, None).default("active"),
        created_at: datetime_field(None, None).required(),
        updated_at: datetime_field(None, None).required(),
    }
    indexes = [
        { fields: ["user_id"], unique: false, name: "idx_user_id" },
        { fields: ["name"], unique: false, name: "idx_name" },
        { fields: ["user_id", "name"], unique: true, name: "idx_user_name" },
        { fields: ["category_id"], unique: false, name: "idx_category_id" },
        { fields: ["status"], unique: false, name: "idx_status" },
        { fields: ["created_at"], unique: false, name: "idx_created_at" },
    ],
}
```

### 5.2 模型实现

```rust
/// 资源状态枚举
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ResourceStatus {
    Active,
    Inactive,
    Archived,
}

impl Default for ResourceStatus {
    fn default() -> Self {
        ResourceStatus::Active
    }
}

impl ResourceModel {
    /// 创建新实例
    pub fn new(
        user_id: String,
        name: String,
        description: Option<String>,
        category_id: String,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: rat_quickdb::generate_uuid(),
            user_id,
            name,
            description,
            category_id,
            status: ResourceStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 业务约束验证
    pub fn validate_business_constraints(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::validation(
                "资源名称不能为空".to_string(),
                vec!["name:required".to_string()],
            ));
        }

        if self.name.len() > 100 {
            return Err(AppError::validation(
                "资源名称不能超过100个字符".to_string(),
                vec!["name:too_long".to_string()],
            ));
        }

        if let Some(desc) = &self.description {
            if desc.len() > 1000 {
                return Err(AppError::validation(
                    "描述不能超过1000个字符".to_string(),
                    vec!["description:too_long".to_string()],
                ));
            }
        }

        Ok(())
    }

    /// 根据用户ID和名称查找
    pub async fn find_by_user_and_name(
        user_id: &str,
        name: &str,
    ) -> AppResult<Option<Self>> {
        let results = Self::find(vec![
            ("user_id", "=", user_id),
            ("name", "=", name),
        ], None).await?;

        if results.is_empty() {
            Ok(None)
        } else {
            Ok(Some(results.into_iter().next().unwrap()))
        }
    }

    /// 分页查询用户资源
    pub async fn find_by_user_id_with_pagination(
        user_id: &str,
        offset: usize,
        limit: usize,
    ) -> AppResult<Vec<Self>> {
        let options = rat_quickdb::QueryOptions {
            offset: Some(offset),
            limit: Some(limit),
            order_by: Some("created_at".to_string()),
            order_direction: Some(rat_quickdb::OrderDirection::Desc),
        };

        Self::find(vec![("user_id", "=", user_id)], Some(options)).await
    }

    /// 统计用户资源数量
    pub async fn count_by_user_id(user_id: &str) -> AppResult<u64> {
        Self::count(vec![("user_id", "=", user_id)]).await
    }
}
```

## 6. 主程序集成

### 6.1 main.rs 完整结构

```rust
// src/public/main.rs
use rat_engine::{RatEngine, Router, Method};
use std::error::Error;
use rat_logger;

// 导入路由模块
use ai_chat_backend::public::routes::*;

// 导入服务
use ai_chat_backend::public::services;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 初始化日志
    rat_logger::init();

    // 初始化数据库
    services::db::initialize_database().await?;

    // 启动SSE心跳保活任务
    tokio::spawn(async {
        services::sse_heartbeat::start_sse_heartbeat_task().await;
    });

    // 创建路由器
    let mut router = Router::new();

    // 注册所有路由
    setup_routes(&mut router);

    // 启动服务器
    let server = RatEngine::new(router);

    info!("🚀 前台API服务器启动成功，端口: 30000");
    server.run("0.0.0.0:30000").await?;

    Ok(())
}

fn setup_routes(router: &mut Router) {
    // 健康检查
    router.add_route(Method::GET, "/health", |_req| {
        Box::pin(async {
            ApiResponse::success(json!({
                "status": "healthy",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).to_http_response()
        })
    });

    // API文档
    router.add_route(Method::GET, "/", |_req| {
        Box::pin(async {
            ApiResponse::success(json!({
                "api_name": "RAT Engine API",
                "version": "1.0.0",
                "endpoints": [
                    "GET /health",
                    "GET /api/notifications/stream",
                    "POST /api/resources",
                    "GET /api/resources"
                ]
            })).to_http_response()
        })
    });

    // SSE通知流
    router.add_route(Method::GET, "/api/notifications/stream", |req| {
        Box::pin(notification::handle_sse_stream(req))
    });

    // 业务路由
    router.add_route(Method::POST, "/api/resources", |req| {
        Box::pin(resource::handle_create_resource(req))
    });
    router.add_route(Method::GET, "/api/resources", |req| {
        Box::pin(resource::handle_get_resources(req))
    });
}
```

## 7. 数据库初始化

### 7.1 数据库服务

```rust
// src/public/services/db.rs
use crate::common::models::*;
use rat_logger::info;

/// 初始化数据库表
pub async fn initialize_database() -> Result<(), Box<dyn std::error::Error>> {
    info!("开始初始化数据库表...");

    // 创建所有模型表
    ModelManager::<User>::create_table().await?;
    ModelManager::<UserDetails>::create_table().await?;
    ModelManager::<ResourceModel>::create_table().await?;
    ModelManager::<FriendshipRequest>::create_table().await?;
    ModelManager::<Notification>::create_table().await?;
    // ... 其他模型表

    info!("✅ 数据库表初始化完成");
    Ok(())
}
```

## 8. 构建和运行命令

### 8.1 Cargo配置

```toml
# Cargo.toml
[dependencies]
# RAT框架
rat_engine = { path = "../rat_engine" }
rat_quickdb = { path = "../rat_quickdb" }

# 异步运行时
tokio = { version = "1.0", features = ["full"] }

# 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 时间处理
chrono = { version = "0.4", features = ["serde"] }

# UUID
uuid = { version = "1.0", features = ["v4", "serde"] }

# 日志
tracing = "0.1"
tracing-subscriber = "0.3"
rat_logger = { path = "../rat_logger" }

# 错误处理
thiserror = "1.0"
anyhow = "1.0"

# 工具
lazy_static = "1.4"

[features]
default = ["public-api"]
public-api = []
public-cron = []
admin-api = []
admin-cron = []
test-full = ["public-api", "admin-api", "public-cron", "admin-cron"]
```

### 8.2 运行命令

```bash
# 运行前台API服务器
cargo run --features public-api --bin public-api

# 运行管理后台
cargo run --features admin-api --bin admin-api

# 检查编译状态
cargo check --features public-api --bin public-api

# 构建不运行
cargo build --features public-api --bin public-api
```

## 9. 开发最佳实践

### 9.1 错误处理

```rust
// 统一应用错误类型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Database(#[from] rat_quickdb::QuickDbError),

    #[error("验证错误: {0}")]
    Validation(String, Vec<String>),

    #[error("认证错误: {0}")]
    Auth(String),

    #[error("权限错误: {0}")]
    Permission(String),

    #[error("用户不存在")]
    UserNotFound,

    #[error("冲突错误: {0}")]
    Conflict(String),

    #[error("内部错误: {0}")]
    Internal(String),
}
```

### 9.2 日志规范

```rust
// 使用tracing日志
use tracing::{info, warn, error, debug, trace};

// 不同级别的日志
trace!("详细调试信息");
debug!("调试信息");
info!("一般信息");
warn!("警告信息");
error!("错误信息");
```

### 9.3 配置管理

```rust
// 环境特定配置
pub struct Config {
    pub database_url: String,
    pub server_port: u16,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost/test".to_string()),
            server_port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            log_level: std::env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "info".to_string()),
        }
    }
}
```

## 10. 总结

本指南基于RAT Engine框架的实际使用经验，提供了完整的三层架构实现方案：

- **表示层**: 使用rat_engine处理HTTP请求和SSE连接
- **业务层**: 实现核心业务逻辑和SSE通知管理
- **数据层**: 使用rat_quickdb宏定义模型和数据库操作

通过遵循这些规范，可以构建出高可维护性、高性能的后端应用，并为未来的项目提供可复用的技术架构基础。