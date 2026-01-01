# H2C over TLS 架构文档

## 概述

本文档说明 Lurker 代理服务中使用的 H2C over TLS 通信架构，以及如何通过 HAProxy 进行反向代理部署。

## 什么是 H2C over TLS

### 定义

- **H2C (HTTP/2 Cleartext)**: 明文 HTTP/2 协议，不使用 TLS 加密
- **H2C over TLS**: 在 TLS 连接内部传输 H2C 帧，即 `TLS(HTTP/2)` 而非 `HTTP/2(TLS)`

### 与传统 HTTPS 的区别

| 协议 | 格式 | 说明 |
|------|------|------|
| HTTPS 1.1 | TLS(HTTP/1.1) | HTTP/1.1 over TLS |
| HTTPS 2.0 | TLS(HTTP/2) | 标准 HTTP/2 over TLS |
| H2C | HTTP/2 cleartext | 明文 HTTP/2 |
| **H2C over TLS** | TLS(H2C) | TLS 内部传输 H2C 帧 |

### 为什么使用 H2C over TLS

1. **协议兼容性**: rat_engine 框架的 H2C 模式在 TLS 内部工作
2. **性能优化**: 利用 HTTP/2 的多路复用和头部压缩
3. **部署灵活**: HAProxy 可以终止 TLS 并重新加密连接后端

## 架构设计

### 完整流量路径

```
客户端                   HAProxy                  服务端
  │                        │                        │
  │  1. TLS 握手            │                        │
  ├───────────────────────>│                        │
  │  (H2C 帧封装在 TLS)     │                        │
  │                        │                        │
  │                        │  2. TLS 握手            │
  │                        ├───────────────────────>│
  │                        │  (HTTP/2 over TLS)     │
  │                        │                        │
  │  3. gRPC 数据           │                        │
  ├───────────────────────>├───────────────────────>│
  │  (H2C 帧)              │  (HTTP/2 帧)           │
  │                        │                        │
  │  4. gRPC 响应           │                        │
  │<──────────────────────┼───────────────────────│
  │                        │                        │
```

### HAProxy 的作用

1. **TLS 终止**: 接收客户端的 TLS 连接，提取 H2C 帧
2. **SNI 路由**: 根据路径将流量分发到不同的后端服务
3. **SSL 重加密**: 重新建立到后端的 TLS 连接
4. **协议转换**: H2C → HTTP/2（后端）

## 配置说明

### 客户端配置

**关键点**：
- 使用 H2C 模式（`h2c_mode=true`）
- 连接到 HTTPS 端点（`https://域名:端口`）
- 证书验证由 H2C 模式处理（跳过或宽松验证）

```toml
[client_config]
# 连接端点使用 HTTPS
grpc_endpoint = "https://sukiyaki.su:443"
grpc_service_name = "lurker.LurkerService"
grpc_method_name = "ProxyStream"

# ed25519 认证密钥
private_key = "客户端私钥(Base64)"
public_key = "客户端公钥(Base64)"
```

**代码实现** (`src/client/grpc_client.rs`):

```rust
let builder = RatGrpcClientBuilder::new()
    .http2_only()
    .h2c_mode();  // 启用 H2C 模式
```

### HAProxy 配置

**前端配置** (接收客户端连接):

```haproxy
frontend https_frontend
    mode http
    # 监听 443 端口，启用 TLS
    bind *:443 ssl crt /etc/sukiyaki-ssl.pem alpn h2,http/1.1

    # gRPC 路径检测
    acl is_lurker_grpc path_beg /lurker.LurkerService

    # 根据路径分发
    use_backend lurker_grpc_backend if is_lurker_grpc
    default_backend other_backend
```

**后端配置** (连接到 Lurker 服务):

```haproxy
backend lurker_grpc_backend
    mode http
    balance roundrobin

    # 关键配置：
    # - ssl: 启用 SSL 重加密
    # - alpn h2: 协商 HTTP/2 协议
    # - verify none: 不验证后端证书（内网通信）
    # - sni str(...): 发送 SNI 以匹配后端证书
    server lurker 127.0.0.1:8443 ssl alpn h2 verify none sni str(sukiyaki.su)
```

### 服务端配置

**关键点**：
- 监听本地端口（127.0.0.1）
- 使用真实 TLS 证书
- 配置授权的客户端公钥

```toml
[server_config]
# 只监听本地，通过 HAProxy 对外
grpc_listen_addr = "127.0.0.1:8443"
grpc_endpoint_path = "/lurker.LurkerService/ProxyStream"

# 授权的客户端公钥列表
authorized_keys = [
    "客户端公钥1(Base64)",
    "客户端公钥2(Base64)",
]

[server_config.tls_config]
# 真实的 TLS 证书
server_cert_path = "/etc/letsencrypt/live/sukiyaki.su/fullchain.pem"
server_key_path = "/etc/letsencrypt/live/sukiyaki.su/privkey.pem"
server_name = "sukiyaki.su"
```

## 部署注意事项

### 1. DNS 和 CDN 问题

**问题描述**:
如果域名使用 CDN（如 Cloudflare），DNS 解析会返回 CDN 的 IP 地址，而不是源服务器 IP。CDN 不会转发 gRPC 流量。

**解决方案**:
在客户端的 `/etc/hosts` 文件中硬编码 DNS 映射：

```bash
# /etc/hosts
149.104.15.105 sukiyaki.su
```

**验证 DNS 解析**:
```bash
# 检查实际解析（可能指向 CDN）
host sukiyaki.su

# 检查 hosts 文件后的解析
ping -c 1 sukiyaki.su
```

### 2. 证书配置

**证书要求**:
- 前端（HAProxy）: 公开信任的证书（如 Let's Encrypt）
- 后端（Lurker）: 可以使用自签名证书或内网证书
- SNI 必须匹配后端证书的 `server_name`

**证书路径**:
```
/etc/letsencrypt/live/sukiyaki.su/
├── fullchain.pem  # 完整证书链
├── privkey.pem    # 私钥
└── README
```

### 3. 防火墙配置

**需要的端口**:
- 443/tcp: HAProxy HTTPS（对外）
- 8443/tcp: Lurker gRPC（仅本地，不对外开放）

**防火墙规则**:
```bash
# 允许 HTTPS
ufw allow 443/tcp

# Lurker 只监听 127.0.0.1，无需额外规则
```

### 4. 权限管理

**证书访问权限**:
```
/etc/letsencrypt/live/sukiyaki.su/
├── fullchain.pem (mode 644, ssl-cert 组可读)
└── privkey.pem   (mode 640, ssl-cert 组可读)
```

**用户组配置**:
```bash
# 将 lurker 用户加入 ssl-cert 组
usermod -a -G ssl-cert lurker

# 验证
groups lurker
```

## 测试验证

### 1. 检查端口监听

```bash
# 服务端
ss -tlnp | grep 8443
# 应该显示: 127.0.0.1:8443

# HAProxy
ss -tlnp | grep 443
# 应该显示: 0.0.0.0:443
```

### 2. 测试客户端连接

```bash
# 启动客户端
./lurker --config config-client.toml --mode client

# 查看日志
# 应该看到: ✅ 双向流连接已建立
```

### 3. 测试代理功能

```bash
# SOCKS5 代理测试
curl --socks5 127.0.0.1:1080 http://httpbin.org/ip

# 应该返回服务器的 IP
{
  "origin": "149.104.15.105"
}
```

### 4. 检查服务端日志

```bash
# 查看最新日志
journalctl -u lurker -n 50 --no-pager

# 应该看到心跳消息
# [SERVER DEBUG] handle_proxy_packet connection_id=0, packet=Heartbeat
```

## 故障排查

### 问题 1: 连接失败 - network_error

**可能原因**:
1. DNS 解析到 CDN 而不是源服务器
2. 防火墙阻止连接
3. 服务端未启动

**解决步骤**:
```bash
# 1. 检查 DNS 解析
host sukiyaki.su

# 2. 如果是 CDN，添加 hosts 映射
echo "149.104.15.105 sukiyaki.su" >> /etc/hosts

# 3. 验证解析
ping -c 1 sukiyaki.su

# 4. 检查服务端状态
ssh root@服务器 "systemctl status lurker"
```

### 问题 2: TLS 握手失败

**可能原因**:
1. SNI 不匹配
2. 证书过期或无效
3. ALPN 协商失败

**解决步骤**:
```bash
# 1. 检查证书有效性
openssl x509 -in /etc/letsencrypt/live/sukiyaki.su/fullchain.pem -noout -dates

# 2. 测试 TLS 连接
openssl s_client -connect sukiyaki.su:443 -servername sukiyaki.su

# 3. 检查 ALPN 支持
openssl s_client -connect sukiyaki.su:443 -alpn h2,http/1.1
```

### 问题 3: HAProxy 无法连接后端

**可能原因**:
1. 后端服务未启动
2. SNI 配置错误
3. 端口监听地址错误

**解决步骤**:
```bash
# 1. 检查后端监听
ss -tlnp | grep 8443

# 2. 检查 HAProxy 日志
journalctl -u haproxy -n 50 --no-pager

# 3. 测试后端连接
curl --insecure https://127.0.0.1:8443/lurker.LurkerService/ProxyStream
```

## 性能优化

### HAProxy 调优

```haproxy
global
    # 增加 SSL 会话缓存
    tune.ssl.cachesize 100000

    # 启用 SSL session ticket
    tune.ssl.session-cache-size 100000

defaults
    # 调整超时
    timeout connect 5s
    timeout client  30s
    timeout server  30s
    timeout tunnel  1h
```

### gRPC 调优

```toml
[client_config]
max_idle_connections = 100  # 增加空闲连接池
connection_timeout_secs = 30
request_timeout_secs = 60
```

## 安全建议

1. **证书管理**:
   - 使用 Let's Encrypt 自动续期
   - 定期检查证书过期时间
   - 私钥权限设置为 640

2. **访问控制**:
   - 只允许授权的客户端公钥
   - 定期轮换 ed25519 密钥对
   - 监控异常连接

3. **网络安全**:
   - Lurker 只监听 127.0.0.1
   - 使用防火墙限制访问
   - 启用 fail2ban 防暴力破解

## 总结

H2C over TLS 架构通过以下方式实现了安全高效的 gRPC 代理：

1. **协议层**: TLS(H2C) 提供加密和性能
2. **反向代理**: HAProxy 提供 TLS 终止和路由
3. **认证**: ed25519 实现客户端身份验证
4. **部署**: 通过 hosts 映射绕过 CDN 限制

这种架构适用于需要通过反向代理部署 gRPC 服务的场景。
