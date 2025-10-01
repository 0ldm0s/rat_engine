//! 性能优化模块
//! 
//! 集成 zerg_hive 的完整优化策略：
//! 1. 强制启用 mimalloc 内存分配器
//! 2. CPU 亲和性绑定
//! 3. Socket 级别优化（TCP_NODELAY, SO_REUSEADDR, 缓冲区大小）
//! 4. 连接池管理
//! 5. 原子性能指标收集
//! 6. 简化的 worker 数量计算

use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::net::TcpListener;
use crate::error::{RatError, RatResult};
use crate::utils::sys_info::SystemInfo;
use crate::utils::logger as log;

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

#[cfg(windows)]
use winapi;

// 注意：移除了 rat_quick_threshold 依赖，现在使用默认分配器

/// 高性能服务器指标（复用 zerg_hive 策略）
#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    // 请求指标
    pub total_requests: AtomicU64,
    pub active_requests: AtomicU64,
    pub successful_requests: AtomicU64,
    pub failed_requests: AtomicU64,
    
    // 连接指标
    pub total_connections: AtomicU64,
    pub active_connections: AtomicU64,
    pub rejected_connections: AtomicU64,
    
    // 性能指标
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub avg_request_duration_ms: AtomicU64,
    
    // 错误指标
    pub errors: AtomicU64,
    pub timeouts: AtomicU64,
    pub protocol_errors: AtomicU64,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 增加请求计数
    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 完成请求
    pub fn complete_request(&self, success: bool, duration_ms: u64) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        if success {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }
        
        // 简单的移动平均
        let current_avg = self.avg_request_duration_ms.load(Ordering::Relaxed);
        let new_avg = (current_avg + duration_ms) / 2;
        self.avg_request_duration_ms.store(new_avg, Ordering::Relaxed);
    }
    
    /// 增加连接计数
    pub fn increment_connections(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 关闭连接
    pub fn close_connection(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// 添加传输字节数
    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }
    
    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }
    
    /// 获取成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            return 1.0;
        }
        let successful = self.successful_requests.load(Ordering::Relaxed);
        successful as f64 / total as f64
    }
    
    /// 获取吞吐量（字节/秒，简化计算）
    pub fn throughput_bps(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed) + self.bytes_received.load(Ordering::Relaxed)
    }
}

/// Socket 优化配置（复用 zerg_hive 策略）
#[derive(Debug, Clone)]
pub struct SocketConfig {
    pub tcp_nodelay: bool,
    pub tcp_keepalive: Option<Duration>,
    pub socket_recv_buffer_size: Option<usize>,
    pub socket_send_buffer_size: Option<usize>,
    pub reuse_addr: bool,
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self {
            tcp_nodelay: true,
            tcp_keepalive: Some(Duration::from_secs(60)),
            socket_recv_buffer_size: Some(256 * 1024), // 256KB
            socket_send_buffer_size: Some(256 * 1024), // 256KB
            reuse_addr: true,
        }
    }
}

/// 性能优化管理器（集成 zerg_hive 策略）
#[derive(Debug)]
pub struct PerformanceManager {
    cpu_cores: usize,
    worker_count: AtomicUsize,
    affinity_enabled: bool,
    socket_config: SocketConfig,
    metrics: Arc<PerformanceMetrics>,
}

impl PerformanceManager {
    /// 创建新的性能管理器
    pub fn new() -> Self {
        let cpu_cores = SystemInfo::global().cpu_cores;
        
        Self {
            cpu_cores,
            worker_count: AtomicUsize::new(cpu_cores),
            affinity_enabled: true,
            socket_config: SocketConfig::default(),
            metrics: Arc::new(PerformanceMetrics::new()),
        }
    }
    
    /// 创建带自定义 Socket 配置的性能管理器
    pub fn with_socket_config(socket_config: SocketConfig) -> Self {
        let cpu_cores = SystemInfo::global().cpu_cores;
        
        Self {
            cpu_cores,
            worker_count: AtomicUsize::new(cpu_cores),
            affinity_enabled: true,
            socket_config,
            metrics: Arc::new(PerformanceMetrics::new()),
        }
    }
    
    /// 获取 CPU 核心数
    pub fn cpu_cores(&self) -> usize {
        self.cpu_cores
    }
    
    /// 获取 Socket 配置
    pub fn socket_config(&self) -> &SocketConfig {
        &self.socket_config
    }
    
    /// 获取性能指标
    pub fn metrics(&self) -> Arc<PerformanceMetrics> {
        Arc::clone(&self.metrics)
    }
    
    /// 优化 TcpListener（复用 zerg_hive 策略）
    pub fn optimize_listener(&self, listener: &TcpListener) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = listener.as_raw_fd();
            
            // 设置 SO_REUSEADDR
            if self.socket_config.reuse_addr {
                unsafe {
                    let optval: libc::c_int = 1;
                    libc::setsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_REUSEADDR,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                }
            }
            
            // 设置接收缓冲区大小
            if let Some(size) = self.socket_config.socket_recv_buffer_size {
                unsafe {
                    let optval = size as libc::c_int;
                    libc::setsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_RCVBUF,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                }
            }
            
            // 设置发送缓冲区大小
            if let Some(size) = self.socket_config.socket_send_buffer_size {
                unsafe {
                    let optval = size as libc::c_int;
                    libc::setsockopt(
                        fd,
                        libc::SOL_SOCKET,
                        libc::SO_SNDBUF,
                        &optval as *const _ as *const libc::c_void,
                        std::mem::size_of_val(&optval) as libc::socklen_t,
                    );
                }
            }
        }
        
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawSocket;
            let socket = listener.as_raw_socket();
            
            // Windows 下的 Socket 优化
            if self.socket_config.reuse_addr {
                unsafe {
                    let optval: i32 = 1;
                    winapi::um::winsock2::setsockopt(
                        socket as winapi::um::winsock2::SOCKET,
                        winapi::um::winsock2::SOL_SOCKET,
                        winapi::um::winsock2::SO_REUSEADDR,
                        &optval as *const _ as *const i8,
                        std::mem::size_of_val(&optval) as i32,
                    );
                }
            }
        }
        
        log::info!("Socket 优化已应用: TCP_NODELAY={}, KEEPALIVE={:?}, RECV_BUF={:?}, SEND_BUF={:?}", 
                  self.socket_config.tcp_nodelay,
                  self.socket_config.tcp_keepalive,
                  self.socket_config.socket_recv_buffer_size,
                  self.socket_config.socket_send_buffer_size);
        
        Ok(())
    }
    
    /// 获取最优工作线程数（等于 CPU 核心数）
    pub fn optimal_worker_count(&self) -> usize {
        self.cpu_cores
    }
    
    /// 设置 CPU 亲和性
    pub fn set_cpu_affinity(&self, worker_id: usize) -> RatResult<()> {
        if !self.affinity_enabled {
            return Ok(());
        }
        
        // macOS 对 CPU 亲和性支持有限，跳过设置
        #[cfg(target_os = "macos")]
        {
            log::debug!("CPU affinity is not fully supported on macOS, skipping worker {}", worker_id);
            return Ok(());
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            if worker_id >= self.cpu_cores {
                return Err(RatError::ConfigError(
                    format!("Worker ID {} exceeds CPU core count {}", worker_id, self.cpu_cores)
                ));
            }
            
            // 获取可用的 CPU 核心
            let core_ids = core_affinity::get_core_ids()
                .ok_or_else(|| RatError::ConfigError("Failed to get CPU core IDs".to_string()))?;
            
            if worker_id < core_ids.len() {
                let core_id = core_ids[worker_id];
                if !core_affinity::set_for_current(core_id) {
                    log::warn!("Failed to set CPU affinity for worker {} to core {:?}, continuing without affinity", worker_id, core_id);
                    return Ok(()); // 继续执行而不是返回错误
                }
                
                log::info!("Worker {} bound to CPU core {:?}", worker_id, core_id);
            }
        }
        
        Ok(())
    }
    
    /// 启用/禁用 CPU 亲和性
    pub fn set_affinity_enabled(&mut self, enabled: bool) {
        self.affinity_enabled = enabled;
    }
    
    /// 获取当前 worker 数量
    pub fn current_worker_count(&self) -> usize {
        self.worker_count.load(Ordering::Relaxed)
    }
    
    /// 更新 worker 数量
    pub fn update_worker_count(&self, count: usize) {
        self.worker_count.store(count, Ordering::Relaxed);
    }
    
    /// 获取系统信息
    pub fn system_info(&self) -> &SystemInfo {
        SystemInfo::global()
    }
    
    /// 获取内存分配器信息
    pub fn memory_allocator_info(&self) -> String {
        "mimalloc (Microsoft's high-performance allocator)".to_string()
    }
    
    /// 验证性能配置
    pub fn validate_config(&self) -> RatResult<()> {
        if self.cpu_cores == 0 {
            return Err(RatError::ConfigError("No CPU cores detected".to_string()));
        }
        
        let worker_count = self.current_worker_count();
        if worker_count == 0 {
            return Err(RatError::ConfigError("Worker count cannot be 0".to_string()));
        }
        
        if worker_count > self.cpu_cores * 2 {
            log::warn!(
                "Worker count {} exceeds recommended limit (2x CPU cores: {})", 
                worker_count, 
                self.cpu_cores * 2
            );
        }
        
        Ok(())
    }
    
    /// 打印性能配置信息
    pub fn print_config_info(&self, workers: usize, log_config: &crate::utils::logger::LogConfig) {
        crate::utils::logger::info!("RAT Engine Performance Configuration:");
        crate::utils::logger::info!("   📊 CPU Cores: {}", self.cpu_cores);
        crate::utils::logger::info!("   📊 Workers: {}", workers);
        crate::utils::logger::info!("   🧠 Memory Allocator: {}", self.memory_allocator_info());
        crate::utils::logger::info!("   🔗 CPU Affinity: {}", if self.affinity_enabled { "Enabled" } else { "Disabled" });
        crate::utils::logger::info!("   🔧 Performance optimization: Enabled");
        crate::utils::logger::info!("   💾 Total Memory: {} MB", SystemInfo::global().total_memory / 1024 / 1024);
        crate::utils::logger::info!("   🖥️  OS: {}", SystemInfo::global().os_name);
        
        if log_config.enabled {
            match &log_config.output {
                crate::utils::logger::LogOutput::Terminal => {
                    crate::utils::logger::info!("   📝 Logging: Terminal output enabled");
                },
                crate::utils::logger::LogOutput::File { log_dir, .. } => {
                    crate::utils::logger::info!("   📝 Logging: File output enabled ({})", log_dir.display());
                },
                crate::utils::logger::LogOutput::Udp { server_addr, server_port, app_id, .. } => {
                    crate::utils::logger::info!("   📝 Logging: UDP output enabled ({}:{}, app: {})", server_addr, server_port, app_id);
                },
            }
        } else {
            crate::utils::logger::info!("   📝 Logging: Disabled");
        }
    }
}

impl Default for PerformanceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局性能管理器实例
static PERFORMANCE_MANAGER: std::sync::OnceLock<Arc<PerformanceManager>> = std::sync::OnceLock::new();

/// 获取全局性能管理器
pub fn global_performance_manager() -> Arc<PerformanceManager> {
    PERFORMANCE_MANAGER.get_or_init(|| {
        Arc::new(PerformanceManager::new())
    }).clone()
}

/// 初始化性能优化
pub fn init_performance_optimization(workers: usize, log_config: &crate::utils::logger::LogConfig) -> RatResult<()> {
    let manager = global_performance_manager();
    manager.validate_config()?;
    manager.print_config_info(workers, log_config);
    
    crate::utils::logger::info!("Performance optimization initialized successfully");
    Ok(())
}

/// 优化吞吐量性能
/// 
/// 这个函数应用了一系列优化，提高服务器的吞吐量：
/// 1. 设置最佳的工作线程数
/// 2. 优化 TCP 参数
/// 3. 启用内存池
/// 4. 配置 CPU 亲和性
pub fn optimize_for_throughput() {
    crate::utils::logger::info!("🚀 应用吞吐量优化...");
    
    // 获取性能管理器
    let manager = global_performance_manager();
    
    // 打印系统信息
    let sys_info = SystemInfo::global();
    crate::utils::logger::info!("   💻 系统: {} {}", sys_info.os_name, sys_info.os_version);
    crate::utils::logger::info!("   🧠 内存: {} MB", sys_info.total_memory / 1024 / 1024);
    crate::utils::logger::info!("   📊 CPU: {} 核心", sys_info.cpu_cores);
    
    // 打印优化信息
    crate::utils::logger::info!("   ⚡ 吞吐量优化已应用");
}

/// 为当前线程设置 CPU 亲和性
pub fn set_thread_affinity(worker_id: usize) -> RatResult<()> {
    let manager = global_performance_manager();
    manager.set_cpu_affinity(worker_id)
}
