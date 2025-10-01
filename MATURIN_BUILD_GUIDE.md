# RAT Engine PyO3 构建指南

## ⚠️ 重要警告

**禁止在项目主目录 (`rat_engine/`) 下运行 `maturin develop`！**

## 📋 正确的构建方式

### 1. 进入 Python 子目录
```bash
cd rat_engine/python
```

### 2. 运行 maturin 命令
```bash
# 开发模式编译
maturin develop

# 发布模式编译（推荐用于测试）
maturin develop --release
```

## 🔍 配置差异说明

### 主目录配置 (pyproject.toml.bak)
```toml
[tool.maturin]
module-name = "rat_engine"
python-source = "python"
```

### Python 子目录配置 (python/pyproject.toml)
```toml
[tool.maturin]
module-name = "rat_engine._rat_engine"
python-source = "."
version-from-cargo = true
features = ["pyo3/extension-module"]
```

## 🎯 为什么必须在 python 目录下编译

1. **模块结构不同**：
   - 主目录：生成 `rat_engine` 模块
   - Python 目录：生成 `rat_engine._rat_engine` 模块

2. **源码路径不同**：
   - 主目录：指向 `python/` 子目录
   - Python 目录：指向当前目录 `.`

3. **功能特性不同**：
   - Python 目录配置包含 `features = ["pyo3/extension-module"]`
   - 版本管理方式不同：`version-from-cargo = true`

## ✅ 验证编译结果

编译完成后，可以通过以下方式验证：

```python
import rat_engine
print(rat_engine.__version__)
print(rat_engine.__file__)
```

## 🚨 常见错误

如果在主目录运行 `maturin develop`，可能会遇到：
- 模块导入错误
- 功能缺失
- 版本不一致
- 路径参数传递问题

## 📝 最佳实践

1. 始终在 `rat_engine/python/` 目录下运行 maturin 命令
2. 使用 `--release` 模式进行性能测试
3. 编译前确保 Rust 代码已保存
4. 编译后重启 Python 解释器以加载最新版本