#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
分端口模式示例 - HTTP 和 gRPC 使用不同端口

基于现有的混合模式架构，实现 HTTP 和 gRPC 分端口部署
使用 gRPC 一元请求 + HTTP GET/POST JSON + 表单提交

特性：
- 分端口部署：HTTP 和 gRPC 使用不同端口
- 委托模式：业务逻辑与传输层分离
- 数据验证：完整的请求数据打印和验证
- 自动测试：端到端验证所有协议
- 性能优化：避免协议检测开销
"""

import asyncio
import threading
import time
import json
import requests
import socket
from dataclasses import dataclass
from typing import Dict, Any, Optional
from rat_engine import RatApp, PyClientManager


# ============================================================================
# 数据结构定义
# ============================================================================

@dataclass
class UserMessage:
    """用户消息数据结构"""
    user_id: int
    name: str
    email: str
    message: str
    timestamp: float
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据 - gRPC 使用"""
        message_str = f"{self.user_id}|{self.name}|{self.email}|{self.message}|{self.timestamp}"
        return message_str.encode('utf-8')
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'UserMessage':
        """从字节数据反序列化 - gRPC 使用"""
        message_str = data.decode('utf-8')
        parts = message_str.split('|', 4)  # 最多分割4次
        if len(parts) != 5:
            raise ValueError(f"无效的消息格式: {message_str}")
        
        return cls(
            user_id=int(parts[0]),
            name=parts[1],
            email=parts[2],
            message=parts[3],
            timestamp=float(parts[4])
        )
    
    def to_dict(self) -> Dict[str, Any]:
        """转换为字典 - HTTP JSON 使用"""
        return {
            "user_id": self.user_id,
            "name": self.name,
            "email": self.email,
            "message": self.message,
            "timestamp": self.timestamp
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'UserMessage':
        """从字典创建 - HTTP JSON 使用"""
        return cls(
            user_id=data.get("user_id", 0),
            name=data.get("name", ""),
            email=data.get("email", ""),
            message=data.get("message", ""),
            timestamp=data.get("timestamp", time.time())
        )


@dataclass
class ResponseMessage:
    """响应消息数据结构"""
    success: bool
    user_id: int
    message: str
    server_timestamp: float
    protocol: str  # 标识使用的协议
    port: int  # 标识使用的端口
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据 - gRPC 使用"""
        success_str = "1" if self.success else "0"
        response_str = f"{success_str}|{self.user_id}|{self.message}|{self.server_timestamp}|{self.protocol}|{self.port}"
        return response_str.encode('utf-8')
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'ResponseMessage':
        """从字节数据反序列化 - gRPC 使用"""
        response_str = data.decode('utf-8')
        parts = response_str.split('|', 5)  # 最多分割5次
        if len(parts) != 6:
            raise ValueError(f"无效的响应格式: {response_str}")
        
        return cls(
            success=parts[0] == "1",
            user_id=int(parts[1]),
            message=parts[2],
            server_timestamp=float(parts[3]),
            protocol=parts[4],
            port=int(parts[5])
        )
    
    def to_dict(self) -> Dict[str, Any]:
        """转换为字典 - HTTP JSON 使用"""
        return {
            "success": self.success,
            "user_id": self.user_id,
            "message": self.message,
            "server_timestamp": self.server_timestamp,
            "protocol": self.protocol,
            "port": self.port
        }


# ============================================================================
# 服务器端委托处理器
# ============================================================================

class SeparatedPortsServerHandler:
    """分端口模式服务器端委托处理器 - 支持多种协议"""
    
    def __init__(self, http_port: int, grpc_port: int):
        self.http_port = http_port
        self.grpc_port = grpc_port
        self.request_count = 0
        self.grpc_count = 0
        self.http_count = 0
        self.start_time = time.time()
        self.messages_db = []  # 存储所有消息
        print(f"🎯 [服务器委托处理器] 分端口模式初始化完成")
        print(f"   📡 HTTP 端口: {self.http_port}")
        print(f"   🔌 gRPC 端口: {self.grpc_port}")
    
    def handle_grpc_request(self, request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
        """处理 gRPC 字节模式请求"""
        try:
            self.request_count += 1
            self.grpc_count += 1
            
            # 调试：打印接收到的原始数据
            print(f"\n🔍 [gRPC服务器:{self.grpc_port}] 接收到字节数据长度: {len(request_data)}")
            print(f"🔍 [gRPC服务器:{self.grpc_port}] 字节数据内容: {request_data.decode('utf-8', errors='replace')[:200]}...")
            
            # 解析 gRPC 请求数据
            user_message = UserMessage.from_bytes(request_data)
            print(f"📥 [gRPC服务器:{self.grpc_port}] 收到请求 #{self.request_count}: {user_message.message} (用户: {user_message.name})")
            
            # 存储消息
            self.messages_db.append(user_message)
            
            # 模拟业务处理
            time.sleep(0.01)
            
            # 构造响应
            response = ResponseMessage(
                success=True,
                user_id=user_message.user_id,
                message=f"gRPC处理成功: {user_message.message}",
                server_timestamp=time.time(),
                protocol="gRPC",
                port=self.grpc_port
            )
            
            response_bytes = response.to_bytes()
            print(f"📤 [gRPC服务器:{self.grpc_port}] 发送响应给用户 {user_message.user_id}: {response.message}")
            
            return response_bytes
            
        except Exception as e:
            print(f"❌ [gRPC服务器:{self.grpc_port}] 处理请求失败: {e}")
            error_response = ResponseMessage(
                success=False,
                user_id=0,
                message=f"gRPC处理失败: {str(e)}",
                server_timestamp=time.time(),
                protocol="gRPC",
                port=self.grpc_port
            )
            return error_response.to_bytes()
    
    def handle_http_get_request(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """处理 HTTP GET JSON 请求"""
        try:
            self.request_count += 1
            self.http_count += 1
            
            print(f"\n🔍 [HTTP GET服务器:{self.http_port}] 接收到请求数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}")
            
            # 返回所有消息列表
            messages_list = [msg.to_dict() for msg in self.messages_db]
            
            response = {
                "success": True,
                "message": "HTTP GET处理成功",
                "protocol": "HTTP-GET",
                "port": self.http_port,
                "server_timestamp": time.time(),
                "total_messages": len(self.messages_db),
                "total_requests": self.request_count,
                "grpc_requests": self.grpc_count,
                "http_requests": self.http_count,
                "messages": messages_list
            }
            
            print(f"📤 [HTTP GET服务器:{self.http_port}] 返回 {len(messages_list)} 条消息")
            return response
            
        except Exception as e:
            print(f"❌ [HTTP GET服务器:{self.http_port}] 处理请求失败: {e}")
            return {
                "success": False,
                "message": f"HTTP GET处理失败: {str(e)}",
                "protocol": "HTTP-GET",
                "port": self.http_port,
                "server_timestamp": time.time()
            }
    
    def handle_http_post_json_request(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """处理 HTTP POST JSON 请求"""
        try:
            self.request_count += 1
            self.http_count += 1
            
            import json
            print(f"\n🔍 [HTTP POST JSON服务器:{self.http_port}] 接收到请求数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}")
            
            # 解析请求数据 - 从 body 字段解析 JSON
            body_str = request_data.get("body", "{}")
            if isinstance(body_str, str):
                try:
                    json_data = json.loads(body_str)
                except json.JSONDecodeError as e:
                    print(f"❌ [HTTP POST JSON服务器:{self.http_port}] JSON解析失败: {e}")
                    json_data = {}
            else:
                json_data = body_str
            
            user_message = UserMessage.from_dict(json_data)
            print(f"📥 [HTTP POST JSON服务器:{self.http_port}] 收到请求 #{self.request_count}: {user_message.message} (用户: {user_message.name})")
            
            # 存储消息
            self.messages_db.append(user_message)
            
            # 模拟业务处理
            time.sleep(0.01)
            
            response = {
                "success": True,
                "user_id": user_message.user_id,
                "message": f"HTTP POST JSON处理成功: {user_message.message}",
                "protocol": "HTTP-POST-JSON",
                "port": self.http_port,
                "server_timestamp": time.time()
            }
            
            print(f"📤 [HTTP POST JSON服务器:{self.http_port}] 发送响应给用户 {user_message.user_id}: {response['message']}")
            return response
            
        except Exception as e:
            print(f"❌ [HTTP POST JSON服务器:{self.http_port}] 处理请求失败: {e}")
            return {
                "success": False,
                "message": f"HTTP POST JSON处理失败: {str(e)}",
                "protocol": "HTTP-POST-JSON",
                "port": self.http_port,
                "server_timestamp": time.time()
            }
    
    def handle_http_post_form_request(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """处理 HTTP POST 表单请求"""
        try:
            self.request_count += 1
            self.http_count += 1
            
            print(f"\n🔍 [HTTP POST FORM服务器:{self.http_port}] 接收到表单数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}")
            
            # 从 body 字段解析表单数据
            body_str = request_data.get("body", "")
            if isinstance(body_str, str):
                from urllib.parse import parse_qs
                form_data = parse_qs(body_str)
                # 从表单数据中提取值（parse_qs 返回的是列表）
                user_id = int(form_data.get("user_id", ["0"])[0])
                name = form_data.get("name", [""])[0]
                email = form_data.get("email", [""])[0]
                message = form_data.get("message", [""])[0]
            else:
                # 直接从 request_data 中获取（备用方案）
                user_id = int(request_data.get("user_id", 0))
                name = request_data.get("name", "")
                email = request_data.get("email", "")
                message = request_data.get("message", "")
            
            # 创建用户消息
            user_message = UserMessage(
                user_id=user_id,
                name=name,
                email=email,
                message=message,
                timestamp=time.time()
            )
            
            print(f"📥 [HTTP POST FORM服务器:{self.http_port}] 收到表单请求 #{self.request_count}: {user_message.message} (用户: {user_message.name})")
            
            # 存储消息
            self.messages_db.append(user_message)
            
            # 模拟业务处理
            time.sleep(0.01)
            
            response = {
                "success": True,
                "user_id": user_message.user_id,
                "message": f"HTTP POST FORM处理成功: {user_message.message}",
                "protocol": "HTTP-POST-FORM",
                "port": self.http_port,
                "server_timestamp": time.time()
            }
            
            print(f"📤 [HTTP POST FORM服务器:{self.http_port}] 发送响应给用户 {user_message.user_id}: {response['message']}")
            return response
            
        except Exception as e:
            print(f"❌ [HTTP POST FORM服务器:{self.http_port}] 处理请求失败: {e}")
            return {
                "success": False,
                "message": f"HTTP POST FORM处理失败: {str(e)}",
                "protocol": "HTTP-POST-FORM",
                "port": self.http_port,
                "server_timestamp": time.time()
            }
    
    def get_stats(self) -> Dict[str, Any]:
        """获取统计信息"""
        elapsed = time.time() - self.start_time
        return {
            "total_requests": self.request_count,
            "grpc_requests": self.grpc_count,
            "http_requests": self.http_count,
            "total_messages": len(self.messages_db),
            "elapsed_time": elapsed,
            "requests_per_second": self.request_count / max(1, elapsed),
            "http_port": self.http_port,
            "grpc_port": self.grpc_port
        }


# ============================================================================
# HTTP 服务器（独立端口）
# ============================================================================

class HttpServer:
    """HTTP 服务器 - 独立端口运行"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 50054):
        self.host = host
        self.port = port
        self.app = RatApp("http_server")
        self.running = False
        self.handler = None
        print(f"🌐 [HTTP服务器] 初始化在 {self.host}:{self.port}")
    
    def set_handler(self, handler: SeparatedPortsServerHandler):
        """设置委托处理器"""
        self.handler = handler
        self.setup_routes()
    
    def setup_routes(self):
        """设置 HTTP 路由"""
        if not self.handler:
            raise ValueError("必须先设置处理器")
        
        # 启用 H2C 支持以避免预期失败测试中的错误
        # 虽然这是预期失败测试，但错误信息应该更清晰
        self.app.enable_h2c()
        
        # HTTP GET JSON 路由
        @self.app.json("/api/messages", methods=["GET"])
        def handle_get_messages(request_data: Dict[str, Any]) -> Dict[str, Any]:
            """委托给 HTTP GET 处理器"""
            return self.handler.handle_http_get_request(request_data)
        
        # HTTP POST JSON 路由
        @self.app.json("/api/messages", methods=["POST"])
        def handle_post_json_message(request_data: Dict[str, Any]) -> Dict[str, Any]:
            """委托给 HTTP POST JSON 处理器"""
            return self.handler.handle_http_post_json_request(request_data)
        
        # HTTP POST 表单路由
        @self.app.json("/api/form-messages", methods=["POST"])
        def handle_post_form_message(request_data: Dict[str, Any]) -> Dict[str, Any]:
            """委托给 HTTP POST 表单处理器"""
            return self.handler.handle_http_post_form_request(request_data)
        
        # 统计信息路由
        @self.app.json("/api/stats")
        def handle_stats(request_data: Dict[str, Any]) -> Dict[str, Any]:
            """获取服务器统计信息"""
            stats = self.handler.get_stats()
            return {
                "success": True,
                "stats": stats,
                "server_info": {
                    "host": self.host,
                    "port": self.port,
                    "protocol": "HTTP"
                }
            }
        
        print(f"📝 [HTTP服务器] 已注册路由:")
        print(f"   🌐 HTTP GET: /api/messages")
        print(f"   🌐 HTTP POST JSON: /api/messages")
        print(f"   🌐 HTTP POST FORM: /api/form-messages")
        print(f"   📊 HTTP STATS: /api/stats")
    
    def start(self):
        """启动 HTTP 服务器"""
        if self.running:
            return
        
        self.running = True
        print(f"🚀 [HTTP服务器] 启动在 {self.host}:{self.port}")
        
        try:
            # 只启用 HTTP 协议
            self.app.configure_protocols(enable_h2c=False, enable_h2=False)
            print("✅ [HTTP服务器] HTTP/1.1 协议已启用")
            
            self.app.run(host=self.host, port=self.port)
        except Exception as e:
            print(f"❌ [HTTP服务器] 启动失败: {e}")
            self.running = False
            raise
    
    def stop(self):
        """停止服务器"""
        self.running = False
        print(f"🛑 [HTTP服务器] 已停止")


# ============================================================================
# gRPC 服务器（独立端口）
# ============================================================================

class GrpcServer:
    """gRPC 服务器 - 独立端口运行"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 50055):
        self.host = host
        self.port = port
        self.app = RatApp("grpc_server")
        self.running = False
        self.handler = None
        print(f"🔌 [gRPC服务器] 初始化在 {self.host}:{self.port}")
    
    def set_handler(self, handler: SeparatedPortsServerHandler):
        """设置委托处理器"""
        self.handler = handler
        self.setup_routes()
    
    def setup_routes(self):
        """设置 gRPC 路由"""
        if not self.handler:
            raise ValueError("必须先设置处理器")
        
        # gRPC 一元请求路由
        @self.app.grpc_unary("/separated.SeparatedService/ProcessMessage")
        def handle_grpc_message(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
            """委托给 gRPC 处理器"""
            return self.handler.handle_grpc_request(request_data, metadata, context)
        
        print(f"📝 [gRPC服务器] 已注册路由:")
        print(f"   🔌 gRPC: /separated.SeparatedService/ProcessMessage")
    
    def start(self):
        """启动 gRPC 服务器"""
        if self.running:
            return
        
        self.running = True
        print(f"🚀 [gRPC服务器] 启动在 {self.host}:{self.port}")
        
        try:
            # 只启用 gRPC 协议（通过 H2C）
            self.app.configure_protocols(enable_h2c=True, enable_h2=True)
            print("✅ [gRPC服务器] gRPC (H2C) 协议已启用")
            
            self.app.run(host=self.host, port=self.port)
        except Exception as e:
            print(f"❌ [gRPC服务器] 启动失败: {e}")
            self.running = False
            raise
    
    def stop(self):
        """停止服务器"""
        self.running = False
        print(f"🛑 [gRPC服务器] 已停止")


# ============================================================================
# 分端口模式服务器管理器
# ============================================================================

class SeparatedPortsServerManager:
    """分端口模式服务器管理器 - 管理 HTTP 和 gRPC 服务器"""
    
    def __init__(self, host: str = "127.0.0.1", http_port: int = 50054, grpc_port: int = 50055):
        self.host = host
        self.http_port = http_port
        self.grpc_port = grpc_port
        
        # 创建共享的委托处理器
        self.handler = SeparatedPortsServerHandler(http_port, grpc_port)
        
        # 创建独立的服务器
        self.http_server = HttpServer(host, http_port)
        self.grpc_server = GrpcServer(host, grpc_port)
        
        # 设置处理器
        self.http_server.set_handler(self.handler)
        self.grpc_server.set_handler(self.handler)
        
        self.running = False
        
        print(f"🎯 [分端口管理器] 初始化完成")
        print(f"   📡 HTTP 服务器: {host}:{http_port}")
        print(f"   🔌 gRPC 服务器: {host}:{grpc_port}")
    
    def start(self):
        """启动所有服务器"""
        if self.running:
            return
        
        self.running = True
        print(f"🚀 [分端口管理器] 启动分端口模式服务器")
        
        # 在独立线程中启动 HTTP 服务器
        http_thread = threading.Thread(
            target=self.http_server.start,
            name="HttpServerThread",
            daemon=True
        )
        
        # 在独立线程中启动 gRPC 服务器
        grpc_thread = threading.Thread(
            target=self.grpc_server.start,
            name="GrpcServerThread",
            daemon=True
        )
        
        # 启动线程
        http_thread.start()
        grpc_thread.start()
        
        # 等待服务器启动
        time.sleep(2)
        
        print(f"✅ [分端口管理器] 所有服务器已启动")
        print(f"   📡 HTTP 服务器运行在: http://{self.host}:{self.http_port}")
        print(f"   🔌 gRPC 服务器运行在: http://{self.host}:{self.grpc_port}")
        
        return http_thread, grpc_thread
    
    def stop(self):
        """停止所有服务器"""
        self.running = False
        self.http_server.stop()
        self.grpc_server.stop()
        print(f"🛑 [分端口管理器] 所有服务器已停止")


# ============================================================================
# 客户端测试器
# ============================================================================

class SeparatedPortsClient:
    """分端口模式客户端 - 测试所有协议"""
    
    def __init__(self, host: str = "127.0.0.1", http_port: int = 50054, grpc_port: int = 50055):
        self.host = host
        self.http_port = http_port
        self.grpc_port = grpc_port
        self.http_uri = f"http://{host}:{http_port}"
        self.grpc_uri = f"http://{host}:{grpc_port}"
        self.grpc_client = None
        self.test_results = {
            "grpc": [],
            "http_get": [],
            "http_post_json": [],
            "http_post_form": [],
            "expected_failures": []
        }
        
        print(f"🎯 [客户端] 分端口模式初始化")
        print(f"   📡 HTTP 服务器: {self.http_uri}")
        print(f"   🔌 gRPC 服务器: {self.grpc_uri}")
    
    async def initialize_grpc(self):
        """初始化 gRPC 客户端"""
        try:
            self.grpc_client = PyClientManager()
            
            config = {
                "connect_timeout": 3000,
                "request_timeout": 5000,
                "max_idle_connections": 10,
                "grpc_user_agent": "RatEngine-SeparatedPorts-Client/1.0",
                "development_mode": True,
                "enable_grpc": True,
                "enable_http": True
            }
            
            self.grpc_client.initialize(config)
            print(f"✅ [客户端] gRPC 客户端初始化完成")
            
        except Exception as e:
            print(f"❌ [客户端] gRPC 初始化失败: {e}")
            raise
    
    async def test_grpc_request(self, user_id: int, name: str, email: str, message: str) -> bool:
        """测试 gRPC 一元请求"""
        try:
            # 创建请求
            user_message = UserMessage(
                user_id=user_id,
                name=name,
                email=email,
                message=message,
                timestamp=time.time()
            )
            
            request_data = user_message.to_bytes()
            
            print(f"\n📤 [gRPC客户端:{self.grpc_port}] 发送请求: {message} (用户: {name})")
            print(f"🔍 [gRPC客户端:{self.grpc_port}] 请求字节数据长度: {len(request_data)}")
            
            # 发送 gRPC 请求
            request_id = self.grpc_client.grpc_unary_delegated(
                uri=self.grpc_uri,
                service="separated.SeparatedService",
                method="ProcessMessage",
                data=request_data,
                metadata=None
            )
            
            print(f"🚀 [gRPC客户端:{self.grpc_port}] 请求已发送，请求ID: {request_id}")
            
            # 等待响应
            max_wait = 2.0
            start_time = time.time()
            
            while time.time() - start_time < max_wait:
                response_data = self.grpc_client.grpc_unary_delegated_receive(request_id)
                
                if response_data is not None:
                    response = ResponseMessage.from_bytes(bytes(response_data))
                    print(f"✅ [gRPC客户端:{self.grpc_port}] 收到响应: {response.message} (端口: {response.port})")
                    self.test_results["grpc"].append(response.success)
                    return response.success
                
                await asyncio.sleep(0.1)
            
            print(f"⏰ [gRPC客户端:{self.grpc_port}] 请求超时: {request_id}")
            self.test_results["grpc"].append(False)
            return False
            
        except Exception as e:
            print(f"❌ [gRPC客户端:{self.grpc_port}] 请求失败: {e}")
            self.test_results["grpc"].append(False)
            return False
    
    def test_http_get_request(self) -> bool:
        """测试 HTTP GET JSON 请求"""
        try:
            url = f"{self.http_uri}/api/messages"
            
            print(f"\n📤 [HTTP GET客户端:{self.http_port}] 发送请求到: {url}")
            
            # 使用 requests 发送 HTTP/1.1 请求
            response = requests.get(url, timeout=5)
            
            print(f"🔍 [HTTP GET客户端:{self.http_port}] 响应状态码: {response.status_code}")
            
            if response.status_code == 200:
                response_data = response.json()
                print(f"✅ [HTTP GET客户端:{self.http_port}] 收到响应: {response_data.get('message', 'N/A')} (端口: {response_data.get('port', 'N/A')})")
                print(f"📊 [HTTP GET客户端:{self.http_port}] 消息总数: {response_data.get('total_messages', 0)}")
                self.test_results["http_get"].append(response_data.get("success", False))
                return response_data.get("success", False)
            else:
                print(f"❌ [HTTP GET客户端:{self.http_port}] 请求失败，状态码: {response.status_code}")
                self.test_results["http_get"].append(False)
                return False
            
        except Exception as e:
            print(f"❌ [HTTP GET客户端:{self.http_port}] 请求失败: {e}")
            self.test_results["http_get"].append(False)
            return False
    
    def test_http_post_json_request(self, user_id: int, name: str, email: str, message: str) -> bool:
        """测试 HTTP POST JSON 请求"""
        try:
            url = f"{self.http_uri}/api/messages"
            
            # 创建 JSON 数据
            json_data = {
                "user_id": user_id,
                "name": name,
                "email": email,
                "message": message,
                "timestamp": time.time()
            }
            
            print(f"\n📤 [HTTP POST JSON客户端:{self.http_port}] 发送请求: {message} (用户: {name})")
            
            # 发送 POST 请求
            response = requests.post(url, json=json_data, timeout=5)
            
            print(f"🔍 [HTTP POST JSON客户端:{self.http_port}] 响应状态码: {response.status_code}")
            
            if response.status_code == 200:
                response_data = response.json()
                print(f"✅ [HTTP POST JSON客户端:{self.http_port}] 收到响应: {response_data.get('message', 'N/A')} (端口: {response_data.get('port', 'N/A')})")
                self.test_results["http_post_json"].append(response_data.get("success", False))
                return response_data.get("success", False)
            else:
                print(f"❌ [HTTP POST JSON客户端:{self.http_port}] 请求失败，状态码: {response.status_code}")
                self.test_results["http_post_json"].append(False)
                return False
            
        except Exception as e:
            print(f"❌ [HTTP POST JSON客户端:{self.http_port}] 请求失败: {e}")
            self.test_results["http_post_json"].append(False)
            return False
    
    def test_http_post_form_request(self, user_id: int, name: str, email: str, message: str) -> bool:
        """测试 HTTP POST 表单请求"""
        try:
            url = f"{self.http_uri}/api/form-messages"
            
            # 创建表单数据
            form_data = {
                "user_id": str(user_id),
                "name": name,
                "email": email,
                "message": message
            }
            
            print(f"\n📤 [HTTP POST FORM客户端:{self.http_port}] 发送表单请求: {message} (用户: {name})")
            
            # 发送 POST 表单请求
            response = requests.post(url, data=form_data, timeout=5)
            
            print(f"🔍 [HTTP POST FORM客户端:{self.http_port}] 响应状态码: {response.status_code}")
            
            if response.status_code == 200:
                response_data = response.json()
                print(f"✅ [HTTP POST FORM客户端:{self.http_port}] 收到响应: {response_data.get('message', 'N/A')} (端口: {response_data.get('port', 'N/A')})")
                self.test_results["http_post_form"].append(response_data.get("success", False))
                return response_data.get("success", False)
            else:
                print(f"❌ [HTTP POST FORM客户端:{self.http_port}] 请求失败，状态码: {response.status_code}")
                self.test_results["http_post_form"].append(False)
                return False
            
        except Exception as e:
            print(f"❌ [HTTP POST FORM客户端:{self.http_port}] 请求失败: {e}")
            self.test_results["http_post_form"].append(False)
            return False
    
    def test_expected_failure_http_to_grpc_port(self) -> bool:
        """预期失败测试：使用 HTTP 请求访问 gRPC 端口"""
        try:
            # 尝试用 HTTP 方式访问 gRPC 端口
            grpc_url = f"http://{self.host}:{self.grpc_port}/api/messages"
            
            print(f"\n🚫 [预期失败测试] 使用 HTTP 请求访问 gRPC 端口: {grpc_url}")
            
            # 发送 HTTP GET 请求到 gRPC 端口
            response = requests.get(grpc_url, timeout=3)
            
            print(f"🔍 [预期失败测试] HTTP->gRPC 响应状态码: {response.status_code}")
            
            # 如果成功了，说明端口没有正确分离
            if response.status_code == 200:
                print(f"❌ [预期失败测试] 意外成功！gRPC 端口不应该处理 HTTP 请求")
                self.test_results["expected_failures"].append(False)  # 预期失败但实际成功
                return False
            else:
                print(f"✅ [预期失败测试] 正确失败！gRPC 端口拒绝了 HTTP 请求 (状态码: {response.status_code})")
                self.test_results["expected_failures"].append(True)  # 预期失败且实际失败
                return True
            
        except requests.exceptions.RequestException as e:
            print(f"✅ [预期失败测试] 正确失败！HTTP 请求 gRPC 端口异常: {e}")
            self.test_results["expected_failures"].append(True)  # 预期失败且实际失败
            return True
        except Exception as e:
            print(f"⚠️ [预期失败测试] 测试异常: {e}")
            self.test_results["expected_failures"].append(False)
            return False
    
    def test_expected_failure_grpc_to_http_port(self) -> bool:
        """预期失败测试：使用 socket 模拟 gRPC 客户端访问 HTTP 端口"""
        try:
            print(f"\n🚫 [预期失败测试] 使用 socket 模拟 gRPC 请求访问 HTTP 端口: {self.host}:{self.http_port}")
            
            # 创建 socket 连接
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(3)
            
            try:
                # 连接到 HTTP 端口
                sock.connect((self.host, self.http_port))
                
                # 发送模拟的 gRPC HTTP/2 连接前导码
                # gRPC 使用 HTTP/2，连接前导码是 "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"
                grpc_preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"
                sock.send(grpc_preface)
                
                # 尝试接收响应
                response = sock.recv(1024)
                
                if response:
                    response_str = response.decode('utf-8', errors='replace')
                    print(f"🔍 [预期失败测试] gRPC->HTTP 收到响应: {response_str[:100]}...")
                    
                    # 检查是否是 HTTP/2 响应或错误响应
                    if b"HTTP/1.1 400" in response or b"Bad Request" in response:
                        print(f"✅ [预期失败测试] 正确失败！HTTP 端口拒绝了 gRPC 连接前导码")
                        self.test_results["expected_failures"].append(True)
                        return True
                    elif b"HTTP/2" in response:
                        print(f"❌ [预期失败测试] 意外成功！HTTP 端口不应该支持 HTTP/2")
                        self.test_results["expected_failures"].append(False)
                        return False
                    else:
                        print(f"✅ [预期失败测试] 正确失败！HTTP 端口返回了非预期响应")
                        self.test_results["expected_failures"].append(True)
                        return True
                else:
                    print(f"✅ [预期失败测试] 正确失败！HTTP 端口没有响应 gRPC 连接")
                    self.test_results["expected_failures"].append(True)
                    return True
                    
            finally:
                sock.close()
            
        except socket.timeout:
            print(f"✅ [预期失败测试] 正确失败！gRPC 连接 HTTP 端口超时")
            self.test_results["expected_failures"].append(True)
            return True
        except socket.error as e:
            print(f"✅ [预期失败测试] 正确失败！gRPC 连接 HTTP 端口 socket 错误: {e}")
            self.test_results["expected_failures"].append(True)
            return True
        except Exception as e:
            print(f"⚠️ [预期失败测试] 测试异常: {e}")
            self.test_results["expected_failures"].append(False)
            return False
    
    async def run_comprehensive_test(self):
        """运行综合测试"""
        print(f"\n🧪 [测试] 开始分端口模式综合测试")
        
        # 等待服务器完全启动
        await asyncio.sleep(1)
        
        # 测试 gRPC 请求
        print(f"\n🔌 [测试] 测试 gRPC 协议...")
        await self.test_grpc_request(1, "Alice", "alice@example.com", "gRPC测试消息")
        await self.test_grpc_request(2, "Bob", "bob@example.com", "另一个gRPC消息")
        
        # 测试 HTTP GET 请求
        print(f"\n🌐 [测试] 测试 HTTP GET 协议...")
        self.test_http_get_request()
        
        # 测试 HTTP POST JSON 请求
        print(f"\n🌐 [测试] 测试 HTTP POST JSON 协议...")
        self.test_http_post_json_request(3, "Charlie", "charlie@example.com", "HTTP JSON测试消息")
        
        # 测试 HTTP POST 表单请求
        print(f"\n🌐 [测试] 测试 HTTP POST FORM 协议...")
        self.test_http_post_form_request(4, "Diana", "diana@example.com", "HTTP表单测试消息")
        
        # 再次测试 HTTP GET 以查看所有消息
        print(f"\n🌐 [测试] 最终 HTTP GET 测试...")
        self.test_http_get_request()
        
        # 测试预期失败场景
        print(f"\n🚫 [测试] 测试预期失败场景...")
        print(f"🔍 [测试] 验证端口分离是否正确工作")
        
        # 测试 HTTP 请求访问 gRPC 端口（应该失败）
        self.test_expected_failure_http_to_grpc_port()
        
        # 测试 gRPC 请求访问 HTTP 端口（应该失败）
        self.test_expected_failure_grpc_to_http_port()
        
        print(f"\n✅ [测试] 分端口模式综合测试完成（包括预期失败验证）")
    
    def print_test_summary(self):
        """打印测试总结"""
        print(f"\n📊 [测试总结] 分端口模式测试结果:")
        
        total_tests = 0
        passed_tests = 0
        
        for protocol, results in self.test_results.items():
            success_count = sum(results)
            total_count = len(results)
            total_tests += total_count
            passed_tests += success_count
            
            if total_count > 0:
                success_rate = (success_count / total_count) * 100
                if protocol == "expected_failures":
                    # 预期失败测试：成功意味着正确失败了
                    status = "✅" if success_count == total_count else "❌"
                    print(f"   {status} {protocol.upper().replace('_', ' ')}: {success_count}/{total_count} ({success_rate:.1f}%) - 预期失败测试")
                else:
                    status = "✅" if success_count == total_count else "❌"
                    print(f"   {status} {protocol.upper().replace('_', ' ')}: {success_count}/{total_count} ({success_rate:.1f}%)")
        
        if total_tests > 0:
            overall_rate = (passed_tests / total_tests) * 100
            overall_status = "✅" if passed_tests == total_tests else "❌"
            print(f"\n{overall_status} 总体成功率: {passed_tests}/{total_tests} ({overall_rate:.1f}%)")
            
            # 特别检查预期失败测试
            expected_failures = self.test_results.get("expected_failures", [])
            if expected_failures:
                expected_success = sum(expected_failures)
                expected_total = len(expected_failures)
                print(f"🔍 [端口分离验证] 预期失败测试: {expected_success}/{expected_total} 正确失败")
            
            if passed_tests == total_tests:
                print(f"🎉 [测试] 所有分端口模式测试通过！端口分离工作正常！")
            else:
                print(f"⚠️ [测试] 部分测试失败，请检查日志")
        else:
            print(f"❌ [测试] 没有运行任何测试")
    
    async def cleanup(self):
        """清理资源"""
        if self.grpc_client:
            try:
                # PyClientManager 使用 close() 方法而不是 cleanup()
                self.grpc_client.close()
                print(f"🧹 [客户端] gRPC 客户端已清理")
            except Exception as e:
                print(f"⚠️ [客户端] gRPC 客户端清理失败: {e}")


# ============================================================================
# 主函数和测试入口
# ============================================================================

def run_separated_ports_server():
    """运行分端口模式服务器"""
    server_manager = SeparatedPortsServerManager()
    server_manager.start()
    
    try:
        # 保持服务器运行
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print(f"\n🛑 [服务器] 收到停止信号")
        server_manager.stop()


async def run_separated_ports_client_test():
    """运行分端口模式客户端测试"""
    client = SeparatedPortsClient()
    
    try:
        # 初始化 gRPC 客户端
        await client.initialize_grpc()
        
        # 运行综合测试
        await client.run_comprehensive_test()
        
        # 打印测试总结
        client.print_test_summary()
        
        return True
        
    except Exception as e:
        print(f"❌ [测试] 客户端测试失败: {e}")
        return False
    finally:
        # 清理资源
        await client.cleanup()


async def main():
    """主函数 - 运行分端口模式演示"""
    print(f"🚀 [主函数] RAT Engine 分端口模式演示")
    print(f"📋 [主函数] 特性:")
    print(f"   🔌 gRPC 服务器: 独立端口 50055")
    print(f"   📡 HTTP 服务器: 独立端口 50054")
    print(f"   🎯 委托模式: 共享业务逻辑")
    print(f"   ⚡ 性能优化: 避免协议检测")
    
    # 启动服务器管理器
    server_manager = SeparatedPortsServerManager()
    
    try:
        # 启动服务器
        server_threads = server_manager.start()
        
        # 等待服务器完全启动
        await asyncio.sleep(3)
        
        # 运行客户端测试
        success = await run_separated_ports_client_test()
        
        # 等待一段时间以查看日志
        await asyncio.sleep(2)
        
        return success
        
    except Exception as e:
        print(f"❌ [主函数] 演示失败: {e}")
        return False
    finally:
        # 停止服务器
        server_manager.stop()
        print(f"🏁 [主函数] 分端口模式演示结束")


if __name__ == "__main__":
    print(f"🎯 [启动] RAT Engine 分端口模式示例")
    print(f"📖 [说明] 本示例展示 HTTP 和 gRPC 分端口部署")
    print(f"🔧 [架构] 基于现有混合模式，优化为分端口架构")
    print(f"⚡ [优势] 避免协议检测开销，提升性能")
    
    success = asyncio.run(main())
    exit(0 if success else 1)