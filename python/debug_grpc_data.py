#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
调试 gRPC 数据传输问题
"""

import json
from dataclasses import dataclass, asdict

@dataclass
class UserRequest:
    """用户请求数据结构"""
    user_id: int
    name: str
    email: str
    timestamp: float
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据"""
        json_str = json.dumps(asdict(self))
        print(f"🔍 [调试] JSON 字符串: {json_str}")
        json_bytes = json_str.encode('utf-8')
        print(f"🔍 [调试] JSON 字节长度: {len(json_bytes)}")
        print(f"🔍 [调试] JSON 字节内容: {json_bytes}")
        return json_bytes
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'UserRequest':
        """从字节数据反序列化"""
        print(f"🔍 [调试] 接收到字节长度: {len(data)}")
        print(f"🔍 [调试] 接收到字节内容: {data}")
        try:
            decoded_str = data.decode('utf-8')
            print(f"🔍 [调试] 解码后字符串: {decoded_str}")
            decoded = json.loads(decoded_str)
            print(f"🔍 [调试] JSON 解析结果: {decoded}")
            return cls(**decoded)
        except Exception as e:
            print(f"❌ [调试] 解析失败: {e}")
            print(f"❌ [调试] 原始数据: {data}")
            raise

@dataclass
class UserResponse:
    """用户响应数据结构"""
    success: bool
    user_id: int
    message: str
    server_timestamp: float
    
    def to_bytes(self) -> bytes:
        """序列化为字节数据"""
        json_str = json.dumps(asdict(self))
        print(f"🔍 [调试] 响应 JSON 字符串: {json_str}")
        json_bytes = json_str.encode('utf-8')
        print(f"🔍 [调试] 响应 JSON 字节长度: {len(json_bytes)}")
        print(f"🔍 [调试] 响应 JSON 字节内容: {json_bytes}")
        return json_bytes
    
    @classmethod
    def from_bytes(cls, data: bytes) -> 'UserResponse':
        """从字节数据反序列化"""
        print(f"🔍 [调试] 响应接收到字节长度: {len(data)}")
        print(f"🔍 [调试] 响应接收到字节内容: {data}")
        print(f"🔍 [调试] 响应字节前20个: {data[:20]}")
        try:
            decoded_str = data.decode('utf-8')
            print(f"🔍 [调试] 响应解码后字符串: {decoded_str}")
            decoded = json.loads(decoded_str)
            print(f"🔍 [调试] 响应 JSON 解析结果: {decoded}")
            return cls(**decoded)
        except Exception as e:
            print(f"❌ [调试] 响应解析失败: {e}")
            print(f"❌ [调试] 响应原始数据: {data}")
            # 尝试以十六进制显示
            print(f"❌ [调试] 响应十六进制: {data.hex()}")
            raise

def test_serialization():
    """测试序列化和反序列化"""
    print("🧪 [测试] 开始序列化测试")
    
    # 测试请求
    request = UserRequest(
        user_id=123,
        name="TestUser",
        email="test@example.com",
        timestamp=1234567890.0
    )
    
    request_bytes = request.to_bytes()
    restored_request = UserRequest.from_bytes(request_bytes)
    print(f"✅ [测试] 请求序列化测试成功: {restored_request}")
    
    # 测试响应
    response = UserResponse(
        success=True,
        user_id=123,
        message="测试成功",
        server_timestamp=1234567890.0
    )
    
    response_bytes = response.to_bytes()
    restored_response = UserResponse.from_bytes(response_bytes)
    print(f"✅ [测试] 响应序列化测试成功: {restored_response}")

if __name__ == "__main__":
    test_serialization()