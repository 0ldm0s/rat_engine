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