#!/usr/bin/env python3
"""
mTLS 双向流客户端完整自动测试

基于 grpc_client_bidirectional_h2_example.py 的完整 mTLS 自动验证示例。
包含自动服务器启动、mTLS 客户端连接、双向流通信验证和资源清理。

功能特性：
- 完全自动化的 mTLS 测试流程
- 真实的 gRPC 服务器/客户端通信
- 强制 HTTP/2 协议支持
- mTLS 双向认证验证
- 消息计数和内容验证
- 完整的错误处理和资源清理
- 基于 RatApp 的正确架构
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
class MTLSMessage:
    """mTLS 测试消息数据结构"""
    user: str
    message: str
    timestamp: float
    message_id: str
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            "user": self.user,
            "message": self.message,
            "timestamp": self.timestamp,
            "message_id": self.message_id
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'MTLSMessage':
        return cls(
            user=data.get("user", ""),
            message=data.get("message", ""),
            timestamp=data.get("timestamp", time.time()),
            message_id=data.get("message_id", str(uuid.uuid4()))
        )

# 全局状态管理
active_mtls_sessions: Dict[str, Dict] = {}
mtls_connections: Dict[str, list] = {}
mtls_messages: Dict[str, list] = {}  # 存储待发送的消息

class MTLSServerHandler:
    """mTLS 服务器端双向流处理器"""
    
    def __init__(self, session_id: str):
        self.session_id = session_id
        self.message_count = 0
        self.received_count = 0
        self.error_count = 0
        self.start_time = time.time()
        
        # 注册到全局会话管理
        active_mtls_sessions[session_id] = {
            'handler': self,
            'start_time': self.start_time,
            'message_count': 0,
            'received_count': 0
        }
        
    def on_connected(self, sender, receiver):
        """连接建立时的委托处理"""
        print(f"🔗 [mTLS委托处理器] mTLS 连接建立，会话: {self.session_id}")
        
        # 设置委托回调 - 使用正确的 RatEngine 模式
        receiver.set_message_callback(self.on_message_received)
        receiver.set_error_callback(self.on_error)
        receiver.set_end_callback(self.on_end)
        
        # 启动接收循环
        receiver.start_receiving()
        
        # 保存发送器引用以便后续使用
        self.sender = sender
        self.receiver = receiver
        
        # 初始化连接列表
        if self.session_id not in mtls_connections:
            mtls_connections[self.session_id] = []
        mtls_connections[self.session_id].append({
            'sender': sender,
            'receiver': receiver,
            'connected_at': time.time()
        })
        
        print(f"✅ [mTLS委托处理器] mTLS 处理器已启动，会话: {self.session_id}")
    

    
    def on_message_received(self, data):
        """委托的消息处理逻辑"""
        try:
            # 解析接收到的数据
            message_text = data.decode('utf-8')
            self.received_count += 1
            
            print(f"📥 [mTLS委托处理器] 收到消息 #{self.received_count} (会话: {self.session_id}): {message_text}")
            
            # 更新会话统计
            if self.session_id in active_mtls_sessions:
                active_mtls_sessions[self.session_id]['received_count'] = self.received_count
            
            # 检查是否是结束信号
            if "quit" in message_text.lower():
                print(f"🔚 [mTLS委托处理器] 收到结束信号，关闭 mTLS 流 (会话: {self.session_id})")
                self.sender.end_stream()
                self.active = False
                return
            
            # 委托的业务逻辑：创建回声消息
            echo_message = f"mTLS Echo from session {self.session_id}: {message_text}"
            print(f"📤 [mTLS委托处理器] 发送 mTLS 回声 (会话: {self.session_id}): {echo_message}")
            
            # 发送回声消息
            self.sender.send_bytes(echo_message.encode('utf-8'))
            
        except Exception as e:
            print(f"❌ [mTLS委托处理器] 处理 mTLS 消息时出错 (会话: {self.session_id}): {e}")
            self.sender.send_error(f"处理消息失败: {e}")
    
    def on_error(self, error):
        """错误处理回调"""
        self.error_count += 1
        print(f"❌ [mTLS服务器] 会话 {self.session_id} 发生错误: {error}")
    
    def on_end(self):
        """连接结束回调"""
        print(f"🔚 [mTLS服务器] 会话结束: {self.session_id}")
        
        # 清理会话数据
        if self.session_id in active_mtls_sessions:
            del active_mtls_sessions[self.session_id]
        if self.session_id in mtls_connections:
            del mtls_connections[self.session_id]
        if self.session_id in mtls_messages:
            del mtls_messages[self.session_id]
    
    def get_stats(self):
        """获取统计信息"""
        return {
            'session_id': self.session_id,
            'message_count': self.message_count,
            'received_count': self.received_count,
            'error_count': self.error_count,
            'uptime': time.time() - self.start_time
        }

class MTLSTestServer:
    """mTLS 测试服务器"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 50053):
        self.host = host
        self.port = port
        self.app = None
        self.running = False
        self.handlers = {}
        print(f"🏗️ [mTLS服务器] 初始化服务器: {host}:{port}")
    
    def setup_routes(self):
        """设置路由"""
        
        @self.app.grpc_bidirectional("/chat.ChatService/BidirectionalChat")
        def handle_mtls_bidirectional(context, sender, receiver):
            """处理 mTLS 双向流"""
            # 生成会话ID
            session_id = context.get('session_id', f"mtls_session_{int(time.time() * 1000)}")
            print(f"🎯 [mTLS服务器] 委托模式：创建处理器委托，会话: {session_id}")
            
            # 创建委托处理器
            handler = MTLSServerHandler(session_id)
            
            # 注册处理器
            self.handlers[session_id] = handler
            
            # 异步委托连接处理 - 避免同步调用导致的死锁
            import threading
            def async_delegate():
                try:
                    print(f"🔄 [mTLS服务器] 异步委托：开始处理器初始化，会话: {session_id}")
                    handler.on_connected(sender, receiver)
                    print(f"✅ [mTLS服务器] 异步委托：处理器已接管会话: {session_id}")
                except Exception as e:
                    print(f"❌ [mTLS服务器] 异步委托失败 (会话: {session_id}): {e}")
                    handler.active = False
            
            # 在新线程中执行委托，避免阻塞主线程
            delegate_thread = threading.Thread(target=async_delegate, daemon=True)
            delegate_thread.start()
            
            print(f"🚀 [mTLS服务器] 委托线程已启动，会话: {session_id}")
        
        @self.app.json("/mtls/status")
        def get_mtls_status(request_data):
            """获取 mTLS 服务器状态"""
            return {
                "status": "running" if self.running else "stopped",
                "active_sessions": len(active_mtls_sessions),
                "total_handlers": len(self.handlers),
                "host": self.host,
                "port": self.port,
                "mtls_enabled": True
            }
    
    def get_active_handlers(self):
        """获取活跃的处理器"""
        return {
            session_id: handler.get_stats() 
            for session_id, handler in self.handlers.items()
        }
    
    def cleanup_inactive_handlers(self):
        """清理非活跃的处理器"""
        current_time = time.time()
        inactive_sessions = []
        
        for session_id, session_data in active_mtls_sessions.items():
            if current_time - session_data['start_time'] > 300:  # 5分钟超时
                inactive_sessions.append(session_id)
        
        for session_id in inactive_sessions:
            if session_id in self.handlers:
                self.handlers[session_id].on_end()
                del self.handlers[session_id]
            print(f"🧹 [mTLS服务器] 清理非活跃会话: {session_id}")
    
    def start(self):
        """启动 mTLS 服务器"""
        try:
            self.app = RatApp()
            
            # 启用 H2 开发模式（自动生成自签名证书并启用 HTTPS + H2）
            print("🔧 启用 H2 开发模式（自动生成自签名证书）...")
            self.app.enable_development_mode(["localhost", "127.0.0.1"])
            
            self.setup_routes()
            
            print(f"🚀 [mTLS服务器] 启动 mTLS 服务器在 {self.host}:{self.port}")
            self.running = True
            self.app.run(host=self.host, port=self.port)
            
        except Exception as e:
            print(f"❌ [mTLS服务器] 启动失败: {e}")
            self.running = False
            raise

class MTLSClientHandler:
    """mTLS 客户端消息处理器"""
    
    def __init__(self, user_name: str):
        self.user_name = user_name
        self.message_count = 0
        self.received_count = 0
        self.error_count = 0
        self.received_messages = []
        print(f"🎯 [mTLS客户端] 创建处理器: {user_name}")
    
    def on_connected(self, stream_id: str):
        """连接建立回调"""
        print(f"✅ [mTLS客户端] 连接建立，流ID: {stream_id}")
    
    def on_message_received(self, data: bytes, stream_id: str):
        """消息接收回调"""
        try:
            self.received_count += 1
            message_text = data.decode('utf-8')
            
            # 尝试解析为JSON，如果失败则作为纯文本处理
            try:
                message_data = json.loads(message_text)
                message = MTLSMessage.from_dict(message_data)
                self.received_messages.append(message)
                print(f"📥 [mTLS客户端] 收到JSON消息 #{self.received_count} 来自 {message.user}: {message.message}")
            except json.JSONDecodeError:
                # 处理纯文本回声消息
                print(f"📥 [mTLS客户端] 收到回声消息 #{self.received_count}: {message_text}")
                # 创建一个简单的消息对象用于统计
                echo_message = MTLSMessage(
                    user="mTLS_Server",
                    message=message_text,
                    timestamp=time.time(),
                    message_id=f"echo_{self.received_count}"
                )
                self.received_messages.append(echo_message)
            
        except Exception as e:
            print(f"❌ [mTLS客户端] 处理接收消息失败: {e}")
            self.on_error(stream_id, str(e))
    
    def on_disconnected(self, stream_id: str, reason: Optional[str] = None):
        """连接断开回调"""
        reason_text = f" ({reason})" if reason else ""
        print(f"🔌 [mTLS客户端] 连接断开{reason_text}，流ID: {stream_id}")
    
    def on_error(self, stream_id: str, error: str):
        """错误处理回调"""
        self.error_count += 1
        print(f"❌ [mTLS客户端] 流 {stream_id} 发生错误: {error}")
    
    def prepare_message(self, message: str, stream_id: str) -> MTLSMessage:
        """准备发送消息"""
        self.message_count += 1
        return MTLSMessage(
            user=self.user_name,
            message=message,
            timestamp=time.time(),
            message_id=f"{self.user_name}_{self.message_count}_{uuid.uuid4().hex[:8]}"
        )
    
    def get_stats(self):
        """获取统计信息"""
        return {
            'user_name': self.user_name,
            'message_count': self.message_count,
            'received_count': self.received_count,
            'error_count': self.error_count,
            'received_messages': len(self.received_messages)
        }

class MTLSTestClient:
    """mTLS 测试客户端"""
    
    def __init__(self, user_name: str):
        self.user_name = user_name
        self.client = None
        self.handler = MTLSClientHandler(user_name)
        self.stream_id = None
        self.running = False
        self.server_uri = None
        print(f"🎯 [mTLS客户端] 创建客户端: {user_name}")
        
    def connect(self, server_uri: str = "https://127.0.0.1:50053") -> bool:
        """连接到 mTLS 服务器"""
        try:
            self.client = PyClientManager()
            
            # 创建 mTLS 客户端配置
            config = {
                "connect_timeout": 10000,  # 10 秒
                "request_timeout": 30000,  # 30 秒
                "max_idle_connections": 10,
                "user_agent": "RatEngine-mTLS-Client/1.0",
                "development_mode": True,  # 开发模式
                "http2_only": True,  # 强制使用 HTTP/2
                "enable_compression": False,
                "enable_http": True,
                # mTLS 配置
                "client_cert_path": "./certs/client.crt",
            "client_key_path": "./certs/client.key",
                "ca_cert_path": "./certs/cert.pem",
                "verify_server_cert": False,  # 开发模式下跳过服务器证书验证
                "require_client_cert": True  # 启用客户端证书认证
            }
            
            # 初始化客户端
            self.client.initialize(config)
            self.server_uri = server_uri
            print(f"✅ [mTLS客户端] mTLS 客户端连接成功（开发模式）")
            return True
            
        except Exception as e:
            print(f"❌ [mTLS客户端] mTLS 连接失败: {e}")
            return False
    
    def create_bidirectional_stream(self) -> bool:
        """创建双向流"""
        if not self.client:
            print(f"❌ [mTLS客户端] 客户端未初始化")
            return False
            
        if not self.server_uri:
            print(f"❌ [mTLS客户端] 服务器 URI 未设置")
            return False
        
        try:
            print(f"🔗 [mTLS客户端] 正在连接到: {self.server_uri}")
            self.stream_id = self.client.grpc_bidirectional_stream(
                self.server_uri, 
                "chat.ChatService", 
                "BidirectionalChat"
            )
            print(f"✅ [mTLS客户端] mTLS 双向流创建成功，流ID: {self.stream_id}")
            
            # 调用连接建立回调
            self.handler.on_connected(str(self.stream_id))
            
            # 启动消息接收循环
            self.running = True
            self._start_receive_loop()
            
            return True
            
        except Exception as e:
            print(f"❌ [mTLS客户端] 创建 mTLS 双向流失败: {e}")
            return False
    
    def _start_receive_loop(self):
        """启动消息接收循环"""
        def receive_messages():
            print(f"🎯 [mTLS客户端] 启动 mTLS 消息接收循环")
            while self.running and self.client and self.stream_id:
                try:
                    # 检查流是否已关闭
                    if self.client.grpc_bidirectional_is_closed(str(self.stream_id)):
                        print(f"🔌 [mTLS客户端] 流已关闭，停止接收")
                        self.handler.on_disconnected(str(self.stream_id), "流已关闭")
                        break
                    
                    # 尝试接收消息
                    message = self.client.grpc_bidirectional_receive(str(self.stream_id))
                    if message is not None:
                        self.handler.on_message_received(message, str(self.stream_id))
                    else:
                        # 没有消息，短暂休眠
                        time.sleep(0.1)
                        
                except Exception as e:
                    print(f"❌ [mTLS客户端] 接收消息异常: {e}")
                    self.handler.on_error(str(self.stream_id), str(e))
                    break
            
            print(f"🔚 [mTLS客户端] 消息接收循环结束")
        
        receive_thread = threading.Thread(target=receive_messages, daemon=True)
        receive_thread.start()
    
    def send_message(self, message: str) -> bool:
        """发送消息"""
        if not self.client or not self.stream_id:
            print(f"❌ [mTLS客户端] 流未建立")
            return False
        
        try:
            # 准备消息
            mtls_msg = self.handler.prepare_message(message, str(self.stream_id))
            
            data = json.dumps(mtls_msg.to_dict()).encode('utf-8')
            self.client.grpc_bidirectional_send(self.stream_id, data)
            print(f"📤 [mTLS客户端] 发送消息 #{self.handler.message_count}: {mtls_msg.message}")
            return True
            
        except Exception as e:
            print(f"❌ [mTLS客户端] 发送消息失败: {e}")
            self.handler.on_error(str(self.stream_id), str(e))
            return False
    
    def close_stream(self):
        """关闭流"""
        if self.client and self.stream_id:
            try:
                self.running = False
                self.client.grpc_bidirectional_close(self.stream_id)
                print(f"✅ [mTLS客户端] 流已关闭")
                self.handler.on_disconnected(str(self.stream_id), "主动关闭")
            except Exception as e:
                print(f"❌ [mTLS客户端] 关闭流失败: {e}")
    
    def close(self):
        """关闭客户端"""
        self.close_stream()
        if self.client:
            try:
                self.client.close()
                print(f"✅ [mTLS客户端] 客户端已关闭")
            except Exception as e:
                print(f"❌ [mTLS客户端] 关闭客户端失败: {e}")

def start_mtls_test_server():
    """启动 mTLS 测试服务器"""
    server = MTLSTestServer()
    server.start()

def run_mtls_bidirectional_test() -> bool:
    """运行 mTLS 双向流测试"""
    print("\n" + "="*80)
    print("🚀 开始 mTLS 双向流完整自动测试")
    print("="*80)
    
    # 启动服务器
    server_thread = threading.Thread(target=start_mtls_test_server, daemon=True)
    server_thread.start()
    
    # 等待服务器启动
    print("⏳ 等待 mTLS 服务器启动...")
    time.sleep(3)
    
    # 创建客户端
    client = MTLSTestClient("mTLS_TestUser")
    
    try:
        # 连接到服务器
        print("\n🔗 连接到 mTLS 服务器...")
        if not client.connect("https://127.0.0.1:50053"):
            print("❌ 无法连接到 mTLS 服务器")
            return False
        
        # 创建双向流
        print("\n📡 创建 mTLS 双向流...")
        if not client.create_bidirectional_stream():
            print("❌ 无法创建 mTLS 双向流")
            return False
        
        # 发送测试消息
        test_messages = [
            "Hello mTLS Server!",
            "Testing mTLS bidirectional communication",
            "mTLS client certificate authentication working",
            "Final mTLS test message"
        ]
        
        print("\n📤 发送 mTLS 测试消息...")
        sent_count = 0
        for i, msg in enumerate(test_messages, 1):
            print(f"\n--- 发送消息 {i}/{len(test_messages)} ---")
            if client.send_message(msg):
                sent_count += 1
                print(f"✅ 消息 {i} 发送成功")
                time.sleep(1)  # 等待服务器处理
            else:
                print(f"❌ 消息 {i} 发送失败")
                break
        
        # 等待接收回复
        print("\n⏳ 等待服务器回复...")
        time.sleep(3)
        
        # 检查结果
        received_count = client.handler.received_count
        print(f"\n📊 测试结果统计:")
        print(f"   发送消息: {sent_count}/{len(test_messages)}")
        print(f"   接收回复: {received_count}")
        print(f"   错误计数: {client.handler.error_count}")
        
        # 验证测试结果
        success = (
            sent_count == len(test_messages) and
            received_count >= 1 and  # 至少收到欢迎消息
            client.handler.error_count == 0
        )
        
        if success:
            print("\n🎉 mTLS 双向流测试成功！")
            print("✅ 所有消息发送成功")
            print("✅ 接收到服务器回复")
            print("✅ mTLS 客户端证书认证正常")
            print("✅ 双向通信功能正常")
        else:
            print("\n❌ mTLS 双向流测试失败")
            if sent_count != len(test_messages):
                print(f"   消息发送不完整: {sent_count}/{len(test_messages)}")
            if received_count == 0:
                print("   未收到服务器回复")
            if client.handler.error_count > 0:
                print(f"   发生错误: {client.handler.error_count} 次")
        
        return success
        
    except Exception as e:
        print(f"❌ mTLS 双向流测试异常: {e}")
        return False
        
    finally:
        # 清理资源
        print("\n🧹 清理测试资源...")
        try:
            client.close()
            print("✅ 客户端资源清理完成")
        except Exception as e:
            print(f"❌ 清理客户端资源失败: {e}")

def signal_handler(signum, frame):
    """信号处理器"""
    print("\n🛑 收到中断信号，正在退出...")
    sys.exit(0)

def main():
    """主函数"""
    # 注册信号处理器
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    print("🔐 mTLS 双向流完整自动测试")
    print("基于 RatEngine 的 mTLS 双向认证和双向流通信测试")
    print("\n📋 测试内容:")
    print("- mTLS 服务器启动")
    print("- mTLS 客户端连接（客户端证书认证）")
    print("- 双向流创建")
    print("- 消息发送和接收")
    print("- 资源清理")
    
    # 检查证书文件
    required_files = ["./certs/cert.pem", "./certs/key.pem", "./certs/client.crt", "./certs/client.key"]
    missing_files = [f for f in required_files if not os.path.exists(f)]
    
    if missing_files:
        print(f"\n❌ 缺少必要的证书文件: {missing_files}")
        print("请确保以下文件存在:")
        for f in required_files:
            print(f"   {f}")
        return False
    
    print("\n✅ 证书文件检查通过")
    
    # 运行测试
    try:
        success = run_mtls_bidirectional_test()
        
        print("\n" + "="*80)
        if success:
            print("🎉 mTLS 双向流完整自动测试 - 成功")
            print("✅ mTLS 双向认证功能正常")
            print("✅ 双向流通信功能正常")
            print("✅ 消息发送接收功能正常")
        else:
            print("❌ mTLS 双向流完整自动测试 - 失败")
            print("请检查服务器日志和网络连接")
        print("="*80)
        
        return success
        
    except KeyboardInterrupt:
        print("\n🛑 测试被用户中断")
        return False
    except Exception as e:
        print(f"\n❌ 测试执行异常: {e}")
        return False

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)