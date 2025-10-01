# RAT Engine 日志系统使用指南

RAT Engine 集成了 `zerg_creep` 日志库，提供了灵活的日志输出配置选项。

## 功能特性

- 🎨 **多种输出方式**: 终端、文件、UDP网络输出
- 🎛️ **灵活配置**: 可启用/禁用日志，自定义日志级别
- 🌈 **彩色终端**: 终端输出支持彩色显示和emoji图标
- 📁 **文件轮转**: 文件输出支持自动轮转和压缩
- 🌐 **网络日志**: UDP输出支持远程日志收集
- ⚡ **高性能**: 基于 `zerg_creep` 的高性能日志库

## 快速开始

### 1. 默认终端输出

```rust
use rat_engine::server::ServerConfig;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

let config = ServerConfig::new(
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
    4
); // 默认使用终端彩色输出
```

### 2. 文件日志输出

```rust
let config = ServerConfig::new(
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
    4
).with_file_logging("logs/rat_engine");
```

### 3. UDP网络日志输出

```rust
let config = ServerConfig::new(
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
    4
).with_udp_logging(
    "127.0.0.1".to_string(),
    54321,
    "your_auth_token".to_string(),
    "your_app_id".to_string()
);
```

### 4. 禁用日志

```rust
let config = ServerConfig::new(
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
    4
).disable_logging();
```

## 高级配置

### 自定义日志配置

```rust
use rat_engine::utils::logger::{LogConfig, LogOutput, LogLevel};
use std::path::PathBuf;

let custom_log_config = LogConfig {
    enabled: true,
    level: LogLevel::Debug,
    output: LogOutput::File {
        log_dir: PathBuf::from("logs/custom"),
        max_file_size: 5 * 1024 * 1024, // 5MB
        max_compressed_files: 10,
    },
    use_colors: false,
    use_emoji: false,
    show_timestamp: true,
    show_module: true,
};

let config = ServerConfig::new(
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
    4
).with_log_config(custom_log_config);
```

### 日志级别

支持以下日志级别（从低到高）：

- `LogLevel::Trace` - 最详细的调试信息
- `LogLevel::Debug` - 调试信息
- `LogLevel::Info` - 一般信息（默认）
- `LogLevel::Warn` - 警告信息
- `LogLevel::Error` - 错误信息

### 输出类型详解

#### 1. 终端输出 (Terminal)

- **特点**: 彩色显示，支持emoji图标
- **适用场景**: 开发调试、本地运行
- **格式**: `[时间] [级别] [图标] 消息内容`

#### 2. 文件输出 (File)

- **特点**: 支持文件轮转和压缩
- **适用场景**: 生产环境、日志归档
- **格式**: `[YYYY-MM-DD HH:MM:SS.fff] [级别] [RAT-Engine] 消息内容`
- **配置选项**:
  - `log_dir`: 日志文件目录
  - `max_file_size`: 单个文件最大大小（字节）
  - `max_compressed_files`: 保留的压缩文件数量

#### 3. UDP网络输出 (UDP)

- **特点**: 远程日志收集，实时传输
- **适用场景**: 分布式系统、集中式日志管理
- **格式**: `[HH:MM:SS.fff] 级别 消息内容`
- **配置选项**:
  - `server_addr`: 日志服务器地址
  - `server_port`: 日志服务器端口
  - `auth_token`: 认证令牌
  - `app_id`: 应用标识

## 在代码中使用日志

```rust
use rat_engine::utils::logger::{info, warn, error, debug, trace};

// 在路由处理器中使用
router.get("/api/users", |req| async {
    info!("Processing user request from {}", req.remote_addr());
    
    match get_users().await {
        Ok(users) => {
            debug!("Found {} users", users.len());
            Ok(serde_json::to_string(&users)?)
        },
        Err(e) => {
            error!("Failed to get users: {}", e);
            Err(e)
        }
    }
});
```

## 示例程序

运行示例程序来测试不同的日志配置：

```bash
# 终端输出（默认）
cargo run --example logging_example 1

# 文件输出
cargo run --example logging_example 2

# UDP输出（需要先启动UDP日志服务器）
cargo run --example logging_example 3

# 禁用日志
cargo run --example logging_example 4

# 自定义配置
cargo run --example logging_example 5
```

## UDP日志服务器

如果要使用UDP日志输出，可以参考 `zerg_creep/examples/dual_handler_server.rs` 启动一个UDP日志服务器：

```bash
cd ../zerg_creep
cargo run --example dual_handler_server
```

## 性能考虑

1. **日志级别**: 生产环境建议使用 `Info` 或更高级别
2. **文件输出**: 合理设置文件大小和轮转策略
3. **UDP输出**: 网络延迟可能影响性能，建议在本地网络使用
4. **禁用日志**: 对性能要求极高的场景可以完全禁用日志

## 最佳实践

1. **开发环境**: 使用终端输出，级别设为 `Debug`
2. **测试环境**: 使用文件输出，级别设为 `Info`
3. **生产环境**: 使用文件输出或UDP输出，级别设为 `Warn` 或 `Error`
4. **错误处理**: 重要的错误信息使用 `error!` 宏
5. **性能监控**: 关键操作使用 `info!` 记录执行时间
6. **调试信息**: 详细的调试信息使用 `debug!` 或 `trace!`

## 故障排除

### 常见问题

1. **日志不显示**
   - 检查日志级别设置
   - 确认日志已启用 (`enabled: true`)

2. **文件日志写入失败**
   - 检查目录权限
   - 确认磁盘空间充足

3. **UDP日志发送失败**
   - 检查网络连接
   - 确认服务器地址和端口正确
   - 验证认证令牌

### 调试技巧

```rust
// 临时启用详细日志
let debug_config = LogConfig {
    enabled: true,
    level: LogLevel::Trace,
    output: LogOutput::Terminal,
    ..Default::default()
};
```

## 相关资源

- [zerg_creep 文档](../zerg_creep/README.md)
- [zerg_creep 示例](../zerg_creep/examples/)
- [RAT Engine 示例](../examples/logging_example.rs)