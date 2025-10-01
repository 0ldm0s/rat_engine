"""安全错误处理模块

实现错误信息分离和安全增强功能：
1. 将详细错误信息记录到日志，向客户端返回通用错误信息
2. 实现错误信息过滤机制，防止敏感信息泄露
"""

import re
import traceback
import os
from typing import Dict, List, Optional, Tuple, Any
from enum import Enum
from dataclasses import dataclass, field


class ErrorLevel(Enum):
    """错误级别枚举"""
    LOW = "low"          # 低风险：可以显示部分信息
    MEDIUM = "medium"    # 中风险：显示通用错误信息
    HIGH = "high"        # 高风险：只显示最基本的错误信息
    CRITICAL = "critical" # 严重：完全隐藏错误详情


class ErrorDetailLevel(Enum):
    """错误详细程度级别"""
    MINIMAL = "minimal"      # 最小信息：只显示通用错误
    BASIC = "basic"          # 基本信息：显示错误类型
    DETAILED = "detailed"    # 详细信息：显示清理后的错误消息
    FULL = "full"            # 完整信息：显示原始错误（仅调试模式）


@dataclass
class SecurityConfig:
    """安全配置类"""
    # 基本配置
    debug_mode: bool = False
    error_detail_level: ErrorDetailLevel = ErrorDetailLevel.BASIC
    
    # 日志配置
    log_full_traceback: bool = True
    log_request_info: bool = True
    log_sensitive_data: bool = False
    
    # 客户端响应配置
    include_error_id: bool = True
    include_error_type: bool = False
    include_timestamp: bool = False
    
    # 敏感信息过滤配置
    filter_file_paths: bool = True
    filter_credentials: bool = True
    filter_ip_addresses: bool = True
    filter_user_paths: bool = True
    filter_env_vars: bool = True
    
    # 自定义配置
    custom_sensitive_patterns: List[str] = field(default_factory=list)
    custom_generic_messages: Dict[str, str] = field(default_factory=dict)
    
    # 错误级别映射配置
    error_level_overrides: Dict[str, ErrorLevel] = field(default_factory=dict)
    
    @classmethod
    def from_env(cls) -> 'SecurityConfig':
        """从环境变量创建配置"""
        return cls(
            debug_mode=os.getenv('RAT_DEBUG_MODE', 'false').lower() == 'true',
            error_detail_level=ErrorDetailLevel(os.getenv('RAT_ERROR_DETAIL_LEVEL', 'basic')),
            log_full_traceback=os.getenv('RAT_LOG_FULL_TRACEBACK', 'true').lower() == 'true',
            log_request_info=os.getenv('RAT_LOG_REQUEST_INFO', 'true').lower() == 'true',
            log_sensitive_data=os.getenv('RAT_LOG_SENSITIVE_DATA', 'false').lower() == 'true',
            include_error_id=os.getenv('RAT_INCLUDE_ERROR_ID', 'true').lower() == 'true',
            include_error_type=os.getenv('RAT_INCLUDE_ERROR_TYPE', 'false').lower() == 'true',
            include_timestamp=os.getenv('RAT_INCLUDE_TIMESTAMP', 'false').lower() == 'true',
            filter_file_paths=os.getenv('RAT_FILTER_FILE_PATHS', 'true').lower() == 'true',
            filter_credentials=os.getenv('RAT_FILTER_CREDENTIALS', 'true').lower() == 'true',
            filter_ip_addresses=os.getenv('RAT_FILTER_IP_ADDRESSES', 'true').lower() == 'true',
            filter_user_paths=os.getenv('RAT_FILTER_USER_PATHS', 'true').lower() == 'true',
            filter_env_vars=os.getenv('RAT_FILTER_ENV_VARS', 'true').lower() == 'true',
        )
    
    @classmethod
    def production(cls) -> 'SecurityConfig':
        """生产环境推荐配置"""
        return cls(
            debug_mode=False,
            error_detail_level=ErrorDetailLevel.MINIMAL,
            log_full_traceback=True,
            log_request_info=True,
            log_sensitive_data=False,
            include_error_id=True,
            include_error_type=False,
            include_timestamp=False,
        )
    
    @classmethod
    def development(cls) -> 'SecurityConfig':
        """开发环境推荐配置"""
        return cls(
            debug_mode=True,
            error_detail_level=ErrorDetailLevel.DETAILED,
            log_full_traceback=True,
            log_request_info=True,
            log_sensitive_data=False,
            include_error_id=True,
            include_error_type=True,
            include_timestamp=True,
        )
    
    @classmethod
    def testing(cls) -> 'SecurityConfig':
        """测试环境推荐配置"""
        return cls(
            debug_mode=True,
            error_detail_level=ErrorDetailLevel.FULL,
            log_full_traceback=True,
            log_request_info=True,
            log_sensitive_data=True,
            include_error_id=True,
            include_error_type=True,
            include_timestamp=True,
            filter_file_paths=False,
            filter_credentials=False,
            filter_ip_addresses=False,
            filter_user_paths=False,
            filter_env_vars=False,
        )


class SecureErrorHandler:
    """安全错误处理器"""
    
    def __init__(self, config: Optional[SecurityConfig] = None):
        self.config = config or SecurityConfig()
        self.sensitive_patterns = self._init_sensitive_patterns()
        self.generic_messages = self._init_generic_messages()
        
        # 向后兼容性
        self.debug_mode = self.config.debug_mode
        
    def _init_sensitive_patterns(self) -> List[re.Pattern]:
        """初始化敏感信息匹配模式"""
        patterns = []
        
        # 根据配置添加相应的过滤模式
        if self.config.filter_file_paths:
            patterns.extend([
                # 文件路径
                re.compile(r'/[\w\-_./]+\.(py|rs|toml|yaml|yml|json|env)', re.IGNORECASE),
                # 内部模块路径
                re.compile(r'\b[\w_]+\.[\w_.]+\s+at\s+0x[0-9a-fA-F]+', re.IGNORECASE),
            ])
        
        if self.config.filter_credentials:
            patterns.extend([
                # 密码和密钥
                re.compile(r'(password|passwd|pwd|secret|key|token)\s*[=:]\s*[\w\-_]+', re.IGNORECASE),
                # 数据库连接字符串
                re.compile(r'(mongodb|mysql|postgres|redis)://[\w\-_.:@/]+', re.IGNORECASE),
            ])
        
        if self.config.filter_ip_addresses:
            patterns.append(
                # IP地址和端口
                re.compile(r'\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}:[0-9]+\b')
            )
        
        if self.config.filter_user_paths:
            patterns.extend([
                # 系统用户路径
                re.compile(r'/Users/[\w\-_]+/', re.IGNORECASE),
                re.compile(r'/home/[\w\-_]+/', re.IGNORECASE),
            ])
        
        if self.config.filter_env_vars:
            patterns.append(
                # 环境变量
                re.compile(r'\$\{?[A-Z_][A-Z0-9_]*\}?', re.IGNORECASE)
            )
        
        # 添加自定义模式
        for pattern_str in self.config.custom_sensitive_patterns:
            try:
                patterns.append(re.compile(pattern_str, re.IGNORECASE))
            except re.error:
                # 忽略无效的正则表达式
                pass
        
        return patterns
    
    def _init_generic_messages(self) -> Dict[str, str]:
        """初始化通用错误消息"""
        messages = {
            'ImportError': 'Required module is not available',
            'FileNotFoundError': 'Requested resource not found',
            'PermissionError': 'Access denied',
            'ConnectionError': 'Service temporarily unavailable',
            'TimeoutError': 'Request timeout',
            'ValueError': 'Invalid input provided',
            'TypeError': 'Invalid data type',
            'KeyError': 'Required parameter missing',
            'AttributeError': 'Invalid operation',
            'RuntimeError': 'Internal processing error',
            'MemoryError': 'Insufficient resources',
            'OSError': 'System operation failed',
            'default': 'An unexpected error occurred'
        }
        
        # 合并自定义消息
        messages.update(self.config.custom_generic_messages)
        return messages
    
    def classify_error_level(self, error: Exception) -> ErrorLevel:
        """根据异常类型分类错误级别"""
        error_type = type(error).__name__
        error_message = str(error).lower()
        
        # 检查配置中的错误级别覆盖
        if error_type in self.config.error_level_overrides:
            return self.config.error_level_overrides[error_type]
        
        # 严重级别：涉及安全和系统的错误
        if any(keyword in error_message for keyword in [
            'permission', 'access', 'unauthorized', 'forbidden',
            'authentication', 'credential', 'token', 'secret'
        ]):
            return ErrorLevel.CRITICAL
        
        # 高风险：可能泄露系统信息
        if error_type in ['OSError', 'PermissionError', 'ConnectionError']:
            return ErrorLevel.HIGH
        
        # 中风险：业务逻辑错误
        if error_type in ['ValueError', 'TypeError', 'KeyError', 'AttributeError']:
            return ErrorLevel.MEDIUM
        
        # 低风险：一般性错误
        if error_type in ['FileNotFoundError', 'ImportError']:
            return ErrorLevel.LOW
        
        # 默认为中风险
        return ErrorLevel.MEDIUM
    
    def sanitize_error_message(self, message: str) -> str:
        """清理错误消息中的敏感信息"""
        sanitized = message
        
        # 移除敏感信息
        for pattern in self.sensitive_patterns:
            sanitized = pattern.sub('[REDACTED]', sanitized)
        
        # 移除过长的堆栈跟踪信息
        lines = sanitized.split('\n')
        if len(lines) > 3:
            sanitized = '\n'.join(lines[:2]) + '\n[Additional details hidden]'
        
        return sanitized
    
    def get_client_error_message(self, error: Exception, error_level: ErrorLevel) -> str:
        """获取发送给客户端的错误消息"""
        error_type = type(error).__name__
        error_message = str(error)
        
        # 根据配置的错误详细程度级别决定返回的信息
        detail_level = self.config.error_detail_level
        
        if detail_level == ErrorDetailLevel.FULL and self.config.debug_mode:
            # 完整信息：显示原始错误（仅调试模式）
            return error_message
        elif detail_level == ErrorDetailLevel.DETAILED:
            # 详细信息：显示清理后的错误消息
            if error_level in [ErrorLevel.LOW, ErrorLevel.MEDIUM]:
                return self.sanitize_error_message(error_message)
            else:
                # 高风险错误仍然使用通用消息
                return self._get_generic_message(error_type, error_level)
        elif detail_level == ErrorDetailLevel.BASIC:
            # 基本信息：显示错误类型和通用消息
            if error_level in [ErrorLevel.LOW, ErrorLevel.MEDIUM]:
                generic_msg = self.generic_messages.get(error_type, self.generic_messages['default'])
                if self.config.include_error_type:
                    return f"{error_type}: {generic_msg}"
                return generic_msg
            else:
                return self._get_generic_message(error_type, error_level)
        else:  # MINIMAL
            # 最小信息：只显示通用错误
            return self._get_generic_message(error_type, error_level)
    
    def _get_generic_message(self, error_type: str, error_level: ErrorLevel) -> str:
        """获取通用错误消息"""
        # 根据错误级别返回通用消息
        if error_level == ErrorLevel.CRITICAL:
            return "Access denied"
        elif error_level == ErrorLevel.HIGH:
            return "Service temporarily unavailable"
        else:
            return self.generic_messages.get(error_type, self.generic_messages['default'])
    
    def log_detailed_error(self, error: Exception, request_info: Optional[Dict[str, Any]] = None) -> str:
        """记录详细错误信息到日志系统"""
        try:
            from rat_engine import log_error
        except ImportError:
            # Fallback logging
            def log_error(msg):
                print(f"ERROR: {msg}")
        
        error_id = f"ERR_{id(error):x}"
        error_type = type(error).__name__
        error_message = str(error)
        
        # 记录基本错误信息
        log_error(f"[{error_id}] Exception Type: {error_type}")
        
        # 根据配置决定是否记录敏感数据
        if self.config.log_sensitive_data:
            log_error(f"[{error_id}] Exception Message: {error_message}")
        else:
            sanitized_message = self.sanitize_error_message(error_message)
            log_error(f"[{error_id}] Exception Message: {sanitized_message}")
        
        # 记录请求信息（如果配置允许且提供了信息）
        if self.config.log_request_info and request_info:
            log_error(f"[{error_id}] Request Method: {request_info.get('method', 'Unknown')}")
            log_error(f"[{error_id}] Request Path: {request_info.get('path', 'Unknown')}")
            log_error(f"[{error_id}] Client IP: {request_info.get('client_ip', 'Unknown')}")
            log_error(f"[{error_id}] User Agent: {request_info.get('user_agent', 'Unknown')}")
            
            # 记录额外的请求信息
            for key, value in request_info.items():
                if key not in ['method', 'path', 'client_ip', 'user_agent']:
                    if self.config.log_sensitive_data:
                        log_error(f"[{error_id}] {key}: {value}")
                    else:
                        sanitized_value = self.sanitize_error_message(str(value))
                        log_error(f"[{error_id}] {key}: {sanitized_value}")
        
        # 记录完整堆栈跟踪（如果配置允许）
        if self.config.log_full_traceback:
            log_error(f"[{error_id}] Full Traceback:")
            traceback_lines = traceback.format_exc().strip().split('\n')
            
            if self.config.log_sensitive_data:
                for line in traceback_lines:
                    log_error(f"[{error_id}]   {line}")
            else:
                for line in traceback_lines:
                    sanitized_line = self.sanitize_error_message(line)
                    log_error(f"[{error_id}]   {sanitized_line}")
        
        # 添加时间戳（如果配置要求）
        if self.config.include_timestamp:
            import datetime
            timestamp = datetime.datetime.now().isoformat()
            log_error(f"[{error_id}] Timestamp: {timestamp}")
        
        return error_id
    
    def handle_error(self, error: Exception, request_info: Optional[Dict[str, Any]] = None) -> Tuple[str, str, ErrorLevel]:
        """处理错误并返回客户端消息和错误ID
        
        Returns:
            Tuple[client_message, error_id, error_level]
        """
        # 分类错误级别
        error_level = self.classify_error_level(error)
        
        # 记录详细错误信息
        error_id = self.log_detailed_error(error, request_info)
        
        # 获取客户端错误消息
        client_message = self.get_client_error_message(error, error_level)
        
        return client_message, error_id, error_level


# 全局安全错误处理器实例
_secure_handler = None
_default_config = None


def get_secure_handler(config: Optional[SecurityConfig] = None, debug_mode: Optional[bool] = None) -> SecureErrorHandler:
    """获取全局安全错误处理器实例"""
    global _secure_handler, _default_config
    
    # 向后兼容性：如果只提供了 debug_mode，创建简单配置
    if config is None and debug_mode is not None:
        config = SecurityConfig(debug_mode=debug_mode)
    
    # 如果没有提供配置，使用默认配置
    if config is None:
        if _default_config is None:
            _default_config = SecurityConfig.from_env()
        config = _default_config
    
    # 如果处理器不存在或配置发生变化，重新创建
    if _secure_handler is None or _secure_handler.config != config:
        _secure_handler = SecureErrorHandler(config)
    
    return _secure_handler


def set_default_config(config: SecurityConfig):
    """设置默认安全配置"""
    global _default_config, _secure_handler
    _default_config = config
    _secure_handler = None  # 重置处理器以使用新配置


def handle_secure_error(
    error: Exception, 
    request_info: Optional[Dict[str, Any]] = None, 
    config: Optional[SecurityConfig] = None,
    debug_mode: Optional[bool] = None
) -> Tuple[str, str]:
    """便捷函数：安全处理错误
    
    Args:
        error: 要处理的异常
        request_info: 请求信息（可选）
        config: 安全配置（可选）
        debug_mode: 调试模式（向后兼容，可选）
    
    Returns:
        Tuple[client_message, error_id]
    """
    handler = get_secure_handler(config, debug_mode)
    client_message, error_id, _ = handler.handle_error(error, request_info)
    return client_message, error_id


# 便捷函数：预设配置
def handle_secure_error_production(error: Exception, request_info: Optional[Dict[str, Any]] = None) -> Tuple[str, str]:
    """生产环境错误处理"""
    return handle_secure_error(error, request_info, SecurityConfig.production())


def handle_secure_error_development(error: Exception, request_info: Optional[Dict[str, Any]] = None) -> Tuple[str, str]:
    """开发环境错误处理"""
    return handle_secure_error(error, request_info, SecurityConfig.development())


def handle_secure_error_testing(error: Exception, request_info: Optional[Dict[str, Any]] = None) -> Tuple[str, str]:
    """测试环境错误处理"""
    return handle_secure_error(error, request_info, SecurityConfig.testing())