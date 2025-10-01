#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
测试通过 PyO3 集成的 HTTP 客户端访问 myip.ipip.net
验证 HTTP 集成是否成功
"""

import sys
import os

# 添加 rat_engine 模块路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

try:
    import rat_engine
    print("✅ 成功导入 rat_engine 模块")
except ImportError as e:
    print(f"❌ 导入 rat_engine 模块失败: {e}")
    print("请确保已经运行 'maturin develop' 构建 Python 绑定")
    sys.exit(1)

def test_myip_access():
    """测试访问 myip.ipip.net 获取 IP 信息"""
    print("\n🧪 开始测试 HTTP 客户端集成...")
    
    # 创建客户端管理器
    client = rat_engine.PyClientManager()
    
    # 配置客户端（HTTP 相关配置）
    config = {
        "grpc_server_uri": "http://127.0.0.1:50051",  # gRPC 配置（必需但不使用）
        "http_server_uri": "https://myip.ipip.net",   # HTTP 基础 URL
        "grpc_connect_timeout": 5000,                 # gRPC 连接超时（毫秒）
        "grpc_request_timeout": 30000,                # gRPC 请求超时（毫秒）
        "http_connect_timeout": 10000,                # HTTP 连接超时（毫秒）
        "http_request_timeout": 30000,                # HTTP 请求超时（毫秒）
        "max_idle_connections": 10,                   # 最大空闲连接数
        "http2_only": False,                          # 允许 HTTP/1.1 和 HTTP/2
        "enable_compression": False,                  # 禁用压缩以简化测试
        "grpc_user_agent": "rat-engine-test/1.0",     # gRPC User-Agent
        "http_user_agent": "rat-engine-test/1.0",     # HTTP User-Agent
        "development_mode": False,                    # 非开发模式（严格证书验证）
    }
    
    try:
        # 初始化客户端
        print("🔧 初始化客户端...")
        client.initialize(config)
        print("✅ 客户端初始化成功")
        
        # 发送 HTTP GET 请求到 myip.ipip.net
        print("📡 发送 HTTP GET 请求到 https://myip.ipip.net/ ...")
        
        # 设置请求头
        headers = {
            "User-Agent": "rat-engine-test/1.0",
            "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "Accept-Language": "zh-CN,zh;q=0.8,en-US;q=0.5,en;q=0.3",
            "Accept-Encoding": "gzip, deflate",
            "Connection": "keep-alive",
            "Upgrade-Insecure-Requests": "1",
        }
        
        response = client.http_get("https://myip.ipip.net/", headers)
        
        # 检查响应
        if response:
            status = response.get("status", 0)
            headers = response.get("headers", {})
            body = response.get("body", b"")
            
            print(f"📊 响应状态码: {status}")
            print(f"📋 响应头数量: {len(headers)}")
            
            # 显示一些关键响应头
            if "content-type" in headers:
                print(f"📄 Content-Type: {headers['content-type']}")
            if "server" in headers:
                print(f"🖥️  Server: {headers['server']}")
            if "content-length" in headers:
                print(f"📏 Content-Length: {headers['content-length']}")
            
            # 解析响应体
            if status == 200:
                try:
                    # 尝试解码响应体
                    body_text = body.decode('utf-8', errors='ignore')
                    print(f"📝 响应体长度: {len(body_text)} 字符")
                    print(f"📝 响应内容: {body_text.strip()}")
                    
                    # 检查是否包含 IP 信息
                    if "当前 IP" in body_text or "来自于" in body_text or any(char.isdigit() for char in body_text):
                        print("🎉 成功获取到 IP 信息！")
                        print("✅ HTTP 客户端集成测试通过")
                        return True
                    else:
                        print("⚠️  响应内容不包含预期的 IP 信息")
                        print("🔍 可能是网站格式变化或网络问题")
                        return False
                        
                except UnicodeDecodeError as e:
                    print(f"❌ 解码响应体失败: {e}")
                    print(f"📝 原始响应体: {body[:100]}...")  # 显示前100字节
                    return False
            else:
                print(f"❌ HTTP 请求失败，状态码: {status}")
                if body:
                    try:
                        error_text = body.decode('utf-8', errors='ignore')
                        print(f"❌ 错误信息: {error_text[:200]}...")
                    except:
                        print(f"❌ 原始错误响应: {body[:100]}...")
                return False
        else:
            print("❌ 未收到响应")
            return False
            
    except Exception as e:
        print(f"❌ 测试过程中发生异常: {e}")
        import traceback
        traceback.print_exc()
        return False
    
    finally:
        # 清理资源
        try:
            print("🧹 清理客户端资源...")
            client.close()
            print("✅ 客户端资源清理完成")
        except Exception as e:
            print(f"⚠️  清理资源时发生异常: {e}")

def test_alternative_ip_service():
    """测试备用 IP 服务（如果主服务失败）"""
    print("\n🔄 测试备用 IP 服务...")
    
    client = rat_engine.PyClientManager()
    
    config = {
        "grpc_server_uri": "http://127.0.0.1:50051",
        "http_server_uri": "http://httpbin.org",
        "grpc_connect_timeout": 5000,
        "grpc_request_timeout": 30000,
        "http_connect_timeout": 10000,
        "http_request_timeout": 30000,
        "max_idle_connections": 10,
        "http2_only": False,                          # 允许 HTTP/1.1 和 HTTP/2
        "enable_compression": False,
        "grpc_user_agent": "rat-engine-test/1.0",
        "http_user_agent": "rat-engine-test/1.0",
        "development_mode": False,
    }
    
    try:
        client.initialize(config)
        print("✅ 备用客户端初始化成功")
        
        # 测试 httpbin.org/ip 服务
        print("📡 测试 httpbin.org/ip 服务...")
        response = client.http_get("http://httpbin.org/ip", None)
        
        if response and response.get("status") == 200:
            body = response.get("body", b"")
            body_text = body.decode('utf-8', errors='ignore')
            print(f"📝 httpbin.org 响应: {body_text.strip()}")
            print("✅ 备用 HTTP 服务测试通过")
            return True
        else:
            print("❌ 备用 HTTP 服务测试失败")
            return False
            
    except Exception as e:
        print(f"❌ 备用服务测试异常: {e}")
        return False
    finally:
        try:
            client.close()
        except:
            pass

def main():
    """主测试函数"""
    print("🚀 RAT Engine PyO3 HTTP 客户端集成测试")
    print("=" * 50)
    
    # 测试主要的 myip.ipip.net 服务
    success = test_myip_access()
    
    # 如果主服务失败，测试备用服务
    if not success:
        print("\n🔄 主服务测试失败，尝试备用服务...")
        backup_success = test_alternative_ip_service()
        if backup_success:
            print("\n🎉 备用服务测试成功，HTTP 客户端集成正常工作")
        else:
            print("\n❌ 所有服务测试都失败，可能存在网络问题或集成问题")
            sys.exit(1)
    
    print("\n" + "=" * 50)
    print("🎉 HTTP 客户端集成测试完成！")
    print("✅ 至少 HTTP 集成是成功的")

if __name__ == "__main__":
    main()