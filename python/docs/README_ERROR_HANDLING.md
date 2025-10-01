# RAT Engine Python 流式错误处理指南

本文档介绍 RAT Engine Python 侧的流式连接错误处理功能，包括连接管理、错误恢复、状态监控等核心特性。

## 🛡️ 核心特性

### 1. 连接管理 (ConnectionManager)
- **自动连接注册和注销**：跟踪所有活跃的流式连接
- **连接状态监控**：实时监控连接状态变化
- **资源限制**：支持最大连接数限制，防止资源耗尽
- **活动记录**：记录连接的数据传输量和消息数
- **自动清理**：定期清理过期和失效的连接

### 2. 错误恢复 (ErrorRecovery)
- **智能重试机制**：基于错误类型判断是否可重试
- **指数退避**：重试延迟时间逐步增加，避免系统过载
- **错误分类**：区分可恢复和不可恢复的错误类型
- **重试计数**：跟踪每个连接的重试次数

### 3. 流式监控 (StreamMonitor)
- **连接存活检测**：定期检查连接是否仍然活跃
- **异步监控**：使用独立线程监控连接状态
- **断开回调**：连接断开时执行自定义回调函数
- **资源清理**：自动清理监控资源

### 4. 优雅关闭 (GracefulShutdown)
- **渐进式关闭**：先停止接受新连接，再等待现有连接完成
- **超时控制**：设置最大等待时间，避免无限等待
- **强制清理**：超时后强制关闭剩余连接
- **状态报告**：关闭过程中提供详细的状态信息

## 📦 安装和导入

```python
# 导入核心错误处理组件
from rat_engine import (
    ConnectionManager,
    ErrorRecovery,
    StreamMonitor,
    GracefulShutdown,
    ConnectionState,
    ConnectionInfo
)

# 导入便捷函数
from rat_engine import (
    register_streaming_connection,
    unregister_streaming_connection,
    get_streaming_stats,
    shutdown_streaming_system
)
```

## 🚀 快速开始

### 基础用法

```python
from rat_engine import PyRatEngine, register_streaming_connection
from rat_engine.streaming import create_sse_response
import asyncio

def sse_handler(request):
    # 生成连接ID
    connection_id = f"sse_{int(time.time())}_{id(request)}"
    remote_addr = getattr(request, 'remote_addr', 'unknown')
    
    # 创建 SSE 响应
    sse_response = create_sse_response()
    
    # 注册连接（自动启用错误处理）
    success = register_streaming_connection(
        connection_id, 
        remote_addr, 
        sse_response,
        metadata={"type": "sse", "user_agent": "browser"},
        on_disconnect=lambda cid: print(f"连接 {cid} 已断开")
    )
    
    if not success:
        # 连接注册失败（可能达到连接上限）
        response = HttpResponse()
        response.set_status(503)
        response.set_body("Too many connections")
        return response
    
    # 启动数据发送任务
    async def send_data():
        try:
            for i in range(10):
                data = {"counter": i, "message": f"数据 #{i}"}
                success = sse_response.send_event("data", json.dumps(data))
                if not success:
                    break  # 连接已断开
                await asyncio.sleep(1)
        except Exception as e:
            print(f"发送数据时出错: {e}")
        finally:
            # 清理连接
            unregister_streaming_connection(connection_id)
    
    asyncio.create_task(send_data())
    return sse_response
```

### 高级用法

```python
from rat_engine import ConnectionManager, StreamMonitor, ErrorRecovery

class AdvancedStreamingServer:
    def __init__(self):
        # 创建错误处理组件
        self.connection_manager = ConnectionManager(
            max_connections=100,  # 最大连接数
            cleanup_interval=30   # 清理间隔（秒）
        )
        
        self.stream_monitor = StreamMonitor(
            self.connection_manager,
            check_interval=1.0    # 检查间隔（秒）
        )
        
        self.error_recovery = ErrorRecovery(
            max_retries=3,        # 最大重试次数
            base_delay=1.0,       # 基础延迟（秒）
            max_delay=30.0        # 最大延迟（秒）
        )
    
    def handle_sse_with_recovery(self, request):
        connection_id = f"sse_{int(time.time())}_{id(request)}"
        remote_addr = getattr(request, 'remote_addr', 'unknown')
        
        # 手动注册连接
        metadata = {
            "type": "sse_with_recovery",
            "start_time": datetime.now().isoformat()
        }
        
        if not self.connection_manager.register_connection(
            connection_id, remote_addr, None, metadata
        ):
            return self._create_error_response(503, "Too many connections")
        
        sse_response = create_sse_response()
        
        # 启动监控
        self.stream_monitor.start_monitoring(
            connection_id, 
            sse_response,
            on_disconnect=self._handle_disconnect
        )
        
        # 启动数据发送任务
        async def send_data_with_recovery():
            retry_count = 0
            
            while retry_count <= self.error_recovery.max_retries:
                try:
                    for i in range(20):
                        # 检查连接状态
                        if not self.connection_manager.is_connection_alive(connection_id):
                            return
                        
                        # 发送数据
                        data = {"counter": i, "timestamp": datetime.now().isoformat()}
                        success = sse_response.send_event("data", json.dumps(data))
                        
                        if not success:
                            raise Exception("发送失败")
                        
                        # 记录活动
                        self.connection_manager.record_activity(
                            connection_id, len(json.dumps(data)), 1
                        )
                        
                        await asyncio.sleep(0.5)
                    
                    # 成功完成，重置重试计数
                    self.error_recovery.reset_retry_count(connection_id)
                    break
                    
                except Exception as e:
                    # 记录错误
                    self.connection_manager.record_error(connection_id, e)
                    
                    # 判断是否应该重试
                    if self.error_recovery.should_retry(connection_id, e):
                        delay = self.error_recovery.get_retry_delay(connection_id)
                        print(f"连接 {connection_id} 将在 {delay:.1f}s 后重试")
                        await asyncio.sleep(delay)
                        retry_count += 1
                    else:
                        print(f"连接 {connection_id} 不可恢复，停止重试")
                        break
            
            # 清理连接
            self.connection_manager.unregister_connection(connection_id)
        
        asyncio.create_task(send_data_with_recovery())
        return sse_response
    
    def _handle_disconnect(self, connection_id):
        print(f"连接 {connection_id} 已断开")
        # 可以在这里添加自定义的断开处理逻辑
    
    def _create_error_response(self, status, message):
        response = HttpResponse()
        response.set_status(status)
        response.set_body(message)
        return response
    
    def get_stats(self):
        return self.connection_manager.get_stats()
    
    def shutdown(self):
        self.stream_monitor.shutdown()
        self.connection_manager.shutdown()
```

## 📊 监控和统计

### 获取连接统计

```python
from rat_engine import get_streaming_stats

# 获取全局统计
stats = get_streaming_stats()
print(f"活跃连接数: {stats['active_connections']}")
print(f"总连接数: {stats['total_connections']}")
print(f"总传输字节数: {stats['total_bytes_sent']}")
print(f"总消息数: {stats['total_messages_sent']}")

# 获取状态分布
for state, count in stats['state_distribution'].items():
    print(f"{state}: {count} 个连接")
```

### 连接信息查询

```python
from rat_engine import get_default_connection_manager

manager = get_default_connection_manager()

# 获取特定连接信息
conn_info = manager.get_connection_info("connection_id")
if conn_info:
    print(f"连接状态: {conn_info.state.value}")
    print(f"创建时间: {conn_info.created_at}")
    print(f"最后活动: {conn_info.last_activity}")
    print(f"错误次数: {conn_info.error_count}")
    print(f"发送字节数: {conn_info.bytes_sent}")
    print(f"发送消息数: {conn_info.messages_sent}")

# 检查连接是否存活
if manager.is_connection_alive("connection_id"):
    print("连接仍然活跃")
else:
    print("连接已断开或不存在")
```

## 🔧 配置选项

### ConnectionManager 配置

```python
manager = ConnectionManager(
    max_connections=1000,     # 最大连接数
    cleanup_interval=30       # 清理间隔（秒）
)
```

### StreamMonitor 配置

```python
monitor = StreamMonitor(
    connection_manager,
    check_interval=1.0        # 检查间隔（秒）
)
```

### ErrorRecovery 配置

```python
recovery = ErrorRecovery(
    max_retries=3,            # 最大重试次数
    base_delay=1.0,           # 基础延迟（秒）
    max_delay=30.0            # 最大延迟（秒）
)
```

### GracefulShutdown 配置

```python
shutdown_handler = GracefulShutdown(
    connection_manager,
    stream_monitor,
    timeout=30.0              # 关闭超时（秒）
)
```

## 🎯 最佳实践

### 1. 连接生命周期管理

```python
def handle_streaming_request(request):
    connection_id = generate_connection_id()
    
    try:
        # 1. 注册连接
        if not register_streaming_connection(connection_id, request.remote_addr, stream_ref):
            return create_error_response(503, "Too many connections")
        
        # 2. 处理业务逻辑
        result = process_streaming_data()
        
        # 3. 记录活动
        record_activity(connection_id, bytes_sent, messages_sent)
        
        return result
        
    except Exception as e:
        # 4. 记录错误
        record_error(connection_id, e)
        
        # 5. 尝试恢复
        if should_retry(connection_id, e):
            return retry_with_delay(connection_id)
        else:
            return create_error_response(500, "Unrecoverable error")
    
    finally:
        # 6. 清理连接
        unregister_streaming_connection(connection_id)
```

### 2. 错误处理策略

```python
def classify_error(error):
    """错误分类示例"""
    error_str = str(error).lower()
    
    # 网络相关错误 - 可重试
    if any(pattern in error_str for pattern in [
        "connection reset", "broken pipe", "timeout", 
        "network unreachable", "connection refused"
    ]):
        return "network_error", True
    
    # 客户端错误 - 不可重试
    if any(pattern in error_str for pattern in [
        "bad request", "unauthorized", "forbidden", 
        "not found", "method not allowed"
    ]):
        return "client_error", False
    
    # 服务器错误 - 可重试
    if any(pattern in error_str for pattern in [
        "internal server error", "service unavailable", 
        "gateway timeout", "temporary failure"
    ]):
        return "server_error", True
    
    # 默认为不可重试
    return "unknown_error", False
```

### 3. 监控和告警

```python
import logging
from datetime import datetime, timedelta

class StreamingMonitor:
    def __init__(self):
        self.logger = logging.getLogger(__name__)
        self.alert_thresholds = {
            "max_connections": 80,      # 连接数告警阈值（百分比）
            "error_rate": 0.1,          # 错误率告警阈值
            "avg_response_time": 5.0    # 平均响应时间告警阈值（秒）
        }
    
    def check_health(self):
        stats = get_streaming_stats()
        
        # 检查连接数
        connection_usage = stats['active_connections'] / stats['max_connections'] * 100
        if connection_usage > self.alert_thresholds['max_connections']:
            self.logger.warning(f"连接数使用率过高: {connection_usage:.1f}%")
        
        # 检查错误率
        total_connections = stats['total_connections']
        if total_connections > 0:
            error_count = sum(1 for conn in get_all_connections() if conn.error_count > 0)
            error_rate = error_count / total_connections
            if error_rate > self.alert_thresholds['error_rate']:
                self.logger.warning(f"错误率过高: {error_rate:.2%}")
        
        # 记录健康状态
        self.logger.info(f"健康检查完成 - 活跃连接: {stats['active_connections']}, "
                        f"总连接: {stats['total_connections']}, "
                        f"传输量: {stats['total_bytes_sent']} 字节")
```

### 4. 优雅关闭

```python
import signal
import sys

class StreamingApplication:
    def __init__(self):
        self.is_shutting_down = False
        self.setup_signal_handlers()
    
    def setup_signal_handlers(self):
        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)
    
    def signal_handler(self, signum, frame):
        if self.is_shutting_down:
            print("强制退出...")
            sys.exit(1)
        
        print(f"收到信号 {signum}，开始优雅关闭...")
        self.is_shutting_down = True
        self.graceful_shutdown()
    
    def graceful_shutdown(self):
        print("1. 停止接受新连接...")
        # 设置标志位，拒绝新连接
        
        print("2. 等待现有连接完成...")
        # 使用 GracefulShutdown 组件
        shutdown_handler = GracefulShutdown(
            get_default_connection_manager(),
            get_default_stream_monitor(),
            timeout=30.0
        )
        shutdown_handler.initiate_shutdown()
        
        print("3. 清理资源...")
        shutdown_streaming_system()
        
        print("✅ 优雅关闭完成")
        sys.exit(0)
```

## 📝 示例代码

完整的示例代码请参考：

1. **基础错误处理示例**：`examples/streaming_error_handling.py`
2. **增强版流式演示**：`examples/enhanced_streaming_demo.py`
3. **错误处理工具模块**：`rat_engine/error_handling.py`

## 🔍 故障排除

### 常见问题

1. **连接数达到上限**
   ```python
   # 检查当前连接数
   stats = get_streaming_stats()
   print(f"当前连接数: {stats['active_connections']}/{stats['max_connections']}")
   
   # 增加连接上限
   manager = ConnectionManager(max_connections=2000)
   ```

2. **连接清理不及时**
   ```python
   # 减少清理间隔
   manager = ConnectionManager(cleanup_interval=10)  # 10秒清理一次
   
   # 手动触发清理
   manager._cleanup_stale_connections()
   ```

3. **错误恢复失效**
   ```python
   # 检查错误类型
   def debug_error_recovery(connection_id, error):
       recovery = get_default_error_recovery()
       is_recoverable = recovery._is_recoverable_error(error)
       retry_count = recovery.retry_counts.get(connection_id, 0)
       
       print(f"错误: {error}")
       print(f"可恢复: {is_recoverable}")
       print(f"重试次数: {retry_count}/{recovery.max_retries}")
   ```

### 调试技巧

1. **启用详细日志**
   ```python
   import logging
   logging.basicConfig(level=logging.DEBUG)
   ```

2. **监控连接状态**
   ```python
   def monitor_connections():
       while True:
           stats = get_streaming_stats()
           print(f"[{datetime.now()}] 活跃连接: {stats['active_connections']}")
           time.sleep(5)
   ```

3. **错误统计**
   ```python
   def error_statistics():
       manager = get_default_connection_manager()
       error_counts = {}
       
       for conn_id, conn_info in manager.connections.items():
           if conn_info.error_count > 0:
               error_counts[conn_id] = conn_info.error_count
       
       return error_counts
   ```

## 🚀 性能优化

### 1. 连接池优化
```python
# 预分配连接池
manager = ConnectionManager(
    max_connections=1000,
    cleanup_interval=60  # 减少清理频率
)
```

### 2. 监控优化
```python
# 调整监控频率
monitor = StreamMonitor(
    connection_manager,
    check_interval=2.0  # 降低检查频率
)
```

### 3. 内存优化
```python
# 使用弱引用避免内存泄漏
import weakref

class OptimizedConnectionManager(ConnectionManager):
    def register_connection(self, connection_id, remote_addr, connection_ref, metadata):
        # 使用弱引用存储连接对象
        if connection_ref is not None:
            self.connection_refs[connection_id] = weakref.ref(connection_ref)
        return super().register_connection(connection_id, remote_addr, None, metadata)
```

---

## 📞 支持

如果您在使用过程中遇到问题，请：

1. 查看本文档的故障排除部分
2. 检查示例代码
3. 启用详细日志进行调试
4. 提交 Issue 到项目仓库

---

**注意**：本错误处理系统与 Rust 侧的错误处理（如 `hyper::Error(IncompleteMessage)` 的日志级别调整）配合使用，提供完整的端到端错误处理解决方案。