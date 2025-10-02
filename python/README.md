# RAT Engine Python

**高性能 Rust + Python Web 框架**

RAT Engine 是一个革命性的 Web 框架，将 Rust 的极致性能与 Python 的开发便利性完美结合。通过工作窃取调度器、零拷贝网络 I/O 和内存池管理，实现了前所未有的性能表现。

## ✨ 特性

### 🌐 HTTP 框架
- 🚀 **极致性能**: 基于Rust的零成本抽象和内存安全
- 🐍 **Web应用兼容**: 100%兼容Web应用API，无缝迁移
- ⚡ **异步支持**: 内置高性能异步处理
- 🔧 **易于使用**: 熟悉的Python API，学习成本低
- 🛡️ **内存安全**: Rust保证的内存安全和并发安全
- 📡 **SSE 流式响应**: 完整的 Server-Sent Events 支持
- 📦 **分块传输**: 高效的大文件和实时数据传输

### ⚡ QuickMem 编解码 (新集成)
- 🏃 **超高性能**: 比 JSON 快 2-10x，体积减少 20-50%
- 🔒 **类型安全**: 完整的 Python 类型支持
- 📦 **批量操作**: 高效的批量编解码处理
- 🧠 **内存优化**: 智能内存池管理
- 🚀 **SIMD 加速**: 硬件级性能优化
- 🔄 **无缝集成**: 与 HTTP 框架完美结合

### 🎯 性能优化
- 🧠 **mimalloc**: Microsoft 高性能内存分配器
- 🔗 **CPU 亲和性**: 自动绑定 CPU 核心优化
- 📊 **多线程**: 基于 CPU 核心数自动配置工作线程
- 💾 **内存池**: 智能内存管理和复用

## 📦 安装

### 开发模式安装

```bash
# 克隆仓库
git clone https://github.com/rat-engine/rat-engine.git
cd rat-engine/rat_engine/python

# 开发模式安装（支持热重载）
make dev
```

### 生产环境安装

```bash
# 构建生产版本
make build

# 安装构建的 wheel 包
pip install dist/rat_engine_py-*.whl
```

## 🚀 快速开始

### 基础 Web 服务器

```python
from rat_engine import WebApp

app = WebApp()

@app.route("/")
def hello():
    return "Hello, RAT Engine!"

@app.route("/api/data")
def get_data():
    return {"message": "Hello from RAT Engine", "status": "success"}

if __name__ == "__main__":
    app.run("127.0.0.1", 3000)
```

### 📡 SSE 流式响应 (新功能)

#### 文本流响应

```python
from rat_engine import WebApp

app = WebApp()

# 支持字符串返回
@app.sse_text
def text_stream_string():
    return "第一行\n第二行\n第三行"

# 支持列表返回（自动转换）
@app.sse_text
def text_stream_list():
    return [
        "第一行文本",
        "第二行文本",
        "第三行文本",
        "最后一行文本"
    ]

app.run("127.0.0.1", 3000)
```

#### JSON 流响应

```python
import time

@app.sse_json
def json_stream():
    for i in range(5):
        yield {"count": i, "timestamp": time.time(), "message": f"数据 {i}"}
        time.sleep(1)
```

#### 通用 SSE 响应

```python
@app.sse
def custom_stream():
    for i in range(10):
        yield f"data: 自定义消息 {i}\n\n"
        time.sleep(0.5)
```

### 📦 分块传输

```python
@app.chunk
def large_data():
    # 适用于大文件或实时数据传输
    for chunk in generate_large_data():
        yield chunk
```

### 🔧 高级用法

#### 请求处理

```python
@app.route("/api/user", methods=["POST"])
def create_user(request):
    # 获取请求数据
    data = request.json()  # JSON 数据
    form_data = request.form()  # 表单数据
    query = request.query()  # 查询参数
    headers = request.headers()  # 请求头

    return {"status": "created", "data": data}
```

## 🛣️ 路径参数 (高级功能)

RAT Engine 支持强大的路径参数功能，包括类型约束和验证。

### ⚠️ **重要设计原则**

#### 🚨 避免路由冲突的最佳实践

**1. 避免相似结构的路由组合**

```python
# ❌ 避免这种设计！容易产生冲突
@app.json("/mixed/<int:user_id>/<str:category>/<float:price>")
def handle_mixed_params(request_data):
    # 期望: /mixed/123/electronics/299.99
    pass

@app.json("/mixed/<int:user_id>/<path:file_path>")
def handle_mixed_file_path(request_data):
    # 期望: /mixed/456/docs/manual.pdf
    # 🚨 问题: docs/manual.pdf 可能被误判为浮点数参数
    pass
```

**2. 如果必须使用相似路由，请遵循注册顺序原则**

```python
# ✅ 正确的注册顺序
@app.json("/mixed/<int:user_id>/<str:category>/<float:price>")  # 先注册更具体的路由
def handle_mixed_params(request_data):
    pass

@app.json("/mixed/<int:user_id>/<path:file_path>")  # 后注册更通用的路由
def handle_mixed_file_path(request_data):
    pass
```

**3. 使用更明确的路由设计**

```python
# ✅ 更好的设计 - 避免冲突
@app.json("/api/products/<int:id>/price/<float:price>")
def get_product_price(request_data):
    # 专门的价格路由，明确且无冲突
    pass

@app.json("/api/products/<int:id>/files/<path:file_path>")
def get_product_files(request_data):
    # 专门的文件路由，明确且无冲突
    pass

@app.json("/api/mixed-data/<int:user_id>/<category>/<price>")
def get_mixed_data(request_data):
    # 使用通用参数，让应用层处理类型转换
    pass
```

**4. 路由注册顺序影响**

```python
# ⚠️ 注意：后注册的路由在某些情况下会影响优先级
# 建议按从具体到通用的顺序注册路由

# 1. 最具体的路由（包含最多类型约束）
app.add_route("/api/v1/users/<int:user_id>/profile/<str:section>", handler)

# 2. 中等具体的路由
app.add_route("/api/v1/users/<int:user_id>", handler)

# 3. 最通用的路由（path参数等）
app.add_route("/api/v1/<path:remaining_path>", handler)
```

### 📋 支持的参数类型

- `<param>` - 默认整数类型 (int)
- `<int:param>` - 整数类型
- `<str:param>` - 字符串类型
- `<float:param>` - 浮点数类型
- `<uuid:param>` - UUID 字符串类型
- `<path:param>` - 路径类型（可包含斜杠）

### ⚠️ 重要：path 类型参数约束

**当使用 `<path:param>` 类型参数时，必须遵守以下规则：**

1. **🚨 必须明确指定 `path:` 类型前缀**
   - ✅ 正确：`/files/<path:file_path>`
   - ❌ 错误：`/files/<file_path>` (这会被当作int类型，无法匹配多级路径)

2. **path 参数必须是路由的最后一个参数**
3. **path 参数会消耗从当前位置开始的所有后续路径段**
4. **path 参数后面不能有其他参数**

### 🚨 为什么必须使用 `<path:param>` 格式？

如果不指定类型前缀，系统会将参数默认为 **int 类型**：

```python
# ❌ 错误！这会被当作int类型，无法匹配包含斜杠的路径
@app.json("/files/<file_path>")
def get_file(request_data, path_args):
    # /files/docs/readme.md 无法匹配，因为 "docs/readme.md" 不是有效整数
    pass

# ✅ 正确！明确指定为path类型
@app.json("/files/<path:file_path>")
def get_file(request_data, path_args):
    # /files/docs/readme.md 可以正确匹配，file_path="docs/readme.md"
    pass
```

### ✅ 正确的路由定义示例

```python
from rat_engine import RatApp

app = RatApp()

# 基础参数
@app.json("/users/<user_id>")
def get_user(request_data, path_args):
    # user_id 会自动转换为整数
    user_id = request_data.get('path_params', {}).get('user_id')
    return {"user_id": int(user_id)}

# 类型约束参数
@app.json("/products/<float:price>")
def get_product_by_price(request_data, path_args):
    price = request_data.get('path_params', {}).get('price')
    return {"price": float(price)}

# UUID 参数
@app.json("/users/<uuid:user_id>")
def get_user_by_uuid(request_data, path_args):
    user_id = request_data.get('path_params', {}).get('user_id')
    return {"user_id": user_id}

# ✅ path 参数 - 正确用法（必须是最后一个参数）
@app.json("/files/<path:file_path>")
def get_file(request_data, path_args):
    file_path = request_data.get('path_params', {}).get('file_path')
    return {"file_path": file_path}

# 混合参数 - path作为最后一个参数
@app.json("/users/<int:user_id>/files/<path:file_path>")
def get_user_file(request_data, path_args):
    params = request_data.get('path_params', {})
    user_id = params.get('user_id')
    file_path = params.get('file_path')
    return {"user_id": int(user_id), "file_path": file_path}
```

### ❌ 错误的路由定义示例

```python
# ❌ 最常见错误：忘记指定path类型前缀
@app.json("/files/<file_path>")
def get_file(request_data, path_args):
    # 🚨 错误！这会被当作int类型处理
    # /files/docs/readme.md 无法匹配，因为 "docs/readme.md" 不是整数
    pass

# ❌ path 参数不能在中间位置
@app.json("/files/<path:file_path>/download")
def download_file(request_data, path_args):
    # 这会导致路由无法正确匹配！
    pass

# ❌ path 参数后面不能有其他参数
@app.json("/files/<path:file_path>/<ext>")
def get_file_with_ext(request_data, path_args):
    # 这也会导致路由无法正确匹配！
    pass

# ❌ 避免易产生冲突的路由组合
@app.json("/mixed/<int:user_id>/<str:category>/<float:price>")
def handle_mixed_params(request_data):
    pass

@app.json("/mixed/<int:user_id>/<path:file_path>")
def handle_mixed_file_path(request_data):
    # 🚨 极端场景警告！
    # 1. 两个路由都有相似的结构（整数参数开头）
    # 2. 一个期望浮点数，一个期望路径
    # 3. 可能导致 /mixed/123/docs/manual.pdf 匹配不明确
    pass
```

### 🔍 常见错误排查

如果你的路由无法匹配包含斜杠的路径，请检查：

1. **是否明确指定了 `<path:param>` 格式？**
2. **path参数是否是路由的最后一个参数？**
3. **请求路径是否与路由模式匹配？**

```python
# 调试技巧：启用debug日志查看路由匹配过程
app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)

# 这将显示详细的路由匹配信息，帮助定位问题
```

### 🧪 路径参数匹配示例

| 路由模式 | 请求路径 | 提取的参数 |
|---------|---------|-----------|
| `/files/<path:file_path>` | `/files/readme.md` | `file_path="readme.md"` |
| `/files/<path:file_path>` | `/files/docs/user/manual.pdf` | `file_path="docs/user/manual.pdf"` |
| `/users/<int:id>/files/<path:file_path>` | `/users/123/docs/report.pdf` | `id="123", file_path="docs/report.pdf"` |

### 🔧 类型转换和验证

```python
@app.json("/products/<float:price>")
def get_product(request_data, path_args):
    params = request_data.get('path_params', {})
    price_str = params.get('price', '0')

    # 手动类型转换和验证
    try:
        price = float(price_str)
        is_valid = True
        param_type = "float"
    except ValueError:
        price = 0.0
        is_valid = False
        param_type = "invalid"

    return {
        "price": price,
        "price_str": price_str,
        "is_valid": is_valid,
        "type": param_type
    }
```

#### 响应类型

```python
from rat_engine import HttpResponse

@app.route("/custom")
def custom_response():
    # 文本响应
    return HttpResponse.text("Hello World")
    
    # JSON 响应
    return HttpResponse.json({"key": "value"})
    
    # HTML 响应
    return HttpResponse.html("<h1>Hello</h1>")
    
    # SSE 响应
    return HttpResponse.sse_text("实时文本数据")
    
    # 重定向
    return HttpResponse.redirect("/new-path")
    
    # 错误响应
    return HttpResponse.error(404, "Not Found")
```

## 🔧 开发工具

### Makefile 命令

```bash
# 开发环境安装
make dev

# 构建生产版本
make build

# 运行测试
make test

# 清理构建文件
make clean

# 格式化代码
make format

# 代码检查
make lint
```

### 调试和日志

```python
# 启用详细日志
app.run("127.0.0.1", 3000, debug=True)

# 性能监控
app.run("127.0.0.1", 3000, metrics=True)
```

## 🧪 完整示例

查看 `examples/streaming_demo.py` 获取完整的功能演示：

```bash
cd examples
python streaming_demo.py
```

演示包含：
- 📡 SSE 文本流和 JSON 流
- 📦 分块传输
- 🔍 请求头信息测试
- 📊 性能监控
- 🧪 自动化测试

访问 http://127.0.0.1:3000 查看交互式演示页面。

## 📊 性能基准

### 内存优化
- **mimalloc**: Microsoft 高性能内存分配器
- **零拷贝**: 与 RAT QuickMem 集成
- **CPU 亲和性**: 自动绑定 CPU 核心
- **内存池**: 智能内存管理和复用

### 并发处理
- **多线程**: 基于 CPU 核心数自动配置工作线程
- **异步 I/O**: Tokio 异步运行时
- **连接池**: 自动管理连接资源
- **工作窃取**: 高效的任务调度

## 🔗 生态系统

RAT Engine 是 RAT 生态系统的核心组件：

- **RAT QuickMem**: 高性能内存管理和零拷贝传输
- **RAT PM**: 进程管理和监控
- **Zerg Creep**: 统一日志系统
- **Zerg Hive**: 分布式服务网格

## 📝 更新日志

### v0.2.1 (最新)
- ✅ **SSE 增强**: `@sse_text` 装饰器支持列表和字符串返回值
- ✅ **类型处理**: 优化 SSE 响应类型自动转换
- ✅ **性能优化**: 改进内存分配和 CPU 亲和性
- ✅ **错误处理**: 完善错误处理和日志记录
- ✅ **开发体验**: 增强调试信息和自动测试

### v0.2.0
- 🎉 首个稳定版本发布
- 🚀 完整的 SSE 和分块传输支持
- 🔧 开发工具链完善
- 📦 QuickMem 集成

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

### 开发环境设置

```bash
# 克隆项目
git clone <repository-url>
cd rat_engine/python

# 设置开发环境
make dev

# 运行测试
make test
```

## 📄 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件。

---

**RAT Engine** - 让 Python Web 开发拥有 Rust 的性能 🚀

*"Performance meets Productivity"*