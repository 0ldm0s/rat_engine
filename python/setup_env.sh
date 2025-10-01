#!/bin/bash
# -*- coding: utf-8 -*-
"""
RAT Engine Python 环境设置脚本

这个脚本会自动配置 PYTHONPATH 和其他必要的环境变量，
确保 rat_engine 模块可以在任何地方正确导入。

使用方法:
1. 一次性设置: source setup_env.sh
2. 永久设置: ./setup_env.sh install
3. 移除设置: ./setup_env.sh uninstall
"""

# 获取脚本所在目录的绝对路径
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAT_ENGINE_PYTHON_DIR="$SCRIPT_DIR"
RAT_ENGINE_ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

# 检查 rat_engine 模块是否存在
check_rat_engine_module() {
    if [ ! -d "$RAT_ENGINE_PYTHON_DIR/rat_engine" ]; then
        print_error "找不到 rat_engine 模块目录: $RAT_ENGINE_PYTHON_DIR/rat_engine"
        return 1
    fi
    
    if [ ! -f "$RAT_ENGINE_PYTHON_DIR/rat_engine/__init__.py" ]; then
        print_error "找不到 rat_engine 模块初始化文件"
        return 1
    fi
    
    return 0
}

# 检测 shell 类型
detect_shell() {
    if [ -n "$ZSH_VERSION" ]; then
        echo "zsh"
    elif [ -n "$BASH_VERSION" ]; then
        echo "bash"
    else
        echo "unknown"
    fi
}

# 获取 shell 配置文件路径
get_shell_config() {
    local shell_type=$(detect_shell)
    
    case $shell_type in
        "zsh")
            echo "$HOME/.zshrc"
            ;;
        "bash")
            if [ -f "$HOME/.bash_profile" ]; then
                echo "$HOME/.bash_profile"
            else
                echo "$HOME/.bashrc"
            fi
            ;;
        *)
            echo "$HOME/.profile"
            ;;
    esac
}

# 设置临时环境变量（当前会话）
setup_temp_env() {
    print_info "设置临时环境变量（仅当前会话有效）..."
    
    # 设置 PYTHONPATH
    if [ -z "$PYTHONPATH" ]; then
        export PYTHONPATH="$RAT_ENGINE_PYTHON_DIR"
    else
        # 检查是否已经包含该路径
        if [[ ":$PYTHONPATH:" != *":$RAT_ENGINE_PYTHON_DIR:"* ]]; then
            export PYTHONPATH="$RAT_ENGINE_PYTHON_DIR:$PYTHONPATH"
        fi
    fi
    
    # 设置 RAT_ENGINE 相关环境变量
    export RAT_ENGINE_HOME="$RAT_ENGINE_ROOT_DIR"
    export RAT_ENGINE_PYTHON_HOME="$RAT_ENGINE_PYTHON_DIR"
    
    # 设置默认的安全配置环境变量
    export RAT_DEBUG_MODE="${RAT_DEBUG_MODE:-false}"
    export RAT_ERROR_DETAIL_LEVEL="${RAT_ERROR_DETAIL_LEVEL:-basic}"
    export RAT_LOG_FULL_TRACEBACK="${RAT_LOG_FULL_TRACEBACK:-true}"
    export RAT_LOG_REQUEST_INFO="${RAT_LOG_REQUEST_INFO:-true}"
    export RAT_LOG_SENSITIVE_DATA="${RAT_LOG_SENSITIVE_DATA:-false}"
    export RAT_INCLUDE_ERROR_ID="${RAT_INCLUDE_ERROR_ID:-true}"
    export RAT_INCLUDE_ERROR_TYPE="${RAT_INCLUDE_ERROR_TYPE:-true}"
    export RAT_INCLUDE_TIMESTAMP="${RAT_INCLUDE_TIMESTAMP:-false}"
    export RAT_FILTER_FILE_PATHS="${RAT_FILTER_FILE_PATHS:-true}"
    export RAT_FILTER_CREDENTIALS="${RAT_FILTER_CREDENTIALS:-true}"
    export RAT_FILTER_IP_ADDRESSES="${RAT_FILTER_IP_ADDRESSES:-true}"
    export RAT_FILTER_USER_PATHS="${RAT_FILTER_USER_PATHS:-true}"
    export RAT_FILTER_ENV_VARS="${RAT_FILTER_ENV_VARS:-true}"
    
    print_success "临时环境变量设置完成"
    print_info "PYTHONPATH: $PYTHONPATH"
    print_info "RAT_ENGINE_HOME: $RAT_ENGINE_HOME"
    print_info "RAT_ENGINE_PYTHON_HOME: $RAT_ENGINE_PYTHON_HOME"
}

# 安装永久环境变量
install_permanent_env() {
    local shell_config=$(get_shell_config)
    local shell_type=$(detect_shell)
    
    print_info "安装永久环境变量到 $shell_config..."
    
    # 创建环境变量配置内容
    local env_config="
# RAT Engine Python 环境配置 - 自动生成
# 生成时间: $(date)
export RAT_ENGINE_HOME=\"$RAT_ENGINE_ROOT_DIR\"
export RAT_ENGINE_PYTHON_HOME=\"$RAT_ENGINE_PYTHON_DIR\"

# 设置 PYTHONPATH
if [ -z \"\\$PYTHONPATH\" ]; then
    export PYTHONPATH=\"$RAT_ENGINE_PYTHON_DIR\"
else
    if [[ \":\\$PYTHONPATH:\" != *\":\\$RAT_ENGINE_PYTHON_HOME:\"* ]]; then
        export PYTHONPATH=\"$RAT_ENGINE_PYTHON_DIR:\\$PYTHONPATH\"
    fi
fi

# RAT Engine 安全配置默认值
export RAT_DEBUG_MODE=\"\\${RAT_DEBUG_MODE:-false}\"
export RAT_ERROR_DETAIL_LEVEL=\"\\${RAT_ERROR_DETAIL_LEVEL:-basic}\"
export RAT_LOG_FULL_TRACEBACK=\"\\${RAT_LOG_FULL_TRACEBACK:-true}\"
export RAT_LOG_REQUEST_INFO=\"\\${RAT_LOG_REQUEST_INFO:-true}\"
export RAT_LOG_SENSITIVE_DATA=\"\\${RAT_LOG_SENSITIVE_DATA:-false}\"
export RAT_INCLUDE_ERROR_ID=\"\\${RAT_INCLUDE_ERROR_ID:-true}\"
export RAT_INCLUDE_ERROR_TYPE=\"\\${RAT_INCLUDE_ERROR_TYPE:-true}\"
export RAT_INCLUDE_TIMESTAMP=\"\\${RAT_INCLUDE_TIMESTAMP:-false}\"
export RAT_FILTER_FILE_PATHS=\"\\${RAT_FILTER_FILE_PATHS:-true}\"
export RAT_FILTER_CREDENTIALS=\"\\${RAT_FILTER_CREDENTIALS:-true}\"
export RAT_FILTER_IP_ADDRESSES=\"\\${RAT_FILTER_IP_ADDRESSES:-true}\"
export RAT_FILTER_USER_PATHS=\"\\${RAT_FILTER_USER_PATHS:-true}\"
export RAT_FILTER_ENV_VARS=\"\\${RAT_FILTER_ENV_VARS:-true}\"
# RAT Engine Python 环境配置结束
"
    
    # 检查是否已经存在配置
    if grep -q "RAT Engine Python 环境配置" "$shell_config" 2>/dev/null; then
        print_warning "检测到已存在的 RAT Engine 配置，将先移除旧配置..."
        uninstall_permanent_env
    fi
    
    # 备份原配置文件
    if [ -f "$shell_config" ]; then
        cp "$shell_config" "${shell_config}.rat_backup.$(date +%Y%m%d_%H%M%S)"
        print_info "已备份原配置文件到 ${shell_config}.rat_backup.$(date +%Y%m%d_%H%M%S)"
    fi
    
    # 添加新配置
    echo "$env_config" >> "$shell_config"
    
    print_success "永久环境变量安装完成"
    print_info "配置文件: $shell_config"
    print_warning "请运行以下命令使配置生效:"
    echo "    source $shell_config"
    print_warning "或者重新打开终端"
}

# 卸载永久环境变量
uninstall_permanent_env() {
    local shell_config=$(get_shell_config)
    
    print_info "从 $shell_config 移除 RAT Engine 环境配置..."
    
    if [ ! -f "$shell_config" ]; then
        print_warning "配置文件不存在: $shell_config"
        return 0
    fi
    
    # 备份原配置文件
    cp "$shell_config" "${shell_config}.rat_uninstall_backup.$(date +%Y%m%d_%H%M%S)"
    
    # 移除 RAT Engine 配置
    sed -i.tmp '/# RAT Engine Python 环境配置 - 自动生成/,/# RAT Engine Python 环境配置结束/d' "$shell_config"
    rm -f "${shell_config}.tmp"
    
    print_success "RAT Engine 环境配置已移除"
    print_info "备份文件: ${shell_config}.rat_uninstall_backup.$(date +%Y%m%d_%H%M%S)"
}

# 测试模块导入
test_import() {
    print_info "测试 rat_engine 模块导入..."
    
    # 测试基本导入
    if python3 -c "import rat_engine; print('✅ rat_engine 基本导入成功')" 2>/dev/null; then
        print_success "基本模块导入测试通过"
    else
        print_error "基本模块导入失败"
        return 1
    fi
    
    # 测试安全配置组件导入
    if python3 -c "from rat_engine import SecurityConfig, ErrorDetailLevel, handle_secure_error; print('✅ 安全配置组件导入成功')" 2>/dev/null; then
        print_success "安全配置组件导入测试通过"
    else
        print_error "安全配置组件导入失败"
        return 1
    fi
    
    # 测试 RatApp 组件导入
if python3 -c "from rat_engine import RatApp; print('✅ RatApp 组件导入成功')" 2>/dev/null; then
    print_success "RatApp 组件导入测试通过"
else
    print_error "RatApp 组件导入失败"
        return 1
    fi
    
    print_success "所有模块导入测试通过！"
    return 0
}

# 显示当前环境状态
show_status() {
    print_info "RAT Engine Python 环境状态:"
    echo
    
    echo "📁 路径信息:"
    echo "   RAT Engine 根目录: $RAT_ENGINE_ROOT_DIR"
    echo "   Python 包目录: $RAT_ENGINE_PYTHON_DIR"
    echo
    
    echo "🔧 环境变量:"
    echo "   PYTHONPATH: ${PYTHONPATH:-未设置}"
    echo "   RAT_ENGINE_HOME: ${RAT_ENGINE_HOME:-未设置}"
    echo "   RAT_ENGINE_PYTHON_HOME: ${RAT_ENGINE_PYTHON_HOME:-未设置}"
    echo
    
    echo "🔒 安全配置:"
    echo "   RAT_DEBUG_MODE: ${RAT_DEBUG_MODE:-未设置}"
    echo "   RAT_ERROR_DETAIL_LEVEL: ${RAT_ERROR_DETAIL_LEVEL:-未设置}"
    echo "   RAT_FILTER_CREDENTIALS: ${RAT_FILTER_CREDENTIALS:-未设置}"
    echo
    
    echo "📋 Shell 信息:"
    echo "   当前 Shell: $(detect_shell)"
    echo "   配置文件: $(get_shell_config)"
    echo
    
    # 检查模块是否可导入
    if python3 -c "import rat_engine" 2>/dev/null; then
        print_success "rat_engine 模块可正常导入"
    else
        print_error "rat_engine 模块导入失败"
    fi
}

# 创建便捷脚本
create_convenience_scripts() {
    print_info "创建便捷脚本..."
    
    # 创建 rat-env 命令
    cat > "$RAT_ENGINE_PYTHON_DIR/rat-env" << 'EOF'
#!/bin/bash
# RAT Engine 环境管理便捷脚本

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/setup_env.sh"

case "$1" in
    "status")
        show_status
        ;;
    "test")
        test_import
        ;;
    "install")
        install_permanent_env
        ;;
    "uninstall")
        uninstall_permanent_env
        ;;
    "temp")
        setup_temp_env
        ;;
    *)
        echo "RAT Engine 环境管理工具"
        echo "用法: rat-env [命令]"
        echo
        echo "可用命令:"
        echo "  status     - 显示环境状态"
        echo "  test       - 测试模块导入"
        echo "  install    - 安装永久环境配置"
        echo "  uninstall  - 卸载永久环境配置"
        echo "  temp       - 设置临时环境变量"
        ;;
esac
EOF
    
    chmod +x "$RAT_ENGINE_PYTHON_DIR/rat-env"
    print_success "便捷脚本创建完成: $RAT_ENGINE_PYTHON_DIR/rat-env"
}

# 主函数
main() {
    echo "🚀 RAT Engine Python 环境设置脚本"
    echo "=================================="
    echo
    
    # 检查模块是否存在
    if ! check_rat_engine_module; then
        exit 1
    fi
    
    case "$1" in
        "install")
            setup_temp_env
            install_permanent_env
            create_convenience_scripts
            echo
            test_import
            ;;
        "uninstall")
            uninstall_permanent_env
            ;;
        "test")
            test_import
            ;;
        "status")
            show_status
            ;;
        "temp")
            setup_temp_env
            test_import
            ;;
        "")
            # 默认行为：设置临时环境变量
            setup_temp_env
            echo
            print_info "💡 使用提示:"
            echo "   • 当前设置仅对本次会话有效"
            echo "   • 要永久安装，请运行: $0 install"
            echo "   • 要测试导入，请运行: $0 test"
            echo "   • 要查看状态，请运行: $0 status"
            ;;
        *)
            echo "用法: $0 [命令]"
            echo
            echo "可用命令:"
            echo "  (无参数)   - 设置临时环境变量（仅当前会话）"
            echo "  install    - 安装永久环境配置"
            echo "  uninstall  - 卸载永久环境配置"
            echo "  test       - 测试模块导入"
            echo "  status     - 显示环境状态"
            echo "  temp       - 设置临时环境变量"
            echo
            echo "示例:"
            echo "  source $0           # 临时设置（推荐用于测试）"
            echo "  $0 install         # 永久安装"
            echo "  $0 test            # 测试导入"
            exit 1
            ;;
    esac
}

# 如果脚本被 source，则只设置环境变量
if [[ "${BASH_SOURCE[0]}" != "${0}" ]]; then
    # 被 source 调用
    setup_temp_env
    print_info "💡 环境变量已设置，现在可以在任何地方导入 rat_engine 模块"
else
    # 直接执行
    main "$@"
fi