# RAT Engine 🚀

[![License: LGPL v3](https://img.shields.io/badge/License-LGPL%20v3-blue.svg)](https://www.gnu.org/licenses/lgpl-3.0)
[![Crates.io](https://img.shields.io/crates/v/rat_engine.svg)](https://crates.io/crates/rat_engine)
[![docs.rs](https://img.shields.io/docsrs/rat_engine)](https://docs.rs/rat_engine/latest/rat_engine/)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://rust-lang.org)
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
- 🛣️ **灵活路由**: 支持 HTTP 方法和路径的精确匹配，**自动路径参数提取**
- 🔄 **HEAD 回退**: 自动将 HEAD 请求回退到 GET 处理器（可配置白名单）
- 📡 **SSE 支持**: 全局 Server-Sent Events 管理器，支持实时通信和连接管理
- 📊 **内置监控**: 请求日志、性能指标、健康检查
- ⚡ **工作窃取**: 高效的任务调度和负载均衡算法
- 🧠 **内存池**: 智能内存管理，减少分配开销
- ⚙️ **配置管理**: 支持 TOML/JSON 配置文件和环境变量
- 🎨 **结构化日志**: 彩色输出、emoji 支持、多级别日志
- 🧪 **全面测试**: 单元测试、集成测试、性能测试
- 🐍 **Python 绑定**: 通过 PyO3 提供 Python 接口

## 快速开始 🏃‍♂️

### 安装

#### Windows 环境编译 ⚠️

本项目支持两种编译模式，针对不同的使用场景优化：

**开发模式（快速编译，使用预编译OpenSSL）**：
```bash
# 开发环境快速编译（约2-3分钟）
cargo build
cargo run
```

**生产模式（静态编译，无依赖问题）**：
```bash
# 设置环境变量（重要！）
export CFLAGS="-O2 -fPIC"
export CXXFLAGS="-O2 -fPIC"

# 静态编译（约25-30分钟，首次编译需要下载和编译OpenSSL源码）
cargo build --release --features static-openssl
```

**Windows 环境注意事项**：
- 推荐使用 MSYS2 + MinGW64 环境
- 首次静态编译需要较长时间（25-30分钟），请耐心等待
- 编译超时建议设置为 40 分钟以上
- 静态编译后的可执行文件无外部依赖，便于分发

#### 🔧 Windows MSYS2 + MinGW64 环境搭建指南

**第1步：安装 MSYS2**
1. 访问 [MSYS2 官网](https://www.msys2.org/)
2. 下载适合您系统的安装程序（64位推荐）
3. 运行安装程序，选择安装路径（建议使用默认路径 `C:\msys64`）
4. 完成安装后，启动 "MSYS2 MINGW64" 终端

**第2步：更新软件包**
在 MSYS2 MINGW64 终端中执行：
```bash
# 更新软件包数据库和基础包
pacman -Syu

# 如果提示重启终端，请关闭并重新打开终端，然后继续更新
pacman -Su
```

**第3步：安装必要的编译工具**
```bash
# 安装 MinGW-w64 工具链
pacman -S --needed base-devel mingw-w64-x86_64-toolchain

# 安装 Git
pacman -S git

# 安装 OpenSSL 开发包（用于静态编译）
pacman -S mingw-w64-x86_64-openssl
```

**第4步：安装 Rust**
```bash
# 通过 rustup 安装 Rust（推荐）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 或者使用 pacman 安装
pacman -S mingw-w64-x86_64-rust
```

**第5步：验证环境**
```bash
# 检查编译器
gcc --version
g++ --version

# 检查 Rust
rustc --version
cargo --version

# 检查 Git
git --version
```

**第6步：配置环境变量**
在项目构建前设置编译标志：
```bash
# 设置兼容的编译标志（重要！）
export CFLAGS="-O2 -fPIC"
export CXXFLAGS="-O2 -fPIC"
```

**故障排除：**
- 如果遇到权限问题，请以管理员身份运行 MSYS2 终端
- 如果网络连接有问题，可以尝试更换镜像源
- 如果编译失败，确保所有软件包都是最新版本

**常用命令：**
```bash
# 清理编译缓存
cargo clean

# 查看安装的软件包
pacman -Qs mingw-w64

# 搜索可用软件包
pacman -Ss 搜索关键词
```

#### Linux/macOS 环境

```bash
# 开发编译
cargo build

# 生产编译（推荐，避免依赖问题）
export CFLAGS="-O2 -fPIC"
export CXXFLAGS="-O2 -fPIC"
cargo build --release --features static-openssl
```

### 基本使用

#### 使用构建器模式（唯一推荐方式）

```rust
use rat_engine::{RatEngine, Router, Method, Response, StatusCode, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建路由器并添加路由
    let mut router = Router::new();

    // 添加 Hello World 路由
    router.add_route(Method::GET, "/hello", |_req: HttpRequest| {
        Box::pin(async {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(r#"{"message":"Hello, World!"}"#)))
                .unwrap())
        })
    });

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

### HEAD 请求自动回退机制

RAT Engine 支持 **HEAD 请求自动回退到 GET 处理器** 的功能，这是 HTTP/1.1 规范的最佳实践：

#### 功能特点
- **自动回退**: 当没有显式定义 HEAD 路由时，自动使用对应的 GET 处理器
- **安全控制**: 可配置白名单，限制哪些路径允许 HEAD 回退
- **性能优化**: 无需为每个 GET 路由手动添加对应的 HEAD 路由
- **兼容性**: 显式定义的 HEAD 路由优先级高于自动回退

#### 使用示例
```rust
use rat_engine::{RatEngine, Router, Method, Response, StatusCode, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use std::collections::HashSet;
use std::pin::Pin;
use std::future::Future;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut router = Router::new();

    // 添加 GET 路由
    router.add_route(Method::GET, "/api/users", |_req: HttpRequest| {
        Box::pin(async {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(r#"{"users":[]}"#)))
                .unwrap())
        })
    });

    // 配置 HEAD 回退功能
    let mut whitelist = HashSet::new();
    whitelist.insert("/api".to_string());  // 只允许 /api 路径使用 HEAD 回退

    // 启用 HEAD 回退，但限制在白名单内
    router.enable_head_fallback(true, Some(whitelist));

    let engine = RatEngine::builder()
        .router(router)
        .build()?;

    engine.start("127.0.0.1".to_string(), 8080).await?;
    Ok(())
}
```

#### 测试命令
```bash
# HEAD 请求会自动回退到 GET 处理器
curl -I http://127.0.0.1:8080/api/users

# 等价于
curl -X GET http://127.0.0.1:8080/api/users
```

#### 安全说明
- **白名单机制**: 默认情况下，HEAD 回退是启用的但没有限制
- **建议配置**: 在生产环境中，建议通过白名单明确指定允许 HEAD 回退的路径
- **显式路由**: 如果需要特殊处理，可以显式定义 HEAD 路由，它会覆盖自动回退

📖 **完整示例**: `examples/head_fallback_demo.rs`

### 特性系统

项目使用Cargo特性进行功能模块化：

#### 默认特性
- `tls`: TLS/SSL支持（默认启用），支持HTTP/2和gRPC

#### 客户端功能
- `client`: 组合特性，包含HTTP和gRPC客户端功能
- `grpc-client`: 仅gRPC客户端功能
- `reqwest`: 独立HTTP客户端支持

#### 缓存功能
- `cache`: L1内存缓存
- `cache-full`: L1+L2缓存（包含持久化存储）

#### 压缩功能
- `compression`: 基础压缩（gzip + lz4）
- `compression-full`: 完整压缩（包含 brotli + zstd）
- `compression-br`: 基础压缩 + Brotli
- `compression-zstd`: 基础压缩 + Zstd

#### 证书和安全
- `acme`: ACME自动证书申请
- `static-openssl`: 静态编译OpenSSL（避免运行时依赖）

#### Python绑定
- `python`: Python绑定支持（需要PyO3）

#### 其他
- `full`: 包含所有可选特性（client、cache-full、compression-full、acme）
- `dev`: 开发环境特性（空特性组）

#### 特性组合示例
```bash
# 启用缓存和压缩
cargo build --features cache,compression

# 启用所有功能
cargo build --features full

# Python绑定开发
cargo build --features python

# 静态编译（避免运行时依赖）
cargo build --release --features static-openssl
```

### 路径参数支持

RAT Engine 支持强大的路径参数自动提取功能，支持多种参数类型：

- **整数**: `<id>` 或 `<int:id>` - 默认为整数类型
- **字符串**: `<str:id>`, `<string:id>`, `<uuid:id>` - 支持 UUID 等字符串
- **浮点数**: `<float:price>` - 支持小数
- **路径**: `<path:file_path>` - 可包含斜杠的完整路径

使用便捷的 API 自动提取参数，无需手动解析：

#### 完整示例
```rust
use rat_engine::{RatEngine, Router, Method, Response, StatusCode, Full, Bytes};
use rat_engine::server::http_request::HttpRequest;
use std::pin::Pin;
use std::future::Future;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut router = Router::new();

    // 添加带有路径参数的路由
    router.add_route(Method::GET, "/users/<id>/posts/<post_id>", |req: HttpRequest| {
        Box::pin(async move {
            // 提取路径参数
            let user_id = req.param_as_i64("id").unwrap_or(0);
            let post_id = req.param_as_i64("post_id").unwrap_or(0);

            let response_data = format!(r#"{{"user_id": {}, "post_id": {}}}"#, user_id, post_id);

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Full::new(Bytes::from(response_data)))
                .unwrap())
        })
    });

    let engine = RatEngine::builder()
        .router(router)
        .build()?;

    engine.start("127.0.0.1".to_string(), 8080).await?;
    Ok(())
}
```

#### 参数提取方法
```rust
// 在路由处理器内部使用这些方法
let user_id = req.param_as_i64("id").unwrap_or(0);          // 获取整数参数
let user_uuid = req.param("uuid").unwrap_or("default");     // 获取字符串参数
let price = req.param_as_f64("price").unwrap_or(0.0);       // 获取浮点参数
let file_path = req.param("file_path").unwrap_or("");       // 获取路径参数
```

📖 **完整示例请查看**:
- `examples/dynamic_routes_demo.rs` - 基础路径参数示例
- `examples/advanced_path_params_demo.rs` - 高级参数类型演示
- `examples/streaming_demo.rs` - 流式响应和全局SSE管理器演示
- `examples/streaming_response_test.rs` - **流式响应功能测试**，验证HTTP状态码、JSON响应、SSE参数验证
- `examples/sse_chat/` - **完整的多房间SSE聊天室示例**，展示实时通信应用
- `examples/proxy_protocol_v2_test.rs` - **HAProxy PROXY protocol v2 兼容性测试**，展示代理支持

## 部署指南 📋

### HAProxy 集成

RAT Engine 完全支持 HAProxy 负载均衡器，包括：

- ✅ **gRPC 请求智能识别** - 在 HTTP 模式下正确识别 gRPC 请求
- ✅ **PROXY Protocol v2 支持** - 获取原始客户端 IP 地址
- ✅ **HTTP/2 协议支持** - 通过 ALPN 协商

📖 **详细配置指南**: [HAProxy 配置指南](docs/haproxy_configuration.md)

### 运行示例

项目提供了多个功能示例：

```bash
# 运行构建器模式示例
cargo run --example builder_pattern_example

# 运行流式处理示例
cargo run --example streaming_demo

# 运行流式响应测试示例（验证HTTP状态码和SSE参数验证）
cargo run --example streaming_response_test

# 运行 SSE 聊天室示例
cargo run --example sse_chat

# 运行 gRPC 综合示例
cargo run --example grpc_comprehensive_example

# 运行缓存性能测试
cargo run --example cache_compression_performance_test

# 运行 gRPC 客户端示例
cargo run --example grpc_client_bidirectional_example

# 运行 ACME 证书管理示例
cargo run --example acme_sandbox_demo

# 运行 HEAD 回退功能演示
cargo run --example head_fallback_demo

# 运行动态路由示例（需要 reqwest 特性）
cargo run --example dynamic_routes_demo --features reqwest

# 运行高级路径参数示例（需要 reqwest 特性）
cargo run --example advanced_path_params_demo --features reqwest
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

- **gRPC 客户端**: 高性能 gRPC 客户端，支持双向流和连接池
- **独立HTTP客户端**: 基于 reqwest 的高性能 HTTP 客户端（需要 reqwest 特性）
- **连接池**: gRPC 连接复用管理
- **下载管理**: gRPC 断点续传和元数据管理

### Python API 模块

- **Python 绑定**: 通过 PyO3 提供 Python 接口
- **Flask 风格 API**: 熟悉的 Web 框架接口
- **异步支持**: 完整的 async/await 支持

## 项目结构 📁

```
src/
├── lib.rs              # 库入口
├── error.rs            # 错误处理
├── error_i18n.rs       # 多语言错误信息
├── cache/              # 缓存模块
│   ├── mod.rs
│   └── builder.rs      # 缓存构建器
├── compression/        # 压缩模块
│   ├── mod.rs
│   ├── compressor.rs   # 压缩器实现
│   ├── config.rs       # 压缩配置
│   ├── types.rs        # 压缩类型
│   └── utils.rs        # 压缩工具
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
│   ├── cert_manager/   # 证书管理模块
│   ├── grpc_handler/   # gRPC 处理模块
│   ├── streaming.rs    # 流式处理
│   └── performance.rs  # 性能管理
├── client/             # 客户端模块
│   ├── mod.rs
│   ├── grpc_client/    # gRPC 客户端目录
│   ├── grpc_builder.rs # gRPC 客户端构建器
│   ├── independent_http_client.rs # 独立HTTP客户端（基于reqwest）
│   ├── connection_pool.rs # 连接池管理
│   ├── download_metadata.rs # 下载元数据管理
│   └── types.rs        # 客户端类型定义
├── python_api/         # Python 绑定
│   ├── mod.rs
│   ├── server.rs       # Python 服务器接口
│   ├── client.rs       # Python 客户端接口
│   ├── engine_builder.rs # Python 引擎构建器
│   ├── handlers.rs     # Python 处理器
│   ├── compression.rs  # Python 压缩接口
│   ├── cert_manager.rs # Python 证书管理
│   ├── streaming.rs    # Python 流式处理
│   ├── codec.rs        # 编解码器
│   ├── response_converter.rs # 响应转换器
│   ├── grpc_queue_bridge.rs # gRPC 队列桥接
│   ├── http_queue_bridge.rs # HTTP 队列桥接
│   ├── smart_transfer.rs # Python 智能传输
│   ├── congestion_control.rs # Python 拥塞控制
│   └── http/          # Python HTTP 模块
│       ├── mod.rs
│       ├── core.rs
│       ├── request.rs
│       ├── response.rs
│       └── http_converter.rs
├── utils/              # 工具模块
│   ├── mod.rs
│   ├── logger.rs       # 日志系统
│   ├── sys_info.rs     # 系统信息
│   ├── ip_extractor.rs # IP 提取
│   ├── crypto_provider.rs # 加密提供者
│   └── feature_check.rs # 特性检查
└── common/             # 公共模块
    ├── mod.rs
    └── path_params.rs  # 路径参数处理

examples/              # 示例文件
├── 基础示例
│   ├── builder_pattern_example.rs # 构建器模式示例
│   ├── streaming_demo.rs   # 流式处理示例
│   ├── streaming_response_test.rs # 流式响应功能测试示例
│   └── head_fallback_demo.rs # HEAD 请求回退演示
├── gRPC 示例
│   ├── grpc_comprehensive_example.rs # gRPC 综合示例
│   ├── grpc_client_bidirectional_example.rs # 双向流gRPC客户端
│   ├── grpc_client_bidirectional_tls_example.rs # TLS双向流gRPC
│   ├── grpc_client_bidirectional_mtls_example.rs # MTLS双向流gRPC
│   └── grpc_resumable_download.rs # gRPC断点续传下载
├── 路由和参数
│   ├── dynamic_routes_demo.rs # 动态路由示例（需要 reqwest 特性）
│   ├── advanced_path_params_demo.rs # 高级路径参数示例（需要 reqwest 特性）
│   └── route_conflict_demo.rs # 路由冲突解决演示
├── 缓存和性能
│   ├── cache_compression_performance_test.rs # 缓存性能测试
│   ├── router_high_speed_cache_example.rs # 路由器高速缓存演示
│   ├── test_direct_l1_cache.rs # L1缓存测试
│   └── test_direct_l2_cache.rs # L2缓存测试
├── 中间件和协议
│   ├── cors_example.rs # CORS跨域资源共享演示
│   ├── https_redirect.rs # HTTPS重定向演示
│   └── simple_protocol_detection.rs # 协议检测演示
├── 证书和安全
│   ├── acme_sandbox_demo.rs # ACME证书自动签发演示
│   └── chunked_upload_demo.rs # 分块上传演示
├── 开发和测试
│   ├── sse_chat/ # SSE聊天室示例
│   │   ├── main.rs
│   │   ├── login.html
│   │   └── chat.html
│   ├── logging_example.rs # 日志系统使用演示
│   ├── independent_http_client_test.rs # 独立HTTP客户端测试
│   └── test_cache_version_manager.rs # 缓存版本管理测试
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
- **吞吐量**: ~50,000 RPS（基于MacBook Air M1测试）
- **延迟**: < 1ms (P99)
- **内存使用**: ~50MB（基础配置）
- **CPU 使用**: 自适应负载均衡

⚠️ **重要说明**: 这些性能数据仅供参考，基于特定硬件和测试环境。实际性能会因以下因素而异：
- 硬件配置（CPU、内存、存储）
- 网络环境（延迟、带宽）
- 请求类型和负载模式
- 并发连接数
- 业务逻辑复杂度

## 版本信息

- **当前版本**: 1.2.0
- **支持Rust版本**: 2024 Edition
- **许可证**: LGPL-3.0
- **维护状态**: 活跃开发中

## 贡献指南

欢迎提交Issue和Pull Request！

### 开发流程
1. Fork本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 创建Pull Request

### 代码规范
- 遵循Rust官方代码规范
- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 检查代码质量
- 添加适当的文档注释
- 确保所有测试通过

## 许可证

本项目采用 [LGPL-3.0](LICENSE) 许可证。

## 致谢

感谢所有为这个项目做出贡献的开发者！