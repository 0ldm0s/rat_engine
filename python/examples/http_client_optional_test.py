#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
HTTP客户端可选功能测试示例

验证:
1. HTTP客户端真正可选
2. HTTP/1.1强制模式正常工作
3. 外部HTTP连接正常
"""

import sys
import time
import threading

def test_http_client_enabled():
    """测试HTTP客户端启用时的功能"""
    print("🔍 测试1: HTTP客户端启用模式")
    print("-" * 50)
    
    try:
        import rat_engine
        client = rat_engine.PyClientManager()
        
        # 启用HTTP客户端，强制HTTP/1.1
        config = {
            "connect_timeout": 5000,
            "request_timeout": 10000,
            "max_idle_connections": 5,
            "enable_http": True,  # 启用HTTP客户端
            "enable_grpc": False,
            "enable_compression": False,
            "http2_only": False,
            "http1_only": True,  # 强制HTTP/1.1模式
            "development_mode": False,
            "user_agent": "curl/7.88.1",  # 模拟curl UA避免被拦截
            "http_user_agent": "curl/7.88.1"
        }
        
        print(f"📋 配置: enable_http=True, http1_only=True")
        client.initialize(config)
        print("✅ 客户端初始化成功")
        
        # 测试HTTP GET请求
        test_url = "http://myip.ipip.net"
        headers = {"User-Agent": "curl/7.88.1"}
        
        print(f"📡 发送HTTP GET请求: {test_url}")
        start_time = time.time()
        
        response = client.http_get(test_url, headers)
        elapsed = time.time() - start_time
        
        print(f"⏱️  耗时: {elapsed:.3f}秒")
        
        if response:
            status = response.get("status", 0)
            body = response.get("body", b"")
            print(f"📊 响应状态: {status}")
            print(f"📄 响应体大小: {len(body)} bytes")
            print(f"📝 响应内容: {body.decode('utf-8', errors='ignore')}")
            
            if status == 200:
                print("✅ HTTP客户端功能正常！")
                return True
            else:
                print(f"⚠️  非200状态: {status}")
                return False
        else:
            print("❌ 响应为None")
            return False
            
    except Exception as e:
        elapsed = time.time() - start_time if 'start_time' in locals() else 0
        print(f"❌ 请求异常 (耗时{elapsed:.3f}秒): {e}")
        import traceback
        traceback.print_exc()
        return False

def test_http_client_disabled():
    """测试HTTP客户端禁用时的功能"""
    print("\n🔍 测试2: HTTP客户端禁用模式")
    print("-" * 50)
    
    try:
        import rat_engine
        client = rat_engine.PyClientManager()
        
        # 禁用HTTP客户端
        config = {
            "connect_timeout": 5000,
            "request_timeout": 10000,
            "max_idle_connections": 5,
            "enable_http": False,  # 禁用HTTP客户端
            "enable_grpc": False,
            "enable_compression": False,
            "http2_only": False,
            "http1_only": True,
            "development_mode": False,
            "user_agent": "curl/7.88.1",
            "http_user_agent": "curl/7.88.1"
        }
        
        print(f"📋 配置: enable_http=False")
        client.initialize(config)
        print("✅ 客户端初始化成功")
        
        # 测试HTTP请求（应该失败）
        test_url = "http://myip.ipip.net"
        headers = {"User-Agent": "curl/7.88.1"}
        
        print(f"📡 尝试HTTP请求（应该失败）: {test_url}")
        start_time = time.time()
        
        try:
            response = client.http_get(test_url, headers)
            print(f"❌ 意外成功: {response}")
            return False
        except Exception as e:
            elapsed = time.time() - start_time
            print(f"✅ 预期的失败 (耗时{elapsed:.3f}秒): {e}")
            return True
        
    except Exception as e:
        print(f"❌ 测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_http_post_functionality():
    """测试HTTP POST功能"""
    print("\n🔍 测试3: HTTP POST功能")
    print("-" * 50)
    
    try:
        import rat_engine
        client = rat_engine.PyClientManager()
        
        config = {
            "connect_timeout": 5000,
            "request_timeout": 10000,
            "max_idle_connections": 5,
            "enable_http": True,
            "enable_grpc": False,
            "enable_compression": False,
            "http2_only": False,
            "http1_only": True,
            "development_mode": False,
            "user_agent": "curl/7.88.1",
            "http_user_agent": "curl/7.88.1"
        }
        
        client.initialize(config)
        print("✅ 客户端初始化成功")
        
        # 测试HTTP POST请求
        test_url = "http://httpbin.org/post"
        headers = {"User-Agent": "curl/7.88.1", "Content-Type": "application/json"}
        post_data = '{"test": "data", "client": "rat_engine"}'
        
        print(f"📡 发送HTTP POST请求: {test_url}")
        print(f"📄 请求数据: {post_data}")
        start_time = time.time()
        
        response = client.http_post(test_url, post_data.encode('utf-8'), headers)
        elapsed = time.time() - start_time
        
        print(f"⏱️  耗时: {elapsed:.3f}秒")
        
        if response:
            status = response.get("status", 0)
            body = response.get("body", b"")
            print(f"📊 响应状态: {status}")
            print(f"📄 响应体大小: {len(body)} bytes")
            
            if status == 200:
                print("✅ HTTP POST功能正常！")
                return True
            else:
                print(f"⚠️  非200状态: {status}")
                return False
        else:
            print("❌ 响应为None")
            return False
            
    except Exception as e:
        elapsed = time.time() - start_time if 'start_time' in locals() else 0
        print(f"❌ 请求异常 (耗时{elapsed:.3f}秒): {e}")
        return False

def main():
    """主函数"""
    print("🚀 开始HTTP客户端可选功能测试")
    print("=" * 60)
    
    # 运行测试
    test1_result = test_http_client_enabled()
    test2_result = test_http_client_disabled()
    test3_result = test_http_post_functionality()
    
    # 输出结果汇总
    print("\n" + "=" * 60)
    print("📊 测试结果汇总:")
    print(f"1. HTTP客户端启用: {'✅ 通过' if test1_result else '❌ 失败'}")
    print(f"2. HTTP客户端禁用: {'✅ 通过' if test2_result else '❌ 失败'}")
    print(f"3. HTTP POST功能: {'✅ 通过' if test3_result else '❌ 失败'}")
    
    if test1_result and test2_result and test3_result:
        print("\n🎉 所有测试通过！HTTP客户端可选功能正常工作")
        print("✅ HTTP/1.1强制模式正常")
        print("✅ 外部HTTP连接正常")
        return True
    else:
        print("\n❌ 部分测试失败，需要修复")
        return False

if __name__ == "__main__":
    success = main()
    exit(0 if success else 1)