#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine gRPC 装饰器模块

提供 gRPC 服务端功能的装饰器，实现责权分离：
- Python 层专注业务逻辑处理
- Rust 层负责网络通信和编解码
- 支持一元、服务端流、客户端流、双向流四种 gRPC 模式

使用示例：
```python
from rat_engine import RatApp

app = RatApp()

@app.grpc_unary("/hello.HelloService/SayHello")
def say_hello(request_data, metadata, context):
    # request_data: bytes - 请求数据
    # metadata: dict - 请求元数据
    # context: dict - 请求上下文（method, peer_addr, headers）
    return b"Hello, World!"

@app.grpc_server_stream("/hello.HelloService/SayHelloStream")
def say_hello_stream(request_data, metadata, context):
    # 返回一个生成器或迭代器
    for i in range(5):
        yield f"Hello {i}".encode('utf-8')

@app.grpc_client_stream("/hello.HelloService/SayHelloClientStream")
def say_hello_client_stream(messages, context):
    # messages: list[bytes] - 客户端发送的所有消息
    # context: dict - 请求上下文
    count = len(messages)
    return f"Received {count} messages".encode('utf-8')

@app.grpc_bidirectional("/hello.HelloService/SayHelloBidirectional")
def say_hello_bidirectional(context):
    # context: dict - 请求上下文
    # 返回一个处理函数，用于处理每个输入消息
    def process_message(message_data):
        # message_data: bytes - 单个输入消息
        return f"Echo: {message_data.decode('utf-8')}".encode('utf-8')
    return process_message
```
"""

import functools
from typing import Callable, Any, Dict, List, Iterator, Generator, Union


class GrpcDecorators:
    """gRPC 装饰器类，提供各种 gRPC 服务装饰器"""
    
    def __init__(self, router, main_thread=None, app_instance=None):
        """
        初始化 gRPC 装饰器
        
        Args:
            router: RAT Engine 路由器实例
            main_thread: PyGrpcMainThread 实例
            app_instance: RatApp 实例，用于通知 gRPC 路由注册状态
        """
        self._router = router
        self.main_thread = main_thread
        self.app_instance = app_instance
    
    def _ensure_grpc_bridge_initialized(self):
        """确保 gRPC 队列桥接适配器已初始化"""
        if self.app_instance and not self.app_instance._grpc_bridge_initialized:
            try:
                # 通过 main_thread 实例调用 initialize_queue_bridge 方法
                if self.main_thread is not None:
                    self.main_thread.initialize_queue_bridge()
                    self.app_instance._grpc_bridge_initialized = True
                    print("✅ gRPC 装饰器：队列桥接适配器已自动初始化")
                else:
                    print("⚠️ gRPC 装饰器：main_thread 未初始化，无法初始化队列桥接适配器")
            except Exception as e:
                print(f"⚠️ gRPC 装饰器：队列桥接适配器初始化失败: {e}")
                print("   请确保在使用 gRPC 功能前正确配置队列桥接")
                # 不抛出异常，让用户手动处理
    
    def grpc_unary(self, method: str):
        """
        gRPC 一元服务装饰器
        
        Args:
            method: gRPC 方法路径，如 "/hello.HelloService/SayHello"
            
        Returns:
            装饰器函数
            
        装饰的函数签名：
            def handler(request_data: bytes, metadata: dict, context: dict) -> bytes
        """
        def decorator(func: Callable[[bytes, Dict[str, Any], Dict[str, Any]], bytes]):
            @functools.wraps(func)
            def wrapper(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
                try:
                    return func(request_data, metadata, context)
                except Exception as e:
                    # 记录错误并返回错误响应
                    print(f"❌ gRPC 一元服务处理错误 [{method}]: {e}")
                    return f"Internal Server Error: {str(e)}".encode('utf-8')
            
            # 确保队列桥接适配器已初始化
            self._ensure_grpc_bridge_initialized()
            
            # 注册到路由器
            if self.main_thread is None:
                raise ValueError("gRPC 一元服务需要 main_thread 参数，请确保 RatApp 正确初始化")
            self._router.add_grpc_unary(method, wrapper, self.main_thread)
            
            # 通知 RatApp 有 gRPC 路由注册
            if self.app_instance:
                self.app_instance._grpc_routes_registered = True
            
            print(f"📝 已注册 gRPC 一元服务: {method}")
            return wrapper
        return decorator
    
    def grpc_server_stream(self, method: str):
        """
        gRPC 服务端流装饰器
        
        Args:
            method: gRPC 方法路径，如 "/hello.HelloService/SayHelloStream"
            
        Returns:
            装饰器函数
            
        装饰的函数签名：
            def handler(request_data: bytes, metadata: dict, context: dict) -> Iterator[bytes]
        """
        def decorator(func: Callable[[bytes, Dict[str, Any], Dict[str, Any]], Iterator[bytes]]):
            @functools.wraps(func)
            def wrapper(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> Iterator[bytes]:
                try:
                    result = func(request_data, metadata, context)
                    # 确保返回值是可迭代的
                    if hasattr(result, '__iter__'):
                        return result
                    else:
                        return [result]  # 包装为列表
                except Exception as e:
                    # 记录错误并返回错误响应
                    print(f"❌ gRPC 服务端流处理错误 [{method}]: {e}")
                    return [f"Internal Server Error: {str(e)}".encode('utf-8')]
            
            # 确保队列桥接适配器已初始化
            self._ensure_grpc_bridge_initialized()
            
            # 注册到路由器
            if self.main_thread is None:
                raise ValueError("gRPC 服务端流需要 main_thread 参数，请确保 RatApp 正确初始化")
            self._router.add_grpc_server_stream(method, wrapper, self.main_thread)
            
            # 通知 RatApp 有 gRPC 路由注册
            if self.app_instance:
                self.app_instance._grpc_routes_registered = True
            
            print(f"📝 已注册 gRPC 服务端流: {method}")
            return wrapper
        return decorator
    
    def grpc_client_stream(self, method: str):
        """
        gRPC 客户端流装饰器
        
        Args:
            method: gRPC 方法路径，如 "/hello.HelloService/SayHelloClientStream"
            
        Returns:
            装饰器函数
            
        装饰的函数签名：
            def handler(messages: List[bytes], context: dict) -> bytes
        """
        def decorator(func: Callable[[List[bytes], Dict[str, Any]], bytes]):
            @functools.wraps(func)
            def wrapper(messages: List[bytes], context: Dict[str, Any]) -> bytes:
                try:
                    return func(messages, context)
                except Exception as e:
                    # 记录错误并返回错误响应
                    print(f"❌ gRPC 客户端流处理错误 [{method}]: {e}")
                    return f"Internal Server Error: {str(e)}".encode('utf-8')
            
            # 确保队列桥接适配器已初始化
            self._ensure_grpc_bridge_initialized()
            
            # 注册到路由器
            if self.main_thread is None:
                raise ValueError("gRPC 客户端流需要 main_thread 参数，请确保 RatApp 正确初始化")
            self._router.add_grpc_client_stream(method, wrapper, self.main_thread)
            
            # 通知 RatApp 有 gRPC 路由注册
            if self.app_instance:
                self.app_instance._grpc_routes_registered = True
            
            print(f"📝 已注册 gRPC 客户端流: {method}")
            return wrapper
        return decorator
    
    def grpc_bidirectional(self, method: str):
        """
        gRPC 双向流装饰器
        
        Args:
            method: gRPC 方法路径，如 "/hello.HelloService/SayHelloBidirectional"
            
        Returns:
            装饰器函数
            
        装饰的函数签名：
            def handler(context: dict, sender: GrpcBidirectionalSender, receiver: GrpcBidirectionalReceiver) -> None
        """
        def decorator(func):
            @functools.wraps(func)
            def wrapper(context, sender, receiver):
                try:
                    # 调用用户函数进行委托模式初始化
                    return func(context, sender, receiver)
                except Exception as e:
                    # 记录错误
                    print(f"❌ gRPC 双向流处理器初始化错误 [{method}]: {e}")
                    # 发送错误消息
                    try:
                        sender.send_error(f"Handler Error: {str(e)}")
                    except:
                        pass
            
            # 确保队列桥接适配器已初始化
            self._ensure_grpc_bridge_initialized()
            
            # 注册到路由器
            if self.main_thread is None:
                raise ValueError("gRPC 双向流需要 main_thread 参数，请确保 RatApp 正确初始化")
            self._router.add_grpc_bidirectional(method, wrapper, self.main_thread)
            
            # 通知 RatApp 有 gRPC 路由注册
            if self.app_instance:
                self.app_instance._grpc_routes_registered = True
            
            print(f"📝 已注册 gRPC 双向流: {method}")
            return wrapper
        return decorator


def add_grpc_decorators_to_app(app_instance):
    """
    为 RatApp 实例添加 gRPC 装饰器方法
    
    Args:
        app_instance: RatApp 实例
    """
    if not hasattr(app_instance, '_grpc_decorators'):
        main_thread = getattr(app_instance, 'main_thread', None)
        app_instance._grpc_decorators = GrpcDecorators(app_instance._router, main_thread, app_instance)
    
    # 添加装饰器方法到应用实例
    app_instance.grpc_unary = app_instance._grpc_decorators.grpc_unary
    app_instance.grpc_server_stream = app_instance._grpc_decorators.grpc_server_stream
    app_instance.grpc_client_stream = app_instance._grpc_decorators.grpc_client_stream
    app_instance.grpc_bidirectional = app_instance._grpc_decorators.grpc_bidirectional


# 便利函数，用于快速创建 gRPC 处理器
def create_grpc_unary_handler(func: Callable[[bytes, Dict[str, Any], Dict[str, Any]], bytes]):
    """
    创建 gRPC 一元处理器
    
    Args:
        func: 处理函数
        
    Returns:
        包装后的处理器
    """
    @functools.wraps(func)
    def wrapper(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
        try:
            return func(request_data, metadata, context)
        except Exception as e:
            print(f"❌ gRPC 一元处理器错误: {e}")
            return f"Error: {str(e)}".encode('utf-8')
    return wrapper


def create_grpc_server_stream_handler(func: Callable[[bytes, Dict[str, Any], Dict[str, Any]], Iterator[bytes]]):
    """
    创建 gRPC 服务端流处理器
    
    Args:
        func: 处理函数
        
    Returns:
        包装后的处理器
    """
    @functools.wraps(func)
    def wrapper(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> Iterator[bytes]:
        try:
            result = func(request_data, metadata, context)
            if hasattr(result, '__iter__'):
                return result
            else:
                return [result]
        except Exception as e:
            print(f"❌ gRPC 服务端流处理器错误: {e}")
            return [f"Error: {str(e)}".encode('utf-8')]
    return wrapper


def create_grpc_client_stream_handler(func: Callable[[List[bytes], Dict[str, Any]], bytes]):
    """
    创建 gRPC 客户端流处理器
    
    Args:
        func: 处理函数
        
    Returns:
        包装后的处理器
    """
    @functools.wraps(func)
    def wrapper(messages: List[bytes], context: Dict[str, Any]) -> bytes:
        try:
            return func(messages, context)
        except Exception as e:
            print(f"❌ gRPC 客户端流处理器错误: {e}")
            return f"Error: {str(e)}".encode('utf-8')
    return wrapper


def create_grpc_bidirectional_handler(func: Callable[[Dict[str, Any]], Callable[[bytes], bytes]]):
    """
    创建 gRPC 双向流处理器
    
    Args:
        func: 处理函数
        
    Returns:
        包装后的处理器
    """
    @functools.wraps(func)
    def wrapper(context: Dict[str, Any]) -> Callable[[bytes], bytes]:
        try:
            message_handler = func(context)
            
            def safe_message_handler(message_data: bytes) -> bytes:
                try:
                    return message_handler(message_data)
                except Exception as e:
                    print(f"❌ gRPC 双向流消息处理错误: {e}")
                    return f"Message Error: {str(e)}".encode('utf-8')
            
            return safe_message_handler
        except Exception as e:
            print(f"❌ gRPC 双向流初始化错误: {e}")
            def error_handler(message_data: bytes) -> bytes:
                return f"Handler Error: {str(e)}".encode('utf-8')
            return error_handler
    return wrapper