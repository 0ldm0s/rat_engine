# RAT Engine 专用模式 - 单端口 gRPC 解决方案

## 概述

本方案通过在 RAT Engine 中添加**专用模式**配置，彻底简化了 nginx 与后端的交互，解决了 gRPC over HTTP/2 的传输问题。

**关键特点**：启用 gRPC 专用模式后，只需要一个端口，无需分端口配置。

## 核心概念

### 1. 专用模式

RAT Engine 现在支持两种专用模式：

- **HTTP 专用模式** (`enable_http_only()`): 只接受 HTTP 请求
- **gRPC 专用模式** (`enable_grpc_only()`): 只接受 gRPC 请求

### 2. 简化协议检测

启用专用模式后，RAT Engine 会：
- 跳过复杂的 PSI 协议检测
- 直接根据模式路由请求
- gRPC 专用模式只检查 `application/grpc+RatEngine` 头部

## 配置方法

### 1. RAT Engine 配置（单个 gRPC 服务）

```rust
// gRPC 专用服务器
let mut router = Router::new();
router.enable_grpc_only();  // 启用 gRPC 专用模式
router.enable_h2();         // 启用 HTTP/2

// 注册 gRPC 方法
router.add_grpc_unary("/dns/Query", handler);

// 启动服务（只需要一个端口）
engine.start("0.0.0.0".to_string(), 50051).await?;
```

如果需要同时支持 HTTP 和 gRPC，需要启动两个独立的 RAT Engine 实例：

```rust
// 实例 1: gRPC 专用服务器
let mut grpc_router = Router::new();
grpc_router.enable_grpc_only();
grpc_router.enable_h2();
grpc_router.add_grpc_unary("/dns/Query", grpc_handler);
let grpc_engine = RatEngine::builder().router(grpc_router).build()?;
tokio::spawn(async move {
    grpc_engine.start("0.0.0.0".to_string(), 50051).await
});

// 实例 2: HTTP 专用服务器
let mut http_router = Router::new();
http_router.enable_http_only();
http_router.add_route(Method::GET, "/", http_handler);
let http_engine = RatEngine::builder().router(http_router).build()?;
tokio::spawn(async move {
    http_engine.start("0.0.0.0".to_string(), 8080).await
});
```

### 2. Nginx 配置

```nginx
# gRPC 专用端口
server {
    listen 50051 http2;
    location /dns/Query {
        proxy_set_header Content-Type "application/grpc+RatEngine";
        proxy_pass http://127.0.0.1:50051;
        proxy_http_version 2.0;
    }
}

# HTTP 专用端口
server {
    listen 8080;
    location / {
        proxy_pass http://127.0.0.1:8080;
    }
}
```

## 工作流程

### 传统方案的问题

```
客户端 (HTTP/2 + ALPN:h2)
    ↓
nginx (降级为 HTTP/1.1)
    ↓
RAT Engine (PSI 检测 → 错误路由)
    ↓
❌ 失败
```

### 专用模式方案

```
客户端 (HTTP/2 + ALPN:h2)
    ↓
nginx (直接转发到专用端口)
    ↓
gRPC 专用端口 50051 → gRPC 专用 RAT Engine
    ↓
✅ 成功
```

## 优势

1. **简单可靠**: 无需复杂的协议检测
2. **性能更好**: 跳过 PSI 检测，减少延迟
3. **易于调试**: 清晰的端口分工
4. **兼容性好**: 支持所有 HTTP/2 客户端

## 注意事项

1. **端口配置**:
   - gRPC 专用模式: 只需要一个端口（如 50051）
   - 如果需要同时支持 HTTP，启动另一个独立的 RAT Engine 实例

2. **nginx 配置**:
   - 必须设置 `application/grpc+RatEngine` 头部
   - gRPC 端口需要 `http2` 指令

3. **RAT Engine 配置**:
   - 启用 `enable_grpc_only()` 后会自动禁用 HTTP 模式
   - 必须设置 `enable_h2()` 以支持 HTTP/2
   - 单个路由器实例只能启用一种专用模式

## 测试验证

```bash
# 测试 gRPC 请求
curl --http2 \
  -H "Content-Type: application/grpc+RatEngine" \
  -H "TE: trailers" \
  --data-binary 'test' \
  http://127.0.0.1:50051/dns/Query

# 如果启动了 HTTP 专用实例，测试 HTTP 请求
curl http://127.0.0.1:8080/
```

## 故障排除

1. **gRPC 请求被拒绝**:
   - 检查是否设置了 `Content-Type: application/grpc+RatEngine`
   - 确认 RAT Engine 启用了 `enable_grpc_only()`

2. **HTTP 请求被拒绝**:
   - 检查 RAT Engine 是否启用了 `enable_http_only()`
   - 确认请求没有 gRPC 头部

3. **协议不匹配**:
   - gRPC 端口必须使用 `http2` 监听
   - HTTP 端口使用普通监听

## 示例文件

- `nginx_grpc_dedicated_ports.conf` - 完整的 nginx 配置示例
- `examples/grpc_comprehensive_example.rs` - gRPC 服务示例

## 总结

通过专用端口模式，我们实现了：
- ✅ 彻底解决 gRPC 传输问题
- ✅ 简化 nginx 配置
- ✅ 提高性能和可靠性
- ✅ 便于维护和调试

这种方法借鉴了业界最佳实践，与 Go gRPC 服务器的工作方式一致，确保了跨语言的兼容性。
