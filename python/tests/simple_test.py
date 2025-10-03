#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
简单的服务器启动测试和 HTTP 类功能验证
"""

import time
import threading
from rat_engine import RatApp, HttpRequest, HttpResponse, HttpMethod, rat_debug, rat_info, rat_warn, rat_error, rat_startup_log

def test_simple_server():
    """测试简单服务器启动"""
    print("🐍 [PYTHON] ===== 开始 simple_test_server 函数 =====")
    print("🐍 [PYTHON] 🚀 创建 RatApp...")
    print("🐍 [PYTHON] RatApp 创建完成，开始配置日志...")
    app = RatApp(name="simple_test")

    # 配置日志
    print("🐍 [PYTHON] 调用 configure_logging...")
    app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)
    print("🐍 [PYTHON] 日志配置完成")

    # 注册路由
    print("🐍 [PYTHON] 注册路由处理器...")

    @app.html("/")
    def home(request_data):
        rat_info("🐍 [PYTHON] 📄 处理主页请求")
        return "<h1>Hello RAT Engine!</h1>"

    @app.json("/api/test")
    def api_test(request_data):
        rat_debug("🐍 [PYTHON] 🔧 处理API测试请求")
        return {"status": "ok", "message": "API working"}

    print("🐍 [PYTHON] 路由注册完成，准备启动服务器...")
    print("🐍 [PYTHON] 📡 启动服务器...")

    # 测试非阻塞模式启动
    print("🐍 [PYTHON] 🔧 测试非阻塞模式启动...")
    try:
        # 使用默认的非阻塞模式启动服务器
        app.run(host="127.0.0.1", port=8082)
        print("✅ 服务器启动命令执行成功")
    except Exception as e:
        rat_error(f"🐍 [PYTHON] ❌ 服务器启动失败: {e}")
        return False
    
    # 等待服务器启动
    print("⏳ 等待服务器启动...")
    time.sleep(3)
    
    print("✅ 服务器应该已启动在 http://127.0.0.1:8082")
    
    # 测试连接
    try:
        import requests
        
        # 测试 HTML 端点
        print("🧪 测试 HTML 端点 /")
        response = requests.get("http://127.0.0.1:8082/", timeout=5)
        if response.status_code == 200:
            print("✅ HTML 端点响应正常")
            print(f"📋 响应头信息: {dict(response.headers)}")
            print(f"📋 Content-Type: {response.headers.get('Content-Type', 'Not Set')}")
            print(f"📄 响应内容: {response.text[:100]}...")
        else:
            print(f"❌ HTML 端点响应异常: {response.status_code}")
            return False
            
        # 测试 JSON 端点
        print("\n🧪 测试 JSON 端点 /api/test")
        json_response = requests.get("http://127.0.0.1:8082/api/test", timeout=5)
        if json_response.status_code == 200:
            print("✅ JSON 端点响应正常")
            print(f"📋 JSON 响应头信息: {dict(json_response.headers)}")
            print(f"📋 JSON Content-Type: {json_response.headers.get('Content-Type', 'Not Set')}")
            try:
                json_data = json_response.json()
                print(f"📄 JSON 响应内容: {json_data}")
                # 验证 JSON 结构
                if json_data.get('status') == 'ok' and 'message' in json_data:
                    print("✅ JSON 端点数据结构验证通过")
                else:
                    print(f"❌ JSON 端点数据结构异常: {json_data}")
                    return False
            except Exception as json_err:
                print(f"❌ JSON 解析失败: {json_err}")
                return False
        else:
            print(f"❌ JSON 端点响应异常: {json_response.status_code}")
            return False
            
        print("\n🎉 所有端点测试通过！")
        return True
        
    except Exception as e:
        print(f"❌ 连接服务器失败: {e}")
        return False

def test_http_classes():
    """测试 HTTP 类的基本功能"""
    print("\n🧪 测试 HTTP 类功能...")
    
    try:
        # 测试 HttpMethod
        print("📋 测试 HttpMethod...")
        method = HttpMethod.Get
        print(f"✅ HttpMethod.Get: {method}")
        
        # 测试 HttpRequest 构造
        print("📨 测试 HttpRequest 构造...")
        request = HttpRequest(
            method="GET",
            path="/test",
            query_string="param=value",
            headers={"Content-Type": "application/json"},
            body=b"test body",  # 使用字节数组
            remote_addr="127.0.0.1:3000",
            real_ip="127.0.0.1"
        )
        print(f"✅ HttpRequest 创建成功: method={request.method}, path={request.path}")
        print(f"   查询字符串: {request.query_string}")
        print(f"   远程地址: {request.remote_addr}")
        
        # 测试 HttpResponse 构造
        print("📤 测试 HttpResponse 构造...")
        response = HttpResponse(
            status=200,
            headers={"Content-Type": "application/json"},
            body=b'{"message": "success"}'  # 使用字节数组
        )
        print(f"✅ HttpResponse 创建成功: status={response.status}")
        print(f"   响应头: {response.headers}")
        print(f"   响应体长度: {len(response.body)} 字节")
        
        # 测试 HttpResponse 便捷方法
        print("🔧 测试 HttpResponse 便捷方法...")
        json_response = HttpResponse.json({"test": "data"})
        print(f"✅ HttpResponse.json() 创建成功")
        
        text_response = HttpResponse.text("Hello World")
        print(f"✅ HttpResponse.text() 创建成功")
        
        print("🎉 HTTP 类测试全部通过！")
        return True
        
    except Exception as e:
        print(f"❌ HTTP 类测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False

if __name__ == "__main__":
    # 首先测试 HTTP 类
    print("=" * 50)
    print("🧪 开始 HTTP 类功能测试")
    print("=" * 50)
    http_success = test_http_classes()
    
    if http_success:
        print("\n" + "=" * 50)
        print("🚀 开始服务器功能测试")
        print("=" * 50)
        # 只有 HTTP 类测试成功才进行服务器测试
        server_success = test_simple_server()
        
        # 总结测试结果
        if server_success:
            print("\n🎉 所有测试成功！")
        else:
            print("\n💥 服务器测试失败！")
    else:
        print("\n💥 HTTP 类测试失败，跳过服务器测试！")
    
    # 等待一下再退出
    time.sleep(1)