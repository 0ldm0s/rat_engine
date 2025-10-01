#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine Python 错误处理工具模块

提供流式连接的错误处理、监控和恢复机制：
- ConnectionManager: 连接生命周期管理
- ErrorRecovery: 错误恢复策略
- StreamMonitor: 流式连接监控
- GracefulShutdown: 优雅关闭处理
"""

import asyncio
import threading
import time
import weakref
from datetime import datetime, timedelta
from typing import Dict, Set, Optional, Callable, Any
from dataclasses import dataclass, field
from enum import Enum
import logging

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class ConnectionState(Enum):
    """连接状态枚举"""
    CONNECTING = "connecting"
    CONNECTED = "connected"
    DISCONNECTING = "disconnecting"
    DISCONNECTED = "disconnected"
    ERROR = "error"

@dataclass
class ConnectionInfo:
    """连接信息"""
    connection_id: str
    remote_addr: str
    created_at: datetime = field(default_factory=datetime.now)
    last_activity: datetime = field(default_factory=datetime.now)
    state: ConnectionState = ConnectionState.CONNECTING
    error_count: int = 0
    bytes_sent: int = 0
    messages_sent: int = 0
    metadata: Dict[str, Any] = field(default_factory=dict)

class ConnectionManager:
    """连接管理器 - 管理所有活跃的流式连接"""
    
    def __init__(self, max_connections: int = 1000, cleanup_interval: int = 30):
        self.max_connections = max_connections
        self.cleanup_interval = cleanup_interval
        self.connections: Dict[str, ConnectionInfo] = {}
        self.connection_refs: Dict[str, Any] = {}  # 弱引用存储
        self.connection_counter = 0
        self._lock = threading.RLock()
        self._cleanup_task = None
        self._shutdown = False
        
        # 启动清理任务
        self._start_cleanup_task()
    
    def _start_cleanup_task(self):
        """启动连接清理任务"""
        def cleanup_worker():
            while not self._shutdown:
                try:
                    self._cleanup_stale_connections()
                    time.sleep(self.cleanup_interval)
                except Exception as e:
                    logger.error(f"连接清理任务错误: {e}")
        
        self._cleanup_task = threading.Thread(target=cleanup_worker, daemon=True)
        self._cleanup_task.start()
    
    def register_connection(self, connection_id: str, remote_addr: str, 
                          connection_ref: Any = None, metadata: Dict[str, Any] = None) -> bool:
        """注册新连接"""
        with self._lock:
            if len(self.connections) >= self.max_connections:
                logger.warning(f"连接数已达上限 {self.max_connections}，拒绝新连接 {connection_id}")
                return False
            
            if connection_id in self.connections:
                logger.warning(f"连接 {connection_id} 已存在")
                return False
            
            # 创建连接信息
            conn_info = ConnectionInfo(
                connection_id=connection_id,
                remote_addr=remote_addr,
                metadata=metadata or {}
            )
            
            self.connections[connection_id] = conn_info
            
            # 存储连接引用（如果提供）
            if connection_ref is not None:
                self.connection_refs[connection_id] = weakref.ref(connection_ref)
            
            self.connection_counter += 1
            
            logger.info(f"📡 注册连接: {connection_id} from {remote_addr}, 活跃连接数: {len(self.connections)}")
            return True
    
    def update_connection_state(self, connection_id: str, state: ConnectionState):
        """更新连接状态"""
        with self._lock:
            if connection_id in self.connections:
                old_state = self.connections[connection_id].state
                self.connections[connection_id].state = state
                self.connections[connection_id].last_activity = datetime.now()
                
                if old_state != state:
                    logger.debug(f"🔄 连接 {connection_id} 状态变更: {old_state.value} -> {state.value}")
    
    def record_activity(self, connection_id: str, bytes_sent: int = 0, messages_sent: int = 0):
        """记录连接活动"""
        with self._lock:
            if connection_id in self.connections:
                conn = self.connections[connection_id]
                conn.last_activity = datetime.now()
                conn.bytes_sent += bytes_sent
                conn.messages_sent += messages_sent
    
    def record_error(self, connection_id: str, error: Exception):
        """记录连接错误"""
        with self._lock:
            if connection_id in self.connections:
                conn = self.connections[connection_id]
                conn.error_count += 1
                conn.state = ConnectionState.ERROR
                conn.last_activity = datetime.now()
                
                logger.warning(f"⚠️ 连接 {connection_id} 发生错误 (第{conn.error_count}次): {error}")
    
    def unregister_connection(self, connection_id: str, reason: str = "normal"):
        """注销连接"""
        with self._lock:
            if connection_id in self.connections:
                conn_info = self.connections.pop(connection_id)
                self.connection_refs.pop(connection_id, None)
                
                duration = datetime.now() - conn_info.created_at
                logger.info(f"🔌 注销连接: {connection_id}, 原因: {reason}, "
                          f"持续时间: {duration.total_seconds():.1f}s, "
                          f"发送: {conn_info.messages_sent} 消息/{conn_info.bytes_sent} 字节")
                
                return conn_info
        return None
    
    def get_connection_info(self, connection_id: str) -> Optional[ConnectionInfo]:
        """获取连接信息"""
        with self._lock:
            return self.connections.get(connection_id)
    
    def is_connection_alive(self, connection_id: str) -> bool:
        """检查连接是否存活"""
        with self._lock:
            if connection_id not in self.connections:
                return False
            
            # 检查弱引用是否仍然有效
            if connection_id in self.connection_refs:
                ref = self.connection_refs[connection_id]
                if ref() is None:  # 弱引用已失效
                    logger.debug(f"🔌 连接 {connection_id} 的引用已失效")
                    self.unregister_connection(connection_id, "reference_lost")
                    return False
            
            return True
    
    def _cleanup_stale_connections(self):
        """清理过期连接"""
        current_time = datetime.now()
        stale_threshold = timedelta(minutes=5)  # 5分钟无活动视为过期
        
        with self._lock:
            stale_connections = []
            
            for conn_id, conn_info in self.connections.items():
                # 检查是否过期
                if current_time - conn_info.last_activity > stale_threshold:
                    stale_connections.append(conn_id)
                # 检查弱引用是否失效
                elif conn_id in self.connection_refs:
                    ref = self.connection_refs[conn_id]
                    if ref() is None:
                        stale_connections.append(conn_id)
            
            # 清理过期连接
            for conn_id in stale_connections:
                self.unregister_connection(conn_id, "stale")
    
    def get_connection_count(self) -> int:
        """获取当前活跃连接数"""
        with self._lock:
            return len(self.connections)
    
    def get_stats(self) -> Dict[str, Any]:
        """获取连接统计"""
        with self._lock:
            state_counts = {}
            total_bytes = 0
            total_messages = 0
            
            for conn in self.connections.values():
                state_counts[conn.state.value] = state_counts.get(conn.state.value, 0) + 1
                total_bytes += conn.bytes_sent
                total_messages += conn.messages_sent
            
            return {
                "active_connections": len(self.connections),
                "total_connections": self.connection_counter,
                "state_distribution": state_counts,
                "total_bytes_sent": total_bytes,
                "total_messages_sent": total_messages,
                "max_connections": self.max_connections
            }
    
    def shutdown(self):
        """关闭连接管理器"""
        logger.info("🛑 关闭连接管理器...")
        self._shutdown = True
        
        # 等待清理任务结束
        if self._cleanup_task and self._cleanup_task.is_alive():
            self._cleanup_task.join(timeout=5)
        
        # 清理所有连接
        with self._lock:
            connection_ids = list(self.connections.keys())
            for conn_id in connection_ids:
                self.unregister_connection(conn_id, "shutdown")

class ErrorRecovery:
    """错误恢复策略"""
    
    def __init__(self, max_retries: int = 3, base_delay: float = 1.0, max_delay: float = 30.0):
        self.max_retries = max_retries
        self.base_delay = base_delay
        self.max_delay = max_delay
        self.retry_counts: Dict[str, int] = {}
    
    def should_retry(self, connection_id: str, error: Exception) -> bool:
        """判断是否应该重试"""
        # 获取重试次数
        retry_count = self.retry_counts.get(connection_id, 0)
        
        # 检查是否超过最大重试次数
        if retry_count >= self.max_retries:
            logger.warning(f"连接 {connection_id} 已达最大重试次数 {self.max_retries}")
            return False
        
        # 检查错误类型是否可重试
        if self._is_recoverable_error(error):
            self.retry_counts[connection_id] = retry_count + 1
            return True
        
        logger.error(f"连接 {connection_id} 发生不可恢复错误: {error}")
        return False
    
    def get_retry_delay(self, connection_id: str) -> float:
        """获取重试延迟时间（指数退避）"""
        retry_count = self.retry_counts.get(connection_id, 0)
        delay = min(self.base_delay * (2 ** retry_count), self.max_delay)
        return delay
    
    def reset_retry_count(self, connection_id: str):
        """重置重试计数"""
        self.retry_counts.pop(connection_id, None)
    
    def _is_recoverable_error(self, error: Exception) -> bool:
        """判断错误是否可恢复"""
        error_str = str(error).lower()
        
        # 网络相关的可恢复错误
        recoverable_patterns = [
            "connection reset",
            "broken pipe",
            "timeout",
            "temporary failure",
            "resource temporarily unavailable"
        ]
        
        # 不可恢复的错误
        unrecoverable_patterns = [
            "permission denied",
            "authentication failed",
            "invalid request",
            "malformed"
        ]
        
        # 检查不可恢复错误
        for pattern in unrecoverable_patterns:
            if pattern in error_str:
                return False
        
        # 检查可恢复错误
        for pattern in recoverable_patterns:
            if pattern in error_str:
                return True
        
        # 默认认为网络错误是可恢复的
        return "network" in error_str or "connection" in error_str

class StreamMonitor:
    """流式连接监控器"""
    
    def __init__(self, connection_manager: ConnectionManager, check_interval: float = 1.0):
        self.connection_manager = connection_manager
        self.check_interval = check_interval
        self.monitors: Dict[str, threading.Thread] = {}
        self.callbacks: Dict[str, Callable] = {}
        self._shutdown = False
    
    def start_monitoring(self, connection_id: str, stream_ref: Any, 
                        on_disconnect: Optional[Callable] = None):
        """开始监控连接"""
        if connection_id in self.monitors:
            logger.warning(f"连接 {connection_id} 已在监控中")
            return
        
        if on_disconnect:
            self.callbacks[connection_id] = on_disconnect
        
        def monitor_worker():
            logger.debug(f"🔍 开始监控连接: {connection_id}")
            
            while not self._shutdown:
                try:
                    # 检查连接是否仍然活跃
                    if not self.connection_manager.is_connection_alive(connection_id):
                        logger.debug(f"🔌 连接 {connection_id} 已断开")
                        break
                    
                    # 检查流引用是否仍然有效
                    if hasattr(stream_ref, 'is_closed') and stream_ref.is_closed():
                        logger.debug(f"🔌 流 {connection_id} 已关闭")
                        self.connection_manager.unregister_connection(connection_id, "stream_closed")
                        break
                    
                    # 更新连接状态
                    self.connection_manager.update_connection_state(connection_id, ConnectionState.CONNECTED)
                    
                    time.sleep(self.check_interval)
                    
                except Exception as e:
                    logger.error(f"监控连接 {connection_id} 时发生错误: {e}")
                    self.connection_manager.record_error(connection_id, e)
                    break
            
            # 清理监控
            self._cleanup_monitor(connection_id)
            
            # 调用断开回调
            if connection_id in self.callbacks:
                try:
                    self.callbacks[connection_id](connection_id)
                except Exception as e:
                    logger.error(f"执行断开回调时发生错误: {e}")
        
        # 启动监控线程
        monitor_thread = threading.Thread(target=monitor_worker, daemon=True)
        monitor_thread.start()
        self.monitors[connection_id] = monitor_thread
    
    def stop_monitoring(self, connection_id: str):
        """停止监控连接"""
        self._cleanup_monitor(connection_id)
    
    def _cleanup_monitor(self, connection_id: str):
        """清理监控资源"""
        self.monitors.pop(connection_id, None)
        self.callbacks.pop(connection_id, None)
        logger.debug(f"🧹 清理连接监控: {connection_id}")
    
    def shutdown(self):
        """关闭监控器"""
        logger.info("🛑 关闭流式监控器...")
        self._shutdown = True
        
        # 等待所有监控线程结束
        for thread in self.monitors.values():
            if thread.is_alive():
                thread.join(timeout=2)
        
        self.monitors.clear()
        self.callbacks.clear()

class GracefulShutdown:
    """优雅关闭处理器"""
    
    def __init__(self, connection_manager: ConnectionManager, 
                 stream_monitor: StreamMonitor, timeout: float = 30.0):
        self.connection_manager = connection_manager
        self.stream_monitor = stream_monitor
        self.timeout = timeout
        self._shutdown_started = False
    
    def initiate_shutdown(self):
        """启动优雅关闭流程"""
        if self._shutdown_started:
            return
        
        self._shutdown_started = True
        logger.info("🛑 启动优雅关闭流程...")
        
        start_time = time.time()
        
        # 1. 停止接受新连接（这需要在应用层实现）
        logger.info("📵 停止接受新连接")
        
        # 2. 等待现有连接完成
        logger.info("⏳ 等待现有连接完成...")
        while time.time() - start_time < self.timeout:
            stats = self.connection_manager.get_stats()
            active_connections = stats["active_connections"]
            
            if active_connections == 0:
                logger.info("✅ 所有连接已完成")
                break
            
            logger.info(f"⏳ 等待 {active_connections} 个连接完成...")
            time.sleep(1)
        
        # 3. 强制关闭剩余连接
        remaining_stats = self.connection_manager.get_stats()
        if remaining_stats["active_connections"] > 0:
            logger.warning(f"⚠️ 强制关闭 {remaining_stats['active_connections']} 个剩余连接")
        
        # 4. 关闭组件
        self.stream_monitor.shutdown()
        self.connection_manager.shutdown()
        
        logger.info("✅ 优雅关闭完成")

# 全局实例（可选）
_default_connection_manager = None
_default_stream_monitor = None
_default_error_recovery = None

def get_default_connection_manager() -> ConnectionManager:
    """获取默认连接管理器"""
    global _default_connection_manager
    if _default_connection_manager is None:
        _default_connection_manager = ConnectionManager()
    return _default_connection_manager

def get_default_stream_monitor() -> StreamMonitor:
    """获取默认流监控器"""
    global _default_stream_monitor
    if _default_stream_monitor is None:
        _default_stream_monitor = StreamMonitor(get_default_connection_manager())
    return _default_stream_monitor

def get_default_error_recovery() -> ErrorRecovery:
    """获取默认错误恢复器"""
    global _default_error_recovery
    if _default_error_recovery is None:
        _default_error_recovery = ErrorRecovery()
    return _default_error_recovery

# 便捷函数
def register_streaming_connection(connection_id: str, remote_addr: str, 
                                stream_ref: Any, metadata: Dict[str, Any] = None,
                                on_disconnect: Optional[Callable] = None) -> bool:
    """注册流式连接（使用默认管理器）"""
    manager = get_default_connection_manager()
    monitor = get_default_stream_monitor()
    
    # 注册连接
    if manager.register_connection(connection_id, remote_addr, stream_ref, metadata):
        # 开始监控
        monitor.start_monitoring(connection_id, stream_ref, on_disconnect)
        return True
    return False

def unregister_streaming_connection(connection_id: str, reason: str = "normal"):
    """注销流式连接（使用默认管理器）"""
    manager = get_default_connection_manager()
    monitor = get_default_stream_monitor()
    
    monitor.stop_monitoring(connection_id)
    manager.unregister_connection(connection_id, reason)

def get_streaming_stats() -> Dict[str, Any]:
    """获取流式连接统计（使用默认管理器）"""
    return get_default_connection_manager().get_stats()

def shutdown_streaming_system():
    """关闭流式系统（使用默认组件）"""
    global _default_connection_manager, _default_stream_monitor, _default_error_recovery
    
    if _default_stream_monitor:
        _default_stream_monitor.shutdown()
        _default_stream_monitor = None
    
    if _default_connection_manager:
        _default_connection_manager.shutdown()
        _default_connection_manager = None
    
    _default_error_recovery = None