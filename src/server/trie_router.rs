//! Trie 树路由器实现
//! 针对动态路径参数和一次性地址优化的高性能路由匹配

use hyper::{Request, Response, Method, StatusCode};
use hyper::body::Incoming;
use http_body_util::{Full, combinators::BoxBody, BodyExt};
use hyper::body::Bytes;
use crate::server::streaming::{StreamingBody, StreamingResponse};
use std::collections::HashMap;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;

// 处理器类型定义
pub type AsyncHandler = Arc<dyn Fn(Request<Incoming>) -> Pin<Box<dyn Future<Output = Result<Response<Full<Bytes>>, hyper::Error>> + Send>> + Send + Sync>;
pub type StreamingHandler = Arc<dyn Fn(Request<Incoming>) -> Pin<Box<dyn Future<Output = Result<Response<StreamingBody>, hyper::Error>> + Send>> + Send + Sync>;
pub type MixedHandler = Arc<dyn Fn(Request<Incoming>) -> Pin<Box<dyn Future<Output = Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error>> + Send>> + Send + Sync>;

/// Trie 节点类型
#[derive(Debug, Clone)]
enum TrieNodeType {
    /// 静态路径段
    Static(String),
    /// 参数节点 (参数名, 参数类型)
    Param(String, ParamType),
    /// 通配符节点
    Wildcard,
}

/// 参数类型
#[derive(Debug, Clone, PartialEq)]
enum ParamType {
    String,  // 默认字符串类型 [^/]+
    Int,     // 整数类型 \d+
    Float,   // 浮点数类型 [\d.]+
    Path,    // 路径类型 .+
}

impl ParamType {
    fn matches(&self, segment: &str) -> bool {
        match self {
            ParamType::String => !segment.is_empty() && !segment.contains('/'),
            ParamType::Int => segment.chars().all(|c| c.is_ascii_digit()),
            ParamType::Float => {
                segment.chars().all(|c| c.is_ascii_digit() || c == '.') &&
                segment.matches('.').count() <= 1
            },
            ParamType::Path => !segment.is_empty(),
        }
    }
}

/// Trie 节点
struct TrieNode {
    /// 节点类型
    node_type: TrieNodeType,
    /// 子节点
    children: Vec<TrieNode>,
    /// 该节点对应的处理器（如果是终端节点）
    handlers: HashMap<Method, RouteHandler>,
}

/// 路由处理器枚举
enum RouteHandler {
    Standard(AsyncHandler),
    Streaming(StreamingHandler),
    Mixed(MixedHandler),
}

impl TrieNode {
    fn new(node_type: TrieNodeType) -> Self {
        Self {
            node_type,
            children: Vec::new(),
            handlers: HashMap::new(),
        }
    }

    /// 插入路由
    fn insert(&mut self, segments: &[&str], method: Method, handler: RouteHandler) {
        if segments.is_empty() {
            self.handlers.insert(method, handler);
            return;
        }

        let segment = segments[0];
        let remaining = &segments[1..];

        // 解析路径段类型
        let target_type = Self::parse_segment_type(segment);

        // 查找或创建匹配的子节点
        let child_index = self.children.iter().position(|child| {
            Self::node_types_compatible(&child.node_type, &target_type)
        });

        let child_index = match child_index {
            Some(index) => index,
            None => {
                self.children.push(TrieNode::new(target_type));
                self.children.len() - 1
            }
        };

        self.children[child_index].insert(remaining, method, handler);
    }

    /// 解析路径段类型
    fn parse_segment_type(segment: &str) -> TrieNodeType {
        if segment.starts_with('<') && segment.ends_with('>') {
            let param_def = &segment[1..segment.len()-1];
            if param_def.contains(':') {
                let parts: Vec<&str> = param_def.split(':').collect();
                if parts.len() == 2 {
                    let param_type = match parts[0] {
                        "int" => ParamType::Int,
                        "float" => ParamType::Float,
                        "path" => ParamType::Path,
                        _ => ParamType::String,
                    };
                    return TrieNodeType::Param(parts[1].to_string(), param_type);
                }
            }
            TrieNodeType::Param(param_def.to_string(), ParamType::String)
        } else if segment == "*" {
            TrieNodeType::Wildcard
        } else {
            TrieNodeType::Static(segment.to_string())
        }
    }

    /// 检查两个节点类型是否兼容
    fn node_types_compatible(existing: &TrieNodeType, new: &TrieNodeType) -> bool {
        match (existing, new) {
            (TrieNodeType::Static(a), TrieNodeType::Static(b)) => a == b,
            (TrieNodeType::Param(_, type_a), TrieNodeType::Param(_, type_b)) => type_a == type_b,
            (TrieNodeType::Wildcard, TrieNodeType::Wildcard) => true,
            _ => false,
        }
    }

    /// 匹配路由
    fn find_match(&self, segments: &[&str], method: &Method, params: &mut HashMap<String, String>) -> Option<&RouteHandler> {
        if segments.is_empty() {
            return self.handlers.get(method);
        }

        let segment = segments[0];
        let remaining = &segments[1..];

        // 按优先级匹配：静态 > 参数 > 通配符
        for child in &self.children {
            match &child.node_type {
                TrieNodeType::Static(static_segment) => {
                    if static_segment == segment {
                        if let Some(handler) = child.find_match(remaining, method, params) {
                            return Some(handler);
                        }
                    }
                },
                TrieNodeType::Param(param_name, param_type) => {
                    if param_type.matches(segment) {
                        params.insert(param_name.clone(), segment.to_string());
                        if let Some(handler) = child.find_match(remaining, method, params) {
                            return Some(handler);
                        }
                        params.remove(param_name);
                    }
                },
                TrieNodeType::Wildcard => {
                    // 通配符匹配剩余所有路径
                    if let Some(handler) = child.handlers.get(method) {
                        return Some(handler);
                    }
                },
            }
        }

        None
    }
}

/// Trie 路由器
pub struct TrieRouter {
    root: TrieNode,
}

impl TrieRouter {
    pub fn new() -> Self {
        Self {
            root: TrieNode::new(TrieNodeType::Static("/".to_string())),
        }
    }

    /// 添加标准路由
    pub fn add_route(&mut self, method: Method, path: &str, handler: AsyncHandler) {
        let segments = Self::parse_path(path);
        self.root.insert(&segments, method, RouteHandler::Standard(handler));
    }

    /// 添加流式路由
    pub fn add_streaming_route(&mut self, method: Method, path: &str, handler: StreamingHandler) {
        let segments = Self::parse_path(path);
        self.root.insert(&segments, method, RouteHandler::Streaming(handler));
    }

    /// 添加混合路由
    pub fn add_mixed_route(&mut self, method: Method, path: &str, handler: MixedHandler) {
        let segments = Self::parse_path(path);
        self.root.insert(&segments, method, RouteHandler::Mixed(handler));
    }

    /// 查找路由匹配
    pub fn find_route(&self, method: &Method, path: &str) -> Option<(HashMap<String, String>, &RouteHandler)> {
        let segments = Self::parse_path(path);
        let mut params = HashMap::new();
        
        if let Some(handler) = self.root.find_match(&segments, method, &mut params) {
            Some((params, handler))
        } else {
            None
        }
    }

    /// 解析路径为段
    fn parse_path(path: &str) -> Vec<&str> {
        path.trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect()
    }
}

impl Default for TrieRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Method;

    #[test]
    fn test_parse_segment_type() {
        assert!(matches!(
            TrieNode::parse_segment_type("users"),
            TrieNodeType::Static(ref s) if s == "users"
        ));

        assert!(matches!(
            TrieNode::parse_segment_type("<id>"),
            TrieNodeType::Param(ref name, ParamType::String) if name == "id"
        ));

        assert!(matches!(
            TrieNode::parse_segment_type("<int:id>"),
            TrieNodeType::Param(ref name, ParamType::Int) if name == "id"
        ));

        assert!(matches!(
            TrieNode::parse_segment_type("*"),
            TrieNodeType::Wildcard
        ));
    }

    #[test]
    fn test_param_type_matches() {
        assert!(ParamType::Int.matches("123"));
        assert!(!ParamType::Int.matches("abc"));
        
        assert!(ParamType::Float.matches("123.45"));
        assert!(ParamType::Float.matches("123"));
        assert!(!ParamType::Float.matches("abc"));
        
        assert!(ParamType::String.matches("hello"));
        assert!(!ParamType::String.matches("hello/world"));
        
        assert!(ParamType::Path.matches("hello/world"));
    }

    #[test]
    fn test_parse_path() {
        assert_eq!(TrieRouter::parse_path("/users/123"), vec!["users", "123"]);
        assert_eq!(TrieRouter::parse_path("/"), Vec::<&str>::new());
        assert_eq!(TrieRouter::parse_path("/api/v1/users/"), vec!["api", "v1", "users"]);
    }
}