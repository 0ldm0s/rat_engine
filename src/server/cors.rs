//! CORS (跨域资源共享) 配置
//!
//! 提供跨域请求支持，默认禁用，可通过配置启用

use hyper::Method;

/// CORS 配置
#[derive(Debug, Clone)]
pub struct CorsConfig {
    /// 是否启用 CORS
    pub enabled: bool,
    /// 允许的来源 (支持通配符)
    pub allowed_origins: Vec<String>,
    /// 允许的 HTTP 方法
    pub allowed_methods: Vec<Method>,
    /// 允许的请求头
    pub allowed_headers: Vec<String>,
    /// 暴露的响应头
    pub exposed_headers: Vec<String>,
    /// 是否允许携带认证信息
    pub allow_credentials: bool,
    /// 预检请求缓存时间（秒）
    pub max_age: Option<u64>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
                Method::HEAD,
                Method::PATCH,
            ],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-Requested-With".to_string(),
            ],
            exposed_headers: vec![],
            allow_credentials: false,
            max_age: Some(86400), // 24小时
        }
    }
}

impl CorsConfig {
    /// 创建新的 CORS 配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用 CORS
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// 禁用 CORS
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 设置允许的来源
    pub fn allowed_origins<I, S>(mut self, origins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_origins = origins.into_iter().map(Into::into).collect();
        self
    }

    /// 设置允许的方法
    pub fn allowed_methods<I, M>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = M>,
        M: Into<Method>,
    {
        self.allowed_methods = methods.into_iter().map(Into::into).collect();
        self
    }

    /// 设置允许的头部
    pub fn allowed_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_headers = headers.into_iter().map(Into::into).collect();
        self
    }

    /// 设置暴露的头部
    pub fn exposed_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.exposed_headers = headers.into_iter().map(Into::into).collect();
        self
    }

    /// 设置是否允许携带认证信息
    pub fn allow_credentials(mut self, allow: bool) -> Self {
        self.allow_credentials = allow;
        self
    }

    /// 设置预检请求缓存时间
    pub fn max_age(mut self, age: u64) -> Self {
        self.max_age = Some(age);
        self
    }

    /// 检查来源是否被允许
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        // 如果包含通配符，允许所有来源
        if self.allowed_origins.contains(&"*".to_string()) {
            return true;
        }

        // 检查是否在允许列表中
        if self.allowed_origins.contains(&origin.to_string()) {
            return true;
        }

        // 通配符模式匹配
        for allowed in &self.allowed_origins {
            if allowed.contains('*') {
                let pattern = allowed
                    .replace('.', r"\.")
                    .replace('*', ".*");
                if let Ok(regex) = regex::Regex::new(&format!("^{}$", pattern)) {
                    if regex.is_match(origin) {
                        return true;
                    }
                }
            }
        }

        false
    }
}