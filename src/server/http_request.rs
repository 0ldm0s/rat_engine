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
#[derive(Debug, Clone)]
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
    /// 路径参数（由路由器填充）
    pub path_params: HashMap<String, String>,
    /// Python处理器名字（仅用于Python集成，避免Python层二次路由匹配）
    pub python_handler_name: Option<String>,
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
            path_params: HashMap::new(),
            python_handler_name: None,
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
            path_params: HashMap::new(),
            python_handler_name: None,
        }
    }

    /// 获取请求路径
    pub fn path(&self) -> &str {
        self.uri.path()
    }
    
    /// 设置请求路径（用于 SPA 回退）
    pub fn set_path(&mut self, new_path: &str) {
        // 构建新的 URI，保留原有的查询字符串
        let mut uri_parts = self.uri.clone().into_parts();
        
        // 构建新的路径和查询字符串
        let path_and_query = if let Some(query) = self.uri.query() {
            format!("{}?{}", new_path, query)
        } else {
            new_path.to_string()
        };
        
        // 解析新的路径和查询字符串
        if let Ok(new_path_and_query) = path_and_query.parse::<hyper::http::uri::PathAndQuery>() {
            uri_parts.path_and_query = Some(new_path_and_query);
            
            // 重新构建 URI
            if let Ok(new_uri) = Uri::from_parts(uri_parts) {
                self.uri = new_uri;
            }
        }
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
        self.header("content-type")
            .map(|ct| ct.starts_with("application/grpc"))
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

    /// 获取客户端真实 IP 地址
    /// 
    /// 按优先级顺序检查以下头部：
    /// 1. CF-Connecting-IP (Cloudflare)
    /// 2. Forwarded (RFC 7239)
    /// 3. X-Forwarded-For
    /// 4. X-Real-IP
    /// 5. 回退到 remote_addr
    pub fn client_ip(&self) -> std::net::IpAddr {
        self.client_ip_with_validation(true)
            .unwrap_or_else(|| "255.255.255.255".parse().expect("Invalid fallback IP"))
    }

    /// 获取客户端真实 IP 地址（可选择是否验证公网 IP）
    /// 
    /// # 参数
    /// * `validate_public_ip` - 是否只返回公网 IP（true），还是接受所有有效 IP（false）
    /// 
    /// 当 `validate_public_ip` 为 false 时，适用于内网部署场景
    pub fn client_ip_with_validation(&self, validate_public_ip: bool) -> Option<std::net::IpAddr> {
        // 1. Cloudflare 专用头（最高优先级）
        if let Some(cf_ip) = self.header("cf-connecting-ip") {
            if let Ok(ip) = cf_ip.trim().parse::<std::net::IpAddr>() {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 2. 标准 Forwarded 头（RFC 7239）
        if let Some(forwarded) = self.header("forwarded") {
            if let Some(ip) = self.parse_forwarded_header(forwarded) {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 3. X-Forwarded-For 头（最常见）
        if let Some(forwarded_for) = self.header("x-forwarded-for") {
            if let Some(ip) = self.parse_x_forwarded_for(forwarded_for) {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 4. X-Real-IP 头（Nginx 常用）
        if let Some(real_ip) = self.header("x-real-ip") {
            if let Ok(ip) = real_ip.trim().parse::<std::net::IpAddr>() {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 5. X-Client-IP 头（Apache 等）
        if let Some(client_ip) = self.header("x-client-ip") {
            if let Ok(ip) = client_ip.trim().parse::<std::net::IpAddr>() {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 6. X-Cluster-Client-IP 头（集群环境）
        if let Some(cluster_ip) = self.header("x-cluster-client-ip") {
            if let Ok(ip) = cluster_ip.trim().parse::<std::net::IpAddr>() {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 7. True-Client-IP 头（Akamai CDN）
        if let Some(true_ip) = self.header("true-client-ip") {
            if let Ok(ip) = true_ip.trim().parse::<std::net::IpAddr>() {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 8. X-Original-Forwarded-For 头（AWS ALB）
        if let Some(original_forwarded) = self.header("x-original-forwarded-for") {
            if let Some(ip) = self.parse_x_forwarded_for(original_forwarded) {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 9. X-Forwarded 头（非标准但常见）
        if let Some(x_forwarded) = self.header("x-forwarded") {
            if let Ok(ip) = x_forwarded.trim().parse::<std::net::IpAddr>() {
                if !validate_public_ip || self.is_valid_public_ip(&ip.to_string()) {
                    return Some(ip);
                }
            }
        }

        // 10. 回退到连接地址
        self.remote_addr.map(|addr| addr.ip())
    }

    /// 解析 Forwarded 头（RFC 7239）
    /// 格式: Forwarded: for=192.0.2.60;proto=http;by=203.0.113.43
    fn parse_forwarded_header(&self, forwarded: &str) -> Option<std::net::IpAddr> {
        for directive in forwarded.split(',') {
            for param in directive.split(';') {
                let param = param.trim();
                if param.starts_with("for=") {
                    let for_value = &param[4..];
                    // 移除可能的引号
                    let for_value = for_value.trim_matches('"');
                    
                    // 处理 IPv6 格式 [::1]:port 或 IPv4 格式 192.168.1.1:port
                    let ip_str = if for_value.starts_with('[') {
                        // IPv6 格式
                        if let Some(end) = for_value.find(']') {
                            &for_value[1..end]
                        } else {
                            for_value
                        }
                    } else {
                        // IPv4 格式，移除端口
                        for_value.split(':').next().unwrap_or(for_value)
                    };
                    
                    if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
                        return Some(ip);
                    }
                }
            }
        }
        None
    }

    /// 解析 X-Forwarded-For 头
    /// 格式: X-Forwarded-For: client, proxy1, proxy2
    fn parse_x_forwarded_for(&self, forwarded_for: &str) -> Option<std::net::IpAddr> {
        // 取第一个 IP（客户端 IP）
        for ip_str in forwarded_for.split(',') {
            let ip_str = ip_str.trim();
            if !ip_str.is_empty() {
                if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
                    return Some(ip);
                }
            }
        }
        None
    }

    /// 检查是否是有效的公网 IP（排除私有地址和保留地址）
    fn is_valid_public_ip(&self, ip_str: &str) -> bool {
        if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(ipv4) => {
                    // 排除私有地址和特殊地址
                    !ipv4.is_private()
                        && !ipv4.is_loopback()
                        && !ipv4.is_link_local()
                        && !ipv4.is_broadcast()
                        && !ipv4.is_multicast()
                        && ipv4.octets()[0] != 0  // 排除 0.0.0.0/8
                }
                std::net::IpAddr::V6(ipv6) => {
                    // 排除私有地址和特殊地址
                    !ipv6.is_loopback()
                        && !ipv6.is_multicast()
                        && !ipv6.is_unspecified()
                        && !ipv6.segments()[0] & 0xfe00 == 0xfc00  // 排除 fc00::/7 (ULA)
                        && !ipv6.segments()[0] & 0xffc0 == 0xfe80  // 排除 fe80::/10 (Link-local)
                }
            }
        } else {
            false
        }
    }

    // ========== 路径参数访问方法 ==========

    /// 获取路径参数（原始字符串值）
    pub fn param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(|s| s.as_str())
    }

    /// 获取路径参数作为字符串
    pub fn param_as_str(&self, name: &str) -> Option<&str> {
        self.param(name)
    }

    /// 获取路径参数作为 i64 整数
    pub fn param_as_i64(&self, name: &str) -> Option<i64> {
        self.param(name)?.parse().ok()
    }

    /// 获取路径参数作为 u64 无符号整数
    pub fn param_as_u64(&self, name: &str) -> Option<u64> {
        self.param(name)?.parse().ok()
    }

    /// 获取路径参数作为 i32 整数
    pub fn param_as_i32(&self, name: &str) -> Option<i32> {
        self.param(name)?.parse().ok()
    }

    /// 获取路径参数作为 u32 无符号整数
    pub fn param_as_u32(&self, name: &str) -> Option<u32> {
        self.param(name)?.parse().ok()
    }

    /// 获取路径参数作为 f64 浮点数
    pub fn param_as_f64(&self, name: &str) -> Option<f64> {
        self.param(name)?.parse().ok()
    }

    /// 获取路径参数作为 f32 浮点数
    pub fn param_as_f32(&self, name: &str) -> Option<f32> {
        self.param(name)?.parse().ok()
    }

    /// 获取路径参数作为 UUID（如果启用了 uuid 特性）
    #[cfg(feature = "uuid")]
    pub fn param_as_uuid(&self, name: &str) -> Option<uuid::Uuid> {
        self.param(name)?.parse().ok()
    }

    /// 设置路径参数（由路由器内部使用）
    pub(crate) fn set_path_param(&mut self, name: String, value: String) {
        self.path_params.insert(name, value);
    }

    /// 设置所有路径参数（由路由器内部使用）
    pub(crate) fn set_path_params(&mut self, params: HashMap<String, String>) {
        self.path_params = params;
    }

    /// 获取所有路径参数（只读）
    pub fn path_params(&self) -> &HashMap<String, String> {
        &self.path_params
    }

    /// 设置Python处理器名字（由路由器内部使用）
    pub(crate) fn set_python_handler_name(&mut self, handler_name: Option<String>) {
        self.python_handler_name = handler_name;
    }

    /// 获取Python处理器名字（只读）
    pub fn python_handler_name(&self) -> Option<&String> {
        self.python_handler_name.as_ref()
    }
}