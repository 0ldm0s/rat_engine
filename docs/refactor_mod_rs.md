# mod.rs 拆分方案

## 目标
将 1264 行的 `mod.rs` 拆分为多个文件，物理隔离 HTTP 和 gRPC 逻辑。

## 拆分后的文件结构

### 1. mod.rs (核心调度，保留)
**保留内容**：
- 1-56 行：use 语句、ProtocolType 枚举
- 57-109 行：ReconstructedStream 结构体及实现
- 110-149 行：mod 声明、pub use
- 150-185 行：run_server_with_router（入口函数，已弃用）
- 185-189 行：create_engine_builder
- 190-336 行：run_separated_server（双端口模式）
- 337-349 行：handle_http_connection（调用 http_connection 模块）
- 351-369 行：handle_grpc_connection（调用 grpc_connection 模块）
- 370-449 行：handle_grpc_tls_connection（移到 grpc_connection.rs）
- 450-471 行：handle_connection（共用入口）
- 472-483 行：detect_and_handle_protocol（无 TLS，共用）
- 484-518 行：is_grpc_request（检测函数，共用）
- 519-708 行：detect_and_handle_protocol_with_tls（核心调度，共用）
- 709-790 行：route_by_detected_protocol（路由分发，共用）

**删除内容**：
- 791-926 行：handle_tls_connection → 移到 http_connection.rs
- 927-978 行：handle_h2_tls_connection → 移到 http_connection.rs
- 979-1017 行：handle_http1_connection → 移到 http_connection.rs
- 1018-1063 行：handle_http1_connection_with_stream → 移到 http_connection.rs
- 1064-1264 行：handle_h2_request → 移到 h2_request_handler.rs

### 2. http_connection.rs (HTTP 专用，新建)
**内容**：
- 从 mod.rs 的 791-926 行：handle_tls_connection (hyper auto builder)
- 从 mod.rs 的 927-978 行：handle_h2_tls_connection (h2 + TLS)
- 从 mod.rs 的 979-1017 行：handle_http1_connection
- 从 mod.rs 的 1018-1063 行：handle_http1_connection_with_stream

**需要的 use 语句**：
```rust
use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsStream;
use h2::server;
use hyper::{Request, Response};
use hyper::body::Incoming;
use hyper::body::Bytes;
use http_body_util::{Full, combinators::BoxBody};
use hyper_util::rt::TokioIo;
use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::StreamExt;
use bytes;
use crate::utils::logger::{debug, info, warn, error};
```

### 3. grpc_connection.rs (gRPC 专用，新建)
**内容**：
- 从 mod.rs 的 370-449 行：handle_grpc_tls_connection

**需要的 use 语句**：
```rust
use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use h2::server;
use hyper::Request;
use crate::utils::logger::{debug, info, error};
```

### 4. h2_request_handler.rs (HTTP/2 请求处理，新建)
**内容**：
- 从 mod.rs 的 1064-1264 行：handle_h2_request

**需要的 use 语句**：
```rust
use hyper::Request;
use h2::{RecvStream, server::SendResponse};
use h2::server;
use crate::server::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::utils::logger::{debug, info, error};
use crate::server::http_request::HttpRequest;
use bytes::Bytes;
use http_body_util::{Full, combinators::BoxBody};
use hyper::{Response, Body};
use hyper::body::Incoming;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use futures_util::stream::StreamExt;
use std::pin::Pin;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};
```

## Sed 命令

### 步骤 1：备份原文件
```bash
cp src/server/mod.rs src/server/mod.rs.bak
```

### 步骤 2：提取 http_connection.rs
```bash
# 提取 791-926 行（handle_tls_connection）
sed -n '791,926p' src/server/mod.rs.bak > /tmp/http_part1.txt

# 提取 927-978 行（handle_h2_tls_connection）
sed -n '927,978p' src/server/mod.rs.bak > /tmp/http_part2.txt

# 提取 979-1017 行（handle_http1_connection）
sed -n '979,1017p' src/server/mod.rs.bak > /tmp/http_part3.txt

# 提取 1018-1063 行（handle_http1_connection_with_stream）
sed -n '1018,1063p' src/server/mod.rs.bak > /tmp/http_part4.txt

# 合并并添加 use 语句
cat > src/server/http_connection.rs << 'EOF'
//! HTTP 连接处理模块
//!
//! 专门处理 HTTP/1.1 和 HTTP/2 连接

use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsStream;
use h2::server;
use hyper::{Request, Response};
use hyper::body::Incoming;
use hyper::body::Bytes;
use http_body_util::{Full, combinators::BoxBody};
use hyper_util::rt::TokioIo;
use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::StreamExt;
use bytes;
use crate::utils::logger::{debug, info, warn, error};

EOF

cat /tmp/http_part1.txt >> src/server/http_connection.rs
echo "" >> src/server/http_connection.rs
cat /tmp/http_part2.txt >> src/server/http_connection.rs
echo "" >> src/server/http_connection.rs
cat /tmp/http_part3.txt >> src/server/http_connection.rs
echo "" >> src/server/http_connection.rs
cat /tmp/http_part4.txt >> src/server/http_connection.rs
```

### 步骤 3：提取 grpc_connection.rs
```bash
# 提取 370-449 行（handle_grpc_tls_connection）
cat > src/server/grpc_connection.rs << 'EOF'
//! gRPC 连接处理模块
//!
//! 专门处理 gRPC over TLS 连接，使用 h2 server builder

use crate::server::Router;
use crate::server::HyperAdapter;
use crate::server::cert_manager::CertificateManager;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use h2::server;
use hyper::Request;
use crate::utils::logger::{debug, info, error};

EOF

sed -n '370,449p' src/server/mod.rs.bak >> src/server/grpc_connection.rs
```

### 步骤 4：提取 h2_request_handler.rs
```bash
# 提取 1064-1264 行（handle_h2_request）
cat > src/server/h2_request_handler.rs << 'EOF'
//! HTTP/2 请求处理模块
//!
//! 处理 HTTP/2 请求，包含 gRPC 检测和路由逻辑

use hyper::Request;
use h2::{RecvStream, server::SendResponse};
use h2::server;
use crate::server::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use crate::utils::logger::{debug, info, error};
use crate::server::http_request::HttpRequest;
use bytes::Bytes;
use http_body_util::{Full, combinators::BoxBody};
use hyper::{Response, Body};
use hyper::body::Incoming;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use futures_util::stream::StreamExt;
use std::pin::Pin;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use bincode::{Encode, Decode};

EOF

sed -n '1064,1264p' src/server/mod.rs.bak >> src/server/h2_request_handler.rs
```

### 步骤 5：从 mod.rs 删除已拆分的代码
```bash
# 删除 791-1264 行（将要拆分到其他文件的部分）
sed -i '791,1264d' src/server/mod.rs

# 在 mod.rs 末尾添加新模块声明
cat >> src/server/mod.rs << 'EOF'

// HTTP/2 请求处理模块
mod h2_request_handler;
pub use h2_request_handler::handle_h2_request;

// HTTP 连接处理模块
mod http_connection;
pub use http_connection::{
    handle_tls_connection,
    handle_h2_tls_connection,
    handle_http1_connection,
    handle_http1_connection_with_stream,
};

// gRPC 连接处理模块
mod grpc_connection;
pub use grpc_connection::handle_grpc_tls_connection;
EOF
```

### 步骤 6：修改 mod.rs 中的函数调用
需要修改以下函数来调用新模块：
- `handle_http_connection` (337-349 行) - 调用 `http_connection::handle_http1_connection_with_stream`
- `handle_grpc_connection` (351-369 行) - 调用 `grpc_connection::handle_grpc_tls_connection`
- `route_by_detected_protocol` (709-790 行) - 调用 `http_connection::*` 和 `grpc_connection::*`

## 拆分后的依赖关系

```
mod.rs (核心调度)
  ├── http_connection.rs (HTTP 专用)
  │   └── 调用 → h2_request_handler.rs
  └── grpc_connection.rs (gRPC 专用)
      └── 调用 → h2_request_handler.rs
```

## 后续手动修复

1. 调整 use 语句
2. 修改函数调用（添加模块前缀）
3. 处理可能的可见性问题（pub/private）
4. 编译测试
5. 运行功能测试
