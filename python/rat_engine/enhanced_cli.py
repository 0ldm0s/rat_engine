#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine 增强 CLI 系统

提供更灵活的命令行接口，支持：
- 传统 CLI 模式
- GUI 集成模式（PyQt、Tkinter 等）
- 编程式接口
- 扩展和插件支持
"""

import sys
import os
import argparse
import logging
import signal
import threading
import time
from typing import Optional, Dict, Any, List, Callable, Union
from pathlib import Path

from .app_manager import AppManager, create_app_manager
from .web_app import RatApp
from .security import handle_secure_error

logger = logging.getLogger(__name__)


class EnhancedCLI:
    """
    增强的 CLI 系统
    
    支持多种运行模式：
    - CLI 模式：传统命令行界面
    - GUI 模式：与 PyQt 等 GUI 框架集成
    - API 模式：提供编程接口
    """
    
    def __init__(self, name: str = "RAT Engine"):
        self.name = name
        self.app_manager: Optional[AppManager] = None
        self.parser = self._create_parser()
        self._gui_mode = False
        self._gui_callbacks: Dict[str, List[Callable]] = {
            'on_status_change': [],
            'on_log_message': [],
            'on_error': [],
            'on_metrics_update': []
        }
        
        # 设置信号处理
        self._setup_signal_handlers()
    
    def _create_parser(self) -> argparse.ArgumentParser:
        """创建命令行参数解析器"""
        parser = argparse.ArgumentParser(
            description=f"{self.name} - 高性能 Web 应用框架",
            formatter_class=argparse.RawDescriptionHelpFormatter,
            epilog="""
示例用法:
  %(prog)s app.py                          # 运行应用
  %(prog)s app.py --host 0.0.0.0 --port 3000  # 指定主机和端口
  %(prog)s app.py --debug --reload            # 开发模式
  %(prog)s --gui app.py                       # GUI 模式
  %(prog)s --api app.py                       # API 模式
"""
        )
        
        # 应用文件
        parser.add_argument(
            'app_file',
            nargs='?',
            help='Python 应用文件路径'
        )
        
        # 运行模式
        mode_group = parser.add_mutually_exclusive_group()
        mode_group.add_argument(
            '--gui',
            action='store_true',
            help='启用 GUI 模式（与 PyQt 等集成）'
        )
        mode_group.add_argument(
            '--api',
            action='store_true',
            help='启用 API 模式（编程接口）'
        )
        
        # 服务器配置
        server_group = parser.add_argument_group('服务器配置')
        server_group.add_argument(
            '--host',
            default='127.0.0.1',
            help='绑定主机地址 (默认: 127.0.0.1)'
        )
        server_group.add_argument(
            '--port',
            type=int,
            default=8000,
            help='绑定端口 (默认: 8000)'
        )
        server_group.add_argument(
            '--workers',
            type=int,
            default=1,
            help='工作线程数 (默认: 1)'
        )
        server_group.add_argument(
            '--max-connections',
            type=int,
            default=1000,
            help='最大连接数 (默认: 1000)'
        )
        
        # 开发配置
        dev_group = parser.add_argument_group('开发配置')
        dev_group.add_argument(
            '--debug',
            action='store_true',
            help='启用调试模式'
        )
        dev_group.add_argument(
            '--reload',
            action='store_true',
            help='启用自动重载'
        )
        dev_group.add_argument(
            '--log-level',
            choices=['debug', 'info', 'warning', 'error'],
            default='info',
            help='日志级别 (默认: info)'
        )
        
        # Celery 配置
        celery_group = parser.add_argument_group('Celery 配置')
        celery_group.add_argument(
            '--celery-broker',
            help='Celery broker URL (例如: redis://localhost:6379/0)'
        )
        celery_group.add_argument(
            '--celery-backend',
            help='Celery result backend URL'
        )
        celery_group.add_argument(
            '--celery-workers',
            type=int,
            default=1,
            help='Celery worker 数量 (默认: 1)'
        )
        
        # 其他选项
        parser.add_argument(
            '--version',
            action='version',
            version=f'{self.name} 1.0.0'
        )
        parser.add_argument(
            '--config',
            help='配置文件路径'
        )
        
        return parser
    
    def _setup_signal_handlers(self):
        """设置信号处理器"""
        def signal_handler(signum, frame):
            logger.info(f"Received signal {signum}, shutting down...")
            self.stop()
            sys.exit(0)
        
        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)
    
    def register_gui_callback(self, event: str, callback: Callable):
        """注册 GUI 回调函数
        
        Args:
            event: 事件名称
            callback: 回调函数
        """
        if event in self._gui_callbacks:
            self._gui_callbacks[event].append(callback)
        else:
            raise ValueError(f"Unknown GUI event: {event}")
    
    def _trigger_gui_callback(self, event: str, *args, **kwargs):
        """触发 GUI 回调"""
        for callback in self._gui_callbacks.get(event, []):
            try:
                callback(*args, **kwargs)
            except Exception as e:
                logger.error(f"GUI 回调执行失败 ({event}): {e}")
    
    def load_config_file(self, config_path: str) -> Dict[str, Any]:
        """加载配置文件
        
        Args:
            config_path: 配置文件路径
            
        Returns:
            配置字典
        """
        try:
            import json
            import yaml
            
            with open(config_path, 'r', encoding='utf-8') as f:
                if config_path.endswith('.json'):
                    return json.load(f)
                elif config_path.endswith(('.yml', '.yaml')):
                    return yaml.safe_load(f)
                else:
                    # 尝试 JSON 格式
                    content = f.read()
                    return json.loads(content)
        
        except Exception as e:
            logger.error(f"Failed to load config file {config_path}: {e}")
            return {}
    
    def run_cli_mode(self, args: argparse.Namespace) -> int:
        """运行 CLI 模式
        
        Args:
            args: 解析后的命令行参数
            
        Returns:
            退出码
        """
        if not args.app_file:
            self.parser.print_help()
            return 1
        
        # 验证应用文件
        if not os.path.exists(args.app_file):
            logger.error(f"App file not found: {args.app_file}")
            return 1
        
        # 加载配置文件
        config = {}
        if args.config:
            config.update(self.load_config_file(args.config))
        
        # 合并命令行参数
        cli_config = {
            'host': args.host,
            'port': args.port,
            'workers': args.workers,
            'max_connections': args.max_connections,
            'debug': args.debug,
            'auto_reload': args.reload,
            'log_level': args.log_level,
            'blocking': True
        }
        config.update(cli_config)
        
        # Celery 配置
        if args.celery_broker:
            config['celery_broker_url'] = args.celery_broker
        if args.celery_backend:
            config['celery_result_backend'] = args.celery_backend
        if args.celery_workers:
            config['celery_workers'] = args.celery_workers
        
        try:
            # 创建应用管理器
            self.app_manager = create_app_manager(args.app_file, **config)
            
            # 打印启动信息
            self._print_startup_banner(args.app_file, config)
            
            # 启动应用
            success = self.app_manager.start_app()
            return 0 if success else 1
        
        except Exception as e:
            client_message, error_id = handle_secure_error(e, {'operation': 'cli_startup'})
            logger.error(f"CLI startup failed [ID: {error_id}]: {client_message}")
            return 1
    
    def run_gui_mode(self, args: argparse.Namespace) -> 'GUIInterface':
        """运行 GUI 模式
        
        Args:
            args: 解析后的命令行参数
            
        Returns:
            GUI 接口对象
        """
        self._gui_mode = True
        
        # 创建 GUI 接口
        gui_interface = GUIInterface(self, args)
        
        return gui_interface
    
    def run_api_mode(self, args: argparse.Namespace) -> 'APIInterface':
        """运行 API 模式
        
        Args:
            args: 解析后的命令行参数
            
        Returns:
            API 接口对象
        """
        # 创建 API 接口
        api_interface = APIInterface(self, args)
        
        return api_interface
    
    def _print_startup_banner(self, app_file: str, config: Dict[str, Any]):
        """打印启动横幅"""
        print(f"\n🚀 {self.name} Starting...")
        print(f"📁 App File: {app_file}")
        print(f"🌐 Server: http://{config['host']}:{config['port']}")
        print(f"👥 Workers: {config.get('workers', 1)}")
        print(f"🔗 Max Connections: {config.get('max_connections', 1000)}")
        print(f"🐛 Debug: {'ON' if config.get('debug') else 'OFF'}")
        print(f"🔄 Auto Reload: {'ON' if config.get('auto_reload') else 'OFF'}")
        
        if self.app_manager and self.app_manager.app:
            routes = getattr(self.app_manager.app, 'routes', [])
            extensions = getattr(self.app_manager.app, 'extensions', {})
            print(f"📍 Routes: {len(routes)}")
            print(f"🔌 Extensions: {len(extensions)}")
            
            # Celery 信息
            celery_config = getattr(self.app_manager.app, 'celery_config', {})
            if celery_config.get('broker_url'):
                print(f"🔄 Celery Broker: {celery_config['broker_url']}")
        
        print("\n" + "="*50)
    
    def stop(self):
        """停止应用"""
        if self.app_manager:
            self.app_manager.stop_app()
    
    def main(self, argv: Optional[List[str]] = None) -> int:
        """主入口函数
        
        Args:
            argv: 命令行参数列表
            
        Returns:
            退出码
        """
        args = self.parser.parse_args(argv)
        
        # 设置日志级别
        log_level = getattr(logging, args.log_level.upper())
        logging.basicConfig(level=log_level, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
        
        if args.gui:
            gui_interface = self.run_gui_mode(args)
            return 0  # GUI 模式不阻塞
        elif args.api:
            api_interface = self.run_api_mode(args)
            return 0  # API 模式不阻塞
        else:
            return self.run_cli_mode(args)


class GUIInterface:
    """
    GUI 集成接口
    
    提供与 PyQt、Tkinter 等 GUI 框架集成的接口
    """
    
    def __init__(self, cli: EnhancedCLI, args: argparse.Namespace):
        self.cli = cli
        self.args = args
        self.app_manager: Optional[AppManager] = None
        self._status = 'stopped'
        
        # 注册回调
        self._setup_callbacks()
    
    def _setup_callbacks(self):
        """设置回调函数"""
        def on_app_loaded(app):
            self._status = 'loaded'
            self.cli._trigger_gui_callback('on_status_change', 'loaded', app)
        
        def on_app_started(app, config):
            self._status = 'running'
            self.cli._trigger_gui_callback('on_status_change', 'running', app, config)
        
        def on_app_stopped(app):
            self._status = 'stopped'
            self.cli._trigger_gui_callback('on_status_change', 'stopped', app)
        
        def on_app_error(error, error_id, message):
            self.cli._trigger_gui_callback('on_error', error, error_id, message)
    
    def load_app(self, app_file: str, **config) -> bool:
        """加载应用
        
        Args:
            app_file: 应用文件路径
            **config: 配置参数
            
        Returns:
            是否加载成功
        """
        try:
            self.app_manager = create_app_manager(app_file, **config)
            return True
        except Exception as e:
            self.cli._trigger_gui_callback('on_error', e, None, str(e))
            return False
    
    def start_app(self, **config) -> bool:
        """启动应用（非阻塞）
        
        Args:
            **config: 运行时配置
            
        Returns:
            是否启动成功
        """
        if not self.app_manager:
            return False
        
        # 在新线程中启动应用
        def start_thread():
            config['blocking'] = False  # GUI 模式下非阻塞
            self.app_manager.start_app(**config)
        
        thread = threading.Thread(target=start_thread, daemon=True)
        thread.start()
        return True
    
    def stop_app(self):
        """停止应用"""
        if self.app_manager:
            self.app_manager.stop_app()
    
    def get_status(self) -> str:
        """获取应用状态"""
        return self._status
    
    def get_app_info(self) -> Dict[str, Any]:
        """获取应用信息"""
        if self.app_manager:
            return self.app_manager.get_app_info()
        return {'status': 'not_loaded'}
    
    def get_metrics(self) -> Dict[str, Any]:
        """获取应用指标"""
        if self.app_manager and self.app_manager.app and hasattr(self.app_manager.app, 'get_metrics'):
            try:
                return self.app_manager.app.get_metrics()
            except:
                pass
        return {}


class APIInterface:
    """
    API 编程接口
    
    提供编程式的应用管理接口
    """
    
    def __init__(self, cli: EnhancedCLI, args: argparse.Namespace):
        self.cli = cli
        self.args = args
        self.app_manager: Optional[AppManager] = None
    
    def create_app_manager(self, app_file: str, **config) -> AppManager:
        """创建应用管理器
        
        Args:
            app_file: 应用文件路径
            **config: 配置参数
            
        Returns:
            AppManager 实例
        """
        self.app_manager = create_app_manager(app_file, **config)
        return self.app_manager
    
    def run_app(self, app_file: str, **config) -> bool:
        """运行应用（便捷方法）
        
        Args:
            app_file: 应用文件路径
            **config: 配置参数
            
        Returns:
            是否运行成功
        """
        try:
            self.app_manager = create_app_manager(app_file, **config)
            return self.app_manager.start_app(**config)
        except Exception as e:
            logger.error(f"API run failed: {e}")
            return False


def main(argv: Optional[List[str]] = None) -> int:
    """主入口函数"""
    cli = EnhancedCLI()
    return cli.main(argv)


if __name__ == '__main__':
    sys.exit(main())