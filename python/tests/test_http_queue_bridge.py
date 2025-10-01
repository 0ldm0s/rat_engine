#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine HTTP 队列桥接功能测试

测试 HTTP 队列桥接模式的装饰器功能，验证：
- 装饰器风格的路由注册
- 队列桥接的消息传递
- 回退到普通 HTTP 处理
- 错误处理机制
"""

import time
import json
import threading
import requests
from rat_engine import RatApp


class DecoratorArchitectureTestServer:
    """装饰器架构测试服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用"""
        app = RatApp(name="decorator_architecture_test")
        
        # 注册路由 - 保持装饰器风格
        self._register_routes(app)
        
        return app
    
    def _register_routes(self, app: RatApp):
        """注册所有测试路由 - 保持装饰器风格"""
        
        # 主页 - HTML 装饰器
        @app.html("/")
        def home(request_data):
            return """
            <!DOCTYPE html>
            <html>
            <head>
                <title>RAT Engine 装饰器架构测试</title>
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
                <h1>🧪 RAT Engine 装饰器架构测试</h1>
                
                <div class="test-section">
                    <h2>🎯 装饰器功能测试</h2>
                    <a href="/simple" class="test-link">📝 简单响应测试</a>
                    <a href="/echo" class="test-link">🔄 回显测试</a>
                    <a href="/json-api" class="test-link">📋 JSON API 测试</a>
                    <a href="/custom-response" class="test-link">🎨 自定义响应测试</a>
                </div>
                
                <div class="test-section">
                    <h2>🔧 测试说明</h2>
                    <p>本测试验证 RAT Engine 装饰器架构的完整性。</p>
                    <p>所有路由都使用标准装饰器风格注册，确保架构一致性。</p>
                </div>
            </body>
            </html>
            """
        
        # 简单响应 - HTML 装饰器
        @app.html("/simple")
        def simple_handler(request_data):
            """简单的 HTML 响应处理器"""
            return "<h1>简单响应测试成功！</h1><p>这是通过装饰器注册的路由。</p>"
        
        # 回显测试 - JSON 装饰器
        @app.json("/echo", methods=["GET", "POST"])
        def echo_handler(request_data):
            """回显请求数据的 JSON 处理器"""
            return {
                "message": "回显测试成功",
                "received_data": str(request_data),
                "timestamp": time.time(),
                "decorator_type": "@app.json"
            }
        
        # JSON API - JSON 装饰器
        @app.json("/json-api")
        def json_api_handler(request_data):
            """JSON API 处理器"""
            return {
                "status": "success",
                "data": {
                    "message": "JSON API 测试成功",
                    "features": [
                        "装饰器风格路由",
                        "队列桥接支持",
                        "自动 JSON 序列化"
                    ]
                },
                "meta": {
                    "version": "1.0.0",
                    "timestamp": time.time()
                }
            }
        
        # 自定义响应 - Custom 装饰器
        @app.custom("/custom-response")
        def custom_response_handler(request_data):
            """自定义响应处理器"""
            xml_content = '''<?xml version="1.0" encoding="UTF-8"?>
            <response>
                <status>success</status>
                <message>自定义响应测试成功</message>
                <decorator>@app.custom</decorator>
                <timestamp>{}</timestamp>
            </response>'''.format(time.time())
            
            return (xml_content, "application/xml; charset=utf-8")
        
        # 错误处理装饰器
        @app.errorhandler(404)
        def not_found(error):
            return {
                "error": "页面未找到",
                "status_code": 404,
                "message": "请检查 URL 是否正确"
            }
        
        @app.errorhandler(500)
        def internal_error(error):
            return {
                "error": "服务器内部错误",
                "status_code": 500,
                "message": "请稍后重试"
            }
    
    def start_server(self, host="127.0.0.1", port=3000, blocking=False):
        """启动测试服务器"""
        if self.running:
            print("⚠️ 服务器已在运行中")
            return
        
        self.app = self.create_app()
        
        def run_server():
            try:
                print(f"🚀 启动装饰器架构测试服务器: http://{host}:{port}")
                print("📋 可用的测试端点:")
                print(f"   - http://{host}:{port}/          (主页)")
                print(f"   - http://{host}:{port}/simple    (简单响应)")
                print(f"   - http://{host}:{port}/echo      (回显测试)")
                print(f"   - http://{host}:{port}/json-api  (JSON API)")
                print(f"   - http://{host}:{port}/custom-response (自定义响应)")
                print("\n🎯 所有路由都使用装饰器风格注册！")
                
                self.running = True
                self.app.run(host=host, port=port, blocking=True)
            except Exception as e:
                print(f"❌ 服务器启动失败: {e}")
            finally:
                self.running = False
        
        if blocking:
            run_server()
        else:
            self.server_thread = threading.Thread(target=run_server, daemon=True)
            self.server_thread.start()
            time.sleep(1)  # 等待服务器启动
    
    def stop_server(self):
        """停止测试服务器"""
        if self.app and hasattr(self.app, 'stop'):
            self.app.stop()
        self.running = False
        print("🛑 测试服务器已停止")


class AutoTester:
    """自动化测试器"""
    
    def __init__(self, base_url="http://127.0.0.1:3000"):
        self.base_url = base_url
        self.results = []
    
    def test_endpoint(self, path, expected_status=200, method="GET", data=None):
        """测试单个端点"""
        url = f"{self.base_url}{path}"
        try:
            if method == "GET":
                response = requests.get(url, timeout=5)
            elif method == "POST":
                response = requests.post(url, json=data, timeout=5)
            else:
                raise ValueError(f"不支持的 HTTP 方法: {method}")
            
            success = response.status_code == expected_status
            result = {
                "path": path,
                "method": method,
                "expected_status": expected_status,
                "actual_status": response.status_code,
                "success": success,
                "content_type": response.headers.get('content-type', ''),
                "response_size": len(response.content)
            }
            
            if success:
                print(f"✅ {method} {path} - 状态码: {response.status_code}")
            else:
                print(f"❌ {method} {path} - 期望: {expected_status}, 实际: {response.status_code}")
            
            self.results.append(result)
            return response
            
        except Exception as e:
            print(f"❌ {method} {path} - 请求失败: {e}")
            self.results.append({
                "path": path,
                "method": method,
                "success": False,
                "error": str(e)
            })
            return None
    
    def run_all_tests(self):
        """运行所有测试"""
        print("\n🧪 开始自动化测试...")
        
        # 测试各个端点
        self.test_endpoint("/")
        self.test_endpoint("/simple")
        self.test_endpoint("/echo")
        self.test_endpoint("/json-api")
        self.test_endpoint("/custom-response")
        
        # 测试 POST 请求
        self.test_endpoint("/echo", method="POST", data={"test": "data"})
        
        # 测试 404
        self.test_endpoint("/nonexistent", expected_status=404)
        
        # 输出测试结果
        self.print_summary()
    
    def print_summary(self):
        """打印测试摘要"""
        total = len(self.results)
        passed = sum(1 for r in self.results if r.get('success', False))
        failed = total - passed
        
        print(f"\n📊 测试摘要:")
        print(f"   总计: {total}")
        print(f"   通过: {passed} ✅")
        print(f"   失败: {failed} ❌")
        print(f"   成功率: {(passed/total*100):.1f}%")


def main():
    """主函数"""
    print("🚀 RAT Engine 装饰器架构测试")
    print("="*50)
    
    # 创建测试服务器
    server = DecoratorArchitectureTestServer()
    
    try:
        # 启动服务器（非阻塞模式）
        server.start_server(blocking=False)
        
        # 等待服务器完全启动
        time.sleep(2)
        
        # 运行自动化测试
        tester = AutoTester()
        tester.run_all_tests()
        
        # 保持服务器运行，供手动测试
        print("\n🌐 服务器继续运行，可进行手动测试...")
        print("按 Ctrl+C 停止服务器")
        
        while server.running:
            time.sleep(1)
            
    except KeyboardInterrupt:
        print("\n⏹️ 收到停止信号")
    except Exception as e:
        print(f"❌ 测试过程中发生错误: {e}")
    finally:
        server.stop_server()
        print("🏁 测试完成")


if __name__ == "__main__":
    main()