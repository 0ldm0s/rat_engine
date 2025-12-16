//! HTTP 请求处理结构体
//! 
//! 专门用于处理标准 HTTP 请求（包含 SSE），替换 hyper::Request<Incoming>

use hyper::{Method, Uri, Version, HeaderMap, body::Incoming};
use hyper::body::Bytes;
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::collections::HashMap;
use serde_json::Value;

/// HTTP 请求来源类型
#[derive(Debug, Clone)]
pub enum RequestSource {
    /// 标准 HTTP/1.1 请求
    Http1,
    /// HTTP/2 请求（非 gRPC）
    Http2,
    /// H2C 请求（非 gRPC）
    H2c,
}

/// 统一的 HTTP 请求结构体
/// 
/// 用于替换 hyper::Request<Incoming>，支持标准 HTTP 请求和 SSE
#[derive(Debug)]
pub struct HttpRequest {
    /// HTTP 方法
    pub method: Method,
    /// 请求 URI
    pub uri: Uri,
    /// HTTP 版本
    pub version: Version,
    /// 请求头
    pub headers: HeaderMap,
    /// 请求体数据
    pub body: Bytes,
    /// 客户端地址
    pub remote_addr: Option<SocketAddr>,
    /// 请求来源
    pub source: RequestSource,
}

impl HttpRequest {
    /// 从 hyper::Request<Incoming> 创建 HttpRequest
    pub async fn from_hyper_request(
        req: hyper::Request<Incoming>,
        remote_addr: Option<SocketAddr>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (parts, body) = req.into_parts();
        
        // 收集请求体
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                crate::utils::logger::error!("收集请求体失败: {}", e);
                return Err(Box::new(e));
            }
        };

        // 根据版本判断请求来源
        let source = match parts.version {
            Version::HTTP_11 => RequestSource::Http1,
            Version::HTTP_2 => {
                // 检查是否是 H2C（通过 Upgrade 头判断）
                if parts.headers.get("upgrade").is_some() {
                    RequestSource::H2c
                } else {
                    RequestSource::Http2
                }
            },
            _ => RequestSource::Http1,
        };

        Ok(HttpRequest {
            method: parts.method,
            uri: parts.uri,
            version: parts.version,
            headers: parts.headers,
            body: body_bytes,
            remote_addr,
            source,
        })
    }

    /// 从 H2 请求创建 HttpRequest
    pub fn from_h2_request(
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
        remote_addr: Option<SocketAddr>,
    ) -> Self {
        HttpRequest {
            method,
            uri,
            version: Version::HTTP_2,
            headers,
            body,
            remote_addr,
            source: RequestSource::Http2,
        }
    }

    /// 获取请求路径
    pub fn path(&self) -> &str {
        self.uri.path()
    }

    /// 获取查询字符串
    pub fn query(&self) -> Option<&str> {
        self.uri.query()
    }

    /// 获取查询参数
    pub fn query_params(&self) -> HashMap<String, String> {
        let mut params = HashMap::new();
        if let Some(query) = self.query() {
            for pair in query.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    params.insert(
                        urlencoding::decode(key).unwrap_or_default().to_string(),
                        urlencoding::decode(value).unwrap_or_default().to_string(),
                    );
                } else {
                    params.insert(
                        urlencoding::decode(pair).unwrap_or_default().to_string(),
                        String::new(),
                    );
                }
            }
        }
        params
    }

    /// 获取请求头值
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)?.to_str().ok()
    }

    /// 检查是否是 gRPC 请求
    pub fn is_grpc(&self) -> bool {
        // 检查 content-type 头部
        let content_type_is_grpc = self.header("content-type")
            .map(|ct| ct.starts_with("application/grpc"))
            .unwrap_or(false);

        if content_type_is_grpc {
            return true;
        }

        // HAProxy 兼容性检查：检查 TE 头部是否为 trailers
        let te_is_trailers = self.header("te")
            .map(|te| te.to_lowercase() == "trailers")
            .unwrap_or(false);

        // HAProxy 兼容性检查：检查 X-Forwarded-Proto 头部
        let proto_is_https = self.header("x-forwarded-proto")
            .map(|proto| proto.to_lowercase() == "https")
            .unwrap_or(false);

        // 如果同时有 TE: trailers 和可能的 HTTPS 标识，认为是 gRPC 请求
        if te_is_trailers {
            return true;
        }

        // 检查 User-Agent 是否包含 grpc
        self.header("user-agent")
            .map(|ua| ua.to_lowercase().contains("grpc"))
            .unwrap_or(false)
    }

    /// 检查是否是 SSE 请求
    pub fn is_sse(&self) -> bool {
        self.header("accept")
            .map(|accept| accept.contains("text/event-stream"))
            .unwrap_or(false)
    }

    /// 检查是否是 WebSocket 升级请求
    pub fn is_websocket(&self) -> bool {
        self.header("upgrade")
            .map(|upgrade| upgrade.to_lowercase() == "websocket")
            .unwrap_or(false)
    }

    /// 获取请求体大小
    pub fn body_size(&self) -> usize {
        self.body.len()
    }

    /// 将请求体转换为字符串
    pub fn body_as_string(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.to_vec())
    }

    /// 将请求体解析为 JSON
    pub fn body_as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// 获取 Content-Type
    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// 检查是否是 JSON 请求
    pub fn is_json(&self) -> bool {
        self.content_type()
            .map(|ct| ct.contains("application/json"))
            .unwrap_or(false)
    }

    /// 检查是否是表单请求
    pub fn is_form(&self) -> bool {
        self.content_type()
            .map(|ct| ct.contains("application/x-www-form-urlencoded"))
            .unwrap_or(false)
    }

    /// 检查是否是多部分表单请求
    pub fn is_multipart(&self) -> bool {
        self.content_type()
            .map(|ct| ct.contains("multipart/form-data"))
            .unwrap_or(false)
    }

    /// 获取用户代理
    pub fn user_agent(&self) -> Option<&str> {
        self.header("user-agent")
    }

    /// 获取客户端 IP（考虑代理头）
    pub fn client_ip(&self) -> Option<std::net::IpAddr> {
        // 优先从代理头获取真实 IP
        if let Some(forwarded_for) = self.header("x-forwarded-for") {
            if let Some(ip_str) = forwarded_for.split(',').next() {
                if let Ok(ip) = ip_str.trim().parse() {
                    return Some(ip);
                }
            }
        }

        if let Some(real_ip) = self.header("x-real-ip") {
            if let Ok(ip) = real_ip.parse() {
                return Some(ip);
            }
        }

        // 回退到连接地址
        self.remote_addr.map(|addr| addr.ip())
    }
}