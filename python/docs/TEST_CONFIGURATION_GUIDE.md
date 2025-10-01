# 流式响应演示 - 测试配置指南

## 概述

`streaming_demo.py` 现在支持通过枚举来选择性地运行测试功能，避免每次都产生大量无关的测试结果。

## 快速开始

### 1. 基本配置

在 `streaming_demo.py` 文件中找到以下配置部分：

```python
# 测试配置 - 可以通过修改这里来选择要运行的测试
TEST_FEATURES = TestFeature.ALL             # 默认运行所有测试
```

### 2. 可用的测试功能

#### 单个功能测试
```python
TEST_FEATURES = TestFeature.HOME        # 只测试主页
TEST_FEATURES = TestFeature.CHUNKED     # 只测试分块传输
TEST_FEATURES = TestFeature.JSON_STREAM # 只测试JSON流
TEST_FEATURES = TestFeature.TEXT_STREAM # 只测试文本流
TEST_FEATURES = TestFeature.HEADERS     # 只测试头信息
TEST_FEATURES = TestFeature.SSE         # 只测试SSE连接
TEST_FEATURES = TestFeature.LOGS        # 只测试实时日志流
```

#### 预定义组合
```python
TEST_FEATURES = TestFeature.BASIC       # 基础功能 (主页 + 头信息)
TEST_FEATURES = TestFeature.STREAMING   # 流式传输 (分块 + JSON流 + 文本流)
TEST_FEATURES = TestFeature.REALTIME    # 实时通信 (SSE + 日志流)
TEST_FEATURES = TestFeature.ALL         # 所有测试
```

#### 自定义组合
```python
# 使用 | 操作符组合多个功能
TEST_FEATURES = TestFeature.CHUNKED | TestFeature.SSE  # 分块传输 + SSE
TEST_FEATURES = TestFeature.HOME | TestFeature.LOGS    # 主页 + 日志流
TEST_FEATURES = TestFeature.STREAMING | TestFeature.SSE # 流式传输 + SSE
```

### 3. 完全禁用测试

```python
AUTO_TEST_ENABLED = False               # 完全禁用自动测试
```

## 使用场景

### 开发调试
当你只想测试特定功能时：
```python
TEST_FEATURES = TestFeature.CHUNKED     # 只测试分块传输问题
```

### 性能测试
当你想测试流式传输性能时：
```python
TEST_FEATURES = TestFeature.STREAMING   # 只测试流式传输功能
```

### 实时功能验证
当你想验证实时通信功能时：
```python
TEST_FEATURES = TestFeature.REALTIME    # 只测试SSE和日志流
```

### 快速验证
当你只想快速验证基本功能时：
```python
TEST_FEATURES = TestFeature.BASIC       # 只测试基础功能
```

## 运行示例

### 示例 1: 只测试分块传输
```python
# 修改配置
TEST_FEATURES = TestFeature.CHUNKED

# 运行
python3 streaming_demo.py
```

输出将显示：
```
⚙️  当前测试配置:
   🎯 自定义测试: CHUNKED
   💡 提示: 可以修改 TEST_FEATURES 变量来选择不同的测试功能

📋 选择的测试功能: CHUNKED
🎯 [CHUNKED] 🧪 测试 分块传输: http://127.0.0.1:3000/chunked
```

### 示例 2: 测试基础功能
```python
# 修改配置
TEST_FEATURES = TestFeature.BASIC

# 运行
python3 streaming_demo.py
```

输出将显示：
```
⚙️  当前测试配置:
   🔹 基础功能测试 (主页 + 头信息)

📋 选择的测试功能: HOME, HEADERS
🎯 [HOME] 🧪 测试 主页: http://127.0.0.1:3000/
🎯 [HEADERS] 🧪 测试 头信息测试: http://127.0.0.1:3000/headers-test
```

## 优势

1. **减少测试时间**: 只运行需要的测试功能
2. **聚焦问题**: 专注于特定功能的调试
3. **灵活配置**: 支持单个功能、预定义组合和自定义组合
4. **清晰输出**: 明确显示将要运行的测试功能
5. **向后兼容**: 保持原有的 `run_all_tests()` 方法

## 注意事项

- 使用 `Flag` 枚举支持位运算组合
- 配置错误时会显示警告信息
- 程序启动时会显示当前的测试配置
- 可以随时修改配置并重新运行