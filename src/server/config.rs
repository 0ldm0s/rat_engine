use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use crate::utils::logger::LogConfig;
use super::port_config::PortConfig;

/// SPA (单页应用) 配置
#[derive(Debug, Clone)]
pub struct SpaConfig {
    /// 是否启用 SPA 支持
    pub enabled: bool,
    /// SPA 回退路径，当路由匹配失败且请求路径不包含扩展名时，重定向到此路径
    /// 例如："/" 表示重定向到根路径
    /// 如果为 None 则不启用 SPA 回退功能
    pub fallback_path: Option<String>,
}

impl Default for SpaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fallback_path: None,
        }
    }
}

impl SpaConfig {
    /// 创建启用的 SPA 配置
    pub fn enabled(fallback_path: impl Into<String>) -> Self {
        Self {
            enabled: true,
            fallback_path: Some(fallback_path.into()),
        }
    }
    
    /// 创建禁用的 SPA 配置
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            fallback_path: None,
        }
    }
    
    /// 检查是否应该进行 SPA 回退
    pub fn should_fallback(&self, path: &str) -> bool {
        self.enabled && 
        self.fallback_path.is_some() && 
        !Self::has_file_extension(path)
    }
    
    /// 检查路径是否包含文件扩展名
    fn has_file_extension(path: &str) -> bool {
        if let Some(last_segment) = path.split('/').last() {
            last_segment.contains('.') && !last_segment.ends_with('.')
        } else {
            false
        }
    }
    
    /// 获取回退路径
    pub fn get_fallback_path(&self) -> Option<&str> {
        self.fallback_path.as_deref()
    }
}

pub fn default_config(port: u16) -> ServerConfig {
    ServerConfig {
        port_config: PortConfig::default_unified().with_port(port)
            .expect("统一端口模式下 with_port 不应该失败"),
        workers: optimal_worker_count(),
        connection_timeout: Some(std::time::Duration::from_secs(30)),
        request_timeout: Some(std::time::Duration::from_secs(10)),
        log_config: None,
        spa_config: SpaConfig::default(),
    }
}

pub fn optimal_worker_count() -> usize {
    use crate::server::performance::global_performance_manager;
    
    // 简化策略: worker 数量 = CPU 核心数
    global_performance_manager().optimal_worker_count()
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port_config: PortConfig,
    pub workers: usize,
    pub connection_timeout: Option<std::time::Duration>,
    pub request_timeout: Option<std::time::Duration>,
    pub log_config: Option<LogConfig>,
    /// SPA (单页应用) 配置
    pub spa_config: SpaConfig,
}


impl ServerConfig {
    pub fn new(addr: SocketAddr, workers: usize) -> Self {
        Self {
            port_config: PortConfig::from_socket_addr(addr),
            workers,
            connection_timeout: Some(std::time::Duration::from_secs(30)),
            request_timeout: Some(std::time::Duration::from_secs(10)),
            log_config: None,
            spa_config: SpaConfig::default(),
        }
    }
    
    /// 使用端口配置创建服务器配置
    pub fn with_port_config(port_config: PortConfig, workers: usize) -> Self {
        Self {
            port_config,
            workers,
            connection_timeout: Some(std::time::Duration::from_secs(30)),
            request_timeout: Some(std::time::Duration::from_secs(10)),
            log_config: None,
            spa_config: SpaConfig::default(),
        }
    }
    
    /// 创建分端口模式的服务器配置
    pub fn separated_ports(http_port: u16, grpc_port: u16, workers: usize) -> Result<Self, super::port_config::PortConfigError> {
        let port_config = PortConfig::separated_localhost(http_port, grpc_port)?;
        Ok(Self::with_port_config(port_config, workers))
    }
    
    // 已移除 to_server_config_data 方法，因为 ServerConfigData 已废弃
    
    pub fn with_timeouts(
        addr: SocketAddr,
        workers: usize,
        connection_timeout: Option<std::time::Duration>,
        request_timeout: Option<std::time::Duration>
    ) -> Self {
        Self {
            port_config: PortConfig::from_socket_addr(addr),
            workers,
            connection_timeout,
            request_timeout,
            log_config: None,
            spa_config: SpaConfig::default(),
        }
    }
    
    /// 设置日志配置
    pub fn with_log_config(mut self, log_config: LogConfig) -> Self {
        self.log_config = Some(log_config);
        self
    }

    /// 清除日志配置
    pub fn without_log_config(mut self) -> Self {
        self.log_config = None;
        self
    }
    
    /// 禁用日志
    pub fn disable_logging(mut self) -> Self {
        self.log_config = Some(LogConfig::disabled());
        self
    }

    /// 设置文件日志
    pub fn with_file_logging<P: Into<std::path::PathBuf>>(mut self, log_dir: P) -> Self {
        self.log_config = Some(LogConfig::file(log_dir));
        self
    }
    
    /// 设置UDP日志
    pub fn with_udp_logging(
        mut self,
        server_addr: String,
        server_port: u16,
        auth_token: String,
        app_id: String
    ) -> Self {
        self.log_config = Some(LogConfig::udp(server_addr, server_port, auth_token, app_id));
        self
    }
    
    pub fn default(port: u16) -> Self {
        default_config(port)
    }

    /// 获取主要的监听地址（HTTP 地址）
    pub fn addr(&self) -> SocketAddr {
        self.port_config.http_addr()
    }

    /// 获取 gRPC 监听地址（如果是分端口模式）
    pub fn grpc_addr(&self) -> Option<SocketAddr> {
        self.port_config.grpc_addr()
    }

    /// 是否为分端口模式
    pub fn is_separated_mode(&self) -> bool {
        self.port_config.is_separated_mode()
    }
    
    /// 设置 SPA 配置
    pub fn with_spa_config(mut self, spa_config: SpaConfig) -> Self {
        self.spa_config = spa_config;
        self
    }
    
    /// 启用 SPA 支持
    pub fn enable_spa(mut self, fallback_path: impl Into<String>) -> Self {
        self.spa_config = SpaConfig::enabled(fallback_path);
        self
    }
    
    /// 禁用 SPA 支持
    pub fn disable_spa(mut self) -> Self {
        self.spa_config = SpaConfig::disabled();
        self
    }
}

// 已移除 From trait 实现，因为 ServerConfigData 已废弃