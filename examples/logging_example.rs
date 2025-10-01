//! RAT Engine 日志功能示例
//! 
//! 演示如何使用不同的日志输出方式：
//! - 终端输出（默认）
//! - 文件输出
//! - UDP网络输出
//! - 禁用日志
//! 
//! 使用方法：
//! ```bash
//! cargo run --example logging_example terminal    # 终端日志输出
//! cargo run --example logging_example file       # 文件日志输出
//! cargo run --example logging_example udp        # UDP日志输出
//! cargo run --example logging_example disabled   # 禁用日志
//! cargo run --example logging_example custom     # 自定义日志配置
//! ```

use rat_engine::utils::logger::{LogConfig, LogOutput, info, warn, error};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 确保加密提供程序已安装
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();
    
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        println!("🚀 RAT Engine 日志功能示例");
        println!("请指定日志输出类型：\n");
        println!("可用选项：");
        println!("  terminal    - 终端日志输出（默认）");
        println!("  file        - 文件日志输出");
        println!("  udp         - UDP网络日志输出");
        println!("  disabled    - 禁用日志");
        println!("  custom      - 自定义日志配置\n");
        println!("示例用法：");
        println!("  cargo run --example logging_example terminal");
        println!("  cargo run --example logging_example file");
        return Ok(());
    }
    
    let log_type = &args[1];
    let config = match log_type.as_str() {
        "terminal" => {
            println!("🚀 RAT Engine 日志功能示例 - 终端输出模式");
            LogConfig::default()
        }
        "file" => {
            println!("🚀 RAT Engine 日志功能示例 - 文件输出模式");
            LogConfig::file("logs/rat_engine")
        }
        "udp" => {
            println!("🚀 RAT Engine 日志功能示例 - UDP输出模式");
            LogConfig::udp(
                "127.0.0.1".to_string(),
                54321,
                "1234567890".to_string(),
                "rat_engine_app".to_string()
            )
        }
        "disabled" => {
            println!("🚀 RAT Engine 日志功能示例 - 禁用日志模式");
            LogConfig::disabled()
        }
        "custom" => {
            println!("🚀 RAT Engine 日志功能示例 - 自定义配置模式");
            LogConfig {
                enabled: true,
                level: rat_engine::utils::logger::LogLevel::Debug,
                output: LogOutput::File {
                    log_dir: PathBuf::from("logs/custom"),
                    max_file_size: 5 * 1024 * 1024, // 5MB
                    max_compressed_files: 10,
                },
                use_colors: false,
                use_emoji: false,
                show_timestamp: true,
                show_module: true,
            }
        }
        _ => {
            println!("❌ 未知的日志类型：{}", log_type);
            println!("支持的类型：terminal, file, udp, disabled, custom");
            return Ok(());
        }
    };
    
    // 使用RatEngineBuilder初始化日志系统
    rat_engine::RatEngine::builder()
        .with_log_config(config.clone())
        .router(rat_engine::server::Router::new())
        .build()
        .map_err(|e| format!("日志系统初始化失败: {}", e))?;
    
    println!("✅ 日志系统已初始化");
    println!("📝 下面演示不同级别的日志输出：\n");
    
    // 演示不同模块的日志输出
    println!("[用户模块] 用户登录处理：");
    info!("用户 admin 登录成功");
    warn!("检测到异常登录尝试");
    error!("用户认证失败：密码错误");
    
    println!("\n[数据库模块] 数据库操作：");
    info!("数据库连接已建立");
    warn!("查询执行时间过长：2.5秒");
    error!("数据库连接失败");
    
    println!("\n[网络模块] 网络请求处理：");
    info!("收到HTTP请求：GET /api/users");
    warn!("请求大小超过限制：10MB");
    error!("网络连接超时");
    
    println!("\n✅ 日志功能演示完成！");
    
    Ok(())
}