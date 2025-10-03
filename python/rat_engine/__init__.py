"""RAT Engine Python bindings"""

# -*- coding: utf-8 -*-
"""
RAT Engine Python 绑定

这个模块提供了 RAT Engine Rust 实现的 Python 接口。
不再提供 fallback 实现，确保错误来源清晰可追踪。
"""

# 直接导入 Rust 实现，不提供 fallback
try:
    from . import _rat_engine

    # 导入rat_logger函数
    from ._rat_engine import (
        rat_debug, rat_info, rat_warn, rat_error,
        rat_trace, rat_emergency, rat_startup_log, rat_flush_logs
    )
except ImportError as e:
    raise ImportError(
        f"无法导入 Rust 模块 '_rat_engine': {e}\n"
        "请确保：\n"
        "1. 已正确编译 Rust 扩展模块\n"
        "2. 运行 'maturin develop' 或 'pip install -e .'\n"
        "3. Python 环境与编译环境一致\n"
        "\n"
        "如需重新编译，请运行：\n"
        "cd /Users/0ldm0s/workspaces/rust/rat/rat_engine/python\n"
        "maturin develop --release"
    ) from e

# 导入核心组件
from ._rat_engine import (
    # 核心组件
    Router as PyRouter, Server as PyServer, ServerConfig,
    
    # 证书管理组件
    CertManagerConfig,
    
    # 客户端组件
    PyClientManager,
    
    # HTTP 组件
    HttpRequest, HttpResponse, HttpMethod,
    
    # 流式传输组件
    SseResponse as PySseResponse, SseSender as PySseSender, ChunkedResponse as PyChunkedResponse,
    
    # 编解码组件
    QuickCodec as PyQuickCodec, QuickEncoder as PyQuickEncoder, QuickDecoder as PyQuickDecoder,
    
    # 压缩组件
    CompressionConfig, CompressionType,
    
    # 处理器组件
    Handler as PyHandler, DataPipeline as PyDataPipeline,
    
    # 处理器函数
    create_counter_handler, create_json_handler, create_log_handler,
    create_text_handler, create_upload_handler, create_validator,
    json_stream, text_stream,

    # 版本信息函数
    get_rat_memcache_version,

    )

# 流式传输功能已通过类直接导出，无需子模块
streaming = None  # 保持兼容性，实际功能通过 PySseResponse 等类提供

# 版本信息
__version__ = getattr(_rat_engine, "__version__", "unknown")

# 实现信息（用于调试）
ENGINE_IMPLEMENTATION = "rust"
ENGINE_VERSION = __version__

print(f"[RAT_ENGINE] 使用 Rust 实现 v{__version__}")

# Web 应用的高级接口
from .web_app import (
    RatApp, 
    request, 
    path_params,
    Middleware,
    CORSMiddleware, 
    LoggingMiddleware,
    create_app_from_file,
    run_cli
)

# 安全错误处理组件
from .security import (
    SecureErrorHandler, 
    handle_secure_error, 
    handle_secure_error_production,
    handle_secure_error_development,
    handle_secure_error_testing,
    ErrorLevel,
    ErrorDetailLevel,
    SecurityConfig,
    set_default_config,
    get_secure_handler
)

# 流式连接错误处理组件
from .error_handling import (
    ConnectionManager,
    ErrorRecovery,
    StreamMonitor,
    GracefulShutdown,
    ConnectionState,
    ConnectionInfo,
    register_streaming_connection,
    unregister_streaming_connection,
    get_streaming_stats,
    shutdown_streaming_system,
    get_default_connection_manager,
    get_default_stream_monitor,
    get_default_error_recovery
)

# 🔥 QuickMem 高性能编解码功能
try:
    # 编解码类已直接导出，尝试导入便捷接口
    from .quickmem import (
        quickmem_encode, quickmem_decode, quickmem_route,
        QuickMemManager, default_manager as quickmem_manager,
        get_global_codec
    )
    quickmem = None  # 保持兼容性，实际功能通过 PyQuickCodec 等类提供

    # 从 Rust 端获取 rat_memcache 版本信息
    try:
        rat_memcache_version = get_rat_memcache_version()
        print(f"[RAT_ENGINE] QuickMem 集成成功 v{rat_memcache_version}")
    except Exception:
        print(f"[RAT_ENGINE] QuickMem 集成成功")

except ImportError as e:
    # 导入失败的回退处理
    quickmem = None
    quickmem_encode = None
    quickmem_decode = None
    quickmem_route = None
    QuickMemManager = None
    quickmem_manager = None
    get_global_codec = None

    import warnings
    warnings.warn(
        f"QuickMem 导入失败: {e}。高性能编解码功能不可用。",
        UserWarning
    )

__all__ = [
    # 核心组件
    'PyRouter', 'PyServer', 'ServerConfig',
    
    # 证书管理组件
    'CertManagerConfig',
    
    # 客户端组件
    'PyClientManager',
    
    # HTTP 组件
    'HttpRequest', 'HttpResponse', 'HttpMethod',
    
    # 流式传输组件
    'PySseResponse', 'PySseSender', 'PyChunkedResponse',
    
    # 编解码组件
    'PyQuickCodec', 'PyQuickEncoder', 'PyQuickDecoder',
    
    # 压缩组件
    'CompressionConfig', 'CompressionType',
    
    # 处理器组件
    'PyHandler', 'PyDataPipeline',
    
    # 处理器函数
    'create_counter_handler', 'create_json_handler', 'create_log_handler',
    'create_text_handler', 'create_upload_handler', 'create_validator',
    'json_stream', 'text_stream',
    
    # Web 应用接口
    'RatApp', 'request', 'path_params',
    
    # 中间件
    'Middleware', 'CORSMiddleware', 'LoggingMiddleware',
    
    # CLI 工具
    'create_app_from_file', 'run_cli',
    
    # 流式传输
    'streaming',
    
    # 安全错误处理
    'SecureErrorHandler', 'handle_secure_error', 'handle_secure_error_production',
    'handle_secure_error_development', 'handle_secure_error_testing',
    'ErrorLevel', 'ErrorDetailLevel', 'SecurityConfig',
    'set_default_config', 'get_secure_handler',
    
    # 流式连接错误处理
    'ConnectionManager', 'ErrorRecovery', 'StreamMonitor', 'GracefulShutdown',
    'ConnectionState', 'ConnectionInfo', 'register_streaming_connection',
    'unregister_streaming_connection', 'get_streaming_stats', 'shutdown_streaming_system',
    'get_default_connection_manager', 'get_default_stream_monitor', 'get_default_error_recovery',
    
    # 🔥 QuickMem 高性能编解码
    'quickmem', 'quickmem_encode', 'quickmem_decode', 'quickmem_route',
    'QuickMemManager', 'quickmem_manager', 'get_global_codec',

    # 版本信息函数
    'get_rat_memcache_version',

    # RAT Logger 函数
    'rat_debug', 'rat_info', 'rat_warn', 'rat_error',
    'rat_trace', 'rat_emergency', 'rat_startup_log', 'rat_flush_logs'
]