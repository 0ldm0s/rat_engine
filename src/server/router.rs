//! 高性能路由器实现

/// 编码优先级枚举
#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum CompressionPriority {
    Brotli,     // 最高优先级
    Gzip,       // 中等优先级
    Deflate,    // 低优先级
    Identity,   // 无压缩
}

use hyper::{Request, Response, Method, StatusCode};
use serde::Serialize;
use hyper::body::Incoming;
use hyper::http;
use http_body_util::{Full, combinators::BoxBody, BodyExt};
use hyper::body::Bytes;
use crate::server::streaming::{StreamingBody, StreamingResponse, SseResponse, ChunkedResponse};
use crate::server::http_request::HttpRequest;
use crate::server::config::SpaConfig;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::future::Future;
use std::pin::Pin;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use crate::utils::ip_extractor::{IpExtractor, IpInfo};
use crate::server::config::ServerConfig;
use regex::Regex;
use crate::common::path_params::compile_pattern;
use crate::server::grpc_handler::{GrpcServiceRegistry, GrpcRequestHandler};
use crate::server::cert_manager::{CertificateManager, CertManagerConfig};
use h2::server::{Connection, SendResponse};
use h2::RecvStream;

// HTTP 处理器类型定义
pub type HttpAsyncHandler = Arc<dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync>;

pub type HttpStreamingHandler = Arc<dyn Fn(HttpRequest, HashMap<String, String>) -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> + Send + Sync>;



/// 路由参数映射信息
///
/// # 设计约束
///
/// ## 关于 path 类型参数的重要说明
///
/// 当使用 `<path:param_name>` 类型参数时：
/// - path 参数会**消耗从当前位置开始的所有后续路径段**
/// - path 参数**必须是路由模式中的最后一个参数**
/// - 不允许在 path 参数后面再有其他路径段或参数
///
/// # 有效的路由示例
///
/// ```rust
/// // ✅ 正确：path 参数作为最后一个参数
/// "/files/<path:file_path>"     // 匹配："/files/docs/readme.md" → file_path: "docs/readme.md"
/// "/api/v1/download/<path:file>" // 匹配："/api/v1/download/user/docs/report.pdf" → file: "user/docs/report.pdf"
///
/// // ✅ 正确：path 参数与其他参数组合，path 在最后
/// "/users/<user_id>/files/<path:file_path>" // 匹配："/users/123/files/docs/report.pdf"
/// ```
///
/// # 无效的路由示例
///
/// ```rust
/// // ❌ 错误：path 参数后面不能有其他路径段
/// "/files/<path:file_path>/download"  // 会导致路由匹配异常
/// "/api/<path:api_path>/version"     // 会导致路由匹配异常
///
/// // ❌ 错误：path 参数后面不能有其他参数
/// "/files/<path:file_path>/<ext>"    // 会导致路由匹配异常
/// ```
///
/// # 参数提取行为
///
/// - 普通参数：`<id>` → 匹配单个段，如 `123`
/// - path 参数：`<path:file_path>` → 匹配从当前位置到路径末尾的所有内容，包含斜杠
#[derive(Debug, Clone)]
pub struct RouteParamMapping {
    /// 路径分段（按/分割）
    segments: Vec<String>,
    /// 参数名到位置的映射（0-based）
    param_positions: HashMap<String, usize>,
    /// 参数名到类型的映射（从路径模式中解析）
    param_types: HashMap<String, String>,
}

impl RouteParamMapping {
    /// 获取参数位置映射
    pub fn get_param_positions(&self) -> &HashMap<String, usize> {
        &self.param_positions
    }

    /// 获取参数类型映射
    pub fn get_param_types(&self) -> &HashMap<String, String> {
        &self.param_types
    }
}

/// 从路由模式创建参数映射表
fn create_param_mapping(pattern: &str) -> Option<RouteParamMapping> {
    if !pattern.contains('<') {
        return None;
    }

    let mut segments = Vec::new();
    let mut param_positions = HashMap::new();
    let mut param_types = HashMap::new();

    // 按斜杠分割路径
    let path_segments: Vec<&str> = pattern.trim_start_matches('/').split('/').collect();

    // 首先检查是否有path类型参数的异常用法
    for (pos, segment) in path_segments.iter().enumerate() {
        if segment.starts_with('<') && segment.ends_with('>') {
            let param_content = &segment[1..segment.len()-1];

            if param_content.contains(':') {
                let parts: Vec<&str> = param_content.split(':').collect();
                if parts.len() == 2 && parts[0] == "path" {
                    // 检查path参数是否是最后一个段
                    if pos != path_segments.len() - 1 {
                        panic!(
                            "路由模式 '{}' 中的 path 参数 '{}' 不是最后一个参数！\n\
                            path 类型参数必须是路由模式中的最后一个参数。\n\
                            当前位置: {} (从0开始)\n\
                            路径段数: {}\n\
                            请修改路由模式，确保 path 参数是最后一个，例如：\n\
                            ✅ 正确: '/files/<path:file_path>'\n\
                            ❌ 错误: '/files/<path:file_path>/download'",
                            pattern, parts[1], pos, path_segments.len()
                        );
                    }
                }
            }
        }
    }

    for (pos, segment) in path_segments.iter().enumerate() {
        if segment.starts_with('<') && segment.ends_with('>') {
            // 这是一个参数段
            let param_content = &segment[1..segment.len()-1]; // 去掉< >

            // 解析参数类型和名称
            if param_content.contains(':') {
                let parts: Vec<&str> = param_content.split(':').collect();
                if parts.len() == 2 {
                    let param_type = parts[0];
                    let param_name = parts[1];

                    // 特殊处理path类型：path类型会消耗后续所有段
                    if param_type == "path" {
                        param_positions.insert(param_name.to_string(), pos);
                        param_types.insert(param_name.to_string(), "path".to_string());
                        segments.push(format!("<{}>", param_name));
                        // path类型是最后一个参数，直接break
                        break;
                    } else {
                        // 普通参数类型
                        param_positions.insert(param_name.to_string(), pos);
                        param_types.insert(param_name.to_string(), param_type.to_string());
                        segments.push(format!("<{}>", param_name));
                    }
                } else {
                    // 格式错误，当作普通参数处理
                    param_positions.insert(param_content.to_string(), pos);
                    param_types.insert(param_content.to_string(), "int".to_string());
                    segments.push(format!("<{}>", param_content));
                }
            } else {
                // 无类型约束的参数
                param_positions.insert(param_content.to_string(), pos);
                param_types.insert(param_content.to_string(), "int".to_string()); // 默认int类型
                segments.push(format!("<{}>", param_content));
            }
        } else {
            // 普通路径段
            segments.push(segment.to_string());
        }
    }

    if param_positions.is_empty() {
        None
    } else {
        Some(RouteParamMapping {
            segments,
            param_positions,
            param_types,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RouteKey {
    method: Method,
    path: String,
    regex: Option<Regex>,
    param_names: Vec<String>,
    /// 新增：路由参数映射表，用于快速参数定位
    param_mapping: Option<RouteParamMapping>,
}

impl RouteKey {
    /// 获取路由参数映射表
    pub fn get_param_mapping(&self) -> Option<&RouteParamMapping> {
        self.param_mapping.as_ref()
    }
    pub fn new(method: Method, path: String) -> Self {
        let (regex, param_names) = compile_pattern(&path)
            .map(|(r, p)| (Some(r), p))
            .unwrap_or_else(|| (None, Vec::new()));

        // 创建路由参数映射表
        let param_mapping = create_param_mapping(&path);

        RouteKey {
            method,
            path,
            regex,
            param_names,
            param_mapping,
        }
    }

    /// 快速参数提取（使用映射表，避免正则匹配）
    pub fn extract_params_fast(&self, method: &Method, path: &str) -> Option<HashMap<String, String>> {
        crate::utils::logger::debug!("🔍 [RouteKey] 快速参数提取: {} {} -> 模式: {}", method, path, self.path);

        if &self.method != method {
            crate::utils::logger::debug!("❌ [RouteKey] 方法不匹配: {} != {}", method, self.method);
            return None;
        }

        // 如果没有参数，直接进行字符串匹配
        if self.param_mapping.is_none() {
            let matches = self.path == path;
            crate::utils::logger::debug!("🔍 [RouteKey] 静态路由匹配: {} -> {}", self.path, matches);
            return if matches {
                Some(HashMap::new())
            } else {
                None
            };
        }

        // 使用映射表进行快速参数提取
        let mapping = self.param_mapping.as_ref().unwrap();

        // 按斜杠分割请求路径
        let request_segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        crate::utils::logger::debug!("🔍 [RouteKey] 请求路径段: {:?}", request_segments);
        crate::utils::logger::debug!("🔍 [RouteKey] 路由模式段: {:?}", mapping.segments);

        // 检查是否有path类型参数
        let has_path_param = mapping.param_types.values().any(|t| t == "path");
        crate::utils::logger::debug!("🔍 [RouteKey] 是否有path参数: {}", has_path_param);

        // 检查静态段是否匹配，同时提取参数
        let mut params = HashMap::new();
        for (i, segment) in mapping.segments.iter().enumerate() {
            crate::utils::logger::debug!("🔍 [RouteKey] 检查段[{}]: {} vs 请求: {:?}", i, segment, request_segments.get(i));

            if segment.starts_with('<') && segment.ends_with('>') {
                // 这是一个参数段
                let param_name = &segment[1..segment.len()-1];
                let param_type = mapping.param_types.get(param_name)?;
                crate::utils::logger::debug!("🔍 [RouteKey] 参数段: {} 类型: {}", param_name, param_type);

                if param_type == "path" {
                    // path类型：提取从当前位置到末尾的所有段，用斜杠拼接
                    if i >= request_segments.len() {
                        crate::utils::logger::debug!("❌ [RouteKey] path参数索引越界: {} >= {}", i, request_segments.len());
                        return None;
                    }
                    let path_value = request_segments[i..].join("/");
                    crate::utils::logger::debug!("✅ [RouteKey] path参数提取: {} = {}", param_name, path_value);
                    params.insert(param_name.to_string(), path_value);
                    break; // path参数后面不再有其他段
                } else {
                    // 普通参数：提取单个段
                    if i >= request_segments.len() {
                        crate::utils::logger::debug!("❌ [RouteKey] 参数索引越界: {} >= {}", i, request_segments.len());
                        return None;
                    }
                    let request_segment = request_segments.get(i)?;
                    crate::utils::logger::debug!("✅ [RouteKey] 普通参数提取: {} = {}", param_name, request_segment);
                    params.insert(param_name.to_string(), request_segment.to_string());
                }
            } else {
                // 这是一个静态段，必须完全匹配
                if i >= request_segments.len() {
                    crate::utils::logger::debug!("❌ [RouteKey] 静态段索引越界: {} >= {}", i, request_segments.len());
                    return None;
                }
                let request_segment = request_segments.get(i)?;
                if segment != request_segment {
                    crate::utils::logger::debug!("❌ [RouteKey] 静态段不匹配: '{}' != '{}'", segment, request_segment);
                    return None;
                }
                crate::utils::logger::debug!("✅ [RouteKey] 静态段匹配: '{}'", segment);
            }
        }

        // 对于没有path参数的路由，检查段数量是否匹配
        if !has_path_param && request_segments.len() != mapping.segments.len() {
            crate::utils::logger::debug!("❌ [RouteKey] 段数量不匹配: {} != {} (无path参数)",
                request_segments.len(), mapping.segments.len());
            return None;
        }

        crate::utils::logger::debug!("✅ [RouteKey] 快速匹配成功: {:?}", params);
        Some(params)
    }

    pub fn matches(&self, method: &Method, path: &str) -> Option<HashMap<String, String>> {
        // 优先使用快速匹配
        if let Some(params) = self.extract_params_fast(method, path) {
            return Some(params);
        }

        // fallback到正则匹配（理论上不应该用到，但保留作为保险）
        crate::utils::logger::debug!(
            "⚠️ [Router] 快速匹配失败，回退到正则匹配 - 路由: {} {}, 模式: {}",
            method,
            path,
            self.path
        );

        if &self.method != method {
            return None;
        }

        if let Some(ref regex) = self.regex {
            if let Some(captures) = regex.captures(path) {
                let mut params = HashMap::new();
                for (i, param_name) in self.param_names.iter().enumerate() {
                    if let Some(capture) = captures.get(i + 1) {
                        params.insert(param_name.clone(), capture.as_str().to_string());
                    }
                }
                Some(params)
            } else {
                None
            }
        } else {
            if self.path == path {
                Some(HashMap::new())
            } else {
                None
            }
        }
    }
}

impl std::hash::Hash for RouteKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
}

impl PartialEq for RouteKey {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl Eq for RouteKey {}

#[derive(Clone)]
pub struct Router {
    // HTTP 处理器
    http_routes: HashMap<RouteKey, HttpAsyncHandler>,
    http_streaming_routes: HashMap<RouteKey, HttpStreamingHandler>,
    
    // IP 黑名单
    blacklist: Arc<RwLock<HashSet<IpAddr>>>,
    
    // SPA 配置
    spa_config: SpaConfig,
    
    // 中间件
    compressor: Option<Arc<crate::compression::Compressor>>,
    #[cfg(feature = "cache")]
    cache_middleware: Option<Arc<crate::server::cache_middleware_impl::CacheMiddlewareImpl>>,

    
    protocol_detection_middleware: Option<Arc<crate::server::protocol_detection_middleware::ProtocolDetectionMiddleware>>,
    
    // gRPC 相关（保持不变）
    grpc_registry: Arc<RwLock<GrpcServiceRegistry>>,
    grpc_handler: Option<Arc<GrpcRequestHandler>>,
    
    // 证书管理
    cert_manager: Option<Arc<RwLock<CertificateManager>>>,
    
    // HTTP/2 支持
    h2_enabled: bool,
    h2c_enabled: bool,
}

impl Router {
    /// 创建新的路由器实例
    pub fn new() -> Self {
        let grpc_registry = Arc::new(RwLock::new(GrpcServiceRegistry::new()));
        
        Router {
            http_routes: HashMap::new(),
            http_streaming_routes: HashMap::new(),
            blacklist: Arc::new(RwLock::new(HashSet::new())),
            spa_config: SpaConfig::default(),
            compressor: None,
            #[cfg(feature = "cache")]
            cache_middleware: None,
            protocol_detection_middleware: None,
            grpc_registry: grpc_registry.clone(),
            grpc_handler: Some(Arc::new(GrpcRequestHandler::new(grpc_registry))),
            cert_manager: None,
            h2_enabled: false,
            h2c_enabled: false,
        }
    }

    /// 兼容性构造函数（已废弃，请使用 new()）
    #[deprecated(since = "0.3.0", note = "请使用 Router::new() 代替")]
    pub fn new_with_config(config: ServerConfig) -> Self {
        let mut router = Self::new();
        router.spa_config = config.spa_config;
        router
    }

    /// 将路径参数设置到请求中
    fn set_path_params_to_request(mut req: HttpRequest, params: HashMap<String, String>) -> HttpRequest {
        req.set_path_params(params);
        req
    }

    /// 添加标准 HTTP 路由
    pub fn add_route<H>(&mut self, method: Method, path: impl Into<String>, handler: H) -> &mut Self
    where
        H: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync + 'static,
    {
        let key = RouteKey::new(method, path.into());
        self.http_routes.insert(key, Arc::new(handler));
        self
    }

    /// 添加支持多个 HTTP 方法的路由
    pub fn add_route_with_methods<H, I>(&mut self, methods: I, path: impl Into<String>, handler: H) -> &mut Self
    where
        H: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync + 'static,
        I: IntoIterator<Item = Method>,
    {
        let path = path.into();
        let handler = Arc::new(handler);
        
        for method in methods {
            let key = RouteKey::new(method, path.clone());
            self.http_routes.insert(key, handler.clone());
        }
        
        self
    }

    /// 添加流式 HTTP 路由
    pub fn add_streaming_route<H>(&mut self, method: Method, path: impl Into<String>, handler: H) -> &mut Self
    where
        H: Fn(HttpRequest, HashMap<String, String>) -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> + Send + Sync + 'static,
    {
        let key = RouteKey::new(method, path.into());
        self.http_streaming_routes.insert(key, Arc::new(handler));
        self
    }



    /// 处理 HTTP 请求的主入口（通用结构体版本）
    pub async fn handle_http(&self, req: HttpRequest) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        // 检查是否是 gRPC 请求（应该不会到这里，但保险起见）
        if req.is_grpc() {
            crate::utils::logger::warn!("gRPC 请求不应该到达 HTTP 处理器");
            return Ok(self.create_error_response(StatusCode::BAD_REQUEST, "gRPC requests should be handled by HTTP/2 layer"));
        }

        self.handle_http_internal(req).await
    }

    /// 处理 Hyper Request<Incoming> 的兼容性入口（用于向后兼容）
    pub async fn handle_hyper_request(&self, req: Request<Incoming>, remote_addr: Option<SocketAddr>) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        // 转换为 HttpRequest
        let http_req = match HttpRequest::from_hyper_request(req, remote_addr).await {
            Ok(req) => req,
            Err(e) => {
                crate::utils::logger::error!("转换 HTTP 请求失败: {}", e);
                return Ok(self.create_error_response(StatusCode::BAD_REQUEST, "Invalid request"));
            }
        };

        // 调用通用入口
        self.handle_http(http_req).await
    }

    /// 内部 HTTP 请求处理逻辑
    async fn handle_http_internal(&self, req: HttpRequest) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        let method = &req.method;
        let path = req.path();

        crate::utils::logger::debug!("🔍 [Router] 处理 HTTP 请求: {} {}", method, path);

        // IP 黑名单检查
        if let Some(client_ip) = req.client_ip() {
            if let Ok(blacklist) = self.blacklist.read() {
                if blacklist.contains(&client_ip) {
                    crate::utils::logger::warn!("🚫 [Router] IP {} 在黑名单中", client_ip);
                    return Ok(self.create_error_response(StatusCode::FORBIDDEN, "Access denied"));
                }
            }
        }

        // 协议检测已在 TCP 层完成，这里不需要额外处理
        crate::utils::logger::debug!("ℹ️ [Router] 协议检测已在 TCP 层完成");

        // 路由匹配和处理
        self.route_and_handle(req).await
    }

    /// 路由匹配和处理
    async fn route_and_handle(&self, req: HttpRequest) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        self.route_and_handle_internal(req, false).await
    }

    async fn route_and_handle_internal(&self, req: HttpRequest, is_spa_fallback: bool) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        let method = req.method.clone(); // 克隆 method 避免借用问题
        let path = req.path().to_string(); // 克隆路径字符串

        crate::utils::logger::debug!("🔍 [Router] 开始路由匹配: {} {}", method, path);
        crate::utils::logger::debug!("🔍 [Router] 注册的标准路由数量: {}", self.http_routes.len());
        crate::utils::logger::debug!("🔍 [Router] 注册的流式路由数量: {}", self.http_streaming_routes.len());

        // 1. 尝试流式路由匹配
        crate::utils::logger::debug!("🔍 [Router] 尝试流式路由匹配...");
        for (route_key, handler) in &self.http_streaming_routes {
            crate::utils::logger::debug!("🔍 [Router] 检查流式路由: {} {}", route_key.method, route_key.path);
            if let Some(params) = route_key.matches(&method, &path) {
                crate::utils::logger::debug!("✅ [Router] 匹配到流式路由: {} {}, 参数: {:?}", method, path, params);
                let req_with_params = Self::set_path_params_to_request(req, params.clone());
                let response = handler(req_with_params, params).await?;
                let (parts, body) = response.into_parts();
                let boxed_body = BoxBody::new(body.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e }));
                let mut response = Response::from_parts(parts, boxed_body);
                // 注意：req 已经被消耗，这里需要一个新的请求对象用于压缩
                // 由于流式路由通常不使用压缩，我们暂时跳过压缩
                return Ok(response);
            } else {
                crate::utils::logger::debug!("❌ [Router] 流式路由不匹配: {} {}", route_key.method, route_key.path);
            }
        }

        // 2. 尝试标准路由匹配
        crate::utils::logger::debug!("🔍 [Router] 尝试标准路由匹配...");
        for (route_key, handler) in &self.http_routes {
            crate::utils::logger::debug!("🔍 [Router] 检查标准路由: {} {}", route_key.method, route_key.path);
            if let Some(params) = route_key.matches(&method, &path) {
                crate::utils::logger::debug!("✅ [Router] 匹配到标准路由: {} {}, 参数: {:?}", method, path, params);
                let req_with_params = Self::set_path_params_to_request(req, params.clone());

                // 对于GET请求，先检查缓存
                if method == hyper::Method::GET {
                    #[cfg(feature = "cache")]
                    {
                        if let Some(cached_response) = self.apply_cache(&req_with_params, &path).await {
                            crate::utils::logger::debug!("🎯 [Router] 缓存命中: GET {}", path);
                            return Ok(cached_response);
                        }
                    }

                    // 缓存未命中或无缓存功能，处理请求
                    let response = handler(req_with_params.clone()).await?;
                    let (parts, body) = response.into_parts();
                    let boxed_body = BoxBody::new(body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
                    let mut response = Response::from_parts(parts, boxed_body);

                    // 应用缓存中间件（如果启用）
                    #[cfg(feature = "cache")]
                    {
                        response = self.apply_cache_middleware(&req_with_params, response).await?;
                    }

                    // 应用压缩
                    return Ok(self.apply_compression_boxed(response, &path, &req_with_params).await?);
                }

                // 非GET请求直接处理
                let response = handler(req_with_params.clone()).await?;
                let (parts, body) = response.into_parts();
                let boxed_body = BoxBody::new(body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
                let mut response = Response::from_parts(parts, boxed_body);

                return Ok(self.apply_compression_boxed(response, &path, &req_with_params).await?);
            } else {
                crate::utils::logger::debug!("❌ [Router] 标准路由不匹配: {} {}", route_key.method, route_key.path);
            }
        }

        // 3. 尝试通配符匹配
        let wildcard_key = RouteKey::new(method.clone(), "/*".to_string());
        if let Some(handler) = self.http_routes.get(&wildcard_key) {
            crate::utils::logger::debug!("🔍 [Router] 匹配到通配符路由: /*");

            // 对于GET请求，先检查缓存
            if method == hyper::Method::GET {
                #[cfg(feature = "cache")]
                {
                    if let Some(cached_response) = self.apply_cache(&req, &path).await {
                        crate::utils::logger::debug!("🎯 [Router] 缓存命中: GET {}", path);
                        return Ok(cached_response);
                    }
                }

                // 缓存未命中或无缓存功能，处理请求
                let response = handler(req.clone()).await?;
                let (parts, body) = response.into_parts();
                let boxed_body = BoxBody::new(body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
                let mut response = Response::from_parts(parts, boxed_body);

                // 应用缓存中间件（如果启用）
                #[cfg(feature = "cache")]
                {
                    response = self.apply_cache_middleware(&req, response).await?;
                }

                // 应用压缩
                return Ok(self.apply_compression_boxed(response, &path, &req).await?);
            }
            
            // 非GET请求直接处理
            let response = handler(req.clone()).await?;
            let (parts, body) = response.into_parts();
            let boxed_body = BoxBody::new(body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
            let mut response = Response::from_parts(parts, boxed_body);
            
            return Ok(self.apply_compression_boxed(response, &path, &req).await?);
        }

        // 4. 检查 SPA 回退（避免无限递归）
        if !is_spa_fallback && self.spa_config.should_fallback(&path) {
            if let Some(fallback_path) = &self.spa_config.fallback_path {
                crate::utils::logger::debug!("🔍 [Router] SPA 回退: {} {} -> {}", method, path, fallback_path);
                
                // 创建新的请求，路径指向 SPA 回退路径
                let mut fallback_req = req.clone();
                fallback_req.set_path(fallback_path);
                
                // 递归调用路由处理，标记为 SPA 回退以避免无限递归
                return Box::pin(self.route_and_handle_internal(fallback_req, true)).await;
            }
        }
        
        // 5. 返回 404
        crate::utils::logger::debug!("🔍 [Router] 未找到匹配路由: {} {}", method, path);
        Ok(self.create_error_response(StatusCode::NOT_FOUND, "Not Found"))
    }

    /// 应用缓存
    #[cfg(feature = "cache")]
    async fn apply_cache(&self, req: &HttpRequest, path: &str) -> Option<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>> {
        crate::utils::logger::debug!("🔍 [Router] apply_cache 方法被调用");

        // 如果没有缓存中间件，直接返回None
        let cache_middleware = match &self.cache_middleware {
            Some(middleware) => {
                crate::utils::logger::debug!("🔍 [Router] 找到缓存中间件，类型: CacheMiddlewareImpl");
                middleware
            },
            None => {
                crate::utils::logger::debug!("🔍 [Router] 未找到缓存中间件");
                return None;
            },
        };

        // 只处理GET请求的缓存
        if req.method != hyper::Method::GET {
            return None;
        }

        // 获取客户端支持的编码
        let accept_encoding = req.header("accept-encoding").unwrap_or("");

        // 生成基础缓存键
        let base_cache_key = format!("GET{}", path);

        // 根据缓存中间件类型处理缓存查找
        #[cfg(feature = "cache")]
        {
            if let crate::server::cache_middleware_impl::CacheMiddlewareImpl::MultiVersion(version_manager) = &**cache_middleware {
                crate::utils::logger::debug!("🔍 [Router] 尝试多版本缓存查找: {}", base_cache_key);

                if let Some(cache_result) = version_manager.handle_cache_lookup(&base_cache_key, accept_encoding).await {
                    crate::utils::logger::debug!("🎯 [Router] 多版本缓存命中: {} -> {}", base_cache_key, cache_result.encoding);

                    let full_body = http_body_util::Full::new(cache_result.data);
                    let boxed_body = BoxBody::new(full_body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));

                    let mut response = Response::builder()
                        .status(200)
                        .header("content-type", "application/octet-stream")
                        .header("x-cache", "HIT")
                        .header("x-cache-type", "MULTI-VERSION")
                        .body(boxed_body)
                        .unwrap();

                    // 设置正确的 Content-Encoding 头部
                    if cache_result.encoding != "identity" {
                        response.headers_mut().insert("content-encoding", cache_result.encoding.parse().unwrap());
                    }

                    return Some(response);
                }
                crate::utils::logger::debug!("🎯 [Router] 多版本缓存未命中: {}", base_cache_key);
            }
        }

        None
    }

    /// 应用缓存中间件（用于写入缓存）
    #[cfg(feature = "cache")]
    async fn apply_cache_middleware(&self, req: &HttpRequest, response: Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        if let Some(cache_middleware) = &self.cache_middleware {
            // 将HttpRequest转换为hyper::Request，并保留原始头部
            let mut hyper_req = hyper::Request::builder()
                .method(req.method.clone())
                .uri(req.uri.clone());
            
            // 复制原始请求的所有头部
            for (name, value) in &req.headers {
                hyper_req = hyper_req.header(name.clone(), value.clone());
            }
            
            let hyper_req = hyper_req.body(()).unwrap();
            
            // 应用缓存中间件
            cache_middleware.process(&hyper_req, response).await
        } else {
            Ok(response)
        }
    }

    
    /// 选择最佳编码
    fn select_best_encoding(&self, accept_encoding: &str) -> &str {
        if accept_encoding.is_empty() {
            return "identity";
        }

        // 解析客户端支持的编码，按优先级排序
        let encodings: Vec<&str> = accept_encoding
            .split(',')
            .map(|s| s.trim())
            .collect();

        // 按优先级选择编码（zstd > br > gzip > deflate）
        let mut found_zstd = false;
        let mut found_br = false;
        let mut found_gzip = false;
        let mut found_deflate = false;
        let mut found_identity = false;
        
        for encoding in encodings {
            if encoding.contains("zstd") {
                found_zstd = true;
            } else if encoding.contains("br") {
                found_br = true;
            } else if encoding.contains("gzip") {
                found_gzip = true;
            } else if encoding.contains("deflate") {
                found_deflate = true;
            } else if encoding.contains("identity") {
                found_identity = true;
            }
        }
        
        // 按优先级返回
        if found_zstd {
            return "zstd";
        } else if found_br {
            return "br";
        } else if found_gzip {
            return "gzip";
        } else if found_deflate {
            return "deflate";
        } else if found_identity {
            return "identity";
        }

        // 如果没有支持的编码，返回identity
        "identity"
    }

    /// 检查响应是否已经包含正确的压缩编码
    fn is_already_properly_compressed(&self, response: &Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, accept_encoding: &str) -> bool {
        // 检查响应是否已经有Content-Encoding头
        if let Some(existing_encoding) = response.headers().get("content-encoding") {
            if let Ok(existing_encoding_str) = existing_encoding.to_str() {
                // 如果响应已经有编码，检查是否与客户端请求匹配
                if !accept_encoding.is_empty() {
                    // 选择客户端最佳编码
                    let best_encoding = self.select_best_encoding(accept_encoding);
                    
                    // 如果现有编码与最佳编码匹配，或者已经是identity，则不需要重新压缩
                    if existing_encoding_str == best_encoding || existing_encoding_str == "identity" {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 应用压缩（BoxBody 版本）
    #[cfg(feature = "compression")]
    async fn apply_compression_boxed(&self, response: Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, path: &str, req: &HttpRequest) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        if let Some(compressor) = &self.compressor {
            // 从路径中提取文件扩展名
            let file_ext = std::path::Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");

            // 从请求中获取 Accept-Encoding 头部
            let accept_encoding = req.header("accept-encoding").unwrap_or("");

            // 检查响应是否已经正确压缩
            if self.is_already_properly_compressed(&response, accept_encoding) {
                crate::utils::logger::info!("🎯 [Router] 响应已正确压缩，跳过重复压缩 - Accept-Encoding: {}, Content-Encoding: {:?}",
                    accept_encoding,
                    response.headers().get("content-encoding"));
                return Ok(response);
            }

            // 使用压缩器压缩响应，使用真实的 Accept-Encoding 头部
            compressor.compress_response(response, accept_encoding, file_ext).await
        } else {
            Ok(response)
        }
    }

    /// 应用压缩（无压缩特性时的 fallback 版本）
    #[cfg(not(feature = "compression"))]
    async fn apply_compression_boxed(&self, response: Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, _path: &str, _req: &HttpRequest) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        // 没有压缩特性，直接返回原始响应
        Ok(response)
    }

    /// 创建错误响应
    fn create_error_response(&self, status: StatusCode, message: &str) -> Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>> {
        let body = Full::new(Bytes::from(format!(r#"{{"error":"{}","code":{}}}"#, message, status.as_u16())));
        let boxed_body = BoxBody::new(body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));
        
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .header("server", format!("RAT-Engine/{}", env!("CARGO_PKG_VERSION")))
            .body(boxed_body)
            .unwrap()
    }

    // ========== gRPC 相关方法（保持不变） ==========

    /// 添加 gRPC 一元服务
    pub fn add_grpc_unary<H>(&mut self, method: impl Into<String>, handler: H) -> &mut Self
    where
        H: crate::server::grpc_handler::UnaryHandler + 'static,
    {
        if let Ok(mut registry) = self.grpc_registry.write() {
            registry.register_unary(method, handler);
        } else {
            crate::utils::logger::error!("❌ 无法获取 gRPC 注册表写锁");
        }
        self
    }

    /// 添加 gRPC 服务端流服务
    pub fn add_grpc_server_stream<H>(&mut self, method: impl Into<String>, handler: H) -> &mut Self
    where
        H: crate::server::grpc_handler::ServerStreamHandler + 'static,
    {
        if let Ok(mut registry) = self.grpc_registry.write() {
            registry.register_server_stream(method, handler);
        } else {
            crate::utils::logger::error!("❌ 无法获取 gRPC 注册表写锁");
        }
        self
    }

    /// 添加泛型 gRPC 服务端流服务（支持框架层统一序列化）
    pub fn add_grpc_typed_server_stream<H, T>(&mut self, method: impl Into<String>, handler: H) -> &mut Self
    where
        H: crate::server::grpc_handler::TypedServerStreamHandler<T> + Clone + 'static,
        T: Serialize + bincode::Encode + Send + Sync + 'static,
    {
        // 创建适配器，将泛型处理器包装为原始处理器
        let adapter = crate::server::grpc_handler::TypedServerStreamAdapter::new(handler);
        if let Ok(mut registry) = self.grpc_registry.write() {
            registry.register_server_stream(method, adapter);
        } else {
            crate::utils::logger::error!("❌ 无法获取 gRPC 注册表写锁");
        }
        self
    }

    /// 添加 gRPC 客户端流服务
    pub fn add_grpc_client_stream<H>(&mut self, method: impl Into<String>, handler: H) -> &mut Self
    where
        H: crate::server::grpc_handler::ClientStreamHandler + 'static,
    {
        if let Ok(mut registry) = self.grpc_registry.write() {
            registry.register_client_stream(method, handler);
        } else {
            crate::utils::logger::error!("❌ 无法获取 gRPC 注册表写锁");
        }
        self
    }

    /// 添加 gRPC 双向流服务
    pub fn add_grpc_bidirectional<H>(&mut self, method: impl Into<String>, handler: H) -> &mut Self
    where
        H: crate::server::grpc_handler::BidirectionalHandler + 'static,
    {
        if let Ok(mut registry) = self.grpc_registry.write() {
            registry.register_bidirectional(method, handler);
        } else {
            crate::utils::logger::error!("❌ 无法获取 gRPC 注册表写锁");
        }
        self
    }

    /// 处理 gRPC 请求
    pub async fn handle_grpc_request(
        &self,
        req: http::Request<h2::RecvStream>,
        respond: h2::server::SendResponse<bytes::Bytes>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(grpc_handler) = &self.grpc_handler {
            grpc_handler.handle_request(req, respond).await
        } else {
            Err("gRPC 处理器未初始化".into())
        }
    }

    // ========== 配置方法 ==========

    /// 启用压缩
    pub fn enable_compression(&mut self, config: crate::compression::CompressionConfig) -> &mut Self {
        self.compressor = Some(Arc::new(crate::compression::Compressor::new(config)));
        self
    }

    /// 启用缓存
    #[cfg(feature = "cache")]
    pub fn enable_cache(&mut self, cache_middleware: Arc<crate::server::cache_middleware_impl::CacheMiddlewareImpl>) -> &mut Self {
        self.cache_middleware = Some(cache_middleware);
        self
    }
    

  
    
    /// 启用协议检测
    pub fn enable_protocol_detection(&mut self, middleware: Arc<crate::server::protocol_detection_middleware::ProtocolDetectionMiddleware>) -> &mut Self {
        self.protocol_detection_middleware = Some(middleware);
        self
    }

    /// 启用 HTTP/2
    pub fn enable_h2(&mut self) -> &mut Self {
        self.h2_enabled = true;
        self
    }

    /// 启用 H2C
    pub fn enable_h2c(&mut self) -> &mut Self {
        self.h2c_enabled = true;
        self
    }

    /// 禁用 H2C
    pub fn disable_h2c(&mut self) -> &mut Self {
        self.h2c_enabled = false;
        self
    }

    /// 检查是否启用了 HTTP/2
    pub fn is_h2_enabled(&self) -> bool {
        self.h2_enabled
    }

    /// 检查是否启用了 H2C
    pub fn is_h2c_enabled(&self) -> bool {
        self.h2c_enabled
    }

    /// 添加 IP 到黑名单
    pub fn add_to_blacklist(&mut self, ip: IpAddr) -> &mut Self {
        if let Ok(mut blacklist) = self.blacklist.write() {
            blacklist.insert(ip);
        }
        self
    }

    /// 从黑名单移除 IP
    pub fn remove_from_blacklist(&mut self, ip: &IpAddr) -> &mut Self {
        if let Ok(mut blacklist) = self.blacklist.write() {
            blacklist.remove(ip);
        }
        self
    }

    /// 设置证书管理器
    pub fn set_cert_manager(&mut self, cert_manager: Arc<RwLock<CertificateManager>>) -> &mut Self {
        self.cert_manager = Some(cert_manager);
        self
    }

    /// 获取证书管理器
    pub fn get_cert_manager(&self) -> Option<Arc<RwLock<CertificateManager>>> {
        self.cert_manager.clone()
    }
    
    /// 获取证书管理器配置
    pub fn get_cert_manager_config(&self) -> Option<CertManagerConfig> {
        if let Some(cert_manager) = &self.cert_manager {
            if let Ok(cert_manager) = cert_manager.read() {
                return Some(cert_manager.get_config().clone());
            }
        }
        None
    }
    
    /// 检查路径是否在 MTLS 白名单中
    pub fn is_mtls_whitelisted(&self, path: &str) -> bool {
        if let Some(cert_manager) = &self.cert_manager {
            if let Ok(cert_manager) = cert_manager.read() {
                return cert_manager.is_mtls_whitelisted(path);
            }
        }
        false
    }
    
    /// 检查是否需要为路径强制 MTLS 认证
    pub fn requires_mtls_auth(&self, path: &str) -> bool {
        // 如果启用了 MTLS 且路径不在白名单中，则需要认证
        if let Some(cert_manager) = &self.cert_manager {
            if let Ok(cert_manager) = cert_manager.read() {
                return cert_manager.is_mtls_enabled() && !cert_manager.is_mtls_whitelisted(path);
            }
        }
        false
    }

    
    
    
    
    /// 列出所有路由
    pub fn list_routes(&self) -> Vec<(String, String)> {
        let mut routes = Vec::new();
        
        routes.extend(
            self.http_routes
                .keys()
                .map(|key| (key.method.to_string(), key.path.clone()))
        );
        
        routes.extend(
            self.http_streaming_routes
                .keys()
                .map(|key| (key.method.to_string(), key.path.clone()))
        );
        

        
        routes
    }
    
    /// 配置 SPA 支持
    pub fn with_spa_config(mut self, spa_config: crate::server::config::SpaConfig) -> Self {
        self.spa_config = spa_config;
        self
    }
    
    /// 启用 SPA 支持
    pub fn enable_spa(mut self, fallback_path: impl Into<String>) -> Self {
        self.spa_config = crate::server::config::SpaConfig::enabled(fallback_path);
        self
    }
    
    /// 禁用 SPA 支持
    pub fn disable_spa(mut self) -> Self {
        self.spa_config = crate::server::config::SpaConfig::disabled();
        self
    }

    /// 列出所有已注册的 gRPC 方法
    pub fn list_grpc_methods(&self) -> Vec<String> {
        if let Ok(registry) = self.grpc_registry.read() {
            registry.list_methods()
        } else {
            crate::utils::logger::error!("❌ 无法获取 gRPC 注册表读锁");
            Vec::new()
        }
    }
}