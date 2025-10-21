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
/// 路由节点 - Radix Tree + 快速匹配混合架构
///
/// # 设计目标
///
/// 1. **Radix Tree 快速定位**：使用前缀树快速缩小搜索范围
/// 2. **快速匹配精确提取**：避免正则表达式，提供高速参数提取
/// 3. **智能优先级排序**：自动计算路由优先级，解决冲突
/// 4. **零回退机制**：完全替代正则匹配，单一架构
///
/// # Radix Tree 结构说明
///
/// - 每个节点代表一个路径段（如 "users", "123", "profile"）
/// - 下一个路径段存储在下一级路径段中
/// - 完整路由信息存储在路径段的末端节点
///
/// # 路由优先级规则
///
/// 优先级分数计算（分数越高优先级越高）：
/// 1. **静态段数量** × 1000（静态段越多优先级越高）
/// 2. **有约束参数数量** × 100（类型约束比无约束优先级高）
/// 3. **总段数** × 10（段数少的路由优先级高）
/// 4. **path参数惩罚** × 50（path参数优先级最低）
#[derive(Debug, Clone)]
pub struct RouteNode {
    /// 当前路径段
    segment: String,
    /// 下一级路径段 - 按段名索引
    next_segments: HashMap<String, RouteNode>,
    /// 终端路由信息列表（支持多个路由共存）
    route_infos: Vec<RouteInfo>,
    /// 路径段深度（根节点为0）
    depth: usize,
}

/// 路由类型
#[derive(Debug, Clone, PartialEq)]
pub enum RouteType {
    /// 标准 HTTP 路由
    Http,
    /// 流式 HTTP 路由（如 SSE）
    Streaming,
}

/// 终端路由信息
#[derive(Debug, Clone)]
pub struct RouteInfo {
    /// 原始路由模式，如 "/users/<int:id>/files/<path:file_path>"
    pattern: String,
    /// HTTP 方法
    method: Method,
    /// 路由类型
    route_type: RouteType,
    /// 路径段配置
    segments: Vec<RouteSegment>,
    /// 参数信息映射
    param_info: HashMap<String, ParamInfo>,
    /// 优先级分数（预计算）
    priority_score: u32,
    /// 是否包含path参数
    has_path_param: bool,
    /// 处理器ID（用于索引到处理器数组）
    handler_id: usize,
    /// Python处理器名字（仅用于Python集成，避免Python层二次路由匹配）
    python_handler_name: Option<String>,
}

/// 路径段类型
#[derive(Debug, Clone, PartialEq)]
pub enum RouteSegment {
    /// 静态段，如 "api", "users"
    Static(String),
    /// 参数段，包含名称和类型
    Param(String, ParamType),
}

/// 参数类型
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    /// 整数类型（默认）
    Int,
    /// 字符串类型
    Str,
    /// 浮点数类型
    Float,
    /// UUID类型
    Uuid,
    /// 路径类型（匹配剩余所有路径）
    Path,
}

/// 参数信息
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// 参数在段中的位置（0-based）
    position: usize,
    /// 参数类型
    param_type: ParamType,
    /// 是否有类型约束
    has_constraint: bool,
}

/// 路由匹配结果
#[derive(Debug, Clone)]
pub struct RouteMatch {
    /// 匹配的路由信息
    pub route_info: RouteInfo,
    /// 提取的参数
    pub params: HashMap<String, String>,
    /// 优先级分数
    pub priority_score: u32,
}

impl RouteNode {
    /// 创建新的路由节点
    pub fn new(segment: String, depth: usize) -> Self {
        RouteNode {
            segment,
            next_segments: HashMap::new(),
            route_infos: Vec::new(),
            depth,
        }
    }

    /// 添加下一级路径段
    pub fn add_next_segment(&mut self, segment: String) -> &mut RouteNode {
        if !self.next_segments.contains_key(&segment) {
            self.next_segments.insert(segment.clone(), RouteNode::new(segment.clone(), self.depth + 1));
        }
        self.next_segments.get_mut(&segment).unwrap()
    }

    /// 设置终端路由信息
    pub fn set_route_info(&mut self, route_info: RouteInfo) {
        self.route_infos.push(route_info);
    }

    /// 遍历所有路由信息
    pub fn collect_all_routes(&self) -> Vec<&RouteInfo> {
        let mut routes = Vec::new();

        // 添加当前节点的所有路由信息
        for route_info in &self.route_infos {
            routes.push(route_info);
        }

        // 递归遍历所有下一级路径段
        for next_segment in self.next_segments.values() {
            routes.extend(next_segment.collect_all_routes());
        }

        routes
    }

    /// 插入路由 - Radix Tree 构建
    pub fn insert_route(&mut self, method: Method, pattern: String, route_type: RouteType, handler_id: usize, python_handler_name: Option<String>) {
        crate::utils::logger::debug!("🔧 [RouteNode] 插入路由: {} {} {:?} (handler_id: {})", method, pattern, route_type, handler_id);

        // ⚠️ 检测潜在的路由冲突
        self.detect_potential_conflicts(&method, &pattern);

        let segments: Vec<&str> = pattern.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        let mut current_node = self;

        // 构建路径段
        let mut route_segments = Vec::new();
        let mut param_info = HashMap::new();

        for (pos, segment) in segments.iter().enumerate() {
            if segment.starts_with('<') && segment.ends_with('>') {
                // 参数段
                let param_content = &segment[1..segment.len()-1];

                if param_content.contains(':') {
                    let parts: Vec<&str> = param_content.split(':').collect();
                    if parts.len() == 2 {
                        let param_type = Self::parse_param_type(parts[0]);
                        let param_name = parts[1];

                        // path参数检查
                        if param_type == ParamType::Path && pos != segments.len() - 1 {
                            panic!(
                                "路由模式 '{}' 中的 path 参数 '{}' 不是最后一个参数！\n\
                                path 类型参数必须是路由模式中的最后一个参数。",
                                pattern, param_name
                            );
                        }

                        route_segments.push(RouteSegment::Param(param_name.to_string(), param_type.clone()));
                        param_info.insert(param_name.to_string(), ParamInfo {
                            position: pos,
                            param_type,
                            has_constraint: true,
                        });
                    } else {
                        // 格式错误，当作默认int参数
                        route_segments.push(RouteSegment::Param(param_content.to_string(), ParamType::Int));
                        param_info.insert(param_content.to_string(), ParamInfo {
                            position: pos,
                            param_type: ParamType::Int,
                            has_constraint: false,
                        });
                    }
                } else {
                    // 无类型约束，默认int
                    route_segments.push(RouteSegment::Param(param_content.to_string(), ParamType::Int));
                    param_info.insert(param_content.to_string(), ParamInfo {
                        position: pos,
                        param_type: ParamType::Int,
                        has_constraint: false,
                    });
                }
            } else {
                // 静态段
                route_segments.push(RouteSegment::Static(segment.to_string()));
            }
        }

        // 计算优先级分数
        let priority_score = Self::calculate_priority_score(&route_segments, &param_info);

        // 检查是否有path参数
        let has_path_param = param_info.values().any(|info| info.param_type == ParamType::Path);

        // 创建路由信息
        let route_info = RouteInfo {
            pattern: pattern.clone(),
            method: method.clone(),
            route_type,
            segments: route_segments.clone(),
            param_info,
            priority_score,
            has_path_param,
            handler_id,
            python_handler_name,
        };

        // 构建Radix Tree路径
        for segment in segments {
            if segment.starts_with('<') {
                // 参数段统一用"<param>"作为key
                current_node = current_node.add_next_segment("<param>".to_string());
            } else {
                // 静态段用实际值作为key
                current_node = current_node.add_next_segment(segment.to_string());
            }
        }

        // 设置终端路由信息
        current_node.set_route_info(route_info);
    }

    /// 解析参数类型
    fn parse_param_type(type_str: &str) -> ParamType {
        match type_str {
            "int" => ParamType::Int,
            "str" => ParamType::Str,
            "float" => ParamType::Float,
            "uuid" => ParamType::Uuid,
            "path" => ParamType::Path,
            _ => ParamType::Int, // 默认为int
        }
    }

    /// 计算路由优先级分数（分数越高优先级越高）
    fn calculate_priority_score(segments: &[RouteSegment], param_info: &HashMap<String, ParamInfo>) -> u32 {
        let static_count = segments.iter().filter(|s| matches!(s, RouteSegment::Static(_))).count() as u32;
        let constrained_param_count = param_info.values().filter(|info| info.has_constraint).count() as u32;
        let total_segments = segments.len() as u32;
        let has_path_param = param_info.values().any(|info| info.param_type == ParamType::Path);

        // 🎯 重新设计优先级算法，确保最具体的路由优先级最高
        let mut score = 0;

        // 1. 静态段越多优先级越高（更具体）
        score += static_count * 10000;

        // 2. 总段数越少优先级越高（路径越短越具体）
        score += (1000 - total_segments) * 1000;

        // 3. 有类型约束的参数比无约束优先级高
        score += constrained_param_count * 1000;

        // 4. path参数适度降低优先级（避免过度惩罚）
        if has_path_param {
            score -= 3000; // 大幅减少惩罚，从50000改为3000
        }

        score
    }

    /// 查找路由 - Radix Tree + 快速匹配混合算法
    pub fn find_routes(&self, method: &Method, path: &str) -> Vec<RouteMatch> {
        let mut matches = Vec::new();
        let request_segments: Vec<&str> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        self.lookup_recursive(method, &request_segments, &HashMap::new(), 0, &mut matches);

        // 按优先级分数排序（降序）
        matches.sort_by(|a, b| b.priority_score.cmp(&a.priority_score));

        matches
    }

    /// 递归查找路由
    fn lookup_recursive(
        &self,
        method: &Method,
        request_segments: &[&str],
        current_params: &HashMap<String, String>,
        segment_index: usize,
        matches: &mut Vec<RouteMatch>,
    ) {
        // 检查当前节点的所有路由信息
        for route_info in &self.route_infos {
            if route_info.method == *method {
                if let Some(params) = self.extract_params_fast(route_info, request_segments) {
                    // 计算动态优先级分数（基于实际请求内容）
                    let dynamic_score = self.calculate_dynamic_priority_score(route_info, request_segments, &params);

                    matches.push(RouteMatch {
                        route_info: route_info.clone(),
                        params,
                        priority_score: dynamic_score,
                    });
                }
            }
        }

        // 如果已经处理完所有请求段，不再继续向下查找
        if segment_index >= request_segments.len() {
            return;
        }

        let current_segment = request_segments[segment_index];

        // 1. 尝试精确匹配静态段
        if let Some(child) = self.next_segments.get(current_segment) {
            child.lookup_recursive(method, request_segments, current_params, segment_index + 1, matches);
        }

        // 2. 尝试参数段匹配
        if let Some(param_child) = self.next_segments.get("<param>") {
            param_child.lookup_recursive(method, request_segments, current_params, segment_index + 1, matches);
        }
    }

    /// 快速参数提取 - 零正则匹配
    fn extract_params_fast(&self, route_info: &RouteInfo, request_segments: &[&str]) -> Option<HashMap<String, String>> {
        let mut params = HashMap::new();

        for (i, segment) in route_info.segments.iter().enumerate() {
            match segment {
                RouteSegment::Static(expected) => {
                    // 静态段必须精确匹配
                    if i >= request_segments.len() || request_segments[i] != expected {
                        return None;
                    }
                }
                RouteSegment::Param(param_name, param_type) => {
                    // 参数段提取
                    if param_type == &ParamType::Path {
                        // path参数：提取剩余所有段
                        if i >= request_segments.len() {
                            return None;
                        }
                        let path_value = request_segments[i..].join("/");
                        params.insert(param_name.clone(), path_value);
                        break; // path参数是最后一个
                    } else {
                        // 普通参数：提取单个段并进行类型验证
                        if i >= request_segments.len() {
                            return None;
                        }
                        let segment_value = request_segments[i];

                        // 暂时关闭类型验证，专注于优先级算法优化
                        // TODO: 重新实现智能类型验证系统

                        params.insert(param_name.clone(), segment_value.to_string());
                    }
                }
            }
        }

        // 确保请求段数量匹配（除非有path参数）
        if !route_info.has_path_param && request_segments.len() != route_info.segments.len() {
            return None;
        }

        Some(params)
    }

    /// 计算动态优先级分数（基于实际请求内容）
    fn calculate_dynamic_priority_score(&self, route_info: &RouteInfo, request_segments: &[&str], params: &HashMap<String, String>) -> u32 {
        let base_score = route_info.priority_score;
        let mut bonus_score: i32 = 0;

        // 对每个参数进行类型匹配度评估
        for (param_name, param_info) in &route_info.param_info {
            if let Some(param_value) = params.get(param_name) {
                match param_info.param_type {
                    ParamType::Float => {
                        // 精细的浮点数判断逻辑：
                        let dot_count = param_value.matches('.').count();
                        if dot_count > 1 {
                            // 多个小数点，肯定是文件路径
                            bonus_score = bonus_score.saturating_sub(40000);
                        } else if dot_count == 1 {
                            // 单个小数点，尝试解析为浮点数
                            if param_value.parse::<f64>().is_ok() {
                                // 真正的浮点数，高分奖励
                                bonus_score = bonus_score.saturating_add(8000);
                            } else {
                                // 不能解析为浮点数，很可能是文件名（如 "readme.md", "docs/manual.pdf"）
                                bonus_score = bonus_score.saturating_sub(40000);
                            }
                        } else {
                            // 没有小数点，可能是整数但要求浮点数，轻微奖励但不应该超过整数路由
                            bonus_score = bonus_score.saturating_add(500);
                        }
                    }
                    ParamType::Int => {
                        // 精细的整数类型判断：
                        if param_value.contains('.') {
                            // 包含小数点，绝对不是整数
                            bonus_score = bonus_score.saturating_sub(40000);
                        } else if param_value.parse::<i64>().is_ok() {
                            // 真正的整数，高分奖励
                            bonus_score = bonus_score.saturating_add(8000);
                        } else {
                            // 不包含小数点但也不能解析为整数（可能是文件名）
                            bonus_score = bonus_score.saturating_sub(20000);
                        }
                    }
                    ParamType::Str | ParamType::Uuid => {
                        // 字符串类型参数总是匹配
                        bonus_score = bonus_score.saturating_add(1000);
                    }
                    ParamType::Path => {
                        // path参数匹配度检查
                        if param_value.contains('/') {
                            // 如果包含斜杠，说明path参数发挥优势
                            bonus_score = bonus_score.saturating_add(8000);
                        } else {
                            // 如果不包含斜杠，path参数优势不明显
                            bonus_score = bonus_score.saturating_add(1000);
                        }
                    }
                }
            }
        }

        let final_score = (base_score as i32 + bonus_score).max(0) as u32;

        final_score
    }

    /// 检测潜在的路由冲突并发出警告
    fn detect_potential_conflicts(&self, method: &Method, new_pattern: &str) {
        let new_segments: Vec<&str> = new_pattern.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        // 检查是否有path参数在中间位置（已通过编译时检查，但这里再次提醒）
        for (i, segment) in new_segments.iter().enumerate() {
            if segment.starts_with("<path:") && i != new_segments.len() - 1 {
                crate::utils::logger::warn!("⚠️ [RouteConflict] 路由 '{}' 中的 path 参数 '{}' 不是最后一个参数！", new_pattern, segment);
                crate::utils::logger::warn!("   这会导致路由匹配异常。path参数必须是路由的最后一个参数。");
                crate::utils::logger::warn!("   建议重新设计路由模式。");
            }
        }

        // 检查是否存在可能导致歧义的路由组合
        let all_routes = self.collect_all_routes();
        for existing_route in all_routes {
            if existing_route.method == *method && existing_route.pattern != new_pattern {
                if self.routes_may_conflict(&existing_route.pattern, new_pattern) {
                    crate::utils::logger::warn!("⚠️ [RouteConflict] 检测到潜在路由冲突:");
                    crate::utils::logger::warn!("   现有路由: {}", existing_route.pattern);
                    crate::utils::logger::warn!("   新增路由: {}", new_pattern);
                    crate::utils::logger::warn!("   建议:");
                    crate::utils::logger::warn!("   1. 使用更具体的路由模式");
                    crate::utils::logger::warn!("   2. 将path类型参数路由放在最后注册");
                    crate::utils::logger::warn!("   3. 考虑重构路由设计以避免冲突");
                }
            }
        }
    }

    /// 判断两个路由模式是否可能产生冲突
    fn routes_may_conflict(&self, pattern1: &str, pattern2: &str) -> bool {
        let segments1: Vec<&str> = pattern1.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        let segments2: Vec<&str> = pattern2.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        // 如果段数不同，通常不会冲突
        if segments1.len() != segments2.len() {
            return false;
        }

        // 检查是否有相似的参数结构但不同的类型约束
        let mut conflicts = 0;
        for (s1, s2) in segments1.iter().zip(segments2.iter()) {
            if s1.starts_with('<') && s2.starts_with('<') {
                // 都是参数段，检查类型约束是否不同
                let type1 = self.extract_param_type(s1);
                let type2 = self.extract_param_type(s2);

                if type1 != type2 {
                    conflicts += 1;
                }
            } else if s1 != s2 {
                // 静态段不同，不会冲突
                return false;
            }
        }

        // 如果有参数类型冲突，可能产生问题
        conflicts > 0
    }

    /// 提取参数类型
    fn extract_param_type(&self, param_str: &str) -> String {
        if param_str.contains(':') {
            // 有类型约束
            if let Some(pos) = param_str.find(':') {
                param_str[pos + 1..param_str.len() - 1].to_string()
            } else {
                "int".to_string() // 默认类型
            }
        } else {
            // 无类型约束，默认为int
            "int".to_string()
        }
    }

    /// 验证是否为有效的浮点数（包含整数）
    fn is_valid_float(value: &str) -> bool {
        value.parse::<f64>().is_ok()
    }

    /// 验证是否为有效的整数（严格模式，不接受浮点数）
    fn is_valid_int_strict(value: &str) -> bool {
        // 首先检查是否包含小数点
        if value.contains('.') {
            return false;
        }
        // 检查是否包含其他非数字字符（除了负号）
        if value.chars().any(|c| !c.is_ascii_digit() && c != '-') {
            return false;
        }
        // 然后尝试解析为整数
        value.parse::<i64>().is_ok()
    }
}

// ⚠️ 已废弃：RouteParamMapping 已被 RouteNode 完全替代
// 保留此函数仅为向后兼容，建议使用 RouteNode::insert_route
#[deprecated(note = "使用 RouteNode::insert_route 替代")]
fn create_param_mapping(_pattern: &str) -> Option<LegacyRouteParamMapping> {
    // 废弃函数，直接返回 None
    None
}

// 向后兼容的类型别名
type LegacyRouteParamMapping = (); // 空类型，实际不再使用

/// 路由键 - 简化为纯HashMap键，不包含匹配逻辑
/// 匹配逻辑完全由 RouteNode 负责
#[derive(Debug, Clone)]
pub struct RouteKey {
    method: Method,
    path: String,
}

impl RouteKey {
    pub fn new(method: Method, path: String) -> Self {
        RouteKey { method, path }
    }

    /// 获取路径
    pub fn path(&self) -> &str {
        &self.path
    }

    /// 获取方法
    pub fn method(&self) -> &Method {
        &self.method
    }

    // ⚠️ 已废弃：参数提取现在由 RouteNode 负责
    #[deprecated(note = "使用 RouteNode::find_routes 替代")]
    pub fn extract_params_fast(&self, _method: &Method, _path: &str) -> Option<HashMap<String, String>> {
        // 废弃方法，直接返回 None
        crate::utils::logger::warn!("⚠️ RouteKey::extract_params_fast 已废弃，请使用 RouteNode::find_routes");
        None
    }

    // ⚠️ 已废弃：参数映射现在由 RouteNode 负责
    #[deprecated(note = "RouteParamMapping 已被 RouteNode 替代")]
    pub fn get_param_mapping(&self) -> Option<()> {
        crate::utils::logger::warn!("⚠️ RouteKey::get_param_mapping 已废弃，请使用 RouteNode");
        None
    }

    // ⚠️ 已废弃：正则匹配已被 Radix Tree + 快速匹配替代
    #[deprecated(note = "正则匹配已被 Radix Tree + 快速匹配替代")]
    pub fn matches(&self, _method: &Method, _path: &str) -> Option<HashMap<String, String>> {
        crate::utils::logger::warn!("⚠️ RouteKey::matches 已废弃，请使用 RouteNode::find_routes");
        None
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
    // 🆕 Radix Tree 路由系统 - 统一的高性能路由核心
    route_tree: RouteNode,

    // 处理器存储 - 通过 handler_id 索引
    http_handlers: Vec<HttpAsyncHandler>,
    http_streaming_handlers: Vec<HttpStreamingHandler>,

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
            // 🆕 初始化 Radix Tree 路由系统
            route_tree: RouteNode::new("root".to_string(), 0),
            http_handlers: Vec::new(),
            http_streaming_handlers: Vec::new(),
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

    /// 将路径参数和Python处理器名字设置到请求中
    fn set_path_params_and_handler_to_request(mut req: HttpRequest, params: HashMap<String, String>, python_handler_name: Option<String>) -> HttpRequest {
        req.set_path_params(params);
        req.set_python_handler_name(python_handler_name);
        req
    }

    /// 添加标准 HTTP 路由
    pub fn add_route<H>(&mut self, method: Method, path: impl Into<String>, handler: H) -> &mut Self
    where
        H: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync + 'static,
    {
        let path_str = path.into();
        let handler_id = self.http_handlers.len(); // 分配处理器ID
        self.http_handlers.push(Arc::new(handler));

        // 🆕 使用 Radix Tree 添加路由
        use crate::server::router::RouteType;
        self.route_tree.insert_route(method.clone(), path_str.clone(), RouteType::Http, handler_id, None); // 暂时传递None，后续实现handler_name捕获

        crate::utils::logger::debug!("🔧 [Router] 添加路由: {} {} -> handler_id: {}", method, path_str, handler_id);
        self
    }

    /// 添加支持多个 HTTP 方法的路由
    pub fn add_route_with_methods<H, I>(&mut self, methods: I, path: impl Into<String>, handler: H) -> &mut Self
    where
        H: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync + 'static,
        I: IntoIterator<Item = Method>,
    {
        let path_str = path.into();
        let handler_id = self.http_handlers.len(); // 分配处理器ID
        self.http_handlers.push(Arc::new(handler));

        // 🆕 为每个方法添加路由到 Radix Tree
        use crate::server::router::RouteType;
        for method in methods {
            self.route_tree.insert_route(method, path_str.clone(), RouteType::Http, handler_id, None); // 暂时传递None，后续实现handler_name捕获
        }

        self
    }

    /// 添加流式 HTTP 路由 (🆕 基于 Radix Tree)
    pub fn add_streaming_route<H>(&mut self, method: Method, path: impl Into<String>, handler: H) -> &mut Self
    where
        H: Fn(HttpRequest, HashMap<String, String>) -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> + Send + Sync + 'static,
    {
        let path_str = path.into();
        let handler_id = self.http_streaming_handlers.len();
        self.http_streaming_handlers.push(Arc::new(handler));

        // 🆕 使用 Radix Tree 添加流式路由
        use crate::server::router::RouteType;
        self.route_tree.insert_route(method.clone(), path_str.clone(), RouteType::Streaming, handler_id, None); // 暂时传递None，后续实现handler_name捕获

        crate::utils::logger::debug!("🔧 [Router] 添加流式路由: {} {} -> handler_id: {}", method, path_str, handler_id);
        self
    }

    /// 🆕 添加带有Python处理器名称的HTTP路由 (基于 Radix Tree)
    ///
    /// 这个方法专门用于Python集成，可以传递python_handler_name来避免Python层的二次路由匹配
    pub fn add_route_with_handler_name<H>(&mut self, method: Method, path: impl Into<String>, handler: H, python_handler_name: Option<String>) -> &mut Self
    where
        H: Fn(HttpRequest) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync + 'static,
    {
        let path_str = path.into();
        let handler_id = self.http_handlers.len(); // 分配处理器ID
        self.http_handlers.push(Arc::new(handler));

        // 🆕 为每个方法添加路由到 Radix Tree，传递python_handler_name
        use crate::server::router::RouteType;
        self.route_tree.insert_route(method.clone(), path_str.clone(), RouteType::Http, handler_id, python_handler_name.clone());

        crate::utils::logger::debug!("🔧 [Router] 添加HTTP路由(带Python处理器名): {} {} -> handler_id: {}, python_handler_name: {:?}",
                                    method, path_str, handler_id, python_handler_name);
        self
    }

    /// 🆕 添加带有Python处理器名称的流式HTTP路由 (基于 Radix Tree)
    ///
    /// 这个方法专门用于Python集成，可以传递python_handler_name来避免Python层的二次路由匹配
    pub fn add_streaming_route_with_handler_name<H>(&mut self, method: Method, path: impl Into<String>, handler: H, python_handler_name: Option<String>) -> &mut Self
    where
        H: Fn(HttpRequest, HashMap<String, String>) -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> + Send + Sync + 'static,
    {
        let path_str = path.into();
        let handler_id = self.http_streaming_handlers.len();
        self.http_streaming_handlers.push(Arc::new(handler));

        // 🆕 使用 Radix Tree 添加流式路由，传递python_handler_name
        use crate::server::router::RouteType;
        self.route_tree.insert_route(method.clone(), path_str.clone(), RouteType::Streaming, handler_id, python_handler_name.clone());

        crate::utils::logger::debug!("🔧 [Router] 添加流式路由(带Python处理器名): {} {} -> handler_id: {}, python_handler_name: {:?}",
                                    method, path_str, handler_id, python_handler_name);
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
        let client_ip = req.client_ip();
        if let Ok(blacklist) = self.blacklist.read() {
            if blacklist.contains(&client_ip) {
                crate::utils::logger::warn!("🚫 [Router] IP {} 在黑名单中", client_ip);
                return Ok(self.create_error_response(StatusCode::FORBIDDEN, "Access denied"));
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

        crate::utils::logger::debug!("🔍 [Router] 开始 Radix Tree 路由匹配: {} {}", method, path);
        crate::utils::logger::debug!("🔍 [Router] 注册的HTTP处理器数量: {}", self.http_handlers.len());

        // 🆕 使用 Radix Tree 进行智能路由匹配
        let matches = self.route_tree.find_routes(&method, &path);

        if !matches.is_empty() {
            // 选择优先级最高的匹配路由
            let best_match = &matches[0]; // 已按优先级排序
            crate::utils::logger::debug!("✅ [Router] Radix Tree 匹配成功: {} {} (优先级: {}) -> 路由: {}",
                method, path, best_match.priority_score, best_match.route_info.pattern);
            crate::utils::logger::debug!("🔍 [Router] 提取的参数: {:?}", best_match.params);

            // 检查是否是流式路由（通过 route_type 字段判断）
            if best_match.route_info.route_type == RouteType::Streaming {
                // 流式路由处理
                if best_match.route_info.handler_id < self.http_streaming_handlers.len() {
                    let handler = &self.http_streaming_handlers[best_match.route_info.handler_id];
                    let req_with_params = Self::set_path_params_and_handler_to_request(req, best_match.params.clone(), best_match.route_info.python_handler_name.clone());
                    let response = handler(req_with_params, best_match.params.clone()).await?;
                    let (parts, body) = response.into_parts();
                    let boxed_body = BoxBody::new(body.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e }));
                    return Ok(Response::from_parts(parts, boxed_body));
                }
            } else {
                // 标准HTTP路由
                if best_match.route_info.handler_id < self.http_handlers.len() {
                    let handler = &self.http_handlers[best_match.route_info.handler_id];
                    let req_with_params = Self::set_path_params_and_handler_to_request(req, best_match.params.clone(), best_match.route_info.python_handler_name.clone());

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
                }
            }
        } else {
            crate::utils::logger::debug!("❌ [Router] Radix Tree 未找到匹配路由: {} {}", method, path);
        }

        // 检查 SPA 回退（避免无限递归）
        crate::utils::logger::debug!("🔍 [Router] SPA 回退检查: enabled={}, is_spa_fallback={}, path={}",
            self.spa_config.enabled, is_spa_fallback, path);
        crate::utils::logger::debug!("🔍 [Router] SPA 配置: fallback_path={:?}", self.spa_config.fallback_path);
        crate::utils::logger::debug!("🔍 [Router] should_fallback 结果: {}", self.spa_config.should_fallback(&path));

        if !is_spa_fallback && self.spa_config.should_fallback(&path) {
            if let Some(fallback_path) = &self.spa_config.fallback_path {
                crate::utils::logger::info!("🔍 [Router] 执行 SPA 回退: {} {} -> {}", method, path, fallback_path);

                // 创建新的请求，路径指向 SPA 回退路径
                let mut fallback_req = req.clone();
                fallback_req.set_path(fallback_path);

                // 递归调用路由处理，标记为 SPA 回退以避免无限递归
                return Box::pin(self.route_and_handle_internal(fallback_req, true)).await;
            }
        }

        // 未找到匹配路由，返回404
        crate::utils::logger::warn!("⚠️ [Router] 未找到匹配路由: {} {} -> 返回404", method, path);

        // 检查Accept头以决定响应格式
        let accept_header = req.header("Accept").unwrap_or("");

        Ok(self.create_error_response_with_accept(StatusCode::NOT_FOUND, "Not Found", accept_header))
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
        self.create_error_response_with_accept(status, message, "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8")
    }

    /// 根据Accept头创建适当的错误响应
    fn create_error_response_with_accept(&self, status: StatusCode, message: &str, preferred_accept: &str) -> Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>> {
        let (body, content_type) = if preferred_accept.contains("text/html") {
            // 返回HTML格式的错误页面
            let html_content = self.generate_html_error_page(status, message);
            (Full::new(Bytes::from(html_content)), "text/html; charset=utf-8")
        } else {
            // 返回JSON格式的错误信息
            let json_content = format!(r#"{{"error":"{}","code":{}}}"#, message, status.as_u16());
            (Full::new(Bytes::from(json_content)), "application/json")
        };

        let boxed_body = BoxBody::new(body.map_err(|never| -> Box<dyn std::error::Error + Send + Sync> { match never {} }));

        Response::builder()
            .status(status)
            .header("Content-Type", content_type)
            .header("server", format!("RAT-Engine/{}", env!("CARGO_PKG_VERSION")))
            .body(boxed_body)
            .unwrap()
    }

    /// 生成HTML错误页面
    fn generate_html_error_page(&self, status: StatusCode, message: &str) -> String {
        format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>错误 {} - RAT Engine</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 0;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
        }}
        .error-container {{
            text-align: center;
            background: white;
            padding: 2rem;
            border-radius: 10px;
            box-shadow: 0 10px 30px rgba(0,0,0,0.2);
            max-width: 500px;
            margin: 20px;
        }}
        .error-code {{
            font-size: 6rem;
            font-weight: bold;
            color: #e74c3c;
            margin: 0;
            line-height: 1;
        }}
        .error-message {{
            font-size: 1.5rem;
            color: #333;
            margin: 1rem 0;
        }}
        .error-description {{
            color: #666;
            margin-bottom: 2rem;
            line-height: 1.6;
        }}
        .back-button {{
            display: inline-block;
            background: #3498db;
            color: white;
            padding: 0.75rem 1.5rem;
            text-decoration: none;
            border-radius: 5px;
            transition: background-color 0.3s;
        }}
        .back-button:hover {{
            background: #2980b9;
        }}
        .engine-info {{
            margin-top: 2rem;
            font-size: 0.9rem;
            color: #999;
        }}
    </style>
</head>
<body>
    <div class="error-container">
        <div class="error-code">{}</div>
        <div class="error-message">{}</div>
        <div class="error-description">
            {}
        </div>
        <a href="/" class="back-button">返回首页</a>
        <div class="engine-info">
            Powered by RAT Engine v{}
        </div>
    </div>
</body>
</html>"#,
            status.as_u16(),
            status.as_u16(),
            message,
            self.get_error_description(status),
            env!("CARGO_PKG_VERSION")
        )
    }

    /// 获取错误状态的描述信息
    fn get_error_description(&self, status: StatusCode) -> &'static str {
        match status.as_u16() {
            404 => "抱歉，您访问的页面不存在。请检查URL是否正确，或者返回首页继续浏览。",
            500 => "服务器内部错误。我们正在处理这个问题，请稍后再试。",
            403 => "访问被拒绝。您没有权限访问此资源。",
            401 => "需要身份验证。请登录以访问此资源。",
            400 => "请求格式错误。请检查您的请求参数。",
            _ => "发生了未知错误。请稍后再试或联系管理员。"
        }
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
        
        // 从 Radix Tree 收集所有路由信息
        let all_route_infos = self.route_tree.collect_all_routes();

        for route_info in all_route_infos {
            let method_str = format!("{:?}", route_info.method);
            let route_type_str = match route_info.route_type {
                RouteType::Http => "HTTP",
                RouteType::Streaming => "STREAMING",
            };
            let display_pattern = format!("{} [{}]", route_info.pattern, route_type_str);

            routes.push((method_str, display_pattern));
        }
        

        
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