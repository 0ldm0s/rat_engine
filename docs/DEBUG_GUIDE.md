# RAT Engine 调试指南

## 🔧 调试信息说明

为了方便后续调试，项目中保留了详细的调试信息，但已注释掉以避免影响性能。当需要调试时，可以快速恢复这些调试信息。

## 📍 调试信息位置

### 1. Rust 层调试信息 (`src/python_api/server.rs`)

#### 路径参数传递调试
```rust
// 🔧 [调试信息] 路径参数传递调试 - 如需调试路径参数问题，可取消注释以下行
// println!("🔍 [RUST-DEBUG] add_route 混合处理器被调用");
// println!("🔍 [RUST-DEBUG] 接收到的路径参数: {:?}", path_params);
```

#### Python GIL 调试
```rust
// 🔧 [调试信息] Python GIL 调试 - 如需调试 Python 调用问题，可取消注释以下行
// println!("🔍 [RUST-DEBUG] 进入 Python::with_gil");
```

#### 请求数据准备调试
```rust
// 🔧 [调试信息] 请求数据准备调试 - 如需调试请求数据转换问题，可取消注释以下行
// println!("🔍 [RUST-DEBUG] prepare_request_data 成功");
```

#### Python Handler 调用调试
```rust
// 🔧 [调试信息] Python handler 调用调试 - 如需调试 handler 调用问题，可取消注释以下行
// println!("🔍 [RUST-DEBUG] 准备调用 Python handler");
// println!("🔍 [RUST-DEBUG] Python handler 调用成功");
```

#### 分块响应处理调试
```rust
// 🔧 [调试信息] 分块响应处理调试 - 如需调试分块响应处理问题，可取消注释以下行
// println!("🔍 [RUST-DEBUG] execute_python_handler_with_params 被调用");
// println!("🔍 [RUST-DEBUG] path_params: {:?}", path_params);
```

### 2. Python 层调试信息 (`python/rat_engine/web_app.py`)

#### 请求处理调试
```python
# 🔧 [调试信息] 请求处理调试 - 如需调试请求处理问题，可取消注释以下行
# print(f"🔧 [PYTHON-DEBUG] _handle_request 被调用")
# print(f"🔧 [PYTHON-DEBUG] request_data 类型: {type(request_data)}")
# print(f"🔧 [PYTHON-DEBUG] request_data 内容: {request_data}")
```

#### 路由匹配调试
```python
# 🔧 [调试信息] 路由匹配调试 - 如需调试路由匹配问题，可取消注释以下行
# print(f"🔧 [PYTHON-DEBUG] 路由匹配成功: {route.pattern}")
# print(f"🔧 [PYTHON-DEBUG] 提取的参数: {params}")
# print(f"🔧 [PYTHON-DEBUG] 请求路径: {request.path}")
```

#### 装饰器调用调试
```python
# 🔧 [调试信息] 装饰器调用调试 - 如需调试装饰器包装函数调用问题，可取消注释以下行
# print(f"🔧 [PYTHON-DEBUG] 调用装饰器包装的函数")
# print(f"🔧 [PYTHON-DEBUG] 传递的参数: {params}")
```

#### 路由注册调试
```python
# 🔧 [调试信息] 路由注册调试 - 如需调试路由注册问题，可取消注释以下行
# print(f"🔧 [PYTHON-DEBUG] 注册普通路由: {method} {route.pattern}")
# print(f"🔧 [PYTHON-DEBUG] 处理器: {self._handle_request}")
```

### 3. 测试文件调试信息 (`python/debug_path_params.py`)

#### 测试处理器调试
```python
# 🔧 [调试信息] 测试处理器调试 - 如需调试测试处理器问题，可取消注释以下行
# print(f"🔍 [PYTHON-DEBUG] test_handler 被调用")
# print(f"🔍 [PYTHON-DEBUG] request_data: {request_data}")
# print(f"🔍 [PYTHON-DEBUG] request_data 类型: {type(request_data)}")
```

## 🚀 快速启用调试

### 方法1：使用 sed 命令批量启用
```bash
# 启用 Rust 层调试信息
sed -i '' 's|// println!(|println!(|g' src/python_api/server.rs

# 启用 Python 层调试信息
sed -i '' 's|# print(f"🔧 \[PYTHON-DEBUG\]|print(f"🔧 [PYTHON-DEBUG]|g' python/rat_engine/web_app.py
sed -i '' 's|# print(f"🔍 \[PYTHON-DEBUG\]|print(f"🔍 [PYTHON-DEBUG]|g' python/debug_path_params.py
```

### 方法2：使用 VS Code 查找替换
1. 打开 VS Code
2. 按 `Ctrl+Shift+H` (Windows/Linux) 或 `Cmd+Shift+H` (Mac)
3. 启用正则表达式模式
4. 查找：`// println!\("🔍 \[RUST-DEBUG\]`
5. 替换：`println!("🔍 [RUST-DEBUG]`
6. 对 Python 文件类似操作

### 方法3：手动取消注释
根据需要调试的具体问题，手动取消相关调试信息的注释。

## 🔄 重新编译

启用调试信息后，需要重新编译：

```bash
cd rat_engine/python
maturin develop --release
```

## 📊 调试信息说明

- `🔍 [RUST-DEBUG]`：Rust 层调试信息
- `🔧 [PYTHON-DEBUG]`：Python 层调试信息
- 调试信息包含：
  - 路径参数传递过程
  - 请求数据转换过程
  - Python Handler 调用过程
  - 路由匹配过程
  - 错误处理过程

## ⚠️ 注意事项

1. **性能影响**：调试信息会影响性能，生产环境请确保已注释
2. **日志量**：启用所有调试信息会产生大量日志输出
3. **选择性启用**：建议根据具体问题选择性启用相关调试信息
4. **及时关闭**：调试完成后及时注释掉调试信息

## 🎯 常见调试场景

### 路径参数不正确
启用：
- 路径参数传递调试
- 路由匹配调试
- 参数提取调试

### Python Handler 调用失败
启用：
- Python GIL 调试
- Handler 调用调试
- 请求数据准备调试

### 路由匹配问题
启用：
- 路由注册调试
- 路由匹配调试
- 请求处理调试