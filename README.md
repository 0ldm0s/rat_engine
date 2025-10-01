# RAT Engine 🚀

[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)
[![Crates.io](https://img.shields.io/crates/v/rat_engine.svg)](https://crates.io/crates/rat_engine)
[![docs.rs](https://img.shields.io/docsrs/rat_engine)](https://docs.rs/rat_engine/latest/rat_engine/)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-lightgrey.svg)](https://github.com/0ldm0s/rat_engine)

高性能的 Rust HTTP 服务器引擎核心库，专注于提供高效的异步网络处理和系统优化功能。

## 📄 许可证

本项目采用 **GNU Lesser General Public License v3.0 (LGPL-3.0)** 许可证。

### LGPL-3.0 要点

- **库使用**: 您可以自由地将此库链接到您的项目中，无论是开源还是商业项目
- **修改分享**: 如果您修改了库的源代码，您需要公开这些修改
- **动态链接**: 允许与专有软件进行动态链接，不会污染您的专有代码
- **静态链接**: 如果进行静态链接，需要提供目标文件以便用户可以重新链接修改后的版本
- **专利授权**: 提供明确的专利授权保护

### 完整许可证

请查看 [LICENSE](LICENSE) 文件获取完整的许可证条款和条件。

## 特性 ✨

- 🚀 **高性能**: 基于 Tokio 和 Hyper 的异步架构
- 🔧 **硬件自适应**: 自动检测 CPU 核心数并优化线程配置
- 🛣️ **灵活路由**: 支持 HTTP 方法和路径的精确匹配
- 📊 **内置监控**: 请求日志、性能指标、健康检查
- ⚡ **工作窃取**: 高效的任务调度和负载均衡算法
- 🧠 **内存池**: 智能内存管理，减少分配开销
- ⚙️ **配置管理**: 支持 TOML/JSON 配置文件和环境变量
- 🎨 **结构化日志**: 彩色输出、emoji 支持、多级别日志
- 🧪 **全面测试**: 单元测试、集成测试、性能测试
- 🐍 **Python 绑定**: 通过 PyO3 提供 Python 接口

## 快速开始 🏃‍♂️

### 安装

```bash
# 构建项目
cargo build --release
```

### 基本使用

#### 使用构建器模式（唯一推荐方式）

```rust
use rat_engine::{RatEngine, Router, Method};
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use http_body_util::Full;
use hyper::body::Bytes;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建路由器并添加路由
    let mut router = Router::new();
    
    // 添加 Hello World 路由
    router.add_route(Method::GET, "/hello", Arc::new(|_req: Request<Incoming>| {
        Box::pin(async {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(r#"{"message":"Hello, World!"}"#)))
                .unwrap())
        })
    }));
    
    // 使用构建器创建引擎（唯一正确的入口）
    let engine = RatEngine::builder()
        .worker_threads(4)
        .router(router)
        .build()?;
    
    // 启动服务器
    engine.start("127.0.0.1".to_string(), 8080).await?;
    
    Ok(())
}
```

**重要说明**: RatEngine 结构体本身是一个空实现，所有功能必须通过 `RatEngine::builder()` 创建构建器来访问。

### 运行示例

项目提供了多个功能示例：

```bash
# 运行构建器模式示例
cargo run --example builder_pattern_example

# 运行流式处理示例
cargo run --example streaming_demo

# 运行 gRPC 综合示例
cargo run --example grpc_comprehensive_example

# 运行缓存性能测试
cargo run --example cache_compression_performance_test

# 运行 gRPC 客户端示例
cargo run --example grpc_client_bidirectional_example

# 运行 ACME 证书管理示例
cargo run --example acme_sandbox_demo
```

## 核心模块 🏗️

### 引擎模块 (Engine)

- **内存池**: 高效的内存分配和回收机制
- **工作窃取**: 智能任务调度算法，最大化 CPU 利用率
- **指标收集**: 实时性能监控和统计
- **拥塞控制**: 网络流量控制算法
- **智能传输**: 数据传输优化

### 服务器模块 (Server)

- **配置管理**: 灵活的服务器配置选项
- **性能优化**: 自动硬件检测和优化
- **路由系统**: 高效的 HTTP 路由匹配
- **流式处理**: 支持分块传输、SSE 和 JSON 流式响应
- **缓存中间件**: 多版本缓存系统
- **压缩中间件**: 内容压缩支持
- **证书管理**: TLS/MTLS 证书管理
- **gRPC 支持**: gRPC 协议处理

### 客户端模块 (Client)

- **HTTP 客户端**: 高性能 HTTP 客户端
- **gRPC 客户端**: gRPC 客户端支持
- **连接池**: 连接复用管理
- **下载管理**: 文件下载支持

### Python API 模块

- **Python 绑定**: 通过 PyO3 提供 Python 接口
- **Flask 风格 API**: 熟悉的 Web 框架接口
- **异步支持**: 完整的 async/await 支持

## 项目结构 📁

```
src/
├── lib.rs              # 库入口
├── error.rs            # 错误处理
├── compression.rs      # 压缩支持
├── cache/              # 缓存模块
├── engine/             # 核心引擎模块
│   ├── mod.rs         # RatEngine 空实现，通过 builder 访问
│   ├── memory.rs       # 内存池管理
│   ├── work_stealing.rs # 工作窃取算法
│   ├── metrics.rs      # 性能指标收集
│   ├── congestion_control.rs # 拥塞控制
│   ├── smart_transfer.rs # 智能传输
│   └── network.rs      # 网络处理
├── server/             # 服务器核心
│   ├── mod.rs
│   ├── config.rs       # 服务器配置
│   ├── router.rs       # 路由系统
│   ├── cache_middleware.rs # 缓存中间件
│   ├── cache_version_manager.rs # 缓存版本管理
│   ├── cert_manager.rs # 证书管理
│   ├── grpc_handler.rs # gRPC 处理
│   ├── streaming.rs    # 流式处理
│   └── performance.rs  # 性能管理
├── client/             # 客户端模块
│   ├── mod.rs
│   ├── http_client.rs  # HTTP 客户端
│   ├── grpc_client.rs  # gRPC 客户端
│   ├── builder.rs      # 客户端构建器
│   └── connection_pool.rs # 连接池
├── python_api/         # Python 绑定
│   ├── mod.rs
│   ├── server.rs       # Python 服务器接口
│   ├── client.rs       # Python 客户端接口
│   ├── engine_builder.rs # Python 引擎构建器
│   └── handlers.rs     # Python 处理器
└── utils/              # 工具模块
    ├── mod.rs
    ├── logger.rs       # 日志系统
    ├── sys_info.rs     # 系统信息
    └── ip_extractor.rs # IP 提取

examples/              # 示例文件
├── builder_pattern_example.rs # 构建器模式示例
├── streaming_demo.rs   # 流式处理示例
├── grpc_comprehensive_example.rs # gRPC 综合示例
├── cache_compression_performance_test.rs # 缓存性能测试
├── grpc_client_bidirectional_example.rs # gRPC 客户端示例
├── acme_sandbox_demo.rs # ACME 证书管理示例
└── dynamic_routes_demo.rs # 动态路由示例
```

## 开发指南 🛠️

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行库测试
cargo test --lib

# 运行集成测试
cargo test integration_tests

# 显示测试输出
cargo test -- --nocapture

# 运行特定模块测试
cargo test engine::memory
cargo test engine::work_stealing
cargo test server::router
```

### 代码规范

- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 检查代码质量
- 添加适当的文档注释
- 确保所有测试通过

## 性能指标 📈

⚠️ **注意**: 以下性能数据基于 **MacBook Air M1** 芯片组测试获得，仅供参考。实际性能会根据硬件配置、网络环境和使用场景有所差异。

### 测试环境
- **设备**: MacBook Air M1
- **芯片**: Apple M1 (8核CPU，8核GPU)
- **内存**: 16GB 统一内存
- **操作系统**: macOS

### 性能数据
- **吞吐量**: > 50,000 RPS
- **延迟**: < 1ms (P99)
- **内存使用**: < 50MB
- **CPU 使用**: 自适应负载均衡

### 重要说明
这些测试结果仅供参考，实际性能取决于：
- 具体的硬件配置
- 网络环境条件
- 请求类型和数据大小
- 并发连接数
- 系统负载情况