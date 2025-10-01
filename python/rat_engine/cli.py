#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine CLI 工具

提供命令行接口来运行 RatApp 应用。

使用方法：
    python -m rat_engine.cli app.py
    python -m rat_engine.cli app.py --host 0.0.0.0 --port 3000 --debug
    rat-engine app.py --workers 8 --max-connections 20000
"""

import sys
import os
import argparse
import importlib.util
from typing import Optional

from .web_app import RatApp, create_app_from_file
from . import __version__


def find_rat_app(module_path: str) -> Optional[RatApp]:
    """在模块中查找 RatApp 应用实例"""
    try:
        # 加载模块
        spec = importlib.util.spec_from_file_location("app_module", module_path)
        if spec is None or spec.loader is None:
            return None
        
        module = importlib.util.module_from_spec(spec)
        sys.modules["app_module"] = module
        spec.loader.exec_module(module)
        
        # 查找 RatApp 应用实例
        app_candidates = []
        
        for name in dir(module):
            obj = getattr(module, name)
            if isinstance(obj, RatApp):
                app_candidates.append((name, obj))
        
        if not app_candidates:
            print(f"❌ No RatApp found in {module_path}", file=sys.stderr)
            return None
        
        if len(app_candidates) == 1:
            name, app = app_candidates[0]
            print(f"✅ Found RatApp: {name}")
            return app
        
        # 多个应用实例，优先选择名为 'app' 的
        for name, app in app_candidates:
            if name == 'app':
                print(f"✅ Found RatApp: {name} (selected from {len(app_candidates)} candidates)")
                return app
        
        # 选择第一个
        name, app = app_candidates[0]
        print(f"✅ Found RatApp: {name} (selected from {len(app_candidates)} candidates)")
        return app
    
    except Exception as e:
        from rat_engine.security import handle_secure_error
        client_message, error_id = handle_secure_error(e, {'operation': 'module_loading', 'module': module_path})
        print(f"Module loading failed [ID: {error_id}]: {client_message}")
        return None


def validate_file(file_path: str) -> str:
    """验证文件路径"""
    if not os.path.exists(file_path):
        raise argparse.ArgumentTypeError(f"File '{file_path}' does not exist")
    
    if not file_path.endswith('.py'):
        raise argparse.ArgumentTypeError(f"File '{file_path}' is not a Python file")
    
    return os.path.abspath(file_path)


def validate_positive_int(value: str) -> int:
    """验证正整数"""
    try:
        ivalue = int(value)
        if ivalue <= 0:
            raise argparse.ArgumentTypeError(f"'{value}' is not a positive integer")
        return ivalue
    except ValueError:
        raise argparse.ArgumentTypeError(f"'{value}' is not a valid integer")


def create_parser() -> argparse.ArgumentParser:
    """创建命令行参数解析器"""
    parser = argparse.ArgumentParser(
        prog="rat-engine",
        description="RAT Engine - High-performance Rust + Python web framework",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  rat-engine app.py                           # Run with default settings
  rat-engine app.py --host 0.0.0.0 --port 80 # Bind to all interfaces on port 80
  rat-engine app.py --debug                   # Enable debug mode
  rat-engine app.py --workers 8               # Use 8 worker threads
  rat-engine app.py --max-connections 20000   # Allow up to 20k connections

For more information, visit: https://github.com/your-org/rat-engine
        """
    )
    
    # 必需参数
    parser.add_argument(
        "app",
        type=validate_file,
        help="Python file containing the RatApp application"
    )
    
    # 服务器配置
    server_group = parser.add_argument_group("Server Configuration")
    server_group.add_argument(
        "--host", "-h",
        default="127.0.0.1",
        help="Host to bind to (default: 127.0.0.1)"
    )
    server_group.add_argument(
        "--port", "-p",
        type=int,
        default=8000,
        help="Port to bind to (default: 8000)"
    )
    
    # 性能配置
    perf_group = parser.add_argument_group("Performance Configuration")
    perf_group.add_argument(
        "--workers", "-w",
        type=validate_positive_int,
        help="Number of worker threads (default: CPU cores)"
    )
    perf_group.add_argument(
        "--max-connections",
        type=validate_positive_int,
        help="Maximum number of concurrent connections (default: 10000)"
    )
    
    # 开发配置
    dev_group = parser.add_argument_group("Development Options")
    dev_group.add_argument(
        "--debug", "-d",
        action="store_true",
        help="Enable debug mode with detailed logging"
    )
    dev_group.add_argument(
        "--reload",
        action="store_true",
        help="Enable auto-reload on file changes (development only)"
    )
    
    # 其他选项
    other_group = parser.add_argument_group("Other Options")
    other_group.add_argument(
        "--version", "-v",
        action="version",
        version=f"RAT Engine {__version__}"
    )
    other_group.add_argument(
        "--quiet", "-q",
        action="store_true",
        help="Suppress startup messages"
    )
    
    return parser


def print_startup_banner(args, app: RatApp):
    """打印启动横幅"""
    if args.quiet:
        return
    
    print("\n" + "="*60)
    print("🚀 RAT Engine - High-Performance Web Framework")
    print("="*60)
    print(f"📁 Application: {os.path.basename(args.app)}")
    print(f"🌐 Server: http://{args.host}:{args.port}")
    
    server_info = app.get_server_info()
    print(f"⚡ Workers: {server_info.get('workers', 'auto')}")
    print(f"🔗 Max Connections: {server_info.get('max_connections', '10000')}")
    print(f"🐍 Implementation: {server_info.get('implementation', 'Rust + Python')}")
    
    if args.debug:
        print("🐛 Debug Mode: ENABLED")
    
    if args.reload:
        print("🔄 Auto-reload: ENABLED")
    
    print("\n📋 Available Routes:")
    if hasattr(app, 'routes') and app.routes:
        for route in app.routes:
            methods = ', '.join(route.methods)
            print(f"   {methods:12} {route.pattern}")
    else:
        print("   (No routes registered)")
    
    print("\n💡 Tips:")
    print("   • Press Ctrl+C to stop the server")
    print("   • Visit /health for server health check")
    print("   • Visit /api/metrics for performance metrics")
    print("="*60 + "\n")


def setup_auto_reload(app_file: str):
    """设置文件监控和自动重载（简单实现）"""
    import threading
    import time
    
    last_modified = os.path.getmtime(app_file)
    
    def check_file_changes():
        nonlocal last_modified
        while True:
            try:
                current_modified = os.path.getmtime(app_file)
                if current_modified > last_modified:
                    print(f"\n🔄 File {app_file} changed, restarting...")
                    last_modified = current_modified
                    # 简单的重启实现：退出进程，让外部工具重启
                    os._exit(3)  # 特殊退出码表示需要重启
                time.sleep(1)
            except Exception as e:
                from rat_engine.security import handle_secure_error
                client_message, error_id = handle_secure_error(e, {'operation': 'file_monitoring'})
                print(f"File monitoring failed [ID: {error_id}]: {client_message}")
                time.sleep(5)
    
    monitor_thread = threading.Thread(target=check_file_changes, daemon=True)
    monitor_thread.start()


def main():
    """主函数"""
    parser = create_parser()
    args = parser.parse_args()
    
    try:
        # 查找 RatApp 应用
        app = find_rat_app(args.app)
        if app is None:
            sys.exit(1)
        
        # 更新引擎配置
        engine_kwargs = {
            'host': args.host,
            'port': args.port
        }
        
        if args.workers:
            engine_kwargs['workers'] = args.workers
        
        if args.max_connections:
            engine_kwargs['max_connections'] = args.max_connections
        
        # 重新创建组件（如果需要）
        if any(engine_kwargs.get(k) != getattr(app.server, f'get_{k}', lambda: None)() 
               for k in ['host', 'port']):
            from .web_app import PyRouter, PyServer
            app._router = PyRouter()
            app.server = PyServer(**engine_kwargs)
            # 新架构中不需要设置全局处理器
        
        # 设置自动重载
        if args.reload:
            setup_auto_reload(args.app)
        
        # 打印启动信息
        print_startup_banner(args, app)
        
        # 启动服务器
        try:
            app.run(host=args.host, port=args.port, debug=args.debug)
        except KeyboardInterrupt:
            if not args.quiet:
                print("\n👋 Server stopped by user")
        except Exception as e:
            from rat_engine.security import handle_secure_error
            client_message, error_id = handle_secure_error(e, {'operation': 'server_startup'})
            print(f"Server startup failed [ID: {error_id}]: {client_message}")
            sys.exit(1)
    
    except Exception as e:
        from rat_engine.security import handle_secure_error
        client_message, error_id = handle_secure_error(e, {'operation': 'cli_execution'})
        print(f"CLI execution failed [ID: {error_id}]: {client_message}")
        sys.exit(1)


def cli_entry_point():
    """CLI 入口点（用于 setup.py 中的 console_scripts）"""
    main()


if __name__ == "__main__":
    main()