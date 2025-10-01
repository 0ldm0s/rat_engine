# RAT Engine Python绑定 Makefile

.PHONY: help build install dev clean test example

# 默认目标
help:
	@echo "RAT Engine Python绑定构建工具"
	@echo ""
	@echo "可用命令:"
	@echo "  build     - 构建Python扩展模块"
	@echo "  install   - 安装到当前Python环境"
	@echo "  dev       - 开发模式安装"
	@echo "  clean     - 清理构建文件"
	@echo "  test      - 运行测试"
	@echo "  example   - 运行Web应用演示"
	@echo "  check     - 检查Rust代码"
	@echo "  format    - 格式化代码"

# 构建扩展模块
build:
	@echo "构建RAT Engine Python扩展..."
	maturin build --release

# 安装到当前环境
install: build
	@echo "安装RAT Engine到当前Python环境..."
	maturin install --release

# 开发模式安装
dev:
	@echo "开发模式安装RAT Engine..."
	maturin develop

# 清理构建文件
clean:
	@echo "清理构建文件..."
	cargo clean
	rm -rf target/
	rm -rf dist/
	rm -rf *.egg-info/
	find . -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null || true
	find . -name "*.pyc" -delete 2>/dev/null || true

# 运行测试
test: dev
	@echo "运行测试..."
	python -m pytest tests/ -v

# 运行Web应用演示
example:
	@echo "运行Web应用演示..."
	python examples/web_app_demo.py

# 检查Rust代码
check:
	@echo "检查Rust代码..."
	cargo check
	cargo clippy -- -D warnings

# 格式化代码
format:
	@echo "格式化Rust代码..."
	cargo fmt
	@echo "格式化Python代码..."
	black rat_engine_py/ examples/ tests/ --line-length 88

# 发布到PyPI
publish: build
	@echo "发布到PyPI..."
	maturin publish

# 本地wheel包
wheel:
	@echo "构建wheel包..."
	maturin build --release --out dist/

# 检查依赖
deps:
	@echo "检查依赖..."
	@command -v maturin >/dev/null 2>&1 || { echo "请安装maturin: pip install maturin"; exit 1; }
	@command -v cargo >/dev/null 2>&1 || { echo "请安装Rust工具链"; exit 1; }
	@echo "依赖检查通过"

# 完整的开发环境设置
setup: deps
	@echo "设置开发环境..."
	pip install maturin pytest black
	make dev
	@echo "开发环境设置完成"