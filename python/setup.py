#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine Python Package Setup

高性能 Rust + Python Web 框架的 Python 包配置
"""

import os
import sys
from pathlib import Path
from setuptools import setup, find_packages
from setuptools_rust import Binding, RustExtension

# 读取版本信息 - 从 Cargo.toml 动态获取
try:
    import subprocess
    result = subprocess.run(["cargo", "metadata", "--format-version", "1"], 
                          capture_output=True, text=True, cwd=Path(__file__).parent)
    if result.returncode == 0:
        import json
        metadata = json.loads(result.stdout)
        for package in metadata["packages"]:
            if package["name"] == "rat_engine_py":
                version = package["version"]
                break
        else:
            version = "0.2.0"  # fallback
    else:
        version = "0.2.0"  # fallback
except Exception:
    version = "0.2.0"  # fallback

# 读取 README
this_directory = Path(__file__).parent
long_description = (this_directory / "README.md").read_text(encoding="utf-8") if (this_directory / "README.md").exists() else ""

# 读取依赖
requirements = []
requirements_file = this_directory / "requirements.txt"
if requirements_file.exists():
    requirements = requirements_file.read_text().strip().split("\n")
    requirements = [req.strip() for req in requirements if req.strip() and not req.startswith("#")]

# 开发依赖
dev_requirements = [
    "pytest>=7.0.0",
    "pytest-asyncio>=0.21.0",
    "pytest-benchmark>=4.0.0",
    "black>=23.0.0",
    "isort>=5.12.0",
    "mypy>=1.0.0",
    "flake8>=6.0.0",
    "coverage>=7.0.0",
    "requests>=2.28.0",  # 用于测试
    "aiohttp>=3.8.0",    # 用于性能对比测试
]

# 文档依赖
docs_requirements = [
    "sphinx>=6.0.0",
    "sphinx-rtd-theme>=1.2.0",
    "myst-parser>=1.0.0",
]

setup(
    name="rat-engine",
    version=version,
    author="RAT Engine Team",
    author_email="team@rat-engine.dev",
    description="High-performance Rust + Python web framework with Web-style API",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/your-org/rat-engine",
    project_urls={
        "Bug Tracker": "https://github.com/your-org/rat-engine/issues",
        "Documentation": "https://rat-engine.readthedocs.io/",
        "Source Code": "https://github.com/your-org/rat-engine",
        "Changelog": "https://github.com/your-org/rat-engine/blob/main/CHANGELOG.md",
    },
    
    # 包配置
    packages=find_packages(),
    package_data={
        "rat_engine": ["*.pyi", "py.typed"],  # 类型提示文件
    },
    include_package_data=True,
    
    # Rust 扩展
    rust_extensions=[
        RustExtension(
            "rat_engine._rat_engine",
            path="../Cargo.toml",  # 指向上级目录的 Cargo.toml
            binding=Binding.PyO3,
            debug=False,
            features=["python-extension"],  # 如果有特定的 feature
        )
    ],
    
    # 依赖
    install_requires=requirements,
    extras_require={
        "dev": dev_requirements,
        "docs": docs_requirements,
        "all": dev_requirements + docs_requirements,
    },
    
    # Python 版本要求
    python_requires=">=3.8",
    
    # 分类器
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Programming Language :: Rust",
        "Topic :: Internet :: WWW/HTTP :: HTTP Servers",
        "Topic :: Internet :: WWW/HTTP :: WSGI :: Server",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "Topic :: System :: Networking",
        "Framework :: AsyncIO",
        "Typing :: Typed",
    ],
    
    # 关键词
    keywords=[
        "web", "framework", "http", "server", "rust", "python", "performance",
        "async", "web", "api", "rest", "microservice", "high-performance",
        "zero-copy", "work-stealing", "pyo3", "tauri"
    ],
    
    # 命令行工具
    entry_points={
        "console_scripts": [
            "rat-engine=rat_engine.cli:cli_entry_point",
            "rat=rat_engine.cli:cli_entry_point",  # 简短别名
        ],
    },
    
    # 构建配置
    zip_safe=False,  # Rust 扩展不支持 zip 安装
    
    # 许可证
    license="MIT",
    
    # 项目状态
    # https://pypi.org/classifiers/
    # "Development Status :: 3 - Alpha",
    # "Development Status :: 4 - Beta",
    # "Development Status :: 5 - Production/Stable",
)

# 构建后的清理和验证
if __name__ == "__main__":
    import subprocess
    import shutil
    
    # 检查 Rust 工具链
    try:
        result = subprocess.run(["cargo", "--version"], capture_output=True, text=True)
        if result.returncode != 0:
            print("❌ Cargo not found. Please install Rust toolchain.")
            print("   Visit: https://rustup.rs/")
            sys.exit(1)
        print(f"✅ Found Cargo: {result.stdout.strip()}")
    except FileNotFoundError:
        print("❌ Cargo not found. Please install Rust toolchain.")
        print("   Visit: https://rustup.rs/")
        sys.exit(1)
    
    # 检查 maturin（推荐的构建工具）
    try:
        result = subprocess.run(["maturin", "--version"], capture_output=True, text=True)
        if result.returncode == 0:
            print(f"✅ Found Maturin: {result.stdout.strip()}")
            print("💡 Tip: You can use 'maturin develop' for faster development builds")
    except FileNotFoundError:
        print("💡 Tip: Install maturin for faster development: pip install maturin")
    
    print("\n🚀 RAT Engine setup configuration complete!")
    print("\n📋 Next steps:")
    print("   1. pip install -e .              # Install in development mode")
    print("   2. rat-engine app.py             # Run your Web app")
    print("   3. python -m rat_engine.cli app.py --help  # See all options")
    print("\n📚 Documentation: https://rat-engine.readthedocs.io/")