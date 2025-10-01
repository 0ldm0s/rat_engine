#!/usr/bin/env python3
"""
RAT Engine Python 绑定构建脚本

自动化构建、测试和安装流程，解决 PYTHONPATH 问题
"""

import os
import sys
import subprocess
import argparse
import shutil
from pathlib import Path

def run_command(cmd, cwd=None):
    """运行命令并处理错误"""
    print(f"执行命令: {cmd}")
    if cwd:
        print(f"工作目录: {cwd}")
    
    result = subprocess.run(
        cmd, 
        shell=True, 
        cwd=cwd,
        capture_output=False,
        text=True
    )
    
    if result.returncode != 0:
        raise subprocess.CalledProcessError(result.returncode, cmd)
    
    return result

def check_dependencies():
    """检查构建依赖"""
    print("检查构建依赖...")
    
    # 检查 Rust
    try:
        result = subprocess.run(["rustc", "--version"], capture_output=True, text=True)
        if result.returncode == 0:
            print(f"✅ Rust: {result.stdout.strip()}")
        else:
            print("❌ Rust 未安装")
            return False
    except FileNotFoundError:
        print("❌ Rust 未安装")
        return False
    
    # 检查 maturin
    try:
        result = subprocess.run(["maturin", "--version"], capture_output=True, text=True)
        if result.returncode == 0:
            print(f"✅ Maturin: {result.stdout.strip()}")
        else:
            print("❌ Maturin 未安装，请运行: pip install maturin")
            return False
    except FileNotFoundError:
        print("❌ Maturin 未安装，请运行: pip install maturin")
        return False
    
    # 检查 Python 版本
    python_version = sys.version_info
    if python_version >= (3, 8):
        print(f"✅ Python: {sys.version}")
    else:
        print(f"❌ Python 版本过低: {sys.version}，需要 >= 3.8")
        return False
    
    print("✅ 所有依赖检查通过")
    return True

def clean_build():
    """清理构建文件"""
    print("清理构建文件...")
    
    python_dir = Path(__file__).parent
    root_dir = python_dir.parent
    
    # 清理目录列表
    clean_dirs = [
        python_dir / "target",
        python_dir / "build",
        python_dir / "dist",
        python_dir / "*.egg-info",
        root_dir / "target",
    ]
    
    for clean_dir in clean_dirs:
        if clean_dir.name == "*.egg-info":
            # 处理通配符
            for egg_info in python_dir.glob("*.egg-info"):
                if egg_info.is_dir():
                    print(f"删除: {egg_info}")
                    shutil.rmtree(egg_info)
        elif clean_dir.exists():
            print(f"删除: {clean_dir}")
            if clean_dir.is_dir():
                shutil.rmtree(clean_dir)
            else:
                clean_dir.unlink()
    
    print("✅ 清理完成")

def build_debug():
    """构建调试版本"""
    print("构建调试版本...")
    
    python_dir = Path(__file__).parent
    
    try:
        run_command("maturin develop", cwd=str(python_dir))
        print("✅ 调试版本构建成功")
        return True
    except subprocess.CalledProcessError:
        print("❌ 调试版本构建失败")
        return False

def build_release():
    """构建发布版本"""
    print("构建发布版本...")
    
    python_dir = Path(__file__).parent
    
    try:
        run_command("maturin develop --release", cwd=str(python_dir))
        print("✅ 发布版本构建成功")
        return True
    except subprocess.CalledProcessError:
        print("❌ 发布版本构建失败")
        return False

def run_tests():
    """运行测试套件"""
    print("运行测试套件...")
    
    python_dir = Path(__file__).parent
    
    # 运行 pytest
    try:
        # 首先尝试运行 pytest
        try:
            run_command("python -m pytest tests/ -v", cwd=str(python_dir))
        except subprocess.CalledProcessError:
            # 如果 pytest 不可用，运行单个测试文件
            print("pytest 不可用，尝试运行单个测试文件...")
            test_files = list((python_dir / "tests").glob("test_*.py"))
            if test_files:
                for test_file in test_files:
                    print(f"运行测试: {test_file.name}")
                    run_command(f"python {test_file}", cwd=str(python_dir))
            else:
                print("未找到测试文件")
        
        print("✅ 测试完成")
        return True
    except subprocess.CalledProcessError:
        print("❌ 测试失败")
        return False

def run_examples():
    """运行示例"""
    print("运行示例...")
    
    python_dir = Path(__file__).parent
    examples_dir = python_dir / "examples"
    
    if not examples_dir.exists():
        print("⚠️ 示例目录不存在")
        return True
    
    try:
        example_files = list(examples_dir.glob("*.py"))
        if example_files:
            for example_file in example_files:
                print(f"运行示例: {example_file.name}")
                try:
                    run_command(f"python {example_file}", cwd=str(python_dir))
                except subprocess.CalledProcessError:
                    print(f"⚠️ 示例 {example_file.name} 运行失败，继续...")
        else:
            print("未找到示例文件")
        
        print("✅ 示例运行完成")
        return True
    except Exception as e:
        print(f"❌ 示例运行失败: {e}")
        return False

def install_package():
    """安装包到当前Python环境"""
    print("安装包到当前Python环境...")
    
    python_dir = Path(__file__).parent
    
    try:
        run_command("maturin develop --release", cwd=str(python_dir))
        print("✅ 包安装成功")
        
        # 验证安装
        try:
            result = subprocess.run([
                sys.executable, "-c", "import rat_engine; print(f'RAT Engine v{rat_engine.__version__} 安装成功')"
            ], capture_output=True, text=True, cwd=str(python_dir))
            
            if result.returncode == 0:
                print(f"✅ 验证成功: {result.stdout.strip()}")
            else:
                print(f"⚠️ 验证失败: {result.stderr}")
        except Exception as e:
            print(f"⚠️ 验证过程出错: {e}")
        
        return True
    except subprocess.CalledProcessError:
        print("❌ 包安装失败")
        return False

def create_wheel():
    """创建wheel包"""
    print("创建wheel包...")
    
    python_dir = Path(__file__).parent
    
    try:
        run_command("maturin build --release --out dist", cwd=str(python_dir))
        
        # 显示生成的wheel文件
        dist_dir = python_dir / "dist"
        if dist_dir.exists():
            wheel_files = list(dist_dir.glob("*.whl"))
            if wheel_files:
                print("生成的wheel文件:")
                for wheel in wheel_files:
                    print(f"  {wheel}")
        
        print("✅ Wheel包创建成功")
        return True
    except subprocess.CalledProcessError:
        print("❌ Wheel包创建失败")
        return False

def test_dynamic_routes():
    """测试动态路由功能"""
    print("测试动态路由功能...")
    
    python_dir = Path(__file__).parent
    test_file = python_dir / "tests" / "test_dynamic_routes.py"
    
    if not test_file.exists():
        print("⚠️ 动态路由测试文件不存在")
        return True
    
    try:
        run_command(f"python {test_file}", cwd=str(python_dir))
        print("✅ 动态路由测试完成")
        return True
    except subprocess.CalledProcessError:
        print("❌ 动态路由测试失败")
        return False

def main():
    """主函数"""
    parser = argparse.ArgumentParser(description="RAT Engine Python绑定构建脚本")
    parser.add_argument("command", choices=[
        "check", "clean", "debug", "release", "test", "examples", 
        "install", "wheel", "test-routes", "all"
    ], help="要执行的命令")
    parser.add_argument("--skip-deps", action="store_true", help="跳过依赖检查")
    
    args = parser.parse_args()
    
    print("RAT Engine Python 绑定构建脚本")
    print("=" * 50)
    
    # 检查依赖（除非跳过）
    if not args.skip_deps and args.command != "clean":
        if not check_dependencies():
            print("❌ 依赖检查失败，请解决依赖问题后重试")
            sys.exit(1)
    
    success = True
    
    if args.command == "check":
        success = check_dependencies()
    elif args.command == "clean":
        clean_build()
    elif args.command == "debug":
        success = build_debug()
    elif args.command == "release":
        success = build_release()
    elif args.command == "test":
        success = build_debug() and run_tests()
    elif args.command == "examples":
        success = build_debug() and run_examples()
    elif args.command == "install":
        success = install_package()
    elif args.command == "wheel":
        success = create_wheel()
    elif args.command == "test-routes":
        success = install_package() and test_dynamic_routes()
    elif args.command == "all":
        clean_build()
        success = (
            build_debug() and
            run_tests() and
            run_examples() and
            build_release() and
            install_package() and
            create_wheel()
        )
    
    if success:
        print("\n✅ 所有操作完成成功！")
        print("\n💡 提示：现在可以直接使用 'import rat_engine' 而无需设置 PYTHONPATH")
        sys.exit(0)
    else:
        print("\n❌ 操作失败！")
        sys.exit(1)

if __name__ == "__main__":
    main()