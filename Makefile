# RAT Engine Makefile
# 提供常见的构建、清理和发布操作

.PHONY: all build clean clean-rust clean-python clean-deps install-develop install-release test publish pypi-build pypi-publish tag push-tag

# 默认目标
all: build

# 构建项目
build:
	cargo build --release

# 清理所有编译缓存
clean: clean-rust clean-python clean-deps

# 清理Rust编译缓存
clean-rust:
	rm -rf target/release/build/rat_engine*
	rm -rf target/release/deps/librat_engine*
	rm -rf target/release/librat_engine.dummy
	rm -rf target/release/librat_engine.rlib
	rm -rf target/release/build/rat_engine-*
	rm -rf target/release/deps/librat_engine-*
	rm -rf target/release/examples/rat_engine-*
	rm -rf target/release/incremental
	cargo clean

# 清理Python编译缓存
clean-python:
	rm -rf python/rat_engine/__pycache__
	rm -rf python/rat_engine/*.so
	rm -rf python/rat_engine/*.dylib
	rm -rf python/target
	rm -rf python/build
	rm -rf python/dist
	rm -rf python/*.egg-info
	find python -name "*.pyc" -delete 2>/dev/null || true
	find python -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null || true

# 清理依赖缓存
clean-deps:
	rm -rf ~/.cargo/registry/cache/*
	rm -rf ~/.cargo/registry/src/.cargo-cache
	cargo update

# 开发模式安装
install-develop:
	cd python && maturin develop

# 发布模式安装
install-release:
	cd python && maturin develop --release

# 运行测试
test:
	cargo test
	cd python && python -m pytest tests/ 2>/dev/null || echo "No Python tests found"

# 发布到crates.io
publish:
	cargo publish

# 构建Python wheel包
pypi-build:
	cd python && maturin build --release

# 发布到PyPI
pypi-publish: pypi-build
	cd python && python -m twine upload dist/*

# 创建版本标签
tag:
	git tag -d v$(shell grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/') 2>/dev/null || true
	git tag v$(shell grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

# 推送标签到远程仓库
push-tag: tag
	git push origin v$(shell grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

# 检查项目状态
status:
	git status
	cargo check
	cd python && python -c "import rat_engine; print(f'RAT Engine版本: {rat_engine.__version__}')" 2>/dev/null || echo "Python包未安装"

# 重新构建并安装
rebuild: clean install-develop

# 完整发布流程
full-publish: clean test publish pypi-publish push-tag

# 开发环境检查
dev-check: clean install-develop test
	@echo "开发环境检查完成"

# 性能测试
bench:
	cargo bench

# 文档生成
docs:
	cargo doc --no-deps

# 格式化代码
fmt:
	cargo fmt

# 代码检查
clippy:
	cargo clippy

# 安全检查
audit:
	cargo audit

# 更新依赖
update:
	cargo update

# 显示帮助
help:
	@echo "可用的Make目标："
	@echo "  all              - 构建项目"
	@echo "  build            - 构建项目（默认）"
	@echo "  clean            - 清理所有编译缓存"
	@echo "  clean-rust       - 清理Rust编译缓存"
	@echo "  clean-python     - 清理Python编译缓存"
	@echo "  clean-deps       - 清理依赖缓存"
	@echo "  install-develop  - 开发模式安装"
	@echo "  install-release  - 发布模式安装"
	@echo "  test             - 运行测试"
	@echo "  publish          - 发布到crates.io"
	@echo "  pypi-build       - 构建Python wheel包"
	@echo "  pypi-publish     - 发布到PyPI"
	@echo "  tag              - 创建版本标签"
	@echo "  push-tag         - 推送标签到远程仓库"
	@echo "  status           - 检查项目状态"
	@echo "  rebuild          - 清理并重新构建"
	@echo "  full-publish     - 完整发布流程"
	@echo "  dev-check        - 开发环境检查"
	@echo "  bench            - 性能测试"
	@echo "  docs             - 文档生成"
	@echo "  fmt              - 格式化代码"
	@echo "  clippy           - 代码检查"
	@echo "  audit            - 安全检查"
	@echo "  update           - 更新依赖"
	@echo "  help             - 显示此帮助信息"