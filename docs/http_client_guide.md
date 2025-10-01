# RAT Engine HTTP客户端使用指南

## 概述

RAT Engine 提供了可选的HTTP客户端功能，允许在RAT Engine应用中进行HTTP请求。虽然HTTP客户端不是RAT Engine的核心功能，但它作为辅助功能提供，主要用于内部服务间通信和简单的HTTP API调用。

## ✅ 已完成的功能

### 1. HTTP客户端真正可选功能
- 通过`enable_http`配置选项控制HTTP客户端的启用/禁用
- 当禁用时，HTTP相关函数正确返回错误信息
- 最小化修改，符合项目设计原则

### 2. gRPC客户端真正可选功能
- 通过`enable_grpc`配置选项控制gRPC客户端的启用/禁用
- 完整的配置管理和错误处理

### 3. HTTP/1.1强制模式
- 通过`http1_only`配置选项强制使用HTTP/1.1协议
- 解决了自动H2C升级导致的问题

## 🔧 HTTP客户端正确用法

### 基本配置

```python
import rat_engine

# 创建客户端管理器
client = rat_engine.PyClientManager()

# 基础配置
config = {
    "connect_timeout": 5000,           # 连接超时时间（毫秒）
    "request_timeout": 10000,          # 请求超时时间（毫秒）
    "max_idle_connections": 5,         # 最大空闲连接数
    "enable_http": True,               # 启用HTTP客户端
    "enable_grpc": False,              # 禁用gRPC客户端
    "enable_compression": False,       # 禁用压缩
    "http2_only": False,               # 允许HTTP/1.1和HTTP/2
    "http1_only": False,               # 不强制HTTP/1.1
    "development_mode": True,          # 开发模式，跳过TLS验证
    "user_agent": "MyApp/1.0",         # 用户代理
    "http_user_agent": "MyApp/1.0"     # HTTP用户代理
}

# 初始化客户端
client.initialize(config)
```

### HTTP/1.1强制模式

```python
config = {
    "connect_timeout": 5000,
    "request_timeout": 10000,
    "max_idle_connections": 5,
    "enable_http": True,
    "enable_grpc": False,
    "http2_only": False,
    "http1_only": True,                 # 强制HTTP/1.1模式
    "development_mode": True,
    "user_agent": "MyApp/1.0",
    "http_user_agent": "MyApp/1.0"
}
```

### HTTP请求示例

#### GET请求

```python
import rat_engine

client = rat_engine.PyClientManager()
config = {
    "connect_timeout": 5000,
    "request_timeout": 10000,
    "max_idle_connections": 5,
    "enable_http": True,
    "enable_grpc": False,
    "http1_only": True,  # 强制HTTP/1.1确保稳定性
    "development_mode": True,
    "user_agent": "curl/7.88.1",
    "http_user_agent": "curl/7.88.1"
}
client.initialize(config)

# 发送GET请求
headers = {"User-Agent": "curl/7.88.1"}
try:
    response = client.http_get("http://myip.ipip.net", headers)
    
    if response:
        status = response.get("status", 0)
        body = response.get("body", b"")
        headers_resp = response.get("headers", {})
        
        print(f"状态码: {status}")
        print(f"响应体大小: {len(body)} bytes")
        print(f"响应内容: {body.decode('utf-8', errors='ignore')}")
    else:
        print("请求返回空响应")
        
except Exception as e:
    print(f"请求失败: {e}")
```

#### POST请求

```python
import rat_engine
import json

client = rat_engine.PyClientManager()
config = {
    "connect_timeout": 5000,
    "request_timeout": 10000,
    "max_idle_connections": 5,
    "enable_http": True,
    "enable_grpc": False,
    "http1_only": True,
    "development_mode": True,
    "user_agent": "MyApp/1.0",
    "http_user_agent": "MyApp/1.0"
}
client.initialize(config)

# 准备POST数据
data = {"test": "data", "client": "rat_engine"}
json_data = json.dumps(data).encode('utf-8')

headers = {
    "User-Agent": "MyApp/1.0",
    "Content-Type": "application/json"
}

try:
    response = client.http_post("http://httpbin.org/post", json_data, headers)
    
    if response:
        status = response.get("status", 0)
        body = response.get("body", b"")
        
        print(f"状态码: {status}")
        print(f"响应体大小: {len(body)} bytes")
        print(f"响应内容: {body.decode('utf-8', errors='ignore')}")
        
except Exception as e:
    print(f"POST请求失败: {e}")
```

### 客户端禁用测试

```python
import rat_engine

client = rat_engine.PyClientManager()

# 禁用HTTP客户端的配置
config = {
    "connect_timeout": 5000,
    "request_timeout": 10000,
    "max_idle_connections": 5,
    "enable_http": False,  # 禁用HTTP客户端
    "enable_grpc": False,
    "development_mode": True,
    "user_agent": "MyApp/1.0",
    "http_user_agent": "MyApp/1.0"
}

client.initialize(config)

try:
    # 这应该会失败，因为HTTP客户端被禁用
    response = client.http_get("http://example.com", {"User-Agent": "MyApp/1.0"})
    print("意外成功")
except Exception as e:
    print(f"预期的失败: {e}")
    # 输出: 预期的失败: HTTP GET 请求失败: HTTP 客户端未启用
```

## ⚠️ 当前限制

### 1. HTTPS功能
- **状态**: 当前TLS/HTTPS实现存在底层问题
- **表现**: HTTPS连接无法建立，出现连接错误
- **建议**: 
  - 生产环境中使用其他HTTP客户端库处理HTTPS请求
  - 开发和测试可以使用HTTP协议
  - 如需HTTPS功能，建议使用成熟的库如`requests`或`aiohttp`

### 2. HTTP/2支持
- **状态**: HTTP/2协议处理存在frame解析问题
- **表现**: 出现`frame with invalid size`错误
- **建议**: 使用HTTP/1.1强制模式确保稳定性

### 3. 生产环境建议
- 对于简单的HTTP请求，可以使用RAT Engine的HTTP客户端
- 对于复杂的HTTPS请求，建议使用成熟的HTTP客户端库
- 内部服务间通信（非HTTPS）是最佳使用场景

## 🎯 适用场景

### 推荐使用
- ✅ 内部服务间HTTP通信（非HTTPS）
- ✅ 简单的HTTP API调用
- ✅ 与RAT Engine服务器配套的客户端功能
- ✅ 需要HTTP客户端可选功能的模块化应用
- ✅ 开发和测试环境中的HTTP请求

### 不推荐使用
- ❌ 生产环境中的HTTPS请求
- ❌ 需要HTTP/2特性的应用
- ❌ 对稳定性和可靠性要求极高的场景
- ❌ 复杂的HTTP客户端需求（如重试、熔断等）

## 📁 相关文件

### 测试示例
- `python/examples/http_client_optional_test.py` - HTTP客户端可选功能测试
- `python/examples/test_https_client.py` - HTTPS功能测试（用于问题排查）
- `python/examples/test_https_simple.py` - 简化HTTPS测试

### 源码文件
- `src/python_api/client.rs` - Python API客户端实现
- `src/client/` - 底层HTTP客户端实现目录
- `src/client/builder.rs` - 客户端构建器
- `src/client/http_client.rs` - HTTP客户端核心实现

## 🔄 版本历史

### v1.0.0 (当前版本)
- ✅ 实现HTTP客户端真正可选功能
- ✅ 实现gRPC客户端真正可选功能
- ✅ 添加HTTP/1.1强制模式支持
- ✅ 完整的配置管理和错误处理
- ⚠️ HTTPS和HTTP/2功能存在已知问题

## 🐛 常见问题

### Q: 为什么HTTPS请求会失败？
A: 当前底层HTTP客户端的TLS/HTTPS实现存在问题。建议使用其他HTTP客户端库处理HTTPS请求。

### Q: 如何避免HTTP/2相关问题？
A: 在配置中设置`"http1_only": true`来强制使用HTTP/1.1协议。

### Q: HTTP客户端是RAT Engine的核心功能吗？
A: 不是。HTTP客户端是辅助功能，RAT Engine的核心是高性能HTTP服务器功能。

### Q: 生产环境应该使用什么HTTP客户端？
A: 对于HTTPS和复杂场景，建议使用成熟的HTTP客户端库如`requests`、`aiohttp`等。

## 📝 总结

RAT Engine的HTTP客户端提供了基本的HTTP请求功能，特别适用于内部服务间通信和简单场景。虽然存在一些限制，但HTTP客户端可选功能已经完全实现并经过测试验证。在选择使用时，请根据具体场景和需求做出合适的选择。