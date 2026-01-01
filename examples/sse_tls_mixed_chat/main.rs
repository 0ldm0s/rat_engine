//! SSE + TLS 聊天室示例
//!
//! 基于 Server-Sent Events 的实时聊天室演示（单端口 + TLS）

use rat_engine::server::{
    Router,
    http_request::HttpRequest,
    global_sse_manager::get_global_sse_manager,
    cert_manager::{CertificateManager, CertConfig, CertManagerConfig},
};
use rat_engine::RatEngine;
use rat_engine::{Request, Method, StatusCode, Response};
use rat_engine::{Incoming, Frame, Bytes, Full};
use std::sync::Arc;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::time::{sleep, Duration};
use serde_json::{json, Value};
use uuid::Uuid;
use dashmap::DashMap;

/// 用户信息
#[derive(Debug, Clone)]
struct UserInfo {
    username: String,
    nickname: String,
    room_id: u32,
    connection_uuid: String,
}

/// 房间信息
#[derive(Debug)]
struct RoomInfo {
    id: u32,
    name: String,
    members: DashMap<String, UserInfo>, // connection_uuid -> UserInfo
}

impl RoomInfo {
    fn new(id: u32, name: String) -> Self {
        Self {
            id,
            name,
            members: DashMap::new(),
        }
    }

    fn add_user(&self, connection_uuid: String, user: UserInfo) {
        self.members.insert(connection_uuid, user);
    }

    fn remove_user(&self, connection_uuid: &str) -> Option<UserInfo> {
        self.members.remove(connection_uuid).map(|(_, user)| user)
    }

    fn get_member_count(&self) -> usize {
        self.members.len()
    }

    fn get_members(&self) -> Vec<(String, UserInfo)> {
        self.members.iter().map(|entry| (entry.key().clone(), entry.value().clone())).collect()
    }
}

/// 全局房间管理器
struct RoomManager {
    rooms: DashMap<u32, Arc<RoomInfo>>,
}

impl RoomManager {
    fn new() -> Self {
        let mut manager = Self {
            rooms: DashMap::new(),
        };

        // 初始化3个房间
        manager.rooms.insert(1, Arc::new(RoomInfo::new(1, "通用聊天室".to_string())));
        manager.rooms.insert(2, Arc::new(RoomInfo::new(2, "技术讨论".to_string())));
        manager.rooms.insert(3, Arc::new(RoomInfo::new(3, "闲聊吹水".to_string())));

        manager
    }

    fn get_room(&self, room_id: u32) -> Option<Arc<RoomInfo>> {
        self.rooms.get(&room_id).map(|room| room.clone())
    }

    fn is_valid_room(&self, room_id: u32) -> bool {
        self.rooms.contains_key(&room_id)
    }
}

/// 全局房间管理器实例
static ROOM_MANAGER: std::sync::OnceLock<Arc<RoomManager>> = std::sync::OnceLock::new();

/// 获取全局房间管理器
fn get_room_manager() -> Arc<RoomManager> {
    ROOM_MANAGER.get_or_init(|| {
        Arc::new(RoomManager::new())
    }).clone()
}

/// 向特定房间的所有用户广播消息
fn broadcast_to_room(room_id: u32, event_type: &str, data: &Value) {
    let room_manager = get_room_manager();
    let sse_manager = get_global_sse_manager();

    if let Some(room) = room_manager.get_room(room_id) {
        let message = json!({
            "type": event_type,
            "room_id": room_id,
            "room_name": room.name,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "data": data
        });

        // 向房间内所有用户发送消息
        for (connection_uuid, _) in room.get_members() {
            if let Err(e) = sse_manager.send_data(&connection_uuid, &message.to_string()) {
                println!("❌ 向用户 {} 发送消息失败: {}", connection_uuid, e);
                // 可以考虑移除失效连接
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 验证证书文件
    let cert_path = "examples/certs/ligproxy-test.0ldm0s.net.pem";
    let key_path = "examples/certs/ligproxy-test.0ldm0s.net-key.pem";

    if !std::path::Path::new(cert_path).exists() {
        return Err(format!("证书文件不存在: {}", cert_path).into());
    }
    if !std::path::Path::new(key_path).exists() {
        return Err(format!("私钥文件不存在: {}", key_path).into());
    }

    println!("✅ 证书验证通过");

    // 配置证书（包含 SNI 域名）
    let cert_config = CertConfig::from_paths(cert_path, key_path)
        .with_domains(vec!["ligproxy-test.0ldm0s.net".to_string()]);
    let cert_manager_config = CertManagerConfig::shared(cert_config);
    let cert_manager = CertificateManager::from_config(cert_manager_config)?;

    // 创建路由器（混合模式，自动检测协议类型）
    let mut router = Router::new();
    // 注意：不调用 enable_http_only()，使用混合模式

    // 单端口同时支持 HTTP 和 SSE

    // 注册主页路由 - 登录页面
    router.add_route(
        Method::GET,
        "/",
        |_req: HttpRequest| {
            Box::pin(async move {
                let html = include_str!("login.html");
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Full::new(Bytes::from(html)))
                    .unwrap())
            })
        }
    );

    // 注册聊天页面路由
    router.add_route(
        Method::GET,
        "/chat",
        |_req: HttpRequest| {
            Box::pin(async move {
                let html = include_str!("chat.html");
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Full::new(Bytes::from(html)))
                    .unwrap())
            })
        }
    );

    // 登录API端点
    router.add_route(
        Method::POST,
        "/api/connect",
        |req: HttpRequest| {
            Box::pin(async move {
                // 解析请求体
                let body_result = req.body_as_json();
                if body_result.is_err() {
                    let error_response = json!({
                        "status": "error",
                        "message": "无效的JSON格式"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                let body = body_result.unwrap();

                // 提取字段
                let username = body.get("username").and_then(|v| v.as_str()).unwrap_or("").trim();
                let nickname = body.get("nickname").and_then(|v| v.as_str()).unwrap_or(username).trim();
                let room_id = body.get("room_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                // 验证输入
                if username.is_empty() || username.len() < 2 || username.len() > 20 {
                    let error_response = json!({
                        "status": "error",
                        "message": "用户名长度必须在2-20个字符之间"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                // 验证房间ID
                let room_manager = get_room_manager();
                if !room_manager.is_valid_room(room_id) {
                    let error_response = json!({
                        "status": "error",
                        "message": "无效的房间ID，请选择1-3的房间"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                // 生成连接UUID
                let connection_uuid = Uuid::new_v4().to_string();

                // 获取房间信息
                let room = room_manager.get_room(room_id).unwrap();

                // 创建用户信息
                let user_info = UserInfo {
                    username: username.to_string(),
                    nickname: nickname.to_string(),
                    room_id,
                    connection_uuid: connection_uuid.clone(),
                };

                // 将用户添加到房间
                room.add_user(connection_uuid.clone(), user_info.clone());

                // 构建成功响应
                let success_response = json!({
                    "status": "success",
                    "message": "登录成功",
                    "data": {
                        "connection_uuid": connection_uuid,
                        "username": username,
                        "nickname": nickname,
                        "room_id": room_id,
                        "room_name": room.name
                    }
                });

                // 向房间内其他用户广播新用户加入消息
                let join_message = json!({
                    "type": "user_join",
                    "user": {
                        "username": username,
                        "nickname": nickname,
                        "connection_uuid": connection_uuid
                    },
                    "member_count": room.get_member_count()
                });
                broadcast_to_room(room_id, "system", &join_message);

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Full::new(Bytes::from(success_response.to_string())))
                    .unwrap())
            })
        }
    );

    // SSE连接端点
    router.add_streaming_route(
        Method::GET,
        "/chat/sse",
        |req: HttpRequest, params: HashMap<String, String>| {
            Box::pin(async move {
                // 从查询参数获取连接UUID（EventSource不支持自定义头部）
                let query_params = req.query_params();
                let connection_uuid = query_params.get("connection_uuid")
                    .unwrap_or(&String::new())
                    .clone();

                if connection_uuid.is_empty() {
                    // 创建临时错误连接
                    let uuid_str = Uuid::new_v4().to_string();
                    let temp_uuid = format!("error-{}", &uuid_str[..8]);
                    let sse_manager = get_global_sse_manager();
                    let response = sse_manager.register_connection(temp_uuid.clone())?;

                    // 发送错误消息
                    let error_message = json!({
                        "type": "error",
                        "error": "缺少连接UUID"
                    });
                    let _ = sse_manager.send_data(&temp_uuid, &error_message.to_string());
                    let _ = sse_manager.send_data(&temp_uuid, "DISCONNECT_EVENT");

                    return Ok(response);
                }

                // 验证用户是否存在
                let room_manager = get_room_manager();
                let mut user_room = None;

                for room_entry in room_manager.rooms.iter() {
                    let room = room_entry.value();
                    if room.members.contains_key(&connection_uuid) {
                        user_room = Some(room.clone());
                        break;
                    }
                }

                if user_room.is_none() {
                    // 用户不存在，创建临时错误连接
                    let uuid_str = Uuid::new_v4().to_string();
                    let temp_uuid = format!("error-{}", &uuid_str[..8]);
                    let sse_manager = get_global_sse_manager();
                    let response = sse_manager.register_connection(temp_uuid.clone())?;

                    let error_message = json!({
                        "type": "error",
                        "error": "无效的连接UUID"
                    });
                    let _ = sse_manager.send_data(&temp_uuid, &error_message.to_string());
                    let _ = sse_manager.send_data(&temp_uuid, "DISCONNECT_EVENT");

                    return Ok(response);
                }

                let room = user_room.unwrap();
                let user = room.members.get(&connection_uuid).unwrap().clone();

                // 注册SSE连接
                let sse_manager = get_global_sse_manager();
                let response = sse_manager.register_connection(connection_uuid.clone())?;

                // 发送欢迎消息
                let welcome_message = json!({
                    "type": "welcome",
                    "user": {
                        "username": user.username,
                        "nickname": user.nickname
                    },
                    "room": {
                        "id": room.id,
                        "name": room.name,
                        "member_count": room.get_member_count()
                    },
                    "members": room.get_members().into_iter().map(|(uuid, info)| {
                        json!({
                            "connection_uuid": uuid,
                            "username": info.username,
                            "nickname": info.nickname
                        })
                    }).collect::<Vec<_>>()
                });

                let _ = sse_manager.send_data(&connection_uuid, &welcome_message.to_string());

                Ok(response)
            })
        }
    );

    // 发送消息API端点
    router.add_route(
        Method::POST,
        "/api/send",
        |req: HttpRequest| {
            Box::pin(async move {
                // 从请求头获取连接UUID
                let connection_uuid = req.header("X-Connection-UUID")
                    .unwrap_or("")
                    .to_string();

                if connection_uuid.is_empty() {
                    let error_response = json!({
                        "status": "error",
                        "message": "缺少连接UUID"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                // 解析请求体
                let body_result = req.body_as_json();
                if body_result.is_err() {
                    let error_response = json!({
                        "status": "error",
                        "message": "无效的JSON格式"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                let body = body_result.unwrap();
                let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();

                if message.is_empty() {
                    let error_response = json!({
                        "status": "error",
                        "message": "消息不能为空"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                if message.len() > 500 {
                    let error_response = json!({
                        "status": "error",
                        "message": "消息长度不能超过500字符"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                // 验证用户并获取房间信息
                let room_manager = get_room_manager();
                let mut user_info = None;
                let mut room_info = None;

                for room_entry in room_manager.rooms.iter() {
                    let room = room_entry.value();
                    if let Some(user) = room.members.get(&connection_uuid) {
                        user_info = Some(user.clone());
                        room_info = Some(room.clone());
                        break;
                    }
                }

                if user_info.is_none() || room_info.is_none() {
                    let error_response = json!({
                        "status": "error",
                        "message": "无效的连接UUID"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                let user = user_info.unwrap();
                let room = room_info.unwrap();

                // 构建消息
                let chat_message = json!({
                    "id": Uuid::new_v4().to_string(),
                    "user": {
                        "connection_uuid": connection_uuid,
                        "username": user.username,
                        "nickname": user.nickname
                    },
                    "content": message,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });

                // 向房间内所有用户广播消息
                broadcast_to_room(room.id, "message", &chat_message);

                let success_response = json!({
                    "status": "success",
                    "message": "消息发送成功"
                });

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Full::new(Bytes::from(success_response.to_string())))
                    .unwrap())
            })
        }
    );

    // 退出房间API端点
    router.add_route(
        Method::POST,
        "/api/leave",
        |req: HttpRequest| {
            Box::pin(async move {
                // 从请求头获取连接UUID
                let connection_uuid = req.header("X-Connection-UUID")
                    .unwrap_or("")
                    .to_string();

                if connection_uuid.is_empty() {
                    let error_response = json!({
                        "status": "error",
                        "message": "缺少连接UUID"
                    });
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap());
                }

                // 查找用户并从房间移除
                let room_manager = get_room_manager();
                let mut removed_user = None;
                let mut room_id = None;

                for room_entry in room_manager.rooms.iter() {
                    let room = room_entry.value();
                    if let Some(user) = room.remove_user(&connection_uuid) {
                        removed_user = Some(user.clone());
                        room_id = Some(room.id);
                        break;
                    }
                }

                if let Some(user) = removed_user {
                    let room = room_manager.get_room(room_id.unwrap()).unwrap();

                    // 断开SSE连接
                    let sse_manager = get_global_sse_manager();
                    sse_manager.disconnect_connection(&connection_uuid);

                    // 向房间内其他用户广播用户离开消息
                    let leave_message = json!({
                        "type": "user_leave",
                        "user": {
                            "username": user.username,
                            "nickname": user.nickname,
                            "connection_uuid": connection_uuid
                        },
                        "member_count": room.get_member_count()
                    });
                    broadcast_to_room(room_id.unwrap(), "system", &leave_message);

                    let success_response = json!({
                        "status": "success",
                        "message": "已退出房间"
                    });

                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(success_response.to_string())))
                        .unwrap())
                } else {
                    let error_response = json!({
                        "status": "error",
                        "message": "用户不存在"
                    });
                    Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(error_response.to_string())))
                        .unwrap())
                }
            })
        }
    );

    println!("💬 SSE + TLS 聊天室服务器启动中...");
    println!("🔐 证书: ligproxy-test.0ldm0s.net");
    println!("📡 服务器地址: https://0.0.0.0:3001");
    println!("🏠 登录页面: https://ligproxy-test.0ldm0s.net:3001/");
    println!("💭 聊天页面: https://ligproxy-test.0ldm0s.net:3001/chat");
    println!();
    println!("📋 可用房间:");
    println!("   1. 通用聊天室");
    println!("   2. 技术讨论");
    println!("   3. 闲聊吹水");
    println!();
    println!("按 Ctrl+C 停止服务器");

    // 启动服务器（单端口 + TLS，绑定 0.0.0.0）
    let engine = RatEngine::builder()
        .worker_threads(4)
        .enable_logger()
        .router(router)
        .certificate_manager(cert_manager)
        .build()?;

    engine.start("0.0.0.0".to_string(), 3001).await?;

    Ok(())
}
