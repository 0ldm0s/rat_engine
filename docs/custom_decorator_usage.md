# RAT Engine Custom 装饰器使用指南

## 概述

`@app.custom` 装饰器现在支持与 `@app.file` 装饰器一致的元组返回格式，提供更灵活的自定义响应处理。

## 基本用法

### 1. 元组格式（推荐）

```python
from rat_engine import RatApp

app = RatApp()

@app.custom("/api/data")
def custom_response(request_data):
    # 返回 (content, content_type) 元组
    content = '{"message": "Hello World"}'
    content_type = "application/json; charset=utf-8"
    return (content, content_type)
```

### 2. 字符串格式（默认 text/plain）

```python
@app.custom("/text")
def text_response(request_data):
    # 直接返回字符串，默认使用 text/plain
    return "这是纯文本响应"
```

### 3. 字节数据格式（默认 application/octet-stream）

```python
@app.custom("/binary")
def binary_response(request_data):
    # 直接返回字节数据
    return b"\x89PNG\r\n\x1a\n..."  # 二进制数据
```

## 实际应用示例

### XML 响应

```python
@app.custom("/api/xml")
def xml_api(request_data):
    xml_content = '''<?xml version="1.0" encoding="UTF-8"?>
    <response>
        <status>success</status>
        <data>Hello World</data>
    </response>'''
    return (xml_content, "application/xml; charset=utf-8")
```

### CSV 数据导出

```python
@app.custom("/export/csv")
def export_csv(request_data):
    csv_data = "名称,年龄,城市\n张三,25,北京\n李四,30,上海"
    return (csv_data, "text/csv; charset=utf-8")
```

### 自定义图片验证码

```python
@app.custom("/captcha")
def generate_captcha(request_data):
    # 生成验证码图片的二进制数据
    image_data = generate_captcha_image()  # 假设的函数
    return (image_data, "image/png")
```

### 动态 JavaScript

```python
@app.custom("/dynamic.js")
def dynamic_js(request_data):
    js_content = f'''
    // 动态生成的 JavaScript
    const timestamp = {int(time.time())};
    console.log("Script loaded at:", new Date(timestamp * 1000));
    '''
    return (js_content, "application/javascript; charset=utf-8")
```

## 与 @app.file 装饰器的对比

| 装饰器 | 用途 | 返回格式 | 适用场景 |
|--------|------|----------|----------|
| `@app.file` | 文件处理 | `文件路径` 或 `(文件路径, MIME类型)` | 静态文件、文件下载 |
| `@app.custom` | 自定义响应 | `(内容, MIME类型)` 或 `内容` | 动态内容、API响应 |

## 最佳实践

### 1. 明确指定 Content-Type

```python
# ✅ 推荐：明确指定 MIME 类型
@app.custom("/api/json")
def api_response(request_data):
    return (json_data, "application/json; charset=utf-8")

# ❌ 不推荐：依赖默认类型
@app.custom("/api/json")
def api_response(request_data):
    return json_data  # 默认为 text/plain
```

### 2. 处理不同数据类型

```python
@app.custom("/api/mixed")
def mixed_response(request_data):
    data_type = request_data.get('type', 'json')
    
    if data_type == 'json':
        content = '{"status": "ok"}'
        return (content, "application/json")
    elif data_type == 'xml':
        content = '<status>ok</status>'
        return (content, "application/xml")
    else:
        return ("Unknown type", "text/plain")
```

### 3. 错误处理

```python
@app.custom("/api/safe")
def safe_response(request_data):
    try:
        # 处理逻辑
        result = process_data(request_data)
        return (result, "application/json")
    except Exception as e:
        error_response = f'{{"error": "{str(e)}"}}'  
        return (error_response, "application/json")
```

## 安全注意事项

1. **避免直接暴露路由功能**：`add_route` 方法已设为私有（`_add_route`），防止用户直接调用产生不可预期的问题。

2. **内容验证**：对用户输入进行适当的验证和清理。

3. **MIME 类型安全**：确保返回的 Content-Type 与实际内容匹配。

## 迁移指南

如果你之前使用的是旧版本的 `@app.custom` 装饰器：

```python
# 旧版本（已废弃）
@app.custom("/api", "application/json")
def old_style(request_data):
    return content

# 新版本（推荐）
@app.custom("/api")
def new_style(request_data):
    return (content, "application/json")
```

新版本提供了更好的一致性和灵活性，建议尽快迁移到元组格式。