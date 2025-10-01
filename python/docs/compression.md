# RAT Engine 压缩功能文档

## 概述

RAT Engine 支持多种压缩算法，可以自动根据客户端支持的算法和内容类型选择最佳的压缩方式。启用压缩功能可以显著减少网络传输的数据量，提高应用性能，特别是对于文本内容（HTML、CSS、JavaScript、JSON 等）。

## 支持的压缩算法

RAT Engine 支持以下压缩算法：

- **Gzip**：广泛支持的通用压缩算法，兼容性最好
- **Deflate**：类似 Gzip 但没有头和校验和，体积稍小
- **Brotli**：Google 开发的压缩算法，对文本内容有更好的压缩率
- **Zstd**：Facebook 开发的压缩算法，提供更好的压缩速度和比率平衡
- **LZ4**：极快的压缩算法，压缩率较低但速度极快

## 使用方法

### 基本用法

最简单的方式是使用默认配置启用压缩：

```python
# 创建应用实例
app = rat_engine.RatApp(config)

# 启用压缩（使用默认配置）
app.enable_compression()
```

### 自定义配置

你可以自定义压缩配置，例如：

```python
# 自定义压缩配置
app.enable_compression(
    min_size=2048,           # 最小压缩大小（字节）
    level=9,                 # 压缩级别（1-9，9为最高压缩率）
    enable_gzip=True,        # 启用 Gzip
    enable_deflate=False,    # 禁用 Deflate
    enable_brotli=True,      # 启用 Brotli
    enable_zstd=True,        # 启用 Zstd
    enable_lz4=False,        # 禁用 LZ4
    excluded_content_types=["image/jpeg", "image/png"],  # 排除的内容类型
    excluded_extensions=["jpg", "png", "gif"],          # 排除的文件扩展名
)
```

### 使用 CompressionConfig 对象

你也可以创建一个 `CompressionConfig` 对象，然后将其传递给 `enable_compression` 方法：

```python
from rat_engine.compression import CompressionConfig, CompressionType

# 创建压缩配置对象
compression_config = CompressionConfig(
    min_size=1024,
    level=6,
    enable_gzip=True,
    enable_deflate=True,
    enable_brotli=True,
    enable_zstd=True,
    enable_lz4=False,
)

# 使用配置对象启用压缩
app.enable_compression(compression_config)
```

### 链式调用

`CompressionConfig` 对象支持链式调用，可以更灵活地配置：

```python
compression_config = CompressionConfig()\
    .min_size(1024)\
    .level(6)\
    .enable_gzip()\
    .disable_deflate()\
    .enable_brotli()\
    .enable_zstd()\
    .disable_lz4()\
    .exclude_content_types(["image/jpeg", "image/png"])\
    .exclude_extensions(["jpg", "png"])

app.enable_compression(compression_config)
```

### 禁用压缩

如果需要禁用压缩功能，可以调用 `disable_compression` 方法：

```python
app.disable_compression()
```

## 配置选项详解

### min_size

`min_size` 参数指定了需要压缩的最小内容大小（以字节为单位）。小于此大小的响应不会被压缩，因为对小内容进行压缩可能会增加总体大小（由于压缩头部的开销）。

默认值为 1024 字节（1KB）。

### level

`level` 参数指定了压缩级别，范围从 1 到 9：

- **1-3**：低压缩率，高速度
- **4-6**：平衡的压缩率和速度（默认为 6）
- **7-9**：高压缩率，低速度

### enable_* 和 disable_*

这些参数用于启用或禁用特定的压缩算法：

- `enable_gzip` / `disable_gzip`：启用/禁用 Gzip 压缩
- `enable_deflate` / `disable_deflate`：启用/禁用 Deflate 压缩
- `enable_brotli` / `disable_brotli`：启用/禁用 Brotli 压缩
- `enable_zstd` / `disable_zstd`：启用/禁用 Zstd 压缩
- `enable_lz4` / `disable_lz4`：启用/禁用 LZ4 压缩

默认情况下，Gzip、Deflate、Brotli 和 Zstd 是启用的，而 LZ4 是禁用的。

### excluded_content_types

`excluded_content_types` 参数指定了不应该被压缩的内容类型列表。这通常包括已经压缩的内容类型，如图像、视频和压缩文件。

默认情况下，以下内容类型会被排除：

- `image/*`（所有图像类型）
- `audio/*`（所有音频类型）
- `video/*`（所有视频类型）
- `application/zip`
- `application/gzip`
- `application/x-gzip`
- `application/x-rar-compressed`
- 等等

### excluded_extensions

`excluded_extensions` 参数指定了不应该被压缩的文件扩展名列表。这通常包括已经压缩的文件类型。

默认情况下，以下扩展名会被排除：

- `jpg`, `jpeg`, `png`, `gif`, `webp`（图像）
- `mp3`, `ogg`, `wav`（音频）
- `mp4`, `webm`, `avi`（视频）
- `zip`, `gz`, `rar`, `7z`（压缩文件）
- 等等

## 压缩算法选择逻辑

RAT Engine 会根据以下因素自动选择最佳的压缩算法：

1. 客户端在 `Accept-Encoding` 头中支持的算法
2. 启用的压缩算法列表
3. 内容类型和文件扩展名（是否在排除列表中）
4. 内容大小（是否大于 `min_size`）

选择算法的优先级为：Brotli > Zstd > Gzip > Deflate > LZ4。

## 性能考虑

- 对于静态内容，压缩通常只需要执行一次，然后可以缓存压缩后的结果
- 对于动态内容，每次请求都需要执行压缩，这可能会增加服务器负载
- 较高的压缩级别会提供更好的压缩率，但也会增加 CPU 使用率和延迟
- 对于高流量网站，建议使用中等压缩级别（4-6）以平衡性能和带宽节省

## 示例

### 基本示例

```python
import rat_engine
import time
import json

# 创建服务器配置
config = rat_engine.ServerConfig(host="127.0.0.1", port=8000)

# 创建应用实例
app = rat_engine.RatApp(config)

# 启用压缩
app.enable_compression()

# 定义返回大量文本的处理函数
@app.route("GET", "/large-text")
def handle_large_text(request):
    # 生成大量文本
    text = "这是一段可以被压缩的重复文本。" * 1000
    return text

# 启动服务器
app.run("127.0.0.1", 8000)
```

### 高级示例

```python
import rat_engine
from rat_engine.compression import CompressionConfig, CompressionType

# 创建服务器配置
config = rat_engine.ServerConfig(host="127.0.0.1", port=8000)

# 创建路由器
router = rat_engine.Router(config)

# 创建自定义压缩配置
compression_config = CompressionConfig()\
    .min_size(2048)\
    .level(9)\
    .enable_brotli()\
    .enable_zstd()\
    .disable_gzip()\
    .disable_deflate()\
    .disable_lz4()

# 启用压缩
app.enable_compression(compression_config)

# 添加路由和启动服务器
# ...
```

## 验证压缩是否生效

你可以使用浏览器的开发者工具来验证压缩是否生效：

1. 打开浏览器开发者工具（F12 或右键 -> 检查）
2. 切换到「网络」标签
3. 刷新页面
4. 查看响应头中的 `Content-Encoding` 字段

如果压缩生效，`Content-Encoding` 字段将显示使用的压缩算法，如 `gzip`、`br`（Brotli）等。

你还可以比较压缩前后的大小，在「网络」标签中查看「大小」列，它通常会显示两个值：原始大小和传输大小。