#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
gRPC 一元请求客户端示例 - 字节模式

基于一元请求示例的委托模式，实现字节模式的直接数据传输
使用 gRPC + 直接字节发送，展示委托模式的责权分离设计

特性：
- 委托模式：业务逻辑与传输层分离
- 字节模式：直接发送字节数据，无需 JSON 序列化
- 短超时：快速定位问题
- 自动测试：完整的端到端验证
"""

import asyncio
import threading
import time
from dataclasses import dataclass
from typing import Dict, Any, Optional
from rat_engine import RatApp, PyClientManager


# ============================================================================
# 数据结构定义（字节模式）
# ============================================================================

@dataclass
class ByteMessage:
    """字节消息数据结构"""
    user_id: int
    content: str
    timestamp: float
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据 - 简单格式"""
        # 使用简单的分隔符格式：user_id|content|timestamp
        message_str = f"{self.user_id}|{self.content}|{self.timestamp}"
        return message_str.encode('utf-8')
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'ByteMessage':
        """从字节数据反序列化"""
        message_str = data.decode('utf-8')
        parts = message_str.split('|', 2)  # 最多分割2次
        if len(parts) != 3:
            raise ValueError(f"无效的消息格式: {message_str}")
        
        return cls(
            user_id=int(parts[0]),
            content=parts[1],
            timestamp=float(parts[2])
        )


@dataclass
class ByteResponse:
    """字节响应数据结构"""
    success: bool
    user_id: int
    message: str
    server_timestamp: float
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据 - 简单格式"""
        # 使用简单的分隔符格式：success|user_id|message|timestamp
        success_str = "1" if self.success else "0"
        response_str = f"{success_str}|{self.user_id}|{self.message}|{self.server_timestamp}"
        return response_str.encode('utf-8')
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'ByteResponse':
        """从字节数据反序列化"""
        response_str = data.decode('utf-8')
        parts = response_str.split('|', 3)  # 最多分割3次
        if len(parts) != 4:
            raise ValueError(f"无效的响应格式: {response_str}")
        
        return cls(
            success=parts[0] == "1",
            user_id=int(parts[1]),
            message=parts[2],
            server_timestamp=float(parts[3])
        )


# ============================================================================
# 服务器端委托处理器
# ============================================================================

class ServerUnaryBytesHandler:
    """服务器端一元请求字节模式委托处理器 - 责权分离的委托模式"""
    
    def __init__(self):
        self.request_count = 0
        self.start_time = time.time()
        print(f"🎯 [服务器委托处理器] 字节模式初始化完成")
    
    def handle_bytes_request(self, request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
        """处理字节模式用户请求"""
        try:
            self.request_count += 1
            
            # 调试：打印接收到的原始数据
            print(f"🔍 [服务器调试] 接收到字节数据长度: {len(request_data)}")
            print(f"🔍 [服务器调试] 字节数据前50字符: {request_data[:50]}")
            print(f"🔍 [服务器调试] 字节数据内容: {request_data.decode('utf-8', errors='replace')[:100]}...")
            
            # 解析字节请求数据
            byte_request = ByteMessage.from_bytes(request_data)
            print(f"📥 [服务器] 收到字节请求 #{self.request_count}: {byte_request.content} (用户ID: {byte_request.user_id})")
            
            # 模拟业务处理
            time.sleep(0.01)  # 模拟处理时间
            
            # 构造字节响应
            response = ByteResponse(
                success=True,
                user_id=byte_request.user_id,
                message=f"字节处理成功: {byte_request.content}",
                server_timestamp=time.time()
            )
            
            response_bytes = response.to_bytes()
            print(f"📤 [服务器] 发送字节响应给用户 {byte_request.user_id}: {response.message}")
            print(f"🔍 [服务器调试] 响应字节数据长度: {len(response_bytes)}")
            print(f"🔍 [服务器调试] 响应字节内容: {response_bytes.decode('utf-8', errors='replace')[:100]}...")
            
            return response_bytes
            
        except Exception as e:
            print(f"❌ [服务器] 处理字节请求失败: {e}")
            print(f"🔍 [服务器调试] 异常详情: {type(e).__name__}: {str(e)}")
            error_response = ByteResponse(
                success=False,
                user_id=0,
                message=f"字节处理失败: {str(e)}",
                server_timestamp=time.time()
            )
            error_bytes = error_response.to_bytes()
            print(f"🔍 [服务器调试] 错误响应字节数据长度: {len(error_bytes)}")
            return error_bytes


# ============================================================================
# 自动测试服务器
# ============================================================================

class AutoTestBytesServer:
    """自动测试字节模式服务器 - 委托模式架构"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 50052):
        self.host = host
        self.port = port
        self.app = RatApp("grpc_unary_bytes_test_server")
        
        # 注意：队列桥接适配器现在由 RatApp 自动初始化
        # 当检测到 gRPC 路由注册时，RatApp.run() 会自动调用 initialize_queue_bridge()
        
        self.running = False
        self.handler_registry = {}  # 委托处理器注册表
        self.setup_routes()
    
    def setup_routes(self):
        """设置路由 - 委托模式"""
        # 创建委托处理器
        bytes_handler = ServerUnaryBytesHandler()
        self.handler_registry["bytes_service"] = bytes_handler
        
        # 注册一元请求路由 - 字节模式
        @self.app.grpc_unary("/bytes.BytesService/ProcessBytes")
        def handle_process_bytes(request_data: bytes, metadata: Dict[str, Any], context: Dict[str, Any]) -> bytes:
            """委托给专门的字节处理器"""
            return bytes_handler.handle_bytes_request(request_data, metadata, context)
        
        print(f"📝 [服务器] 已注册 gRPC 一元字节路由: /bytes.BytesService/ProcessBytes")
    
    def start(self):
        """启动服务器"""
        if self.running:
            return
        
        self.running = True
        print(f"🚀 [服务器] 字节模式启动在 {self.host}:{self.port}")
        
        try:
            # 启用 H2C 支持以实现单端口多协议（应用层级配置）
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
# 客户端委托处理器
# ============================================================================

class ClientUnaryBytesHandler:
    """客户端一元请求字节模式委托处理器 - 责权分离的委托模式"""
    
    def __init__(self, user_name: str):
        self.user_name = user_name
        self.request_count = 0
        self.success_count = 0
        self.error_count = 0
        self.start_time = time.time()
        self.responses = []  # 存储响应
        
        print(f"🎯 [客户端委托处理器] 创建字节处理器: {user_name}")
    
    def create_bytes_request(self, user_id: int, content: str) -> ByteMessage:
        """创建字节用户请求"""
        self.request_count += 1
        return ByteMessage(
            user_id=user_id,
            content=content,
            timestamp=time.time()
        )
    
    def handle_bytes_response(self, response_data: bytes) -> bool:
        """处理字节响应数据"""
        try:
            # 调试：打印接收到的原始响应数据
            print(f"🔍 [客户端调试] 接收到响应字节数据长度: {len(response_data)}")
            print(f"🔍 [客户端调试] 响应字节数据前50字符: {response_data[:50]}")
            print(f"🔍 [客户端调试] 响应字节内容: {response_data.decode('utf-8', errors='replace')[:100]}...")
            
            response = ByteResponse.from_bytes(response_data)
            self.responses.append(response)
            
            if response.success:
                self.success_count += 1
                print(f"✅ [客户端] 收到成功字节响应: {response.message}")
            else:
                self.error_count += 1
                print(f"❌ [客户端] 收到错误字节响应: {response.message}")
            
            return response.success
            
        except Exception as e:
            self.error_count += 1
            print(f"❌ [客户端] 解析字节响应失败: {e}")
            print(f"🔍 [客户端调试] 异常详情: {type(e).__name__}: {str(e)}")
            print(f"🔍 [客户端调试] 原始响应字节数据: {response_data}")
            return False
    
    def get_stats(self) -> Dict[str, Any]:
        """获取统计信息"""
        elapsed = time.time() - self.start_time
        return {
            "user_name": self.user_name,
            "request_count": self.request_count,
            "success_count": self.success_count,
            "error_count": self.error_count,
            "elapsed_time": elapsed,
            "success_rate": self.success_count / max(1, self.request_count) * 100,
            "mode": "bytes"
        }


# ============================================================================
# 自动测试客户端
# ============================================================================

class AutoTestBytesClient:
    """自动测试字节模式客户端 - 委托模式架构"""
    
    def __init__(self, server_uri: str = "http://127.0.0.1:50052"):
        self.server_uri = server_uri
        self.client = None
        self.handlers = {}  # 委托处理器注册表
        
        print(f"🎯 [客户端] 字节模式初始化，目标服务器: {server_uri}")
    
    async def initialize(self):
        """初始化客户端"""
        try:
            # 创建客户端管理器（短超时）
            self.client = PyClientManager()
            
            # 创建客户端配置字典（超时时间以毫秒为单位）
            config = {
                "connect_timeout": 3000,  # 3 秒 = 3000 毫秒，快速定位问题
                "request_timeout": 5000,  # 5 秒 = 5000 毫秒
                "max_idle_connections": 10,
                "enable_grpc": True,  # 启用gRPC客户端
                "grpc_user_agent": "RatEngine-Python-BytesClient/1.0",
                "development_mode": True  # 开发模式，跳过证书验证
            }
            
            # 初始化客户端
            self.client.initialize(config)
            print(f"✅ [客户端] 字节模式客户端管理器初始化完成")
            
        except Exception as e:
            print(f"❌ [客户端] 初始化失败: {e}")
            raise
    
    async def send_bytes_unary_request(self, handler: ClientUnaryBytesHandler, user_id: int, content: str) -> bool:
        """发送字节模式一元请求 - 委托模式"""
        try:
            # 创建字节请求
            request = handler.create_bytes_request(user_id, content)
            request_data = request.to_bytes()
            
            print(f"📤 [客户端] 发送字节一元请求: {request.content} (用户ID: {request.user_id})")
            print(f"🔍 [客户端调试] 请求字节数据长度: {len(request_data)}")
            print(f"🔍 [客户端调试] 请求字节内容: {request_data.decode('utf-8', errors='replace')[:100]}...")
            
            # 使用委托模式发送字节请求
            request_id = self.client.grpc_unary_delegated(
                uri=self.server_uri,
                service="bytes.BytesService",
                method="ProcessBytes",
                data=request_data,
                metadata=None
            )
            
            print(f"🚀 [客户端] 字节一元请求已发送，请求ID: {request_id}")
            
            # 等待响应（短超时）
            max_wait = 2.0  # 2秒最大等待时间
            start_time = time.time()
            
            while time.time() - start_time < max_wait:
                response_data = self.client.grpc_unary_delegated_receive(request_id)
                
                if response_data is not None:
                    # 委托给处理器处理响应
                    success = handler.handle_bytes_response(bytes(response_data))
                    return success
                
                await asyncio.sleep(0.1)  # 100ms 轮询间隔
            
            print(f"⏰ [客户端] 字节请求超时: {request_id}")
            return False
            
        except Exception as e:
            print(f"❌ [客户端] 发送字节请求失败: {e}")
            return False
    
    async def run_bytes_test_sequence(self):
        """运行字节模式测试序列"""
        print(f"🧪 [客户端] 开始字节模式测试序列")
        
        # 创建委托处理器
        handler = ClientUnaryBytesHandler("BytesTestUser")
        self.handlers["bytes_test_user"] = handler
        
        # 发送多个字节请求
        test_cases = [
            {"user_id": 2001, "content": "字节消息测试 #1", "description": "第一个字节消息"},
            {"user_id": 2002, "content": "字节消息测试 #2", "description": "第二个字节消息"},
            {"user_id": 2003, "content": "字节消息测试 #3", "description": "第三个字节消息"},
            {"user_id": 2004, "content": "特殊字符测试: @#$%^&*()", "description": "特殊字符字节消息"},
            {"user_id": 2005, "content": "长消息测试: " + "A" * 100, "description": "长字节消息"},
        ]
        
        success_count = 0
        
        for i, test_case in enumerate(test_cases, 1):
            print(f"\n--- 字节测试用例 {i}/{len(test_cases)}: {test_case['description']} ---")
            
            success = await self.send_bytes_unary_request(
                handler, 
                test_case["user_id"], 
                test_case["content"]
            )
            if success:
                success_count += 1
            
            # 短暂间隔
            await asyncio.sleep(0.5)
        
        # 输出统计信息
        stats = handler.get_stats()
        print(f"\n📊 [客户端] 字节模式测试完成统计:")
        print(f"   用户: {stats['user_name']}")
        print(f"   模式: {stats['mode']}")
        print(f"   总请求: {stats['request_count']}")
        print(f"   成功: {stats['success_count']}")
        print(f"   失败: {stats['error_count']}")
        print(f"   成功率: {stats['success_rate']:.1f}%")
        print(f"   耗时: {stats['elapsed_time']:.2f}秒")
        
        return success_count == len(test_cases)
    
    async def cleanup(self):
        """清理资源"""
        if self.client:
            # 注意：RatGrpcClient 可能没有显式的 close 方法
            # 依赖 Python 的垃圾回收机制
            self.client = None
        print(f"🧹 [客户端] 字节模式资源清理完成")


# ============================================================================
# 主程序入口
# ============================================================================

def run_bytes_server():
    """运行字节模式服务器"""
    server = AutoTestBytesServer()
    server.start()


async def run_bytes_client_test():
    """运行字节模式客户端测试"""
    # 等待服务器启动
    await asyncio.sleep(1.0)
    
    client = AutoTestBytesClient()
    
    try:
        await client.initialize()
        success = await client.run_bytes_test_sequence()
        
        if success:
            print(f"\n🎉 [测试] 所有字节模式测试用例通过！")
        else:
            print(f"\n❌ [测试] 部分字节模式测试用例失败！")
        
        return success
        
    finally:
        await client.cleanup()


async def main():
    """主函数 - 并发运行服务器和客户端"""
    print(f"🚀 启动 gRPC 一元请求字节模式测试")
    print(f"📋 特性: 委托模式 + 字节传输 + 直接序列化")
    print(f"⏱️  超时设置: 短超时快速定位问题")
    print(f"🔧 端口: 50052 (避免与其他示例冲突)")
    print(f"="*60)
    
    try:
        # 在线程中运行服务器
        server_thread = threading.Thread(target=run_bytes_server, daemon=True)
        server_thread.start()
        
        # 运行客户端测试
        success = await run_bytes_client_test()
        
        print(f"\n🏁 字节模式测试完成，结果: {'成功' if success else '失败'}")
        return success
        
    except Exception as e:
        print(f"❌ 字节模式测试执行失败: {e}")
        return False


if __name__ == "__main__":
    # 运行字节模式测试
    success = asyncio.run(main())
    exit(0 if success else 1)