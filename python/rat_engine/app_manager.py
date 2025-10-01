#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine 应用管理器

提供统一的应用管理接口，支持 CLI、PyQt 等多种集成方式。
专注于应用生命周期管理和配置。
"""

import sys
import os
import threading
import time
import importlib.util
from typing import Optional, Dict, Any, Callable, List, Union
from pathlib import Path
import logging

from .web_app import RatApp
from .security import handle_secure_error

logger = logging.getLogger(__name__)


class AppManager:
    """
    RAT Engine 应用管理器
    
    提供统一的应用管理接口，支持：
    - 应用发现和加载
    - 生命周期管理
    - 配置管理
    - 文件监控和热重载
    - 多种集成方式（CLI、GUI 等）
    """
    
    def __init__(self, app_file: Optional[str] = None, config: Optional[Dict[str, Any]] = None):
        self.app_file = app_file
        self.app: Optional[RatApp] = None
        self.config = config or {}
        self._running = False
        self._monitor_thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()
        self._callbacks: Dict[str, List[Callable]] = {
            'on_app_loaded': [],
            'on_app_started': [],
            'on_app_stopped': [],
            'on_app_error': [],
            'on_file_changed': []
        }
        
        # 默认配置
        self.default_config = {
            'host': '127.0.0.1',
            'port': 8000,
            'debug': False,
            'auto_reload': False,
            'log_level': 'info',
            'enable_access_log': True,
            'enable_error_log': True,
            'blocking': True
        }
    
    def register_callback(self, event: str, callback: Callable):
        """注册事件回调
        
        Args:
            event: 事件名称 ('on_app_loaded', 'on_app_started', 'on_app_stopped', 'on_app_error', 'on_file_changed')
            callback: 回调函数
        """
        if event in self._callbacks:
            self._callbacks[event].append(callback)
        else:
            raise ValueError(f"Unknown event: {event}")
    
    def _trigger_callback(self, event: str, *args, **kwargs):
        """触发事件回调"""
        for callback in self._callbacks.get(event, []):
            try:
                callback(*args, **kwargs)
            except Exception as e:
                logger.error(f"回调函数执行失败 ({event}): {e}")
    
    def find_app(self, module_path: str) -> Optional[RatApp]:
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
                logger.error(f"No RatApp found in {module_path}")
                return None
            
            if len(app_candidates) == 1:
                name, app = app_candidates[0]
                logger.info(f"Found RatApp: {name}")
                return app
            
            # 多个应用实例，优先选择名为 'app' 的
            for name, app in app_candidates:
                if name == 'app':
                    logger.info(f"Found RatApp: {name} (selected from {len(app_candidates)} candidates)")
                    return app
            
            # 选择第一个
            name, app = app_candidates[0]
            logger.info(f"Found RatApp: {name} (selected from {len(app_candidates)} candidates)")
            return app
        
        except Exception as e:
            client_message, error_id = handle_secure_error(e, {'operation': 'module_loading', 'module': module_path})
            logger.error(f"Module loading failed [ID: {error_id}]: {client_message}")
            self._trigger_callback('on_app_error', e, error_id, client_message)
            return None
    
    def load_app(self, app_file: Optional[str] = None) -> bool:
        """加载应用
        
        Args:
            app_file: 应用文件路径，如果不提供则使用初始化时的路径
            
        Returns:
            是否加载成功
        """
        if app_file:
            self.app_file = app_file
        
        if not self.app_file:
            logger.error("No app file specified")
            return False
        
        if not os.path.exists(self.app_file):
            logger.error(f"App file not found: {self.app_file}")
            return False
        
        self.app = self.find_app(self.app_file)
        if self.app:
            self._trigger_callback('on_app_loaded', self.app)
            return True
        return False
    
    def configure_app(self, **config):
        """配置应用
        
        Args:
            **config: 配置参数
        """
        self.config.update(config)
    
    def get_config(self, key: str, default=None):
        """获取配置值
        
        Args:
            key: 配置键
            default: 默认值
            
        Returns:
            配置值
        """
        return self.config.get(key, self.default_config.get(key, default))
    
    def start_app(self, **run_config) -> bool:
        """启动应用
        
        Args:
            **run_config: 运行时配置
            
        Returns:
            是否启动成功
        """
        if not self.app:
            logger.error("No app loaded")
            return False
        
        if self._running:
            logger.warning("App is already running")
            return True
        
        try:
            # 合并配置
            final_config = {**self.default_config, **self.config, **run_config}
            
            # 启动文件监控（如果启用）
            if final_config.get('auto_reload') and self.app_file:
                self._start_file_monitor()
            
            self._running = True
            self._trigger_callback('on_app_started', self.app, final_config)
            
            # 启动应用
            self.app.run(**final_config)
            
            return True
        
        except Exception as e:
            self._running = False
            client_message, error_id = handle_secure_error(e, {'operation': 'app_startup'})
            logger.error(f"App startup failed [ID: {error_id}]: {client_message}")
            self._trigger_callback('on_app_error', e, error_id, client_message)
            return False
    
    def stop_app(self):
        """停止应用"""
        if not self._running:
            return
        
        self._running = False
        self._stop_event.set()
        
        if self.app:
            try:
                self.app.stop()
                self._trigger_callback('on_app_stopped', self.app)
            except Exception as e:
                logger.error(f"Error stopping app: {e}")
        
        # 停止文件监控
        if self._monitor_thread and self._monitor_thread.is_alive():
            self._monitor_thread.join(timeout=1)
    
    def restart_app(self):
        """重启应用"""
        logger.info("Restarting app...")
        self.stop_app()
        time.sleep(0.5)  # 短暂等待
        
        # 重新加载应用
        if self.load_app():
            self.start_app()
    
    def _start_file_monitor(self):
        """启动文件监控"""
        if not self.app_file or self._monitor_thread:
            return
        
        def monitor_files():
            last_modified = os.path.getmtime(self.app_file)
            
            while not self._stop_event.is_set():
                try:
                    current_modified = os.path.getmtime(self.app_file)
                    if current_modified > last_modified:
                        logger.info(f"File {self.app_file} changed, triggering reload...")
                        last_modified = current_modified
                        self._trigger_callback('on_file_changed', self.app_file)
                        
                        # 在新线程中重启应用，避免阻塞监控线程
                        restart_thread = threading.Thread(target=self.restart_app)
                        restart_thread.start()
                        break
                    
                    self._stop_event.wait(1)  # 每秒检查一次
                
                except Exception as e:
                    client_message, error_id = handle_secure_error(e, {'operation': 'file_monitoring'})
                    logger.error(f"File monitoring failed [ID: {error_id}]: {client_message}")
                    self._stop_event.wait(5)  # 错误时等待更长时间
        
        self._monitor_thread = threading.Thread(target=monitor_files, daemon=True)
        self._monitor_thread.start()
        logger.info(f"File monitoring started for {self.app_file}")
    
    def get_app_info(self) -> Dict[str, Any]:
        """获取应用信息
        
        Returns:
            应用信息字典
        """
        if not self.app:
            return {'status': 'not_loaded'}
        
        info = {
            'status': 'running' if self._running else 'loaded',
            'name': self.app.name,
            'app_file': self.app_file,
            'routes_count': len(getattr(self.app, 'routes', [])),
            'extensions_count': len(getattr(self.app, 'extensions', {})),
            'config': self.config
        }
        
        if self._running and hasattr(self.app, 'get_server_info'):
            try:
                info['server_info'] = self.app.get_server_info()
            except:
                pass
        
        return info
    
    def is_running(self) -> bool:
        """检查应用是否正在运行"""
        return self._running
    
    def __enter__(self):
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.stop_app()


def create_app_manager(app_file: str, **config) -> AppManager:
    """创建应用管理器的便捷函数
    
    Args:
        app_file: 应用文件路径
        **config: 配置参数
        
    Returns:
        AppManager 实例
    """
    manager = AppManager(app_file, config)
    if manager.load_app():
        return manager
    else:
        raise RuntimeError(f"Failed to load app from {app_file}")