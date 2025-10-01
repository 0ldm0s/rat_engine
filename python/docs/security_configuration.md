# RAT Engine 安全错误处理配置指南

## 概述

RAT Engine 提供了强大的配置化安全错误处理系统，允许您根据不同的环境和需求精确控制错误信息的详细程度、敏感信息过滤和日志记录行为。

## 核心组件

### 1. ErrorDetailLevel 枚举

控制返回给客户端的错误信息详细程度：

```python
from rat_engine import ErrorDetailLevel

# 最小信息 - 仅返回通用错误消息
ErrorDetailLevel.MINIMAL

# 基本信息 - 包含错误类型但过滤敏感信息
ErrorDetailLevel.BASIC

# 详细信息 - 包含过滤后的错误消息
ErrorDetailLevel.DETAILED

# 完整信息 - 包含完整错误信息（仅调试模式）
ErrorDetailLevel.FULL
```

### 2. SecurityConfig 配置类

主要的配置类，控制所有安全错误处理行为：

```python
from rat_engine import SecurityConfig, ErrorDetailLevel, ErrorLevel

# 创建自定义配置
config = SecurityConfig(
    # 基本设置
    debug_mode=False,
    error_detail_level=ErrorDetailLevel.BASIC,
    
    # 客户端响应控制
    include_error_id=True,
    include_error_type=True,
    include_timestamp=False,
    
    # 日志记录控制
    log_full_traceback=True,
    log_request_info=True,
    log_sensitive_data=False,
    
    # 敏感信息过滤
    filter_file_paths=True,
    filter_credentials=True,
    filter_ip_addresses=True,
    filter_user_paths=True,
    filter_env_vars=True,
    
    # 自定义配置
    custom_sensitive_patterns=[r'secret_\w+'],
    custom_generic_messages={
        'DatabaseError': 'Database operation failed'
    },
    error_level_overrides={
        'CustomError': ErrorLevel.HIGH
    }
)
```

## 使用方式

### 1. 预设环境配置

#### 生产环境
```python
from rat_engine import handle_secure_error_production

try:
    # 可能出错的代码
    raise ValueError("Sensitive error with /path/to/secret")
except Exception as e:
    client_message, error_id = handle_secure_error_production(e, {
        'method': 'POST',
        'path': '/api/data',
        'client_ip': '192.168.1.100'
    })
    # 返回安全的错误信息给客户端
    return {'error': client_message, 'error_id': error_id}, 500
```

#### 开发环境
```python
from rat_engine import handle_secure_error_development

try:
    # 可能出错的代码
    raise FileNotFoundError("/Users/dev/config.yaml not found")
except Exception as e:
    client_message, error_id = handle_secure_error_development(e, {
        'method': 'GET',
        'path': '/api/config'
    })
    # 开发环境会显示更多详细信息
    return {'error': client_message, 'error_id': error_id}, 500
```

#### 测试环境
```python
from rat_engine import handle_secure_error_testing

try:
    # 可能出错的代码
    raise PermissionError("Access denied to database password=test123")
except Exception as e:
    client_message, error_id = handle_secure_error_testing(e, {
        'method': 'POST',
        'path': '/api/test',
        'test_data': 'sensitive_test_info'
    })
    # 测试环境会记录详细信息但过滤敏感数据
    return {'error': client_message, 'error_id': error_id}, 500
```

### 2. 自定义配置

```python
from rat_engine import SecurityConfig, handle_secure_error, ErrorDetailLevel

# 创建自定义配置
custom_config = SecurityConfig(
    debug_mode=True,
    error_detail_level=ErrorDetailLevel.DETAILED,
    include_error_type=True,
    include_timestamp=True,
    filter_credentials=True,
    custom_generic_messages={
        'DatabaseError': 'Database connection issue occurred'
    }
)

try:
    # 可能出错的代码
    raise RuntimeError("Database error: mysql://user:pass@localhost/db")
except Exception as e:
    client_message, error_id = handle_secure_error(e, {
        'method': 'GET',
        'path': '/api/data'
    }, config=custom_config)
    return {'error': client_message, 'error_id': error_id}, 500
```

### 3. 环境变量配置

设置环境变量来控制配置：

```bash
# 基本设置
export RAT_DEBUG_MODE=false
export RAT_ERROR_DETAIL_LEVEL=basic

# 客户端响应控制
export RAT_INCLUDE_ERROR_ID=true
export RAT_INCLUDE_ERROR_TYPE=true
export RAT_INCLUDE_TIMESTAMP=false

# 日志记录控制
export RAT_LOG_FULL_TRACEBACK=true
export RAT_LOG_REQUEST_INFO=true
export RAT_LOG_SENSITIVE_DATA=false

# 敏感信息过滤
export RAT_FILTER_FILE_PATHS=true
export RAT_FILTER_CREDENTIALS=true
export RAT_FILTER_IP_ADDRESSES=true
export RAT_FILTER_USER_PATHS=true
export RAT_FILTER_ENV_VARS=true
```

然后在代码中使用：

```python
from rat_engine import SecurityConfig, handle_secure_error

# 从环境变量加载配置
config = SecurityConfig.from_env()

try:
    # 可能出错的代码
    raise ImportError("Module not found in /opt/secrets/")
except Exception as e:
    client_message, error_id = handle_secure_error(e, config=config)
    return {'error': client_message, 'error_id': error_id}, 500
```

### 4. 全局默认配置

```python
from rat_engine import SecurityConfig, set_default_config, handle_secure_error
import os

# 应用启动时设置全局配置
def setup_security_config():
    env = os.getenv('ENVIRONMENT', 'development')
    
    if env == 'production':
        config = SecurityConfig.production()
    elif env == 'testing':
        config = SecurityConfig.testing()
    else:
        config = SecurityConfig.development()
    
    set_default_config(config)

# 在应用启动时调用
setup_security_config()

# 之后的错误处理会自动使用全局配置
try:
    # 可能出错的代码
    raise KeyError("API key not found")
except Exception as e:
    # 自动使用全局默认配置
    client_message, error_id = handle_secure_error(e)
    return {'error': client_message, 'error_id': error_id}, 500
```

## 配置详解

### 错误详细程度对比

| 级别 | 客户端看到的信息 | 适用场景 |
|------|------------------|----------|
| MINIMAL | "An error occurred" | 生产环境，最高安全性 |
| BASIC | "ValueError occurred" | 生产环境，需要基本错误类型 |
| DETAILED | "Invalid input format" | 开发/测试环境 |
| FULL | "ValueError: Invalid email format for user@domain.com" | 调试模式 |

### 敏感信息过滤

系统会自动过滤以下类型的敏感信息：

- **文件路径**: `/Users/admin/secret.key` → `[FILTERED_PATH]`
- **凭据信息**: `password=secret123` → `password=[FILTERED]`
- **IP地址**: `192.168.1.100` → `[FILTERED_IP]`
- **用户路径**: `/home/user/` → `[FILTERED_USER_PATH]`
- **环境变量**: `API_KEY=abc123` → `API_KEY=[FILTERED]`

### 自定义过滤模式

```python
config = SecurityConfig(
    custom_sensitive_patterns=[
        r'token_\w+',           # 匹配 token_xxx
        r'secret_[a-zA-Z0-9]+', # 匹配 secret_xxx
        r'\b\d{4}-\d{4}-\d{4}-\d{4}\b'  # 匹配信用卡号格式
    ]
)
```

### 自定义错误消息

```python
config = SecurityConfig(
    custom_generic_messages={
        'DatabaseError': 'Database service temporarily unavailable',
        'AuthenticationError': 'Authentication failed',
        'ValidationError': 'Input validation failed'
    }
)
```

### 错误级别覆盖

```python
config = SecurityConfig(
    error_level_overrides={
        'CustomSecurityError': ErrorLevel.CRITICAL,
        'BusinessLogicError': ErrorLevel.MEDIUM,
        'ValidationError': ErrorLevel.LOW
    }
)
```

## 最佳实践

### 1. 环境分离

```python
# config.py
import os
from rat_engine import SecurityConfig

def get_security_config():
    env = os.getenv('ENVIRONMENT', 'development')
    
    if env == 'production':
        return SecurityConfig(
            debug_mode=False,
            error_detail_level=ErrorDetailLevel.MINIMAL,
            log_sensitive_data=False,
            filter_credentials=True,
            filter_file_paths=True
        )
    elif env == 'staging':
        return SecurityConfig(
            debug_mode=False,
            error_detail_level=ErrorDetailLevel.BASIC,
            log_sensitive_data=False,
            filter_credentials=True
        )
    else:  # development
        return SecurityConfig(
            debug_mode=True,
            error_detail_level=ErrorDetailLevel.DETAILED,
            log_sensitive_data=True,
            filter_credentials=False
        )
```

### 2. 统一错误处理装饰器

```python
from functools import wraps
from rat_engine import handle_secure_error

def secure_error_handler(config=None):
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            try:
                return func(*args, **kwargs)
            except Exception as e:
                client_message, error_id = handle_secure_error(e, config=config)
                return {'error': client_message, 'error_id': error_id}, 500
        return wrapper
    return decorator

# 使用装饰器
@secure_error_handler()
def api_endpoint():
    # API 逻辑
    pass
```

### 3. 中间件集成

```python
from rat_engine import RatApp, handle_secure_error

app = RatApp(__name__)

@app.errorhandler(Exception)
def handle_exception(e):
    client_message, error_id = handle_secure_error(e, {
        'method': request.method,
        'path': request.path,
        'client_ip': request.remote_addr
    })
    return {'error': client_message, 'error_id': error_id}, 500
```

## 监控和调试

### 错误ID追踪

每个错误都会生成唯一的错误ID，用于日志关联：

```python
client_message, error_id = handle_secure_error(e)
print(f"Error ID: {error_id}")  # 例如: ERR_20231201_143052_abc123
```

### 日志格式

详细的错误日志会包含：
- 时间戳
- 错误ID
- 错误类型和消息
- 请求信息
- 堆栈跟踪（如果启用）
- 敏感信息过滤状态

### 性能考虑

- 敏感信息过滤会增加少量处理时间
- 建议在生产环境中禁用详细日志记录
- 使用适当的错误详细程度级别

## 示例应用

查看 `examples/security_config_example.py` 获取完整的使用示例，包括：
- 不同环境配置的对比
- 自定义配置的使用
- 环境变量配置
- 全局配置设置
- 配置效果对比

运行示例：

```bash
cd rat_engine/python/examples
python security_config_example.py
```

然后访问 `http://127.0.0.1:3000` 查看不同配置的效果。