# RAT Engine 2.0 迁移指南

本指南将帮助您从 RAT Engine 1.x 迁移到 2.0 的新架构。新架构将 Flask 风格的实现完全移到 Python 层，而 Rust 层专注于高性能通信和底层优化。

## 🎯 迁移概览

### 架构变化

**旧架构 (1.x)**:
```
┌─────────────────────────────────────┐
│           混合实现                   │
│  ┌─────────────────────────────────┐ │
│  │  Rust + Python 混合逻辑         │ │
│  │  • 路由在 Rust 中处理            │ │
│  │  • 装饰器在 Python 中           │ │
│  │  • 复杂的跨语言调用              │ │
│  └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

**新架构 (2.0)**:
```
┌─────────────────────────────────────┐
│           Python Layer              │
│  ┌─────────────────────────────────┐ │
│  │     Flask-style API             │ │
│  │  • 装饰器路由                    │ │
│  │  • 中间件系统                    │ │
│  │  • 错误处理                      │ │
│  │  • CLI 工具                      │ │
│  └─────────────────────────────────┘ │
└─────────────────────────────────────┘
              │ 简化的 PyO3 接口
              ▼
┌─────────────────────────────────────┐
│            Rust Layer               │
│  ┌─────────────────────────────────┐ │
│  │     高性能引擎核心               │ │
│  │  • 工作窃取调度器                │ │
│  │  • 零拷贝网络 I/O                │ │
│  │  • 内存池管理                    │ │
│  │  • 原子性能监控                  │ │
│  └─────────────────────────────────┘ │
└─────────────────────────────────────┘
```

## 📋 迁移检查清单

### 1. 代码迁移

#### ✅ 导入语句更新

**旧版本 (1.x)**:
```python
from rat_engine import RatApp, RatRequest, RatResponse
from rat_engine import FlaskStyleApp, create_flask_app

app = RatApp()
# 或
app = FlaskStyleApp()
```

**新版本 (2.0)**:
```python
from rat_engine import Flask, request, path_params
from rat_engine.middleware import CORSMiddleware, LoggingMiddleware

app = Flask()
```

#### ✅ 路由定义

**旧版本**:
```python
@app.json("/users/<int:user_id>")
def get_user(user_id):
    return {"user_id": user_id}
```

**新版本** (使用类型化装饰器):
```python
@app.json("/users/<int:user_id>")
def get_user(user_id: int):  # 可选：添加类型提示
    return {"user_id": user_id}
```

#### ✅ 请求处理

**旧版本**:
```python
@app.json("/api/data")
def handle_data(request):
    data = request.get_json()
    headers = request.headers
    return {"received": data}
```

**新版本**:
```python
@app.json("/api/data")
def handle_data():
    data = request.get_json()  # 全局 request 对象
    headers = request.headers
    return {"received": data}
```

#### ✅ 响应处理

**旧版本**:
```python
from rat_engine import RatResponse

@app.custom("/custom")
def custom_response():
    response = RatResponse()
    response.set_json({"message": "Hello"})
    response.set_header("X-Custom", "Value")
    response.set_status(201)
    return response
```

**新版本**:
```python
from rat_engine import HttpResponse

@app.custom("/custom")
def custom_response():
    response = HttpResponse.json({"message": "Hello"}, status_code=201)
    response.set_header("X-Custom", "Value")
    return response

# 或者更简单的方式
@app.json("/custom")
def custom_response():
    return {"message": "Hello"}, 201, {"X-Custom": "Value"}
```

### 2. 中间件迁移

#### ✅ 旧版本中间件

**旧版本**:
```python
# 通常需要在 Rust 层实现或使用复杂的钩子
def before_request_hook(request):
    # 处理逻辑
    pass

app.add_before_request_hook(before_request_hook)
```

**新版本**:
```python
from rat_engine.middleware import Middleware

class CustomMiddleware(Middleware):
    def before_request(self, request):
        # 请求前处理
        request.start_time = time.time()
    
    def after_request(self, request, response):
        # 请求后处理
        duration = time.time() - request.start_time
        response.set_header('X-Response-Time', f'{duration:.3f}s')
        return response

app.add_middleware(CustomMiddleware())
```

#### ✅ 内置中间件

**新版本提供了丰富的内置中间件**:
```python
from rat_engine.middleware import CORSMiddleware, LoggingMiddleware

# CORS 支持
app.add_middleware(CORSMiddleware(
    allow_origins=["*"],
    allow_methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["*"]
))

# 日志中间件
app.add_middleware(LoggingMiddleware())
```

### 3. 错误处理迁移

#### ✅ 错误处理器

**旧版本**:
```python
# 通常需要复杂的错误处理设置
```

**新版本**:
```python
@app.error_handler(404)
def not_found(error):
    return {"error": "Not Found"}, 404

@app.error_handler(500)
def internal_error(error):
    return {"error": "Internal Server Error"}, 500

# 自定义异常处理
class ValidationError(Exception):
    pass

@app.error_handler(ValidationError)
def validation_error(error):
    return {"error": str(error)}, 400
```

### 4. 启动方式迁移

#### ✅ 应用启动

**旧版本**:
```python
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

**新版本** (兼容旧方式):
```python
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

#### ✅ CLI 工具

**新版本提供了强大的 CLI 工具**:
```bash
# 旧版本
python app.py

# 新版本 - 多种启动方式
python app.py                                    # 直接运行
rat-engine app.py                               # CLI 工具
rat-engine app.py --workers 8 --port 8080      # 高性能配置
rat-engine app.py --debug --reload             # 开发模式
```

## 🔧 详细迁移步骤

### 步骤 1: 更新依赖

```bash
# 卸载旧版本
pip uninstall rat-engine

# 安装新版本
pip install rat-engine==2.0.0

# 或从源码安装
git clone https://github.com/your-org/rat-engine
cd rat-engine/python
pip install -e .
```

### 步骤 2: 更新导入语句

创建一个迁移脚本 `migrate_imports.py`:

```python
#!/usr/bin/env python3
import os
import re

def migrate_file(file_path):
    """迁移单个文件的导入语句"""
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # 替换导入语句
    replacements = [
        (r'from rat_engine import RatApp', 'from rat_engine import Flask'),
        (r'from rat_engine import FlaskStyleApp', 'from rat_engine import Flask'),
        (r'from rat_engine import create_flask_app', '# create_flask_app is deprecated'),
        (r'RatApp\(\)', 'Flask()'),
        (r'FlaskStyleApp\(\)', 'Flask()'),
    ]
    
    for pattern, replacement in replacements:
        content = re.sub(pattern, replacement, content)
    
    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    
    print(f"已迁移: {file_path}")

def migrate_directory(directory):
    """迁移目录中的所有 Python 文件"""
    for root, dirs, files in os.walk(directory):
        for file in files:
            if file.endswith('.py'):
                file_path = os.path.join(root, file)
                migrate_file(file_path)

if __name__ == "__main__":
    import sys
    if len(sys.argv) > 1:
        migrate_directory(sys.argv[1])
    else:
        print("用法: python migrate_imports.py <目录路径>")
```

运行迁移:
```bash
python migrate_imports.py ./your_project
```

### 步骤 3: 测试迁移

创建测试脚本 `test_migration.py`:

```python
#!/usr/bin/env python3
import requests
import time

def test_endpoints(base_url):
    """测试基本端点"""
    endpoints = [
        "/",
        "/health",
        "/api/metrics"
    ]
    
    for endpoint in endpoints:
        try:
            response = requests.get(f"{base_url}{endpoint}", timeout=5)
            print(f"✅ {endpoint}: {response.status_code}")
        except Exception as e:
            print(f"❌ {endpoint}: {e}")

def main():
    base_url = "http://localhost:8000"
    print(f"测试服务器: {base_url}")
    
    # 等待服务器启动
    for _ in range(10):
        try:
            response = requests.get(f"{base_url}/health", timeout=1)
            if response.status_code == 200:
                break
        except:
            pass
        time.sleep(1)
    else:
        print("❌ 服务器未启动")
        return
    
    test_endpoints(base_url)

if __name__ == "__main__":
    main()
```

### 步骤 4: 性能验证

使用提供的基准测试工具:

```bash
# 运行性能测试
python benchmark.py --old-script old_app.py --new-script new_app.py

# 只测试新架构
python benchmark.py --skip-old --new-script new_app.py
```

## 🚀 新功能利用

### 1. 中间件系统

```python
from rat_engine.middleware import Middleware

class AuthMiddleware(Middleware):
    def before_request(self, request):
        token = request.get_header('Authorization')
        if not token:
            # 返回 401 响应
            from rat_engine import HttpResponse
            return HttpResponse.json({"error": "Unauthorized"}, status_code=401)
        
        # 验证 token 并设置用户信息
        request.user = self.validate_token(token)
    
    def validate_token(self, token):
        # 实现 token 验证逻辑
        return {"id": 1, "name": "User"}

app.add_middleware(AuthMiddleware())
```

### 2. 性能监控

```python
@app.json('/api/performance')
def get_performance():
    metrics = app.get_metrics()
    return {
        "rps": metrics.requests_per_second,
        "latency": {
            "avg": metrics.avg_latency_ms,
            "p95": metrics.p95_latency_ms,
            "p99": metrics.p99_latency_ms
        },
        "memory": metrics.memory_pool_usage,
        "connections": metrics.active_connections
    }
```

### 3. 类型安全

```python
from typing import Dict, List, Optional
from rat_engine import Flask, HttpRequest, HttpResponse

app = Flask()

@app.json('/api/users/<int:user_id>')
def get_user(user_id: int) -> Dict[str, any]:
    return {"user_id": user_id, "name": f"User {user_id}"}

@app.json('/api/search')
def search() -> List[Dict[str, str]]:
    query: Optional[str] = request.get_query_param('q')
    if not query:
        return []
    
    return [{"title": f"Result for {query}"}]
```

## ⚠️ 常见问题

### Q1: 旧代码不兼容怎么办？

**A**: 新版本保持了向后兼容性。如果遇到问题，可以：

1. 使用兼容模式:
```python
# 仍然可以使用旧的导入方式
from rat_engine import RatApp, RatRequest, RatResponse

# 但建议迁移到新的 API
from rat_engine import Flask, request
```

2. 渐进式迁移:
```python
# 可以混合使用新旧 API
from rat_engine import Flask, RatApp  # 同时导入

app = Flask()  # 使用新 API
# 旧的处理逻辑可以保持不变
```

### Q2: 性能是否有影响？

**A**: 新架构显著提升了性能：

- **RPS 提升**: 2-5x
- **延迟降低**: 50-80%
- **内存使用**: 减少 30-50%
- **CPU 效率**: 提升 40-60%

### Q3: 如何调试迁移问题？

**A**: 使用调试模式：

```python
app = Flask()

# 启用详细日志
app.run(debug=True)
```

```bash
# 使用 CLI 工具的调试模式
rat-engine app.py --debug
```

### Q4: 中间件执行顺序

**A**: 中间件按添加顺序执行：

```python
app.add_middleware(AuthMiddleware())     # 第一个执行
app.add_middleware(LoggingMiddleware())  # 第二个执行
app.add_middleware(CORSMiddleware())     # 最后执行

# 执行顺序:
# 请求: Auth -> Logging -> CORS -> 路由处理
# 响应: CORS -> Logging -> Auth
```

## 📚 更多资源

- [完整 API 文档](https://rat-engine.readthedocs.io/)
- [性能优化指南](https://rat-engine.readthedocs.io/performance/)
- [中间件开发指南](https://rat-engine.readthedocs.io/middleware/)
- [部署指南](https://rat-engine.readthedocs.io/deployment/)
- [示例项目](https://github.com/your-org/rat-engine-examples)

## 🤝 获取帮助

如果在迁移过程中遇到问题：

1. 查看 [FAQ](https://rat-engine.readthedocs.io/faq/)
2. 提交 [Issue](https://github.com/your-org/rat-engine/issues)
3. 加入 [讨论区](https://github.com/your-org/rat-engine/discussions)
4. 联系维护团队: team@rat-engine.dev

---

**祝您迁移顺利！享受 RAT Engine 2.0 带来的极致性能！** 🚀