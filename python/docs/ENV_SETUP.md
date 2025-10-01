# RAT Engine Python 环境设置指南

## 🚀 快速开始

### 方法一：临时设置（推荐用于测试）
```bash
# 进入 rat_engine/python 目录
cd /path/to/rat/rat_engine/python

# 设置临时环境变量（仅当前终端会话有效）
source setup_env.sh

# 测试导入
python3 -c "import rat_engine; print('✅ 导入成功')"
```

### 方法二：永久安装（推荐用于开发）
```bash
# 进入 rat_engine/python 目录
cd /path/to/rat/rat_engine/python

# 永久安装环境配置
./setup_env.sh install

# 重新加载配置或重启终端
source ~/.zshrc  # 或 ~/.bashrc

# 测试导入
python3 -c "import rat_engine; print('✅ 导入成功')"
```

## 📋 可用命令

| 命令 | 说明 | 示例 |
|------|------|------|
| `source setup_env.sh` | 临时设置环境变量 | `source setup_env.sh` |
| `./setup_env.sh install` | 永久安装环境配置 | `./setup_env.sh install` |
| `./setup_env.sh uninstall` | 卸载永久环境配置 | `./setup_env.sh uninstall` |
| `./setup_env.sh test` | 测试模块导入 | `./setup_env.sh test` |
| `./setup_env.sh status` | 显示环境状态 | `./setup_env.sh status` |
| `./setup_env.sh temp` | 仅设置临时环境变量 | `./setup_env.sh temp` |

## 🔧 环境变量说明

### 核心环境变量
- `PYTHONPATH`: 添加 rat_engine 模块路径
- `RAT_ENGINE_HOME`: RAT Engine 根目录
- `RAT_ENGINE_PYTHON_HOME`: Python 包目录

### 安全配置环境变量
- `RAT_DEBUG_MODE`: 调试模式 (true/false)
- `RAT_ERROR_DETAIL_LEVEL`: 错误详细程度 (minimal/basic/detailed/full)
- `RAT_LOG_FULL_TRACEBACK`: 记录完整堆栈跟踪 (true/false)
- `RAT_LOG_REQUEST_INFO`: 记录请求信息 (true/false)
- `RAT_LOG_SENSITIVE_DATA`: 记录敏感数据 (true/false)
- `RAT_INCLUDE_ERROR_ID`: 包含错误ID (true/false)
- `RAT_INCLUDE_ERROR_TYPE`: 包含错误类型 (true/false)
- `RAT_INCLUDE_TIMESTAMP`: 包含时间戳 (true/false)
- `RAT_FILTER_FILE_PATHS`: 过滤文件路径 (true/false)
- `RAT_FILTER_CREDENTIALS`: 过滤凭据信息 (true/false)
- `RAT_FILTER_IP_ADDRESSES`: 过滤IP地址 (true/false)
- `RAT_FILTER_USER_PATHS`: 过滤用户路径 (true/false)
- `RAT_FILTER_ENV_VARS`: 过滤环境变量 (true/false)

## 💡 使用场景

### 开发环境设置
```bash
# 一次性永久安装
./setup_env.sh install

# 设置开发环境的安全配置
export RAT_DEBUG_MODE=true
export RAT_ERROR_DETAIL_LEVEL=detailed
export RAT_LOG_SENSITIVE_DATA=true
```

### 生产环境设置
```bash
# 永久安装
./setup_env.sh install

# 设置生产环境的安全配置
export RAT_DEBUG_MODE=false
export RAT_ERROR_DETAIL_LEVEL=minimal
export RAT_LOG_SENSITIVE_DATA=false
export RAT_FILTER_CREDENTIALS=true
```

### 测试环境设置
```bash
# 临时设置（用于CI/CD）
source setup_env.sh

# 或者设置测试专用配置
export RAT_ERROR_DETAIL_LEVEL=basic
export RAT_LOG_FULL_TRACEBACK=true
```

## 🛠️ 便捷工具

安装后会创建 `rat-env` 便捷脚本：

```bash
# 查看环境状态
./rat-env status

# 测试模块导入
./rat-env test

# 安装/卸载配置
./rat-env install
./rat-env uninstall

# 设置临时环境
./rat-env temp
```

## 🔍 故障排除

### 问题：模块导入失败
```bash
# 检查环境状态
./setup_env.sh status

# 测试导入
./setup_env.sh test

# 重新设置环境
source setup_env.sh
```

### 问题：永久配置不生效
```bash
# 检查配置文件
cat ~/.zshrc | grep "RAT Engine"

# 重新加载配置
source ~/.zshrc

# 或重新安装
./setup_env.sh uninstall
./setup_env.sh install
```

### 问题：权限错误
```bash
# 确保脚本有执行权限
chmod +x setup_env.sh

# 检查目录权限
ls -la setup_env.sh
```

## 📁 文件结构

```
rat_engine/python/
├── setup_env.sh          # 主环境设置脚本
├── rat-env               # 便捷管理脚本（安装后生成）
├── ENV_SETUP.md          # 本文档
├── rat_engine/           # Python 模块目录
│   ├── __init__.py
│   ├── security.py       # 安全配置模块
│   └── ...
└── examples/             # 示例代码
    ├── security_config_example.py
    ├── simple_config_demo.py
    └── ...
```

## 🎯 最佳实践

1. **开发阶段**：使用 `source setup_env.sh` 进行临时设置
2. **长期开发**：使用 `./setup_env.sh install` 进行永久安装
3. **CI/CD**：在脚本中使用 `source setup_env.sh`
4. **生产部署**：设置适当的安全配置环境变量
5. **团队协作**：将此文档分享给团队成员

## ⚡ 一键解决方案

如果你只是想快速解决导入问题，运行这个命令：

```bash
# 进入项目目录并永久安装
cd /Users/0ldm0s/workspaces/rust/rat/rat_engine/python && ./setup_env.sh install
```

然后重新打开终端或运行：
```bash
source ~/.zshrc  # 或 ~/.bashrc
```

现在你可以在任何地方使用 `import rat_engine` 了！