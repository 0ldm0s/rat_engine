#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
混合模式示例 - gRPC + HTTP 多协议支持

基于现有示例的混合架构，实现多种通信协议的统一服务
使用 gRPC 一元请求 + HTTP GET/POST JSON + 表单提交

特性：
- 混合协议：同时支持 gRPC 和 HTTP
- 委托模式：业务逻辑与传输层分离
- 数据验证：完整的请求数据打印和验证
- 自动测试：端到端验证所有协议
"""

import asyncio
import threading
import time
import json
import httpx  # 使用 httpx 替代 requests，支持 HTTP/2
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
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据 - gRPC 使用"""
        success_str = "1" if self.success else "0"
        response_str = f"{success_str}|{self.user_id}|{self.message}|{self.server_timestamp}|{self.protocol}"
        return response_str.encode('utf-8')
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'ResponseMessage':
        """从字节数据反序列化 - gRPC 使用"""
        response_str = data.decode('utf-8')
        parts = response_str.split('|', 4)  # 最多分割4次
        if len(parts) != 5:
            raise ValueError(f"无效的响应格式: {response_str}")
        
        return cls(
            success=parts[0] == "1",
            user_id=int(parts[1]),
            message=parts[2],
            server_timestamp=float(parts[3]),
            protocol=parts[4]
        )
    
    def to_dict(self) -> Dict[str, Any]:
        """转换为字典 - HTTP JSON 使用"""
        return {
            "success": self.success,
            "user_id": self.user_id,
            "message": self.message,
            "server_timestamp": self.server_timestamp,
            "protocol": self.protocol
        }


# ============================================================================
# 服务器端委托处理器
# ============================================================================

class HybridServerHandler:
    """混合模式服务器端委托处理器 - 支持多种协议"""
    
    def __init__(self):
        self.request_count = 0
        self.grpc_count = 0
        self.http_count = 0
        self.start_time = time.time()
        self.messages_db = []  # 存储所有消息
        print(f"🎯 [服务器委托处理器] 混合模式初始化完成")
    
    def handle_grpc_request(self, request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
        """处理 gRPC 字节模式请求"""
        try:
            self.request_count += 1
            self.grpc_count += 1
            
            # 调试：打印接收到的原始数据
            print(f"\n🔍 [gRPC服务器调试] 接收到字节数据长度: {len(request_data)}")
            print(f"🔍 [gRPC服务器调试] 字节数据内容: {request_data.decode('utf-8', errors='replace')[:200]}...")
            
            # 解析 gRPC 请求数据
            user_message = UserMessage.from_bytes(request_data)
            print(f"📥 [gRPC服务器] 收到请求 #{self.request_count}: {user_message.message} (用户: {user_message.name})")
            
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
                protocol="gRPC"
            )
            
            response_bytes = response.to_bytes()
            print(f"📤 [gRPC服务器] 发送响应给用户 {user_message.user_id}: {response.message}")
            
            return response_bytes
            
        except Exception as e:
            print(f"❌ [gRPC服务器] 处理请求失败: {e}")
            error_response = ResponseMessage(
                success=False,
                user_id=0,
                message=f"gRPC处理失败: {str(e)}",
                server_timestamp=time.time(),
                protocol="gRPC"
            )
            return error_response.to_bytes()
    
    def handle_http_get_request(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """处理 HTTP GET JSON 请求"""
        try:
            self.request_count += 1
            self.http_count += 1
            
            print(f"\n🔍 [HTTP GET服务器调试] 接收到请求数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}")
            
            # 返回所有消息列表
            messages_list = [msg.to_dict() for msg in self.messages_db]
            
            response = {
                "success": True,
                "message": "HTTP GET处理成功",
                "protocol": "HTTP-GET",
                "server_timestamp": time.time(),
                "total_messages": len(self.messages_db),
                "total_requests": self.request_count,
                "grpc_requests": self.grpc_count,
                "http_requests": self.http_count,
                "messages": messages_list
            }
            
            print(f"📤 [HTTP GET服务器] 返回 {len(messages_list)} 条消息")
            return response
            
        except Exception as e:
            print(f"❌ [HTTP GET服务器] 处理请求失败: {e}")
            return {
                "success": False,
                "message": f"HTTP GET处理失败: {str(e)}",
                "protocol": "HTTP-GET",
                "server_timestamp": time.time()
            }
    
    def handle_http_post_json_request(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """处理 HTTP POST JSON 请求"""
        try:
            self.request_count += 1
            self.http_count += 1
            
            print(f"\n🔍 [HTTP POST JSON服务器调试] 接收到请求数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}")
            
            # 解析请求数据
            user_message = UserMessage.from_dict(request_data)
            print(f"📥 [HTTP POST JSON服务器] 收到请求 #{self.request_count}: {user_message.message} (用户: {user_message.name})")
            
            # 存储消息
            self.messages_db.append(user_message)
            
            # 模拟业务处理
            time.sleep(0.01)
            
            response = {
                "success": True,
                "user_id": user_message.user_id,
                "message": f"HTTP POST JSON处理成功: {user_message.message}",
                "protocol": "HTTP-POST-JSON",
                "server_timestamp": time.time()
            }
            
            print(f"📤 [HTTP POST JSON服务器] 发送响应给用户 {user_message.user_id}: {response['message']}")
            return response
            
        except Exception as e:
            print(f"❌ [HTTP POST JSON服务器] 处理请求失败: {e}")
            return {
                "success": False,
                "message": f"HTTP POST JSON处理失败: {str(e)}",
                "protocol": "HTTP-POST-JSON",
                "server_timestamp": time.time()
            }
    
    def handle_http_post_form_request(self, request_data: Dict[str, Any]) -> Dict[str, Any]:
        """处理 HTTP POST 表单请求"""
        try:
            self.request_count += 1
            self.http_count += 1
            
            print(f"\n🔍 [HTTP POST FORM服务器调试] 接收到表单数据: {json.dumps(request_data, ensure_ascii=False, indent=2)}")
            
            # 从表单数据创建用户消息
            user_message = UserMessage(
                user_id=int(request_data.get("user_id", 0)),
                name=request_data.get("name", ""),
                email=request_data.get("email", ""),
                message=request_data.get("message", ""),
                timestamp=time.time()
            )
            
            print(f"📥 [HTTP POST FORM服务器] 收到表单请求 #{self.request_count}: {user_message.message} (用户: {user_message.name})")
            
            # 存储消息
            self.messages_db.append(user_message)
            
            # 模拟业务处理
            time.sleep(0.01)
            
            response = {
                "success": True,
                "user_id": user_message.user_id,
                "message": f"HTTP POST FORM处理成功: {user_message.message}",
                "protocol": "HTTP-POST-FORM",
                "server_timestamp": time.time()
            }
            
            print(f"📤 [HTTP POST FORM服务器] 发送响应给用户 {user_message.user_id}: {response['message']}")
            return response
            
        except Exception as e:
            print(f"❌ [HTTP POST FORM服务器] 处理请求失败: {e}")
            return {
                "success": False,
                "message": f"HTTP POST FORM处理失败: {str(e)}",
                "protocol": "HTTP-POST-FORM",
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
            "requests_per_second": self.request_count / max(1, elapsed)
        }


# ============================================================================
# 混合模式服务器
# ============================================================================

class HybridModeServer:
    """混合模式服务器 - 同时支持 gRPC 和 HTTP"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 50053):
        self.host = host
        self.port = port
        self.app = RatApp("hybrid_mode_server")
        self.running = False
        self.handler = HybridServerHandler()  # 统一的委托处理器
        self.setup_routes()
    
    def setup_routes(self):
        """设置路由 - 混合协议支持"""
        
        # gRPC 一元请求路由
        @self.app.grpc_unary("/hybrid.HybridService/ProcessMessage")
        def handle_grpc_message(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
            """委托给 gRPC 处理器"""
            return self.handler.handle_grpc_request(request_data, metadata, context)
        
        # HTTP GET JSON 路由
        @self.app.json("/api/messages", methods=["GET"])
        def handle_get_messages(request_data: Dict[str, Any]) -> Dict[str, Any]:
            """委托给 HTTP GET 处理器"""
            return self.handler.handle_http_get_request(request_data)
        
        # HTTP POST JSON 路由 (raw)
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
                    "protocols": ["gRPC", "HTTP"]
                }
            }
        
        print(f"📝 [服务器] 已注册混合模式路由:")
        print(f"   🔌 gRPC: /hybrid.HybridService/ProcessMessage")
        print(f"   🌐 HTTP GET: /api/messages")
        print(f"   🌐 HTTP POST JSON: /api/messages")
        print(f"   🌐 HTTP POST FORM: /api/form-messages")
        print(f"   📊 HTTP STATS: /api/stats")
    
    def start(self):
        """启动混合模式服务器"""
        if self.running:
            return
        
        self.running = True
        print(f"🚀 [服务器] 混合模式启动在 {self.host}:{self.port}")
        print(f"🔧 [服务器] 支持协议: gRPC + HTTP (H2C)")
        
        try:
            # 启用 H2C 支持以实现单端口多协议
            self.app.configure_protocols(enable_h2c=True, enable_h2=True)
            print("✅ [服务器] H2C 和 HTTP/2 支持已启用")
            
            self.app.run(host=self.host, port=self.port)
        except Exception as e:
            print(f"❌ [服务器] 启动失败: {e}")
            self.running = False
            raise
    
    def stop(self):
        """停止服务器"""
        self.running = False
        print(f"🛑 [服务器] 已停止")


# ============================================================================
# 客户端测试器
# ============================================================================

class HybridModeClient:
    """混合模式客户端 - 测试所有协议"""
    
    def __init__(self, server_uri: str = "http://127.0.0.1:50053"):
        self.server_uri = server_uri
        self.grpc_client = None
        self.test_results = {
            "grpc": [],
            "http_get": [],
            "http_post_json": [],
            "http_post_form": []
        }
        
        print(f"🎯 [客户端] 混合模式初始化，目标服务器: {server_uri}")
    
    async def initialize_grpc(self):
        """初始化 gRPC 客户端"""
        try:
            self.grpc_client = PyClientManager()
            
            config = {
                "connect_timeout": 3000,
                "request_timeout": 5000,
                "max_idle_connections": 10,
                "enable_grpc": True,
                "grpc_user_agent": "RatEngine-Hybrid-Client/1.0",
                "development_mode": True
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
            
            print(f"\n📤 [gRPC客户端] 发送请求: {message} (用户: {name})")
            print(f"🔍 [gRPC客户端调试] 请求字节数据长度: {len(request_data)}")
            
            # 发送 gRPC 请求
            request_id = self.grpc_client.grpc_unary_delegated(
                uri=self.server_uri,
                service="hybrid.HybridService",
                method="ProcessMessage",
                data=request_data,
                metadata=None
            )
            
            print(f"🚀 [gRPC客户端] 请求已发送，请求ID: {request_id}")
            
            # 等待响应
            max_wait = 2.0
            start_time = time.time()
            
            while time.time() - start_time < max_wait:
                response_data = self.grpc_client.grpc_unary_delegated_receive(request_id)
                
                if response_data is not None:
                    response = ResponseMessage.from_bytes(bytes(response_data))
                    print(f"✅ [gRPC客户端] 收到响应: {response.message}")
                    self.test_results["grpc"].append(response.success)
                    return response.success
                
                await asyncio.sleep(0.1)
            
            print(f"⏰ [gRPC客户端] 请求超时: {request_id}")
            self.test_results["grpc"].append(False)
            return False
            
        except Exception as e:
            print(f"❌ [gRPC客户端] 请求失败: {e}")
            self.test_results["grpc"].append(False)
            return False
    
    def test_http_get_request(self) -> bool:
        """测试 HTTP GET JSON 请求"""
        try:
            url = f"{self.server_uri}/api/messages"
            
            print(f"\n📤 [HTTP GET客户端] 发送请求到: {url}")
            
            # 使用 httpx 发送 HTTP/2 请求（支持 H2C）
            with httpx.Client(http2=True) as client:
                response = client.get(url, timeout=5)
            
            print(f"🔍 [HTTP GET客户端调试] 响应状态码: {response.status_code}")
            print(f"🔍 [HTTP GET客户端调试] 响应头: {dict(response.headers)}")
            print(f"🔍 [HTTP GET客户端调试] HTTP 版本: {response.http_version}")
            
            if response.status_code == 200:
                response_data = response.json()
                print(f"✅ [HTTP GET客户端] 收到响应: {response_data.get('message', 'N/A')}")
                print(f"📊 [HTTP GET客户端] 消息总数: {response_data.get('total_messages', 0)}")
                self.test_results["http_get"].append(response_data.get("success", False))
                return response_data.get("success", False)
            else:
                print(f"❌ [HTTP GET客户端] 请求失败，状态码: {response.status_code}")
                self.test_results["http_get"].append(False)
                return False
            
        except Exception as e:
            print(f"❌ [HTTP GET客户端] 请求失败: {e}")
            self.test_results["http_get"].append(False)
            return False
    
    def test_http_post_json_request(self, user_id: int, name: str, email: str, message: str) -> bool:
        """测试 HTTP POST JSON 请求 (raw)"""
        try:
            url = f"{self.server_uri}/api/messages"
            
            # 创建 JSON 数据
            json_data = {
                "user_id": user_id,
                "name": name,
                "email": email,
                "message": message,
                "timestamp": time.time()
            }
            
            print(f"\n📤 [HTTP POST JSON客户端] 发送请求到: {url}")
            print(f"🔍 [HTTP POST JSON客户端调试] 请求数据: {json.dumps(json_data, ensure_ascii=False, indent=2)}")
            
            # 使用 httpx 发送 HTTP POST JSON 请求（支持 HTTP/2）
            with httpx.Client(http2=True) as client:
                response = client.post(
                    url,
                    json=json_data,
                    headers={"Content-Type": "application/json"},
                    timeout=5
                )
            
            print(f"🔍 [HTTP POST JSON客户端调试] 响应状态码: {response.status_code}")
            print(f"🔍 [HTTP POST JSON客户端调试] HTTP 版本: {response.http_version}")
            
            if response.status_code == 200:
                response_data = response.json()
                print(f"✅ [HTTP POST JSON客户端] 收到响应: {response_data.get('message', 'N/A')}")
                self.test_results["http_post_json"].append(response_data.get("success", False))
                return response_data.get("success", False)
            else:
                print(f"❌ [HTTP POST JSON客户端] 请求失败，状态码: {response.status_code}")
                self.test_results["http_post_json"].append(False)
                return False
            
        except Exception as e:
            print(f"❌ [HTTP POST JSON客户端] 请求失败: {e}")
            self.test_results["http_post_json"].append(False)
            return False
    
    def test_http_post_form_request(self, user_id: int, name: str, email: str, message: str) -> bool:
        """测试 HTTP POST 表单请求"""
        try:
            url = f"{self.server_uri}/api/form-messages"
            
            # 创建表单数据
            form_data = {
                "user_id": str(user_id),
                "name": name,
                "email": email,
                "message": message
            }
            
            print(f"\n📤 [HTTP POST FORM客户端] 发送请求到: {url}")
            print(f"🔍 [HTTP POST FORM客户端调试] 表单数据: {json.dumps(form_data, ensure_ascii=False, indent=2)}")
            
            # 使用 httpx 发送 HTTP POST 表单请求（支持 HTTP/2）
            with httpx.Client(http2=True) as client:
                response = client.post(
                    url,
                    data=form_data,
                    headers={"Content-Type": "application/x-www-form-urlencoded"},
                    timeout=5
                )
            
            print(f"🔍 [HTTP POST FORM客户端调试] 响应状态码: {response.status_code}")
            print(f"🔍 [HTTP POST FORM客户端调试] HTTP 版本: {response.http_version}")
            
            if response.status_code == 200:
                response_data = response.json()
                print(f"✅ [HTTP POST FORM客户端] 收到响应: {response_data.get('message', 'N/A')}")
                self.test_results["http_post_form"].append(response_data.get("success", False))
                return response_data.get("success", False)
            else:
                print(f"❌ [HTTP POST FORM客户端] 请求失败，状态码: {response.status_code}")
                self.test_results["http_post_form"].append(False)
                return False
            
        except Exception as e:
            print(f"❌ [HTTP POST FORM客户端] 请求失败: {e}")
            self.test_results["http_post_form"].append(False)
            return False
    
    async def run_comprehensive_test(self):
        """运行综合测试 - 测试所有协议"""
        print(f"\n🧪 [客户端] 开始混合模式综合测试")
        
        # 测试用例数据
        test_cases = [
            {"user_id": 3001, "name": "张三", "email": "zhangsan@test.com", "message": "gRPC测试消息"},
            {"user_id": 3002, "name": "李四", "email": "lisi@test.com", "message": "HTTP JSON测试消息"},
            {"user_id": 3003, "name": "王五", "email": "wangwu@test.com", "message": "HTTP表单测试消息"},
        ]
        
        # 1. 测试 gRPC 请求
        print(f"\n--- 测试 gRPC 一元请求 ---")
        for case in test_cases:
            await self.test_grpc_request(**case)
            await asyncio.sleep(0.5)
        
        # 2. 测试 HTTP POST JSON 请求
        print(f"\n--- 测试 HTTP POST JSON 请求 ---")
        for case in test_cases:
            self.test_http_post_json_request(**case)
            time.sleep(0.5)
        
        # 3. 测试 HTTP POST 表单请求
        print(f"\n--- 测试 HTTP POST 表单请求 ---")
        for case in test_cases:
            self.test_http_post_form_request(**case)
            time.sleep(0.5)
        
        # 4. 测试 HTTP GET 请求
        print(f"\n--- 测试 HTTP GET 请求 ---")
        for _ in range(2):  # 测试2次
            self.test_http_get_request()
            time.sleep(0.5)
        
        # 输出测试结果统计
        self.print_test_summary()
    
    def print_test_summary(self):
        """打印测试结果摘要"""
        print(f"\n📊 [客户端] 混合模式测试结果摘要:")
        
        total_tests = 0
        total_success = 0
        
        for protocol, results in self.test_results.items():
            success_count = sum(results)
            total_count = len(results)
            success_rate = (success_count / max(1, total_count)) * 100
            
            total_tests += total_count
            total_success += success_count
            
            print(f"   {protocol.upper()}: {success_count}/{total_count} ({success_rate:.1f}%)")
        
        overall_rate = (total_success / max(1, total_tests)) * 100
        print(f"   总体: {total_success}/{total_tests} ({overall_rate:.1f}%)")
        
        return overall_rate >= 80  # 80% 成功率视为通过
    
    async def cleanup(self):
        """清理资源"""
        if self.grpc_client:
            self.grpc_client = None
        print(f"🧹 [客户端] 混合模式资源清理完成")


# ============================================================================
# 主程序入口
# ============================================================================

def run_hybrid_server():
    """运行混合模式服务器"""
    server = HybridModeServer()
    server.start()


async def run_hybrid_client_test():
    """运行混合模式客户端测试"""
    # 等待服务器启动
    await asyncio.sleep(1.5)
    
    client = HybridModeClient()
    
    try:
        await client.initialize_grpc()
        await client.run_comprehensive_test()
        
        success = client.print_test_summary()
        
        if success:
            print(f"\n🎉 [测试] 混合模式测试通过！")
        else:
            print(f"\n❌ [测试] 混合模式测试失败！")
        
        return success
        
    finally:
        await client.cleanup()


async def main():
    """主函数 - 并发运行服务器和客户端"""
    print(f"🚀 启动混合模式测试 - gRPC + HTTP 多协议支持")
    print(f"📋 特性: gRPC一元 + HTTP GET/POST JSON + 表单提交")
    print(f"🔧 端口: 50053 (避免与其他示例冲突)")
    print(f"🌐 HTTP客户端: requests + H2C模式")
    print(f"="*60)
    
    try:
        # 在线程中运行服务器
        server_thread = threading.Thread(target=run_hybrid_server, daemon=True)
        server_thread.start()
        
        # 运行客户端测试
        success = await run_hybrid_client_test()
        
        print(f"\n🏁 混合模式测试完成，结果: {'成功' if success else '失败'}")
        return success
        
    except Exception as e:
        print(f"❌ 混合模式测试执行失败: {e}")
        return False


if __name__ == "__main__":
    # 运行混合模式测试
    success = asyncio.run(main())
    exit(0 if success else 1)