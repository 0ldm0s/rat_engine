# RAT Engine 流式响应功能指南

## 概述

RAT Engine 现已支持完整的 HTTP 流式响应功能，包括：

- **Server-Sent Events (SSE)** - 实时数据推送
- **分块传输编码 (Chunked Transfer)** - 大数据分块传输
- **自定义流式响应** - 灵活的流式数据处理
- **Rust 和 Python 双重支持** - 通过 PyO3 绑定提供 Python API

## 🚀 快速开始

### Rust 版本

```bash
# 运行 Rust 流式演示
cd rat_engine
cargo run --example streaming_demo
```

### Python 版本

```bash
# 运行 Python 流式演示
python examples/python_streaming_demo.py
```

### 功能测试

```bash
# 运行自动化测试脚本
python test_streaming.py
```

## 📡 核心功能

### 1. Server-Sent Events (SSE)

SSE 允许服务器向客户端推送实时数据，适用于：
- 实时通知
- 进度更新
- 系统监控
- 聊天应用

#### Rust 实现

```rust
use rat_engine::server::streaming::SseResponse;

// 创建 SSE 响应
let sse = SseResponse::new();

// 发送事件
sse.send_event("update", "Hello from Rust!")?;

// 后台任务发送定期更新
let sender = sse.get_sender();
tokio::spawn(async move {
    for i in 1..=10 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let data = format!("Update #{}", i);
        let formatted = format!("event: update\ndata: {}\n\n", data);
        if sender.send(Ok(Bytes::from(formatted))).is_err() {
            break;
        }
    }
});
```

#### Python 实现

```python
from rat_engine.streaming import create_sse_response
import asyncio
import json

def sse_handler(request):
    sse_response = create_sse_response()
    
    # 发送初始事件
    sse_response.send_event("connected", "Python SSE 连接已建立")
    
    # 异步发送更新
    async def send_updates():
        for i in range(1, 11):
            await asyncio.sleep(1)
            data = {
                "counter": i,
                "message": f"Python 更新 #{i}"
            }
            sse_response.send_event("update", json.dumps(data))
    
    asyncio.create_task(send_updates())
    return sse_response

# 注册 SSE 路由
engine.register_sse_route("GET", "/sse", sse_handler)
```

### 2. 分块传输编码

分块传输适用于：
- 大文件下载
- 数据处理进度
- 流式计算结果

#### Rust 实现

```rust
use rat_engine::server::streaming::ChunkedResponse;

let response = ChunkedResponse::new()
    .add_chunk("开始处理...\n")
    .add_chunk("处理中...\n")
    .add_chunk("处理完成！\n")
    .with_delay(Duration::from_millis(500));
```

#### Python 实现

```python
from rat_engine.streaming import create_chunked_response

def chunked_handler(request):
    response = create_chunked_response()
    
    steps = [
        "🔄 初始化处理器...",
        "📊 加载数据...",
        "🧮 执行分析...",
        "✅ 处理完成！"
    ]
    
    for step in steps:
        response.add_chunk(f"{step}\n")
        response.add_delay(800)  # 800ms 延迟
    
    return response

engine.register_chunked_route("GET", "/chunked", chunked_handler)
```

### 3. JSON 和文本流

#### JSON 流

```python
from rat_engine.streaming import create_json_stream

def json_stream_handler(request):
    data = [
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"},
        {"id": 3, "name": "Charlie"}
    ]
    return create_json_stream(data)
```

#### 文本流

```python
from rat_engine.streaming import create_text_stream

def text_stream_handler(request):
    lines = [
        "第一行文本",
        "第二行文本",
        "第三行文本"
    ]
    return create_text_stream(lines)
```

## 🏗️ 架构设计

### 核心组件

1. **`streaming.rs`** - Rust 流式响应核心实现
   - `StreamingResponse` - 通用流式响应
   - `SseResponse` - SSE 专用响应
   - `ChunkedResponse` - 分块传输响应
   - 工具函数：`json_stream`, `text_stream`, `binary_stream`

2. **`python_api/streaming.rs`** - PyO3 绑定层
   - `PySseResponse` - Python SSE 响应类
   - `PyChunkedResponse` - Python 分块响应类
   - `PyStreamingResponse` - Python 通用流式响应类
   - 转换函数：Python 对象 ↔ Rust 结构体

3. **路由器增强** - `router.rs`
   - `StreamingHandler` - 流式处理器类型
   - `add_sse_route` - SSE 路由注册
   - `add_chunked_route` - 分块路由注册
   - `handle_mixed` - 混合处理支持

### 数据流

```
客户端请求 → 路由器 → 流式处理器 → 响应流 → 客户端
     ↑                                    ↓
     └─────── 实时数据推送 ←──────────────┘
```

## 🔧 技术实现

### 依赖项

```toml
[dependencies]
hyper = { version = "1.0", features = ["server", "http1", "http2"] }
hyper-util = "0.1"
http-body-util = "0.1"
tokio = { version = "1.0", features = ["full"] }
tokio-stream = "0.1"  # 新增：流式支持
futures-util = "0.3"  # 新增：异步工具
pyo3 = { version = "0.20", features = ["extension-module"] }
```

### 关键特性

1. **内存效率** - 使用 Rust 的零拷贝和流式处理
2. **并发安全** - 基于 Tokio 的异步运行时
3. **类型安全** - 强类型系统防止运行时错误
4. **Python 集成** - 通过 PyO3 提供 Pythonic API
5. **HTTP 标准兼容** - 完全符合 HTTP/1.1 和 HTTP/2 规范

## 📊 性能特点

### 优势

- **低延迟** - 数据实时推送，无需轮询
- **高并发** - 支持数千个并发连接
- **内存友好** - 流式处理，不会积累大量内存
- **CPU 高效** - Rust 的零成本抽象

### 适用场景

- ✅ 实时数据监控
- ✅ 进度条和状态更新
- ✅ 聊天和通知系统
- ✅ 大文件传输
- ✅ 流式数据分析
- ✅ 实时日志查看

## 🌐 客户端集成

### JavaScript (浏览器)

```javascript
// SSE 连接
const eventSource = new EventSource('/sse');

eventSource.addEventListener('update', function(event) {
    const data = JSON.parse(event.data);
    console.log('收到更新:', data);
});

eventSource.addEventListener('error', function(event) {
    console.error('SSE 连接错误:', event);
});
```

### Python 客户端

```python
import requests

# SSE 客户端
response = requests.get('http://localhost:8080/sse', stream=True)
for line in response.iter_lines(decode_unicode=True):
    if line.startswith('data: '):
        data = line[6:]  # 移除 'data: ' 前缀
        print(f'收到数据: {data}')

# 分块传输客户端
response = requests.get('http://localhost:8080/chunked', stream=True)
for chunk in response.iter_content(chunk_size=None, decode_unicode=True):
    if chunk:
        print(f'收到块: {chunk}')
```

### cURL 测试

```bash
# 测试 SSE
curl -N http://localhost:8080/sse

# 测试分块传输
curl -N http://localhost:8080/chunked

# 测试 JSON 流
curl -N http://localhost:8080/json-stream
```

## 🔍 调试和监控

### 日志输出

```rust
// 启用详细日志
rat_engine::utils::logger::init();
```

### 性能监控

```python
# Python 性能指标
metrics = engine.get_performance_metrics()
print(f"活跃连接: {metrics.active_connections}")
print(f"处理请求: {metrics.total_requests}")
```

### 错误处理

```rust
// Rust 错误处理
match sse.send_event("update", data) {
    Ok(_) => println!("事件发送成功"),
    Err(e) => eprintln!("发送失败: {}", e),
}
```

## 🚨 注意事项

### 连接管理

1. **客户端断开检测** - 自动清理断开的连接
2. **超时处理** - 设置合理的连接超时
3. **资源限制** - 限制并发连接数

### 安全考虑

1. **CORS 配置** - 正确设置跨域策略
2. **认证授权** - 验证客户端身份
3. **速率限制** - 防止滥用

### 最佳实践

1. **优雅关闭** - 正确处理服务器关闭
2. **错误重试** - 客户端自动重连机制
3. **数据压缩** - 大数据流启用压缩
4. **监控告警** - 监控连接状态和性能

## 📚 示例项目

### 完整示例

- **`examples/streaming_demo.rs`** - Rust 完整演示
- **`examples/python_streaming_demo.py`** - Python 完整演示
- **`test_streaming.py`** - 自动化测试脚本

### 运行演示

```bash
# 1. 启动 Rust 演示服务器
cargo run --example streaming_demo

# 2. 或启动 Python 演示服务器
python examples/python_streaming_demo.py

# 3. 在浏览器中访问
open http://localhost:8080

# 4. 运行测试脚本
python test_streaming.py
```

## 🔮 未来规划

### 计划功能

- [ ] WebSocket 支持
- [ ] 二进制流优化
- [ ] 自动重连机制
- [ ] 流式压缩
- [ ] 更多 Python 工具函数
- [ ] 性能基准测试
- [ ] 集群支持

### 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 项目
2. 创建功能分支
3. 提交更改
4. 创建 Pull Request

---

**RAT Engine** - 高性能 Rust + Python 混合 Web 框架

*支持传统 HTTP 响应和现代流式响应的完整解决方案*