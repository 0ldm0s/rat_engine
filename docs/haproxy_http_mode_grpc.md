# HAProxy HTTP 模式 gRPC 代理测试指南

本文档记录了 RAT Engine 在 HAProxy HTTP 模式下的 gRPC 代理完整测试结果和配置方案。

## 目录

- [测试概述](#测试概述)
- [一、gRPC 四种通信模式测试](#一grpc-四种通信模式测试)
- [二、基于路径的路由测试](#二基于路径的路由测试)
- [关键配置要点](#关键配置要点)
- [测试命令](#测试命令)

## 测试概述

### 测试环境

- **HAProxy**: 3.2.10
- **代理模式**: HTTP 模式（非 TCP 模式）
- **证书**: ligproxy-test.0ldm0s.net（真实签发证书）
- **DNS 配置**: 修改 /etc/hosts 让域名指向 127.0.0.1 进行本地测试

### 测试目标

验证 HAProxy 在 HTTP 模式下能够正确代理 gRPC 请求，支持：
1. gRPC 四种通信模式（一元、客户端流、服务端流、双向流）
2. 基于路径的混合路由（HTTP + gRPC）

---

## 一、gRPC 四种通信模式测试

### 1.1 HAProxy 配置

**文件**: `tests/haproxy_grpc.cfg`

```haproxy
# HTTPS 前端（统一入口）
frontend grpc_front
    mode http
    bind *:8443 ssl crt /path/to/cert.pem alpn h2,http/1.1
    default_backend grpc_backend

# gRPC 后端
backend grpc_backend
    mode http
    server grpc1 127.0.0.1:50051 ssl alpn h2 verify none sni str(domain.com)
```

**关键配置**:
- `alpn h2,http/1.1` - 允许 HTTP/2 协议协商
- `ssl alpn h2` - 后端连接使用 HTTP/2
- `sni str(domain.com)` - **重要**: 指定 SNI 让后端接受 TLS 连接

### 1.2 测试结果

| 通信模式 | 测试客户端 | 测试结果 | 详情 |
|---------|-----------|---------|------|
| 一元请求 | `grpc_haproxy_client.rs` | ✅ | Hello/Ping 成功 |
| 客户端流 | `grpc_tls_client_stream_haproxy_client.rs` | ✅ | 3个数据块 (60字节) |
| 服务端流 | `grpc_tls_server_stream_haproxy_client.rs` | ✅ | 5条股票报价 |
| 双向流 | `grpc_tls_bidirectional_haproxy_client.rs` | ✅ | 3次 Ping/Pong |

### 1.3 SNI 问题解决

**问题**: HAProxy 连接后端时返回 "tlsv1 alert access denied"

**原因**: HAProxy 使用 IP (127.0.0.1) 连接后端，SNI 不匹配导致后端拒绝

**解决方案**: 在 HAProxy backend 配置中添加 SNI 参数
```haproxy
server grpc1 127.0.0.1:50051 ssl alpn h2 verify none sni str(ligproxy-test.0ldm0s.net)
```

---

## 二、基于路径的路由测试

### 2.1 场景描述

**单端口** + **基于路径的混合路由**:
- gRPC 路径 → gRPC 后端 (50051)
- HTTP 路径 → HTTP 后端 (3001)

### 2.2 HAProxy 配置

**文件**: `tests/haproxy_http_grpc.cfg`

```haproxy
# HTTPS 前端（统一入口）
frontend main_front
    mode http
    bind *:8443 ssl crt /path/to/cert.pem alpn h2,http/1.1

    # gRPC 路径检测
    acl is_grpc_path path_beg /hello. /ping. /chat. /stock. /upload.

    # 根据路径分发
    use_backend grpc_backend if is_grpc_path
    default_backend http_backend

# gRPC 后端（50051）
backend grpc_backend
    mode http
    server grpc1 127.0.0.1:50051 ssl alpn h2 verify none sni str(domain.com)

# HTTP 后端（3001）
backend http_backend
    mode http
    server http1 127.0.0.1:3001
```

### 2.3 服务端配置

使用双端口模式示例: `examples/dual_port_mode.rs`

- HTTP 服务: 端口 3001
- gRPC 服务: 端口 50051

```bash
cargo run --example dual_port_mode
```

### 2.4 测试结果

| 测试 | 路径 | 目标后端 | 结果 |
|------|------|---------|------|
| HTTP 首页 | `/` | HTTP (3001) | ✅ |
| gRPC 双向流 | `/chat.ChatService/Chat` | gRPC (50051) | ✅ |

**HTTP 测试**:
```bash
curl -k https://127.0.0.1:8443/
# 返回: "RAT Engine 双端口模式" 页面
```

**gRPC 测试**:
```bash
cargo run --example grpc_tls_bidirectional_haproxy_client --features client
# 3次 Ping/Pong 成功
```

---

## 关键配置要点

### 3.1 SNI 参数（必须）

```haproxy
server grpc1 127.0.0.1:50051 ssl alpn h2 verify none sni str(domain.com)
```

**作用**: 让 HAProxy 与后端建立 TLS 连接时使用正确的 SNI，避免后端拒绝连接。

### 3.2 ALPN 配置

```haproxy
# 前端：允许 HTTP/2 协商
bind *:8443 ssl crt /path/to/cert.pem alpn h2,http/1.1

# 后端：强制使用 HTTP/2
server grpc1 127.0.0.1:50051 ssl alpn h2 ...
```

### 3.3 路径 ACL 规则

```haproxy
# gRPC 服务通常以包名开头
acl is_grpc_path path_beg /hello. /ping. /chat. /stock. /upload.
```

---

## 测试命令

### 启动服务端

```bash
# 双端口模式 (HTTP 3001 + gRPC 50051)
cargo run --example dual_port_mode

# 或分别启动
cargo run --example http_test_server  # HTTP 9090
cargo run --example grpc_tls_unary_server  # gRPC 50051
```

### 启动 HAProxy

```bash
# gRPC 专用配置
haproxy -f tests/haproxy_grpc.cfg

# 路由混合配置
haproxy -f tests/haproxy_http_grpc.cfg
```

### 运行测试

```bash
# HTTP 测试
curl -k https://127.0.0.1:8443/

# gRPC 一元请求
cargo run --example grpc_haproxy_client --features client

# gRPC 客户端流
cargo run --example grpc_tls_client_stream_haproxy_client --features client

# gRPC 服务端流
cargo run --example grpc_tls_server_stream_haproxy_client --features client

# gRPC 双向流
cargo run --example grpc_tls_bidirectional_haproxy_client --features client
```

---

## 故障排除

### 问题 1: 503 Service Unavailable

**原因**: HAProxy 无法连接到后端

**解决**:
1. 检查后端服务是否运行
2. 检查 SNI 配置是否正确
3. 查看端口是否开放: `lsof -ti:50051`

### 问题 2: 502 Bad Gateway

**原因**: 后端返回无效响应

**解决**:
1. 确认后端使用正确的协议（HTTP/2）
2. 检查 ALPN 协商是否成功
3. 验证证书配置

### 问题 3: tlsv1 alert access denied

**原因**: SNI 不匹配

**解决**: 添加 `sni str(domain.com)` 到 server 配置

---

## 生产环境建议

1. **多证书 SNI 路由** - 根据域名使用不同证书，支持多域名服务
2. **启用健康检查** - `check inter 5s`
3. **配置超时** - `timeout tunnel 1h` 用于长连接
4. **负载均衡** - 配置多个后端服务器
5. **日志监控** - 配置日志收集和分析

---

## 总结

通过本次测试验证：

1. ✅ HAProxy HTTP 模式可以正确代理 gRPC 请求
2. ✅ gRPC 四种通信模式全部工作正常
3. ✅ 基于路径的混合路由（HTTP + gRPC）可行
4. ✅ SNI 配置是关键，必须正确设置

**关键配置文件**:
- `tests/haproxy_grpc.cfg` - gRPC 专用代理
- `tests/haproxy_http_grpc.cfg` - 基于路径的混合路由
