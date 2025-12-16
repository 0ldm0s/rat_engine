# HAProxy 配置指南

本指南详细说明如何将 HAProxy 与 RAT Engine 配合使用，包括 HTTP 和 gRPC 代理、PROXY protocol v2 支持等高级功能。

## 目录

- [基本概念](#基本概念)
- [HTTP 代理配置](http-代理配置)
- [gRPC 代理配置](grpc-代理配置)
- [PROXY Protocol v2 支持](proxy-protocol-v2-支持)
- [生产环境配置](生产环境配置)
- [故障排除](故障排除)

## 基本概念

HAProxy 是一个高性能的负载均衡器，可以与 RAT Engine 完美配合。RAT Engine 特别为 HAProxy 做了以下优化：

1. **gRPC 请求智能识别** - 通过 TE: trailers 头部识别 gRPC 请求
2. **PROXY Protocol v2 支持** - 获取原始客户端 IP 地址
3. **HTTP/2 协议支持** - 通过 ALPN 协商

## HTTP 代理配置

### 基础 HTTP 配置

```haproxy
frontend http_frontend
    bind *:80
    default_backend rat_servers

backend rat_servers
    mode http
    balance roundrobin

    # 服务器配置
    server rat1 127.0.0.1:8080 check
    server rat2 127.0.0.1:8081 check
```

### 带健康检查的配置

```haproxy
backend rat_servers
    mode http
    balance roundrobin

    # 健康检查端点
    option httpchk GET /health

    # 健康检查设置
    http-check expect status 200

    server rat1 127.0.0.1:8080 check inter 5s rise 2 fall 3
    server rat2 127.0.0.1:8081 check inter 5s rise 2 fall 3
```

## gRPC 代理配置

### TCP 模式（推荐）

对于 gRPC 服务，使用 TCP 模式可以避免协议解析问题：

```haproxy
frontend grpc_frontend
    bind *:50051
    mode tcp
    default_backend grpc_servers

backend grpc_servers
    mode tcp
    balance roundrobin

    # 启用 PROXY protocol v2
    server grpc1 127.0.0.1:50051 send-proxy-v2 check
    server grpc2 127.0.0.1:50052 send-proxy-v2 check
```

### HTTP 模式（需要特殊配置）

如果必须在 HTTP 模式下运行 gRPC，需要添加以下头部：

```haproxy
backend grpc_servers
    mode http
    balance roundrobin

    # 强制添加 gRPC 相关头部
    http-request set-header Content-Type application/grpc
    http-request set-header TE trailers
    http-request set-header X-Forwarded-Proto https if { ssl_fc }

    # 设置 ALPN
    server grpc1 127.0.0.1:50051 alpn h2 send-proxy-v2
    server grpc2 127.0.0.1:50052 alpn h2 send-proxy-v2
```

## PROXY Protocol v2 支持

PROXY protocol v2 允许 HAProxy 向后端服务器传递原始客户端连接信息。RAT Engine 完全支持 PROXY protocol v2。

### 配置示例

```haproxy
frontend combined_frontend
    bind *:80
    bind *:443 ssl crt /path/to/cert.pem

    # 根据 SNI 路由
    acl is_grpc hdr(host) -i grpc.example.com
    acl is_http hdr(host) -i www.example.com

    use_backend grpc_servers if is_grpc
    use_backend http_servers if is_http

backend http_servers
    mode http
    balance roundrobin

    # 发送 PROXY protocol v2
    server http1 127.0.0.1:8080 send-proxy-v2

backend grpc_servers
    mode http
    balance roundrobin

    # gRPC 特定配置
    http-request set-header Content-Type application/grpc
    http-request set-header TE trailers
    http-request set-header X-Forwarded-Proto https if { ssl_fc }

    # 发送 PROXY protocol v2 并设置 ALPN
    server grpc1 127.0.0.1:50051 alpn h2 send-proxy-v2
```

### RAT Engine 服务器端配置

在 RAT Engine 中，PROXY protocol v2 是自动检测的，无需特殊配置。服务器会自动：

1. 检测 PROXY protocol v2 头部
2. 解析原始客户端 IP 和端口
3. 更新请求的远程地址
4. 提取 ALPN 协议信息

### 测试 PROXY Protocol v2

使用提供的测试示例验证功能：

```bash
# 启动测试服务器
cargo run --example proxy_protocol_v2_test

# 使用 socat 发送 PROXY protocol v2 请求
echo -e "\x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A\x21\x11\x00\x0C\xC0\xA8\x01\x64\x0A\x00\x00\x01\x30\x39\x01\xBBGET /test HTTP/1.1\r\nHost: test\r\n\r\n" | socat - TCP4:127.0.0.1:8080
```

## 生产环境配置

### 完整的生产配置示例

```haproxy
global
    daemon
    maxconn 4096
    log stdout format raw local0
    tune.ssl.default-dh-param 2048

defaults
    mode http
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms
    option httplog
    option dontlognull

frontend www_frontend
    bind *:80
    bind *:443 ssl crt /etc/ssl/certs/example.pem alpn h2,http/1.1

    # HTTP 到 HTTPS 重定向
    redirect scheme https if !{ ssl_fc }

    # ACL 规则
    acl is_api path_beg /api
    acl is_grpc hdr(content-type) -m sub application/grpc
    acl is_grpc hdr(te) -i trailers

    # 路由规则
    use_backend api_servers if is_api
    use_backend grpc_servers if is_grpc
    default_backend web_servers

backend web_servers
    mode http
    balance roundrobin

    # 启用 PROXY protocol
    server web1 10.0.1.10:8080 send-proxy-v2 check
    server web2 10.0.1.11:8080 send-proxy-v2 check

backend api_servers
    mode http
    balance leastconn

    # 启用连接复用
    http-reuse always

    server api1 10.0.1.20:8080 send-proxy-v2 check
    server api2 10.0.1.21:8080 send-proxy-v2 check

backend grpc_servers
    mode http
    balance roundrobin

    # gRPC 配置
    http-request set-header Content-Type application/grpc
    http-request set-header TE trailers
    http-request set-header X-Forwarded-Proto https if { ssl_fc }

    # HTTP/2 支持
    server grpc1 10.0.1.30:50051 alpn h2 send-proxy-v2 check
    server grpc2 10.0.1.31:50051 alpn h2 send-proxy-v2 check

# 统计页面
listen stats
    bind *:8404
    mode http
    stats enable
    stats uri /stats
    stats refresh 30s
```

### 性能优化建议

1. **使用 TCP 模式处理 gRPC**
   ```haproxy
   mode tcp  # 比 mode http 性能更好
   ```

2. **启用 HTTP/2**
   ```haproxy
   bind *:443 ssl crt /path/to/cert.pem alpn h2,http/1.1
   ```

3. **调整超时设置**
   ```haproxy
   timeout tunnel 1h  # gRPC 长连接
   ```

4. **使用 keep-alive**
   ```haproxy
   option http-server-close
   ```

## 故障排除

### 常见问题

1. **gRPC 请求被识别为 HTTP 请求**
   - 确保添加了 `TE: trailers` 头部
   - 检查 `Content-Type: application/grpc`
   - 考虑使用 TCP 模式

2. **无法获取原始客户端 IP**
   - 确保 HAProxy 配置了 `send-proxy-v2`
   - 检查防火墙是否阻止 PROXY protocol
   - 查看服务器日志中的 PROXY protocol 检测信息

3. **连接超时**
   - 增加 timeout 设置
   - 检查健康检查配置
   - 验证服务器端口可访问性

### 调试命令

```bash
# 查看 HAProxy 统计
curl http://localhost:8404/stats

# 查看连接状态
echo "show info" | socat stdio /var/run/haproxy.sock

# 测试 gRPC 连接
grpcurl -plaintext localhost:50051 list
```

### 日志分析

RAT Engine 会输出详细的调试信息：

```
📡 [服务端] 检测到 PROXY protocol v2: 127.0.0.1:54321
✅ [服务端] PROXY protocol v2 解析成功
📍 [服务端] PROXY protocol v2 - 原始客户端IP: 198.51.100.100
📍 [服务端] PROXY protocol v2 - 原始客户端端口: 45678
🔐 [服务端] PROXY ALPN: h2
🚀 [服务端] ALPN指示为HTTP/2
```

## 参考资料

- [HAProxy 官方文档](https://www.haproxy.com/documentation/)
- [PROXY Protocol 规范](http://www.haproxy.org/download/2.2/doc/proxy-protocol.txt)
- [gRPC 负载均衡指南](https://grpc.io/docs/guides/load-balancing/)