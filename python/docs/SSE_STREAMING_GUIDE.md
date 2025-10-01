# RAT Engine SSE 流式传输使用指南

## 概述

RAT Engine 提供了完整的 Server-Sent Events (SSE) 支持，允许服务器向客户端实时推送数据。本指南详细介绍如何使用 `@app.sse` 装饰器实现流式数据传输。

## 核心特性

- ✅ **标准 SSE 协议支持**：完全符合 W3C SSE 规范
- ✅ **生成器友好**：直接支持 Python 生成器函数
- ✅ **路径参数支持**：支持动态路由参数
- ✅ **自动格式化**：自动处理 SSE 数据格式
- ✅ **连接管理**：自动处理连接生命周期

## 基本用法

### 1. 简单 SSE 端点

```python
from rat_engine import RatApp
import json
import time

app = RatApp(name="sse_demo")

@app.sse("/api/events")
def event_stream(request_data):
    """简单的事件流"""
    for i in range(10):
        yield json.dumps({
            'type': 'counter',
            'value': i,
            'timestamp': time.time()
        })
        time.sleep(1)
```

### 2. 带路径参数的 SSE 端点

```python
@app.sse("/api/progress/<session_id>")
def progress_stream(request_data, session_id):
    """进度监控流"""
    # 发送初始状态
    yield json.dumps({
        'type': 'init',
        'session_id': session_id,
        'status': 'connected'
    })
    
    # 模拟进度更新
    for progress in range(0, 101, 10):
        yield json.dumps({
            'type': 'progress',
            'session_id': session_id,
            'progress': progress
        })
        time.sleep(0.5)
    
    # 发送完成消息
    yield json.dumps({
        'type': 'completed',
        'session_id': session_id,
        'progress': 100
    })
```

## SSE 数据格式规范

### 标准 SSE 格式

SSE 协议要求每条消息必须以 `data: ` 开头，以双换行符 `\n\n` 结尾：

```
data: {"type": "message", "content": "Hello World"}

```

### 自动格式化

RAT Engine 会自动将生成器产生的字符串包装成标准 SSE 格式：

```python
# 你的代码
yield json.dumps({"message": "hello"})

# 实际发送的格式
# data: {"message": "hello"}
# 
```

### 手动格式化（可选）

如果需要更精细的控制，可以手动格式化：

```python
@app.sse("/api/custom")
def custom_stream(request_data):
    # 手动格式化（推荐让框架自动处理）
    yield f"data: {json.dumps({'type': 'custom'})}\n\n"
    
    # 发送事件类型
    yield f"event: notification\ndata: {json.dumps({'alert': 'New message'})}\n\n"
```

## 实际应用示例

### 文件上传进度监控

```python
import uuid
import json
import time
from collections import defaultdict

# 全局消息队列
sse_messages = defaultdict(list)
upload_sessions = {}

@app.sse("/api/progress/<session_id>")
def progress_stream(request_data, session_id):
    """文件上传进度流"""
    try:
        # 获取会话信息
        session = upload_sessions.get(session_id, {})
        
        # 发送初始状态
        initial_message = {
            'type': 'init',
            'session_id': session_id,
            'filename': session.get('filename', ''),
            'progress': session.get('progress', 0.0),
            'completed': session.get('completed', False)
        }
        yield json.dumps(initial_message)
        
        # 如果已完成，发送完成消息
        if session.get('completed', False):
            yield json.dumps({
                'type': 'completed',
                'session_id': session_id,
                'filename': session['filename'],
                'file_size': session['file_size'],
                'download_url': f'/api/download/{session_id}',
                'progress': 100.0
            })
            return
        
        # 监听消息队列
        start_time = time.time()
        last_heartbeat = time.time()
        
        while time.time() - start_time < 300:  # 5分钟超时
            time.sleep(0.1)
            
            # 处理待发送的消息
            if session_id in sse_messages and sse_messages[session_id]:
                message = sse_messages[session_id].pop(0)
                if message.get('session_id') == session_id:
                    yield json.dumps(message)
                    if message.get('type') == 'completed':
                        return
            
            # 检查会话状态
            session = upload_sessions.get(session_id, {})
            if session.get('completed', False):
                yield json.dumps({
                    'type': 'completed',
                    'session_id': session_id,
                    'filename': session['filename'],
                    'file_size': session['file_size'],
                    'download_url': f'/api/download/{session_id}',
                    'progress': 100.0
                })
                return
            
            # 心跳检测
            current_time = time.time()
            if current_time - last_heartbeat >= 10:
                yield json.dumps({
                    'type': 'heartbeat',
                    'timestamp': current_time
                })
                last_heartbeat = current_time
                
    except Exception as e:
        yield json.dumps({
            'type': 'error',
            'message': str(e)
        })

def broadcast_progress(session_id, message):
    """广播进度消息到 SSE 流"""
    message['session_id'] = session_id
    sse_messages[session_id].append(message)
    print(f"📡 [{session_id}] {message['type']}: {message}")
```

### 实时日志流

```python
import subprocess
import threading

@app.sse("/api/logs/<service_name>")
def log_stream(request_data, service_name):
    """实时日志流"""
    try:
        # 启动日志进程
        process = subprocess.Popen(
            ['tail', '-f', f'/var/log/{service_name}.log'],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True
        )
        
        # 发送连接确认
        yield json.dumps({
            'type': 'connected',
            'service': service_name,
            'timestamp': time.time()
        })
        
        # 流式读取日志
        for line in iter(process.stdout.readline, ''):
            if line:
                yield json.dumps({
                    'type': 'log',
                    'service': service_name,
                    'message': line.strip(),
                    'timestamp': time.time()
                })
            
    except Exception as e:
        yield json.dumps({
            'type': 'error',
            'message': str(e)
        })
    finally:
        if 'process' in locals():
            process.terminate()
```

## 客户端使用

### JavaScript EventSource

```javascript
// 连接 SSE 流
const eventSource = new EventSource('/api/progress/session123');

// 监听消息
eventSource.onmessage = function(event) {
    const data = JSON.parse(event.data);
    console.log('收到消息:', data);
    
    switch(data.type) {
        case 'init':
            console.log('连接已建立:', data.session_id);
            break;
        case 'progress':
            updateProgressBar(data.progress);
            break;
        case 'completed':
            console.log('任务完成');
            eventSource.close();
            break;
        case 'error':
            console.error('错误:', data.message);
            break;
    }
};

// 错误处理
eventSource.onerror = function(event) {
    console.error('SSE 连接错误:', event);
};

// 关闭连接
// eventSource.close();
```

### curl 测试

```bash
# 测试 SSE 端点
curl -N -H "Accept: text/event-stream" http://localhost:8087/api/progress/test123

# 输出示例：
# data: {"type": "init", "session_id": "test123", "progress": 0}
# 
# data: {"type": "progress", "session_id": "test123", "progress": 50}
# 
# data: {"type": "completed", "session_id": "test123", "progress": 100}
```

## 最佳实践

### 1. 错误处理

```python
@app.sse("/api/safe-stream")
def safe_stream(request_data):
    try:
        for i in range(100):
            yield json.dumps({'progress': i})
            time.sleep(0.1)
    except Exception as e:
        yield json.dumps({
            'type': 'error',
            'message': str(e)
        })
    finally:
        # 清理资源
        yield json.dumps({'type': 'closed'})
```

### 2. 连接超时管理

```python
@app.sse("/api/timeout-stream")
def timeout_stream(request_data):
    start_time = time.time()
    timeout = 300  # 5分钟超时
    
    while time.time() - start_time < timeout:
        # 业务逻辑
        yield json.dumps({'timestamp': time.time()})
        time.sleep(1)
    
    # 超时处理
    yield json.dumps({
        'type': 'timeout',
        'message': '连接超时'
    })
```

### 3. 心跳检测

```python
@app.sse("/api/heartbeat-stream")
def heartbeat_stream(request_data):
    last_heartbeat = time.time()
    
    while True:
        current_time = time.time()
        
        # 每30秒发送心跳
        if current_time - last_heartbeat >= 30:
            yield json.dumps({
                'type': 'heartbeat',
                'timestamp': current_time
            })
            last_heartbeat = current_time
        
        time.sleep(1)
```

## 性能优化

### 1. 避免阻塞操作

```python
# ❌ 错误：阻塞操作
@app.sse("/api/blocking")
def blocking_stream(request_data):
    for i in range(1000):
        # 阻塞的数据库查询
        result = expensive_database_query()
        yield json.dumps(result)

# ✅ 正确：使用缓存或异步处理
@app.sse("/api/non-blocking")
def non_blocking_stream(request_data):
    for i in range(1000):
        # 从缓存或消息队列获取数据
        result = get_from_cache(i)
        yield json.dumps(result)
        time.sleep(0.01)  # 让出 CPU
```

### 2. 内存管理

```python
@app.sse("/api/memory-efficient")
def memory_efficient_stream(request_data):
    # 使用生成器避免一次性加载大量数据
    for chunk in process_large_dataset_in_chunks():
        yield json.dumps(chunk)
        # 及时释放内存
        del chunk
```

## 故障排除

### 常见问题

1. **数据格式错误**
   - 确保 JSON 数据有效
   - 检查特殊字符转义

2. **连接中断**
   - 实现心跳检测
   - 添加重连机制

3. **性能问题**
   - 避免在生成器中执行耗时操作
   - 使用适当的 sleep 间隔

### 调试技巧

```python
@app.sse("/api/debug-stream")
def debug_stream(request_data):
    # 添加调试信息
    yield json.dumps({
        'type': 'debug',
        'request_info': {
            'method': request_data.get('method'),
            'path': request_data.get('path'),
            'headers': dict(request_data.get('headers', {}))
        }
    })
    
    # 正常业务逻辑
    for i in range(10):
        yield json.dumps({'data': i})
```

## 总结

RAT Engine 的 SSE 支持提供了强大而灵活的实时数据推送能力。通过合理使用 `@app.sse` 装饰器和遵循最佳实践，可以构建高性能的实时应用。

关键要点：
- 使用生成器函数实现流式数据
- 遵循标准 SSE 数据格式
- 实现适当的错误处理和超时管理
- 注意性能优化和内存管理
- 添加心跳检测保持连接活跃