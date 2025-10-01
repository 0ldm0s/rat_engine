#!/usr/bin/env python3
"""
gRPC 双向流客户端示例 - 强制 HTTP/2 版本

完整的自动验证双向流通信示例，基于 RatApp 架构，强制使用 HTTP/2 协议。
包含自动服务器启动、客户端连接、消息发送接收验证和资源清理。

功能特性：
- 完全自动化的测试流程
- 真实的 gRPC 服务器/客户端通信
- 强制 HTTP/2 协议支持（不使用 H2C）
- 消息计数和内容验证
- 完整的错误处理和资源清理
- 基于 RatApp 的正确架构
- 开发模式支持
"""

import asyncio
import json
import time
import threading
import signal
import sys
import os
import uuid
from datetime import datetime
from typing import Optional, Dict, Any, List
from dataclasses import dataclass
from rat_engine import RatApp, PyClientManager

@dataclass
class ChatMessage:
    """聊天消息数据结构"""
    user: str
    message: str
    timestamp: float
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            "user": self.user,
            "message": self.message,
            "timestamp": self.timestamp
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'ChatMessage':
        return cls(
            user=data.get("user", ""),
            message=data.get("message", ""),
            timestamp=data.get("timestamp", time.time())
        )

# 全局状态管理 - 类似SSE示例
active_bidirectional_sessions: Dict[str, Dict] = {}
bidirectional_connections: Dict[str, list] = {}
bidirectional_messages: Dict[str, list] = {}  # 存储待发送的消息

class ServerBidirectionalHandler:
    """服务器端双向流委托处理器 - 责权分离的委托模式"""
    
    def __init__(self, session_id: str):
        self.session_id = session_id
        self.message_count = 0
        self.active = True
        self.start_time = time.time()
        
        # 注册会话
        active_bidirectional_sessions[session_id] = {
            'active': True,
            'start_time': self.start_time,
            'message_count': 0,
            'handler': self
        }
        
        print(f"🎯 [委托处理器] 创建会话处理器: {session_id}")
    
    def on_connected(self, sender, receiver):
        """连接建立时的委托处理"""
        print(f"🔗 [委托处理器] HTTP/2 连接建立，会话: {self.session_id}")
        
        # 设置委托回调
        receiver.set_message_callback(self.on_message_received)
        receiver.set_error_callback(self.on_error)
        receiver.set_end_callback(self.on_end)
        
        # 启动接收循环
        receiver.start_receiving()
        
        # 保存发送器引用以便后续使用
        self.sender = sender
        self.receiver = receiver
        
        print(f"✅ [委托处理器] HTTP/2 处理器已启动，会话: {self.session_id}")
    
    def on_message_received(self, data):
        """委托的消息处理逻辑"""
        try:
            # 解析接收到的数据
            message_text = data.decode('utf-8')
            self.message_count += 1
            
            print(f"📥 [委托处理器] HTTP/2 收到消息 #{self.message_count} (会话: {self.session_id}): {message_text}")
            
            # 更新会话统计
            if self.session_id in active_bidirectional_sessions:
                active_bidirectional_sessions[self.session_id]['message_count'] = self.message_count
            
            # 检查是否是结束信号
            if "quit" in message_text.lower():
                print(f"🔚 [委托处理器] 收到结束信号，关闭 HTTP/2 流 (会话: {self.session_id})")
                self.sender.end_stream()
                self.active = False
                return
            
            # 委托的业务逻辑：创建回声消息
            echo_message = f"HTTP/2 Echo from session {self.session_id}: {message_text}"
            print(f"📤 [委托处理器] 发送 HTTP/2 回声 (会话: {self.session_id}): {echo_message}")
            
            # 发送回声消息
            self.sender.send_bytes(echo_message.encode('utf-8'))
            
        except Exception as e:
            print(f"❌ [委托处理器] 处理 HTTP/2 消息时出错 (会话: {self.session_id}): {e}")
            self.sender.send_error(f"处理消息失败: {e}")
    
    def on_error(self, error):
        """委托的错误处理"""
        print(f"❌ [委托处理器] HTTP/2 流错误 (会话: {self.session_id}): {error}")
        self.active = False
    
    def on_end(self):
        """委托的流结束处理"""
        print(f"🏁 [委托处理器] HTTP/2 流已结束 (会话: {self.session_id})")
        self.active = False
        
        # 清理会话
        if self.session_id in active_bidirectional_sessions:
            del active_bidirectional_sessions[self.session_id]
    
    def get_stats(self):
        """获取处理器统计信息"""
        return {
            'session_id': self.session_id,
            'message_count': self.message_count,
            'active': self.active,
            'duration': time.time() - self.start_time,
            'protocol': 'HTTP/2'
        }

class AutoTestServerH2:
    """自动测试服务器 - 强制 HTTP/2 版本"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 50052):
        self.host = host
        self.port = port
        self.app = RatApp("grpc_auto_test_server_h2")
        
        # 注意：队列桥接适配器现在由 RatApp 自动初始化
        # 当检测到 gRPC 路由注册时，RatApp.run() 会自动调用 initialize_queue_bridge()
        
        self.running = False
        self.handler_registry = {}  # 委托处理器注册表
        self.setup_routes()
    
    def setup_routes(self):
        """设置路由 - 委托模式路由配置"""
        @self.app.grpc_bidirectional("/chat.ChatService/BidirectionalChat")
        def delegate_to_handler(context, sender, receiver):
            """委托给专门的处理器 - 责权分离的核心"""
            # 生成会话ID
            session_id = context.get('session_id', f"h2_session_{int(time.time() * 1000)}")
            
            print(f"🎯 [HTTP/2 服务器] 委托模式：创建处理器委托，会话: {session_id}")
            
            # 创建委托处理器
            handler = ServerBidirectionalHandler(session_id)
            
            # 注册处理器
            self.handler_registry[session_id] = handler
            
            # 异步委托连接处理 - 避免同步调用导致的死锁
            import threading
            def async_delegate():
                try:
                    print(f"🔄 [HTTP/2 服务器] 异步委托：开始处理器初始化，会话: {session_id}")
                    handler.on_connected(sender, receiver)
                    print(f"✅ [HTTP/2 服务器] 异步委托：处理器已接管会话: {session_id}")
                except Exception as e:
                    print(f"❌ [HTTP/2 服务器] 异步委托失败 (会话: {session_id}): {e}")
                    handler.active = False
            
            # 在新线程中执行委托，避免阻塞主线程
            delegate_thread = threading.Thread(target=async_delegate, daemon=True)
            delegate_thread.start()
            
            print(f"🚀 [HTTP/2 服务器] 委托线程已启动，会话: {session_id}")
    
    def get_active_handlers(self):
        """获取活跃的委托处理器列表"""
        active_handlers = []
        for session_id, handler in self.handler_registry.items():
            if handler.active:
                active_handlers.append(handler.get_stats())
        return active_handlers
    
    def cleanup_inactive_handlers(self):
        """清理非活跃的委托处理器"""
        inactive_sessions = []
        for session_id, handler in self.handler_registry.items():
            if not handler.active:
                inactive_sessions.append(session_id)
        
        for session_id in inactive_sessions:
            del self.handler_registry[session_id]
            print(f"🧹 [HTTP/2 服务器] 清理非活跃处理器: {session_id}")
        
        return len(inactive_sessions)
    
    def start(self):
        """启动服务器 - H2 开发模式"""
        print(f"🚀 HTTP/2 gRPC 服务器启动在 {self.host}:{self.port}")
        self.running = True
        try:
            # 启用 H2 开发模式（自动生成自签名证书并启用 HTTPS + H2）
            print("🔧 启用 H2 开发模式（自动生成自签名证书）...")
            
            # 启用开发模式，这会自动配置证书管理器
            self.app.enable_development_mode(["localhost", "127.0.0.1"])
            print("✅ 自签名证书已配置")
            
            # 配置协议支持
            self.app.configure_protocols(enable_h2c=True, enable_h2=True)
            print("✅ H2 和 H2C 协议已启用")
                
            print("🔒 自签名证书已自动生成，支持 HTTPS 和 H2 协议")
            
            self.app.run(host=self.host, port=self.port, blocking=True)
        except Exception as e:
            print(f"❌ [HTTP/2 服务器] 启动失败: {e}")
        finally:
            self.running = False

class ClientBidirectionalHandlerH2:
    """客户端双向流委托处理器 - HTTP/2 版本"""
    
    def __init__(self, user_name: str):
        self.user_name = user_name
        self.message_count = 0
        self.received_count = 0
        self.connected = False
        self.error_count = 0
        self.start_time = time.time()
        
        print(f"🎯 [HTTP/2 客户端委托处理器] 创建处理器: {user_name}")
        
    def on_connected(self, stream_id: str):
        """连接建立时的委托处理"""
        self.connected = True
        print(f"🔗 [HTTP/2 客户端委托处理器] HTTP/2 连接建立，流ID: {stream_id}")
        return True
        
    def on_message_received(self, data: bytes, stream_id: str):
        """委托的消息接收处理逻辑"""
        try:
            message_text = data.decode('utf-8')
            self.received_count += 1
            
            # 尝试解析 JSON 格式的消息
            try:
                message_dict = json.loads(message_text)
                message = ChatMessage.from_dict(message_dict)
                print(f"📥 [HTTP/2 客户端委托处理器] 收到JSON消息 #{self.received_count} (流ID: {stream_id}): {message.user} - {message.message}")
                self._process_received_message(message, stream_id)
            except json.JSONDecodeError:
                # 如果不是 JSON 格式，则作为纯文本处理（例如服务器的回声消息）
                print(f"📥 [HTTP/2 客户端委托处理器] 收到文本消息 #{self.received_count} (流ID: {stream_id}): {message_text}")
                self._process_text_message(message_text, stream_id)
            
            return True
        except Exception as e:
            self.error_count += 1
            print(f"❌ [HTTP/2 客户端委托处理器] 解析消息失败: {e}")
            return False
    
    def _process_received_message(self, message: ChatMessage, stream_id: str):
        """委托的消息处理业务逻辑"""
        # 这里可以实现具体的业务逻辑
        # 例如：解析消息、触发事件、更新状态等
        if "HTTP/2 Echo from session" in message.message:
            print(f"✨ [HTTP/2 客户端委托处理器] 确认收到服务器 HTTP/2 回声 (流ID: {stream_id})")
    
    def _process_text_message(self, message_text: str, stream_id: str):
        """委托的纯文本消息处理业务逻辑"""
        # 处理服务器发送的纯文本回声消息
        if "HTTP/2 Echo from session" in message_text:
            print(f"✨ [HTTP/2 客户端委托处理器] 确认收到服务器 HTTP/2 文本回声 (流ID: {stream_id})")
        else:
            print(f"📝 [HTTP/2 客户端委托处理器] 处理文本消息 (流ID: {stream_id}): {message_text[:100]}...")  # 截断长消息
    
    def on_disconnected(self, stream_id: str, reason: Optional[str] = None):
        """委托的连接断开处理"""
        self.connected = False
        if reason:
            print(f"🔌 [HTTP/2 客户端委托处理器] 与服务器 HTTP/2 连接断开 (流ID: {stream_id}): {reason}")
        else:
            print(f"🔌 [HTTP/2 客户端委托处理器] 与服务器 HTTP/2 连接断开 (流ID: {stream_id})")
    
    def on_error(self, stream_id: str, error: str):
        """委托的错误处理"""
        self.error_count += 1
        print(f"❌ [HTTP/2 客户端委托处理器] 处理器错误 (流ID: {stream_id}): {error}")
    
    def delegate_send_message(self, message: str, stream_id: str) -> ChatMessage:
        """委托的消息发送预处理逻辑"""
        self.message_count += 1
        
        # 委托的发送前处理
        processed_message = self._preprocess_message(message)
        
        chat_msg = ChatMessage(
            user=self.user_name,
            message=processed_message,
            timestamp=time.time()
        )
        
        print(f"📤 [HTTP/2 客户端委托处理器] 预处理消息 #{self.message_count} (流ID: {stream_id}): {processed_message}")
        
        return chat_msg
    
    def _preprocess_message(self, message: str) -> str:
        """委托的消息预处理逻辑"""
        # 这里可以实现消息的预处理逻辑
        # 例如：添加时间戳、格式化、加密等
        timestamp = int(time.time() * 1000)
        return f"[HTTP/2-{timestamp}] {message}"
    
    def get_stats(self):
        """获取委托处理器统计信息"""
        return {
            'user_name': self.user_name,
            'message_count': self.message_count,
            'received_count': self.received_count,
            'error_count': self.error_count,
            'connected': self.connected,
            'duration': time.time() - self.start_time,
            'handler_type': 'ClientBidirectionalHandlerH2',
            'protocol': 'HTTP/2'
        }

class BidirectionalStreamHandlerH2:
    """双向流处理器门面 - HTTP/2 版本"""
    
    def __init__(self, user_name: str):
        self.user_name = user_name
        # 创建委托处理器
        self.delegate_handler = ClientBidirectionalHandlerH2(user_name)
        
        print(f"📋 [HTTP/2 客户端门面] 双向流处理器创建: {user_name}")
        
    def on_connected(self, stream_id: str):
        """连接建立时的处理 - 委托给专门的处理器"""
        print(f"🔗 [HTTP/2 客户端门面] HTTP/2 连接建立，委托给处理器，流ID: {stream_id}")
        return self.delegate_handler.on_connected(stream_id)
        
    def on_message_received(self, data: bytes, stream_id: str):
        """消息接收回调 - 委托给专门的处理器"""
        return self.delegate_handler.on_message_received(data, stream_id)
    
    def on_disconnected(self, stream_id: str, reason: Optional[str] = None):
        """连接断开回调 - 委托给专门的处理器"""
        return self.delegate_handler.on_disconnected(stream_id, reason)
    
    def on_error(self, stream_id: str, error: str):
        """错误处理回调 - 委托给专门的处理器"""
        return self.delegate_handler.on_error(stream_id, error)
    
    def prepare_message(self, message: str, stream_id: str) -> ChatMessage:
        """准备发送消息 - 委托给专门的处理器"""
        return self.delegate_handler.delegate_send_message(message, stream_id)
    
    def get_stats(self):
        """获取统计信息 - 委托给专门的处理器"""
        return self.delegate_handler.get_stats()
    
    @property
    def message_count(self):
        """消息计数 - 委托给专门的处理器"""
        return self.delegate_handler.message_count
    
    @property
    def received_count(self):
        """接收计数 - 委托给专门的处理器"""
        return self.delegate_handler.received_count
    
    @property
    def error_count(self):
        """错误计数 - 委托给专门的处理器"""
        return self.delegate_handler.error_count

class AutoTestClientH2:
    """自动测试客户端 - HTTP/2 版本"""
    
    def __init__(self, user_name: str):
        self.user_name = user_name
        self.client = None
        # 使用委托模式的双向流处理器
        self.handler = BidirectionalStreamHandlerH2(user_name)
        self.stream_id = None
        self.running = False
        self.server_uri = None  # 存储服务器 URI
        print(f"🎯 [HTTP/2 客户端] 创建委托模式客户端: {user_name}")
        
    def connect(self, server_uri: str = "https://127.0.0.1:50052") -> bool:
        """连接到服务器 - 开发模式下的 HTTPS + HTTP/2"""
        try:
            # 创建客户端管理器
            self.client = PyClientManager()
            
            # 创建客户端配置字典（超时时间以毫秒为单位）
            # 开发模式下使用 HTTPS 连接自签名证书服务器
            config = {
                "connect_timeout": 10000,  # 10 秒 = 10000 毫秒
                "request_timeout": 30000,  # 30 秒 = 30000 毫秒
                "max_idle_connections": 10,
                "enable_grpc": True,  # 启用gRPC客户端
                "user_agent": "RatEngine-Python-Client-H2/1.0",
                "development_mode": True,  # 开发模式，自动跳过自签名证书验证
                "http2_only": True,  # 强制使用 HTTP/2
                "enable_compression": False,  # 禁用压缩
                "enable_http": True,  # 启用 HTTP 客户端
            }
            
            # 初始化客户端
            self.client.initialize(config)
            # 保存服务器 URI 供后续使用
            self.server_uri = server_uri
            print(f"✅ [{self.user_name}] HTTP/2 客户端连接成功（开发模式）")
            return True
            
        except Exception as e:
            print(f"❌ [{self.user_name}] HTTP/2 连接失败: {e}")
            return False
    
    def create_bidirectional_stream(self) -> bool:
        """创建双向流"""
        if not self.client:
            print(f"❌ [{self.user_name}] HTTP/2 客户端未初始化")
            return False
            
        if not self.server_uri:
            print(f"❌ [{self.user_name}] 服务器 URI 未设置")
            return False
        
        try:
            # 创建双向流，使用保存的服务器 URI
            print(f"🔗 [{self.user_name}] 正在连接到: {self.server_uri}")
            self.stream_id = self.client.grpc_bidirectional_stream(self.server_uri, "chat.ChatService", "BidirectionalChat")
            print(f"✅ [{self.user_name}] HTTP/2 双向流创建成功，流ID: {self.stream_id}")
            
            # 调用连接建立回调
            self.handler.on_connected(str(self.stream_id))
            
            # 启动消息接收循环
            self.running = True
            self._start_receive_loop()
            
            return True
            
        except Exception as e:
            print(f"❌ [{self.user_name}] 创建 HTTP/2 双向流失败: {e}")
            return False
    
    def _start_receive_loop(self):
        """启动消息接收循环 - 委托模式"""
        import threading
        
        def receive_messages():
            """消息接收线程 - 委托给处理器"""
            print(f"🎯 [{self.user_name}] 启动 HTTP/2 委托模式消息接收循环")
            while self.running and self.client and self.stream_id:
                try:
                    # 检查流是否已关闭
                    if self.client.grpc_bidirectional_is_closed(str(self.stream_id)):
                        print(f"🔌 [{self.user_name}] HTTP/2 流已关闭，停止接收")
                        self.handler.on_disconnected(str(self.stream_id), "HTTP/2 流已关闭")
                        break
                    
                    # 尝试接收消息
                    message = self.client.grpc_bidirectional_receive(str(self.stream_id))
                    if message is not None:
                        # 委托给处理器进行消息处理
                        self.handler.on_message_received(message, str(self.stream_id))
                    else:
                        # 没有消息，短暂休眠
                        time.sleep(0.1)
                        
                except Exception as e:
                    print(f"❌ [{self.user_name}] 接收 HTTP/2 消息失败: {e}")
                    self.handler.on_error(str(self.stream_id), str(e))
                    break
            
            print(f"🔚 [{self.user_name}] HTTP/2 委托模式消息接收循环结束")
        
        # 启动接收线程
        receive_thread = threading.Thread(target=receive_messages, daemon=True)
        receive_thread.start()
    
    def send_message(self, message: str) -> bool:
        """发送消息 - 委托模式"""
        if not self.client or not self.stream_id:
            print(f"❌ [{self.user_name}] HTTP/2 流未建立")
            return False
        
        try:
            # 委托给处理器进行消息预处理
            chat_msg = self.handler.prepare_message(message, str(self.stream_id))
            
            data = json.dumps(chat_msg.to_dict()).encode('utf-8')
            self.client.grpc_bidirectional_send(self.stream_id, data)
            print(f"📤 [{self.user_name}] HTTP/2 委托发送消息 #{self.handler.message_count}: {chat_msg.message}")
            return True
            
        except Exception as e:
            print(f"❌ [{self.user_name}] 发送 HTTP/2 消息失败: {e}")
            self.handler.on_error(str(self.stream_id), str(e))
            return False
    
    def close_stream(self):
        """关闭流"""
        if self.client and self.stream_id:
            try:
                self.running = False
                self.client.grpc_bidirectional_close(self.stream_id)
                print(f"✅ [{self.user_name}] HTTP/2 流已关闭")
                self.handler.on_disconnected(str(self.stream_id), "主动关闭")
            except Exception as e:
                print(f"❌ [{self.user_name}] 关闭 HTTP/2 流失败: {e}")
    
    def close(self):
        """关闭客户端"""
        self.close_stream()
        if self.client:
            try:
                self.client.close()
                print(f"✅ [{self.user_name}] HTTP/2 客户端已关闭")
            except Exception as e:
                print(f"❌ [{self.user_name}] 关闭 HTTP/2 客户端失败: {e}")

def start_test_server_h2():
    """启动测试服务器 - HTTP/2 版本"""
    server = AutoTestServerH2()
    server.start()

def run_delegated_mode_test_h2() -> bool:
    """运行委托模式测试 - HTTP/2 版本"""
    print("\n" + "="*80)
    print("🚀 开始 HTTP/2 委托模式双向流测试")
    print("="*80)
    
    # 启动服务器
    import threading
    server_thread = threading.Thread(target=start_test_server_h2, daemon=True)
    server_thread.start()
    
    # 等待服务器启动
    print("⏳ 等待 HTTP/2 服务器启动...")
    time.sleep(3)
    
    # 创建客户端
    client = AutoTestClientH2("TestUser_H2")
    
    try:
        # 连接到服务器
        print("\n🔗 连接到 HTTP/2 服务器...")
        if not client.connect("https://127.0.0.1:50052"):
            print("❌ 无法连接到 HTTP/2 服务器")
            return False
        
        # 创建双向流
        print("\n📡 创建 HTTP/2 双向流...")
        if not client.create_bidirectional_stream():
            print("❌ 无法创建 HTTP/2 双向流")
            return False
        
        # 发送测试消息
        test_messages = [
            "Hello HTTP/2 Server!",
            "This is a test message via HTTP/2",
            "Testing bidirectional communication over HTTP/2",
            "Final test message"
        ]
        
        print("\n📤 发送 HTTP/2 测试消息...")
        for i, msg in enumerate(test_messages, 1):
            print(f"\n--- 发送消息 {i}/{len(test_messages)} ---")
            if client.send_message(msg):
                print(f"✅ 消息 {i} 发送成功")
                time.sleep(1)  # 等待响应
            else:
                print(f"❌ 消息 {i} 发送失败")
                return False
        
        # 等待所有响应
        print("\n⏳ 等待 HTTP/2 服务器响应...")
        time.sleep(3)
        
        # 获取统计信息
        stats = client.handler.get_stats()
        print("\n📊 HTTP/2 测试统计:")
        print(f"   - 协议: {stats.get('protocol', 'Unknown')}")
        print(f"   - 发送消息数: {stats['message_count']}")
        print(f"   - 接收消息数: {stats['received_count']}")
        print(f"   - 错误数: {stats['error_count']}")
        print(f"   - 连接状态: {'已连接' if stats['connected'] else '已断开'}")
        print(f"   - 测试时长: {stats['duration']:.2f} 秒")
        
        # 验证测试结果
        success = (
            stats['message_count'] == len(test_messages) and
            stats['received_count'] >= len(test_messages) and
            stats['error_count'] == 0
        )
        
        if success:
            print("\n✅ HTTP/2 委托模式测试成功！")
        else:
            print("\n❌ HTTP/2 委托模式测试失败！")
        
        return success
        
    except Exception as e:
        print(f"\n❌ HTTP/2 测试过程中出错: {e}")
        return False
    
    finally:
        # 清理资源
        print("\n🧹 清理 HTTP/2 测试资源...")
        client.close()
        print("✅ HTTP/2 资源清理完成")

def run_auto_tests_h2():
    """运行自动化测试 - HTTP/2 版本"""
    print("\n" + "="*100)
    print("🎯 RatEngine HTTP/2 双向流自动化测试套件")
    print("="*100)
    
    test_results = []
    
    # 测试1: HTTP/2 委托模式测试
    print("\n🧪 测试 1: HTTP/2 委托模式双向流通信")
    try:
        result = run_delegated_mode_test_h2()
        test_results.append(("HTTP/2 委托模式测试", result))
        if result:
            print("✅ HTTP/2 委托模式测试通过")
        else:
            print("❌ HTTP/2 委托模式测试失败")
    except Exception as e:
        print(f"❌ HTTP/2 委托模式测试异常: {e}")
        test_results.append(("HTTP/2 委托模式测试", False))
    
    # 等待资源清理
    print("\n⏳ 等待资源清理...")
    time.sleep(2)
    
    # 输出测试总结
    print("\n" + "="*100)
    print("📋 HTTP/2 测试总结")
    print("="*100)
    
    passed = 0
    total = len(test_results)
    
    for test_name, result in test_results:
        status = "✅ 通过" if result else "❌ 失败"
        print(f"   {test_name}: {status}")
        if result:
            passed += 1
    
    print(f"\n📊 总体结果: {passed}/{total} 测试通过")
    
    if passed == total:
        print("🎉 所有 HTTP/2 测试都通过了！")
        return True
    else:
        print("⚠️  部分 HTTP/2 测试失败，请检查日志")
        return False

async def run_server_with_delegation_h2():
    """运行带委托的服务器 - HTTP/2 版本"""
    print("\n🚀 启动 HTTP/2 委托模式服务器...")
    
    # 创建服务器实例
    server = AutoTestServerH2()
    
    # 设置信号处理
    def signal_handler(signum, frame):
        print("\n🛑 收到停止信号，正在关闭 HTTP/2 服务器...")
        server.running = False
        sys.exit(0)
    
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    try:
        # 启动服务器
        server.start()
    except KeyboardInterrupt:
        print("\n🛑 HTTP/2 服务器被用户中断")
    except Exception as e:
        print(f"\n❌ HTTP/2 服务器运行出错: {e}")
    finally:
        print("\n✅ HTTP/2 服务器已停止")

def main():
    """主函数 - HTTP/2 版本"""
    if len(sys.argv) > 1:
        if sys.argv[1] == "server":
            # 仅启动服务器
            asyncio.run(run_server_with_delegation_h2())
        elif sys.argv[1] == "test":
            # 仅运行测试
            success = run_auto_tests_h2()
            sys.exit(0 if success else 1)
        else:
            print("用法: python grpc_client_bidirectional_h2_example.py [server|test]")
            sys.exit(1)
    else:
        # 默认运行完整测试
        success = run_auto_tests_h2()
        sys.exit(0 if success else 1)

if __name__ == "__main__":
    main()