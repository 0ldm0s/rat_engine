# TXT 文件逐行读取器使用指南

## 概述

`txt_line_reader.py` 是一个基于 RAT Engine 的大文件逐行读取演示应用，支持分块上传和实时逐行读取功能。该应用展示了如何使用 RAT Engine 处理大型文本文件，并通过 SSE（Server-Sent Events）实现实时进度推送。

## 主要特性

### 🚀 核心功能
- **分块上传**: 支持大文件分块上传，避免内存溢出
- **逐行读取**: 实时逐行读取文件内容并推送到前端
- **进度监控**: 通过 SSE 实时推送上传和读取进度
- **文件验证**: 支持 SHA-256 哈希验证文件完整性
- **现代化 UI**: 响应式设计，支持拖拽上传

### 📊 技术规格
- **最大文件大小**: 100MB
- **分块大小**: 32KB（可配置）
- **支持格式**: TXT 文本文件
- **编码支持**: UTF-8（自动处理 BOM）
- **并发连接**: 支持多会话同时处理

## 快速开始

### 1. 环境准备

确保已安装 RAT Engine Python 绑定：

```bash
cd /path/to/rat_engine/python
pip install -e .
```

### 2. 启动应用

```bash
python examples/txt_line_reader.py
```

应用将在 `http://127.0.0.1:8089` 启动。

### 3. 使用流程

1. **选择文件**: 点击上传区域或拖拽 TXT 文件
2. **开始上传**: 点击"开始上传"按钮
3. **监控进度**: 观察上传进度条和分块状态
4. **开始读取**: 上传完成后点击"开始逐行读取"
5. **查看内容**: 实时查看文件内容逐行显示

## API 接口

### 上传相关

#### POST `/api/init`
初始化上传会话

**请求体**:
```json
{
  "filename": "example.txt",
  "file_size": 1024000,
  "file_hash": "sha256_hash_optional"
}
```

**响应**:
```json
{
  "session_id": "uuid",
  "chunk_size": 32768,
  "total_chunks": 32
}
```

#### POST `/api/chunk`
上传文件分块

**请求体**:
```json
{
  "session_id": "uuid",
  "chunk_index": 0,
  "chunk_data": "base64_encoded_data"
}
```

### 读取相关

#### POST `/api/start_reading/<session_id>`
开始逐行读取文件

**响应**:
```json
{
  "success": true,
  "message": "开始逐行读取文件"
}
```

#### GET `/api/progress/<session_id>` (SSE)
SSE 进度推送端点

**事件类型**:
- `init`: 连接建立
- `upload_progress`: 上传进度更新
- `upload_completed`: 上传完成
- `reading_started`: 开始读取
- `line_content`: 行内容推送
- `reading_completed`: 读取完成
- `heartbeat`: 心跳保持连接

### 状态查询

#### GET `/api/status/<session_id>`
获取会话状态

**响应**:
```json
{
  "session_id": "uuid",
  "filename": "example.txt",
  "file_size": 1024000,
  "upload_progress": 100.0,
  "upload_completed": true,
  "reading_status": "completed",
  "current_line": 1000,
  "total_lines": 1000,
  "reading_completed": true
}
```

## 配置选项

### 服务器配置

```python
# 服务器设置
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8089

# 文件处理设置
UPLOAD_DIR = Path(__file__).parent / "txt_uploads"
CHUNK_SIZE = 32 * 1024  # 32KB
MAX_FILE_SIZE = 100 * 1024 * 1024  # 100MB
READ_BUFFER_SIZE = 8192
```

### 前端配置

```javascript
class TxtLineReader {
    constructor() {
        this.chunkSize = 32768; // 32KB，需与后端一致
        // 其他配置...
    }
}
```

## 技术实现

### 分块上传机制

1. **文件分割**: 前端将文件按 32KB 分块
2. **Base64 编码**: 分块数据进行 Base64 编码传输
3. **顺序写入**: 后端按索引顺序写入临时文件
4. **完整性验证**: 可选的 SHA-256 哈希验证

### SSE 实时推送

```python
@app.sse("/api/progress/<session_id>")
def progress_stream(request_data, session_id):
    # SSE 连接管理
    # 消息队列处理
    # 心跳保持连接
```

### 逐行读取策略

```python
def _read_file_lines(session_id: str):
    # 1. 计算总行数
    # 2. 逐行读取并广播
    # 3. 进度更新
    # 4. 完成通知
```

## 错误处理

### 常见错误

| 错误类型 | 原因 | 解决方案 |
|---------|------|----------|
| 文件过大 | 超过 100MB 限制 | 分割文件或调整 MAX_FILE_SIZE |
| 编码错误 | 非 UTF-8 编码 | 转换文件编码为 UTF-8 |
| 网络中断 | SSE 连接断开 | 刷新页面重新连接 |
| 内存不足 | 文件过大导致内存溢出 | 减小 CHUNK_SIZE 或增加系统内存 |

### 错误日志

应用会在控制台输出详细的错误信息：

```
❌ 读取文件时出错: [Errno 2] No such file or directory
⚠️ 检测到会话ID不匹配的消息，已丢弃
🔍 文件哈希验证: ❌ 失败
```

## 性能优化

### 建议配置

**小文件（< 1MB）**:
```python
CHUNK_SIZE = 16 * 1024  # 16KB
```

**大文件（> 10MB）**:
```python
CHUNK_SIZE = 64 * 1024  # 64KB
READ_BUFFER_SIZE = 16384  # 16KB
```

### 内存优化

1. **流式处理**: 避免一次性加载整个文件
2. **分块读取**: 使用固定大小的缓冲区
3. **及时清理**: 完成后清理临时文件和会话数据

## 安全考虑

### 文件验证

```python
# 文件类型检查
if not filename.lower().endswith('.txt'):
    return {"error": "仅支持 TXT 文件"}

# 文件大小限制
if file_size > MAX_FILE_SIZE:
    return {"error": f"文件过大，最大支持 {MAX_FILE_SIZE} 字节"}
```

### 路径安全

```python
# 安全的文件名处理
safe_filename = secure_filename(filename)
final_file = UPLOAD_DIR / safe_filename
```

## 扩展开发

### 添加新的文件格式支持

```python
# 在 init_upload 函数中添加
SUPPORTED_EXTENSIONS = ['.txt', '.csv', '.log']
if not any(filename.lower().endswith(ext) for ext in SUPPORTED_EXTENSIONS):
    return {"error": "不支持的文件格式"}
```

### 自定义读取策略

```python
def _read_file_custom(session_id: str, strategy='line'):
    if strategy == 'line':
        # 逐行读取
    elif strategy == 'chunk':
        # 分块读取
    elif strategy == 'word':
        # 逐词读取
```

## 故障排除

### 调试模式

启用详细日志：

```python
app.configure_logging(level="debug", enable_access_log=True)
```

### 常见问题

**Q: 上传进度卡住不动？**
A: 检查网络连接和服务器日志，可能是分块数据损坏。

**Q: SSE 连接频繁断开？**
A: 调整心跳间隔或检查防火墙设置。

**Q: 文件读取速度慢？**
A: 增大 READ_BUFFER_SIZE 或减少读取间隔时间。

## 相关文档

- [RAT Engine 官方文档](../README.md)
- [SSE 流式传输指南](./SSE_STREAMING_GUIDE.md)
- [环境配置指南](./ENV_SETUP.md)
- [错误处理指南](./README_ERROR_HANDLING.md)

## 许可证

本示例遵循 RAT Engine 的开源许可证。