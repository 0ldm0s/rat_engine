//! 基于 rat_logger 的日志系统
//! 提供统一的日志接口和配置

use rat_logger::{Level, LevelFilter, LoggerBuilder};
use rat_logger::{FileConfig, NetworkConfig};
use rat_logger::handler::term::TermConfig;
use rat_logger::config::{FormatConfig, ColorConfig, LevelStyle};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use chrono::Local;

// 重新导出 rat_logger 的日志宏
pub use rat_logger::{error, warn, info, debug, trace, emergency, startup_log, flush_logs};

/// 日志级别映射
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => Level::Error,
            LogLevel::Warn => Level::Warn,
            LogLevel::Info => Level::Info,
            LogLevel::Debug => Level::Debug,
            LogLevel::Trace => Level::Trace,
        }
    }
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
        }
    }
}

/// 日志输出类型
#[derive(Debug, Clone)]
pub enum LogOutput {
    /// 终端输出
    Terminal,
    /// 文件输出
    File {
        log_dir: PathBuf,
        max_file_size: u64,
        max_compressed_files: u32,
    },
    /// UDP网络输出
    Udp {
        server_addr: String,
        server_port: u16,
        auth_token: String,
        app_id: String,
    },
}

/// 日志配置
#[derive(Debug, Clone)]
pub struct LogConfig {
    pub enabled: bool,
    pub level: LogLevel,
    pub output: LogOutput,
    pub use_colors: bool,
    pub use_emoji: bool,
    pub show_timestamp: bool,
    pub show_module: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            enabled: true,
            level: LogLevel::Info,
            output: LogOutput::Terminal,
            use_colors: true,
            use_emoji: true,
            show_timestamp: true,
            show_module: true,
        }
    }
}

impl LogConfig {
    /// 创建禁用日志的配置
    pub fn disabled() -> Self {
        LogConfig {
            enabled: false,
            ..Default::default()
        }
    }
    
    /// 创建文件日志配置
    pub fn file<P: Into<PathBuf>>(log_dir: P) -> Self {
        LogConfig {
            enabled: true,
            level: LogLevel::Info,
            output: LogOutput::File {
                log_dir: log_dir.into(),
                max_file_size: 10 * 1024 * 1024, // 10MB
                max_compressed_files: 5,
            },
            use_colors: false, // 文件日志不使用颜色
            use_emoji: false,  // 文件日志不使用emoji
            show_timestamp: true,
            show_module: true,
        }
    }
    
    /// 创建UDP日志配置
    pub fn udp(server_addr: String, server_port: u16, auth_token: String, app_id: String) -> Self {
        LogConfig {
            enabled: true,
            level: LogLevel::Info,
            output: LogOutput::Udp {
                server_addr,
                server_port,
                auth_token,
                app_id,
            },
            use_colors: false, // UDP日志不使用颜色
            use_emoji: false,  // UDP日志不使用emoji
            show_timestamp: true,
            show_module: true,
        }
    }
}

/// 终端彩色格式化函数
fn rat_engine_format(
    buf: &mut dyn Write,
    record: &rat_logger::config::Record
) -> std::io::Result<()> {
    let level = record.metadata.level;
    
    // RAT Engine 主题配色方案
    let (level_color, level_bg, level_icon) = match level {
        Level::Error => ("\x1b[97m", "\x1b[41m", "❌"), // 白字红底
        Level::Warn => ("\x1b[30m", "\x1b[43m", "⚠️ "), // 黑字黄底
        Level::Info => ("\x1b[97m", "\x1b[44m", "🚀"), // 白字蓝底
        Level::Debug => ("\x1b[30m", "\x1b[46m", "🔧"), // 黑字青底
        Level::Trace => ("\x1b[97m", "\x1b[45m", "🔍"), // 白字紫底
    };
    
    // 时间戳颜色
    let timestamp_color = "\x1b[90m"; // 灰色
    let message_color = "\x1b[37m";   // 亮白色
    let reset = "\x1b[0m";
    
    // 获取当前时间
    let now = Local::now();
    let timestamp = now.format("%H:%M:%S%.3f");
    
    writeln!(
        buf,
        "{}{} {}{}{:5}{} {} {}{}{}",
        timestamp_color, timestamp,        // 时间戳
        level_color, level_bg, level, reset, // 彩色级别标签
        level_icon,                        // 级别图标
        message_color, record.args, reset  // 消息内容
    )
}

/// 文件格式化函数（无颜色）
fn file_format(
    buf: &mut dyn Write,
    record: &rat_logger::config::Record
) -> std::io::Result<()> {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d %H:%M:%S%.3f");
    
    writeln!(
        buf,
        "[{}] [{}] [RAT-Engine] {}",
        timestamp,
        record.metadata.level,
        record.args
    )
}

/// UDP格式化函数（简洁格式）
fn udp_format(
    buf: &mut dyn Write,
    record: &rat_logger::config::Record
) -> std::io::Result<()> {
    let now = Local::now();
    let timestamp = now.format("%H:%M:%S%.3f");
    
    writeln!(
        buf,
        "[{}] {} {}",
        timestamp,
        record.metadata.level,
        record.args
    )
}

/// 日志器结构
pub struct Logger;

impl Logger {
    /// 初始化日志系统 - 调用者必须显式调用此方法才能启用日志
    pub fn init(config: LogConfig) -> Result<(), Box<dyn std::error::Error>> {
        // 如果日志被禁用，直接返回
        if !config.enabled {
            return Ok(());
        }

        let mut builder = LoggerBuilder::new();
        builder = builder.with_level(LevelFilter::from(config.level));

        match &config.output {
            LogOutput::Terminal => {
                // 创建 RAT Engine 风格的格式配置
                let format_config = FormatConfig {
                    timestamp_format: "%H:%M:%S%.3f".to_string(),
                    level_style: LevelStyle {
                        error: "ERROR".to_string(),
                        warn: "WARN ".to_string(),
                        info: "INFO ".to_string(),
                        debug: "DEBUG".to_string(),
                        trace: "TRACE".to_string(),
                    },
                    format_template: "{timestamp} {level} {message}".to_string(),
                };

                // 创建 RAT Engine 风格的颜色配置
                let color_config = if config.use_colors {
                    Some(ColorConfig {
                        error: "\x1b[97m\x1b[41m".to_string(), // 白字红底
                        warn: "\x1b[30m\x1b[43m".to_string(),  // 黑字黄底
                        info: "\x1b[97m\x1b[44m".to_string(),  // 白字蓝底
                        debug: "\x1b[30m\x1b[46m".to_string(), // 黑字青底
                        trace: "\x1b[97m\x1b[45m".to_string(), // 白字紫底
                        timestamp: "\x1b[90m".to_string(),    // 灰色
                        target: "\x1b[37m".to_string(),      // 亮白色
                        file: "\x1b[37m".to_string(),        // 亮白色
                        message: "\x1b[37m".to_string(),     // 亮白色
                    })
                } else {
                    None
                };

                let term_config = TermConfig {
                    enable_color: config.use_colors,
                    format: Some(format_config),
                    color: color_config,
                };
                builder = builder.add_terminal_with_config(term_config);
            }
            LogOutput::File { log_dir, max_file_size, max_compressed_files } => {
                let file_config = FileConfig {
                    log_dir: log_dir.clone(),
                    max_file_size: *max_file_size,
                    max_compressed_files: *max_compressed_files as usize,
                    compression_level: 4,
                    min_compress_threads: 2,
                    skip_server_logs: false,
                    is_raw: false,
                    compress_on_drop: false,
                    force_sync: false,
                    format: None, // 使用默认格式
                };
                builder = builder.add_file(file_config);
            }
            LogOutput::Udp { server_addr, server_port, auth_token, app_id } => {
                let network_config = NetworkConfig {
                    server_addr: server_addr.clone(),
                    server_port: *server_port,
                    auth_token: auth_token.clone(),
                    app_id: app_id.clone(),
                };
                builder = builder.add_udp(network_config);
            }
        }

        match builder.init() {
            Ok(_) => Ok(()),
            Err(e) => {
                // 如果已经初始化过了，这是正常的
                eprintln!("Logger init warning: {}", e);
                Ok(())
            }
        }
    }

    /// 使用默认配置初始化（内部使用）
    pub(crate) fn init_default() -> Result<(), Box<dyn std::error::Error>> {
        Self::init(LogConfig::default())
    }
}

/// 检查日志器是否已初始化（内部使用）
pub(crate) fn is_logger_initialized() -> bool {
    rat_logger::core::LOGGER.lock().unwrap().is_some()
}

/// 时间格式化工具方法
/// 将Duration格式化为人类可读的时间字符串，自动选择最合适的单位（微秒、毫秒、秒）
pub fn format_duration(duration: std::time::Duration) -> String {
    let micros = duration.as_micros();

    if micros < 1000 {
        format!("{}μs", micros)
    } else if micros < 1_000_000 {
        format!("{}ms", duration.as_millis())
    } else {
        format!("{}s", duration.as_secs())
    }
}

/// Python 模块专用的日志初始化函数（内部使用）
/// 确保使用正确的 RAT Engine 格式和配置
/// 注意：调用者必须显式调用此函数才能启用Python模块的日志
pub fn init_python_logger() -> Result<(), Box<dyn std::error::Error>> {
    let config = LogConfig {
        enabled: true,
        level: LogLevel::Info,
        output: LogOutput::Terminal,
        use_colors: true,
        use_emoji: true,
        show_timestamp: true,
        show_module: true,
    };
    Logger::init(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_logger_init() {
        // 测试logger初始化，允许重复初始化失败
        let result = Logger::init_default();
        // 无论成功还是失败都是可接受的，因为可能已经初始化过了
        match result {
            Ok(_) => println!("Logger initialized successfully"),
            Err(_) => println!("Logger already initialized"),
        }
    }
    
    #[test]
    fn test_log_levels() {
        let _ = Logger::init_default();
        
        error!("Test error message");
        warn!("Test warning message");
        info!("Test info message");
        debug!("Test debug message");
        trace!("Test trace message");
    }
}