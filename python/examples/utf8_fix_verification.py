#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
UTF-8 编码修复验证脚本

验证 @app.custom 装饰器在返回中文内容时是否正确设置 charset=utf-8
"""

import sys
import time
import requests
from rat_engine import RatApp

def test_utf8_fix():
    """测试 UTF-8 编码修复"""
    
    # 创建测试应用
    app = RatApp(name="utf8_test")
    
    @app.custom("/test-chinese")
    def test_chinese(request_data):
        # 返回包含中文的字符串（不使用元组格式）
        return "这是中文测试内容：你好世界！🌍"
    
    @app.custom("/test-chinese-tuple")
    def test_chinese_tuple(request_data):
        # 返回元组格式，明确指定编码
        return ("这是中文测试内容（元组格式）：你好世界！🌍", "text/plain; charset=utf-8")
    
    @app.html("/")
    def home(request_data):
        return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="utf-8">
            <title>UTF-8 编码测试</title>
        </head>
        <body>
            <h1>🧪 UTF-8 编码修复验证</h1>
            <p><a href="/test-chinese">测试中文（默认处理）</a></p>
            <p><a href="/test-chinese-tuple">测试中文（元组格式）</a></p>
        </body>
        </html>
        """
    
    print("🚀 启动 UTF-8 编码测试服务器...")
    print("📡 服务器地址: http://127.0.0.1:8084")
    print("🔍 测试路由:")
    print("   - /test-chinese (默认处理)")
    print("   - /test-chinese-tuple (元组格式)")
    print("\n⏸️ 按 Ctrl+C 停止服务器")
    
    try:
        app.run(host="127.0.0.1", port=8084)
    except KeyboardInterrupt:
        print("\n🛑 服务器已停止")

def run_automated_test():
    """运行自动化测试"""
    import threading
    import time
    
    # 创建应用
    app = RatApp(name="utf8_auto_test")
    
    @app.custom("/test")
    def test_endpoint(request_data):
        return "中文测试：你好世界！🌍"
    
    # 启动服务器线程
    def start_server():
        app.run(host="127.0.0.1", port=8085)
    
    server_thread = threading.Thread(target=start_server, daemon=True)
    server_thread.start()
    
    # 等待服务器启动
    time.sleep(2)
    
    try:
        print("🧪 执行自动化 UTF-8 编码测试...")
        response = requests.get("http://127.0.0.1:8085/test", timeout=5)
        
        print(f"📊 测试结果:")
        print(f"   状态码: {response.status_code}")
        print(f"   Content-Type: {response.headers.get('Content-Type', 'N/A')}")
        print(f"   响应内容: {response.text}")
        
        # 验证编码
        content_type = response.headers.get('Content-Type', '')
        if 'charset=utf-8' in content_type:
            print("   ✅ UTF-8 编码声明正确")
        else:
            print("   ❌ 缺少 UTF-8 编码声明")
            
        # 验证中文显示
        if "中文测试" in response.text and "你好世界" in response.text:
            print("   ✅ 中文内容显示正确")
        else:
            print("   ❌ 中文内容显示异常")
            
    except Exception as e:
        print(f"❌ 测试失败: {e}")

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--auto":
        run_automated_test()
    else:
        test_utf8_fix()