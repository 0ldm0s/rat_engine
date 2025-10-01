#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine @app.custom 装饰器测试

测试新的元组格式支持：
- @app.custom 装饰器支持 (content, content_type) 元组返回格式
- 验证不同内容类型的处理
- 确保与 @app.file 装饰器的一致性
"""

import os
import sys
import time
import threading
from datetime import datetime
from io import BytesIO

try:
    import requests
except ImportError:
    print("❌ 请安装 requests: pip install requests")
    sys.exit(1)

try:
    from rat_engine import RatApp, HttpResponse
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    print("请确保 rat_engine 已正确安装")
    sys.exit(1)

# 服务器配置
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8083  # 使用不同端口避免冲突
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

# 测试配置
AUTO_TEST_ENABLED = True
TEST_DELAY = 2

class CustomDecoratorTestServer:
    """@app.custom 装饰器测试服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用"""
        app = RatApp(name="custom_test")
        
        # 注册路由
        self._register_routes(app)
        
        return app
    
    def _register_routes(self, app: RatApp):
        """注册所有测试路由"""
        
        # 主页
        @app.html("/")
        def home(request_data):
            return """
            <!DOCTYPE html>
            <html>
            <head>
                <title>RAT Engine Custom 装饰器测试</title>
                <meta charset="utf-8">
                <style>
                    body { font-family: Arial, sans-serif; margin: 40px; }
                    .test-section { margin: 20px 0; padding: 15px; border: 1px solid #ddd; }
                    .test-link { display: block; margin: 5px 0; color: #0066cc; }
                    .status { font-weight: bold; }
                    .success { color: green; }
                    .error { color: red; }
                </style>
            </head>
            <body>
                <h1>🧪 RAT Engine Custom 装饰器测试</h1>
                
                <div class="test-section">
                    <h2>📄 Custom 装饰器测试（元组格式）</h2>
                    <a href="/custom-text" class="test-link">📝 自定义文本 (text/plain)</a>
                    <a href="/custom-json" class="test-link">📋 自定义 JSON (application/json)</a>
                    <a href="/custom-xml" class="test-link">📄 自定义 XML (application/xml)</a>
                    <a href="/custom-csv" class="test-link">📊 自定义 CSV (text/csv)</a>
                    <a href="/custom-bytes" class="test-link">🔢 自定义字节数据 (application/octet-stream)</a>
                    <a href="/custom-default" class="test-link">⚙️ 默认处理（无元组）</a>
                </div>
                
                <div class="test-section">
                    <h2>🔧 测试状态</h2>
                    <p>服务器运行在: <code>""" + SERVER_URL + """</code></p>
                    <p class="status success">✅ Custom 装饰器已注册</p>
                </div>
            </body>
            </html>
            """
        
        # Custom 装饰器测试 - 元组格式
        @app.custom("/custom-text")
        def custom_text(request_data):
            # 返回 (content, content_type) 元组
            content = f"这是自定义文本内容\n生成时间: {datetime.now()}\n测试成功！"
            return (content, "text/plain; charset=utf-8")
        
        @app.custom("/custom-json")
        def custom_json(request_data):
            # 返回 (content, content_type) 元组
            import json
            data = {
                "message": "这是自定义 JSON 响应",
                "timestamp": datetime.now().isoformat(),
                "test": "success",
                "format": "tuple"
            }
            return (json.dumps(data, ensure_ascii=False, indent=2), "application/json; charset=utf-8")
        
        @app.custom("/custom-xml")
        def custom_xml(request_data):
            # 返回 (content, content_type) 元组
            xml_content = f"""<?xml version="1.0" encoding="UTF-8"?>
<response>
    <message>这是自定义 XML 响应</message>
    <timestamp>{datetime.now().isoformat()}</timestamp>
    <test>success</test>
    <format>tuple</format>
</response>"""
            return (xml_content, "application/xml; charset=utf-8")
        
        @app.custom("/custom-csv")
        def custom_csv(request_data):
            # 返回 (content, content_type) 元组
            csv_content = f"""名称,值,时间
测试项目,成功,{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}
Custom装饰器,正常,{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}
元组格式,支持,{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}"""
            return (csv_content, "text/csv; charset=utf-8")
        
        @app.custom("/custom-bytes")
        def custom_bytes(request_data):
            # 返回 (bytes_content, content_type) 元组
            binary_data = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde"
            return (binary_data, "application/octet-stream")
        
        @app.custom("/custom-default")
        def custom_default(request_data):
            # 不使用元组，测试默认处理
            return f"默认处理测试\n时间: {datetime.now()}\n这将使用默认的 text/plain 类型"
    
    def start_server(self):
        """启动服务器"""
        if self.running:
            print("⚠️ 服务器已在运行中")
            return
            
        print("🚀 创建 RAT Engine 应用...")
        self.app = self.create_app()
        
        print(f"📡 启动服务器在 {SERVER_URL}...")
        try:
            self.app.run(host=SERVER_HOST, port=SERVER_PORT)
            self.running = True
            print(f"✅ 服务器已启动: {SERVER_URL}")
        except Exception as e:
            print(f"❌ 服务器启动失败: {e}")
            return False
        
        return True

def run_tests():
    """运行自动化测试"""
    print("\n🧪 开始自动化测试...")
    time.sleep(3)  # 等待服务器启动
    
    # Custom 装饰器测试用例
    test_cases = [
        ("/", "主页 HTML 测试", "text/html"),
        ("/custom-text", "自定义文本测试", "text/plain"),
        ("/custom-json", "自定义 JSON 测试", "application/json"),
        ("/custom-xml", "自定义 XML 测试", "application/xml"),
        ("/custom-csv", "自定义 CSV 测试", "text/csv"),
        ("/custom-bytes", "自定义字节数据测试", "application/octet-stream"),
        ("/custom-default", "默认处理测试", "text/plain"),
    ]
    
    print("\n📋 执行 Custom 装饰器测试...")
    success_count = 0
    total_count = 0
    
    for endpoint, description, expected_content_type in test_cases:
        total_count += 1
        try:
            print(f"\n🔍 测试: {description}")
            print(f"   URL: {SERVER_URL}{endpoint}")
            
            response = requests.get(f"{SERVER_URL}{endpoint}", timeout=10)
            
            # 检查状态码
            if response.status_code == 200:
                print(f"   ✅ 状态码: {response.status_code}")
                
                # 检查 Content-Type
                content_type = response.headers.get('Content-Type', 'N/A')
                print(f"   📄 Content-Type: {content_type}")
                
                # 验证 Content-Type 是否符合预期
                if expected_content_type in content_type:
                    print(f"   ✅ Content-Type 匹配预期: {expected_content_type}")
                    success_count += 1
                else:
                    print(f"   ❌ Content-Type 不匹配，预期: {expected_content_type}")
                
                # 显示响应内容（限制长度）
                if endpoint == "/custom-bytes":
                    print(f"   📦 响应内容: {len(response.content)} 字节的二进制数据")
                else:
                    content_preview = response.text[:200] + "..." if len(response.text) > 200 else response.text
                    print(f"   📄 响应内容: {content_preview}")
                    
            else:
                print(f"   ❌ 状态码: {response.status_code}")
                print(f"   📄 错误信息: {response.text[:200]}")
                
        except requests.exceptions.RequestException as e:
            print(f"   ❌ 请求失败: {e}")
        except Exception as e:
            print(f"   ❌ 测试异常: {e}")
        
        time.sleep(0.5)  # 短暂延迟
    
    # 测试结果汇总
    print(f"\n📊 测试结果汇总:")
    print(f"   ✅ 成功: {success_count}/{total_count}")
    print(f"   ❌ 失败: {total_count - success_count}/{total_count}")
    
    if success_count == total_count:
        print(f"\n🎉 所有测试通过！Custom 装饰器元组格式工作正常")
    else:
        print(f"\n⚠️ 部分测试失败，请检查实现")

def main():
    """主函数"""
    print("🚀 RAT Engine Custom 装饰器测试启动")
    
    # 创建测试服务器
    server = CustomDecoratorTestServer()
    
    try:
        # 启动服务器（非阻塞）
        if server.start_server():
            print(f"\n🌐 服务器已启动，访问: {SERVER_URL}")
            
            if AUTO_TEST_ENABLED:
                # 运行自动化测试
                run_tests()
            else:
                print("\n⏸️ 自动测试已禁用，手动访问 URL 进行测试")
                print("按 Ctrl+C 停止服务器")
                
                # 保持服务器运行
                try:
                    while True:
                        time.sleep(1)
                except KeyboardInterrupt:
                    print("\n🛑 收到停止信号")
        else:
            print("❌ 服务器启动失败")
            return 1
            
    except KeyboardInterrupt:
        print("\n🛑 程序被用户中断")
    except Exception as e:
        print(f"❌ 程序异常: {e}")
        import traceback
        traceback.print_exc()
        return 1
    finally:
        # 清理资源
        if server.app:
            try:
                server.app.stop()
                print("🧹 服务器已停止")
            except:
                pass
    
    print("👋 程序结束")
    return 0

if __name__ == "__main__":
    sys.exit(main())