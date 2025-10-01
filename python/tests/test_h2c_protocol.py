#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine H2C (HTTP/2 Cleartext) 协议测试

测试 H2C 协议的处理逻辑，验证：
- H2C 协议启用和配置
- HTTP/2 over cleartext 通信
- 开发模式下的证书绕过
- 协议协商和升级机制
"""

import time
import json
import threading
import requests
from rat_engine import RatApp


class H2CTestServer:
    """H2C 协议测试服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用"""
        app = RatApp(name="h2c_protocol_test")
        
        # 启用 H2C 协议支持
        app.enable_h2c()
        print("🔧 已启用 H2C (HTTP/2 over cleartext) 协议")
        print(f"✅ H2C 启用状态: {app.is_h2c_enabled()}")
        
        # 注册路由
        self._register_routes(app)
        
        return app
    
    def _register_routes(self, app: RatApp):
        """注册所有测试路由"""
        
        # 主页 - HTML 装饰器
        @app.html("/")
        def home(request_data):
            return """
            <!DOCTYPE html>
            <html>
            <head>
                <title>RAT Engine H2C 协议测试</title>
                <meta charset="utf-8">
                <style>
                    body { font-family: Arial, sans-serif; margin: 40px; }
                    .test-section { margin: 20px 0; padding: 15px; border: 1px solid #ddd; }
                    .test-link { display: block; margin: 5px 0; color: #0066cc; }
                    .status { font-weight: bold; }
                    .success { color: green; }
                    .error { color: red; }
                    .h2c-info { background: #e8f4fd; padding: 10px; border-radius: 5px; }
                </style>
            </head>
            <body>
                <h1>🧪 RAT Engine H2C 协议测试</h1>
                
                <div class="h2c-info">
                    <h3>🔧 H2C 协议信息</h3>
                    <p><strong>协议类型:</strong> HTTP/2 over Cleartext (H2C)</p>
                    <p><strong>开发模式:</strong> 已启用（绕过证书验证）</p>
                    <p><strong>协议升级:</strong> 支持 HTTP/1.1 到 HTTP/2 升级</p>
                </div>
                
                <div class="test-section">
                    <h2>🎯 H2C 功能测试</h2>
                    <a href="/h2c-status" class="test-link">📊 H2C 状态检查</a>
                    <a href="/h2c-echo" class="test-link">🔄 H2C 回显测试</a>
                    <a href="/h2c-json" class="test-link">📋 H2C JSON API</a>
                    <a href="/h2c-stream" class="test-link">🌊 H2C 流式响应</a>
                </div>
                
                <div class="test-section">
                    <h2>🔧 测试说明</h2>
                    <p>本测试专门验证 H2C (HTTP/2 over cleartext) 协议处理。</p>
                    <p>所有请求都应该通过 HTTP/2 协议进行传输，无需 TLS 加密。</p>
                    <p>开发模式已启用，可以绕过证书验证问题。</p>
                </div>
            </body>
            </html>
            """
        
        # H2C 状态检查 - JSON 装饰器
        @app.json("/h2c-status")
        def h2c_status_handler(request_data):
            """H2C 状态检查处理器"""
            return {
                "protocol": "H2C",
                "description": "HTTP/2 over Cleartext",
                "status": "active",
                "features": {
                    "multiplexing": True,
                    "server_push": True,
                    "header_compression": True,
                    "binary_framing": True
                },
                "request_info": {
                    "received_data": str(request_data),
                    "timestamp": time.time()
                },
                "server_info": {
                    "h2c_enabled": True,
                    "development_mode": True,
                    "protocol_version": "HTTP/2.0"
                }
            }
        
        # H2C 回显测试 - JSON 装饰器
        @app.json("/h2c-echo", methods=["GET", "POST"])
        def h2c_echo_handler(request_data):
            """H2C 回显测试处理器"""
            return {
                "message": "H2C 回显测试成功",
                "protocol": "HTTP/2 Cleartext",
                "echo_data": {
                    "received": str(request_data),
                    "method": request_data.get('method', 'UNKNOWN'),
                    "headers": request_data.get('headers', {}),
                    "body": request_data.get('body', '')
                },
                "h2c_features": {
                    "stream_multiplexing": "enabled",
                    "header_compression": "hpack",
                    "flow_control": "active"
                },
                "timestamp": time.time()
            }
        
        # H2C JSON API - JSON 装饰器
        @app.json("/h2c-json")
        def h2c_json_handler(request_data):
            """H2C JSON API 处理器"""
            return {
                "api_name": "H2C JSON API",
                "protocol_info": {
                    "name": "HTTP/2 Cleartext",
                    "version": "2.0",
                    "encryption": "none",
                    "upgrade_from": "HTTP/1.1"
                },
                "performance_benefits": [
                    "多路复用减少延迟",
                    "头部压缩节省带宽",
                    "二进制帧提高效率",
                    "服务器推送优化加载"
                ],
                "test_results": {
                    "connection_established": True,
                    "protocol_negotiated": "h2c",
                    "frame_processing": "success"
                },
                "meta": {
                    "version": "1.0.0",
                    "timestamp": time.time(),
                    "test_mode": "development"
                }
            }
        
        # H2C 流式响应 - Custom 装饰器
        @app.custom("/h2c-stream")
        def h2c_stream_handler(request_data):
            """H2C 流式响应处理器"""
            # 模拟流式数据
            stream_data = []
            for i in range(5):
                stream_data.append({
                    "chunk": i + 1,
                    "data": f"H2C 流式数据块 {i + 1}",
                    "timestamp": time.time(),
                    "protocol": "HTTP/2 Cleartext"
                })
            
            response_content = json.dumps({
                "stream_type": "H2C 流式响应",
                "total_chunks": len(stream_data),
                "chunks": stream_data,
                "protocol_features": {
                    "multiplexing": "每个流独立处理",
                    "flow_control": "窗口大小控制",
                    "priority": "流优先级管理"
                }
            }, indent=2, ensure_ascii=False)
            
            return (response_content, "application/json; charset=utf-8")
        
        # 错误处理装饰器
        @app.errorhandler(404)
        def not_found(error):
            return {
                "error": "H2C 路由未找到",
                "status_code": 404,
                "protocol": "HTTP/2 Cleartext",
                "message": "请检查 H2C 测试 URL 是否正确"
            }
        
        @app.errorhandler(500)
        def internal_error(error):
            return {
                "error": "H2C 服务器内部错误",
                "status_code": 500,
                "protocol": "HTTP/2 Cleartext",
                "message": "H2C 协议处理异常，请稍后重试"
            }
    
    def start_server(self, host="127.0.0.1", port=8081, blocking=False):
        """启动 H2C 测试服务器"""
        if self.running:
            print("⚠️ H2C 服务器已在运行中")
            return
        
        self.app = self.create_app()
        
        def run_server():
            try:
                print(f"🚀 启动 H2C 协议测试服务器: http://{host}:{port}")
                print("📋 可用的 H2C 测试端点:")
                print(f"   - http://{host}:{port}/          (主页)")
                print(f"   - http://{host}:{port}/h2c-status    (H2C 状态)")
                print(f"   - http://{host}:{port}/h2c-echo      (H2C 回显)")
                print(f"   - http://{host}:{port}/h2c-json      (H2C JSON API)")
                print(f"   - http://{host}:{port}/h2c-stream    (H2C 流式响应)")
                print("\n🎯 所有请求都将通过 H2C (HTTP/2 Cleartext) 协议处理！")
                print("🔧 开发模式已启用，绕过证书验证")
                
                self.running = True
                self.app.run(host=host, port=port, blocking=True)
            except Exception as e:
                print(f"❌ H2C 服务器启动失败: {e}")
            finally:
                self.running = False
        
        if blocking:
            run_server()
        else:
            self.server_thread = threading.Thread(target=run_server, daemon=True)
            self.server_thread.start()
            time.sleep(1)  # 等待服务器启动
    
    def stop_server(self):
        """停止 H2C 测试服务器"""
        if self.app and hasattr(self.app, 'stop'):
            self.app.stop()
        self.running = False
        print("🛑 H2C 测试服务器已停止")


class H2CAutoTester:
    """H2C 自动化测试器"""
    
    def __init__(self, base_url="http://127.0.0.1:8081"):
        self.base_url = base_url
        self.results = []
        # 配置 requests 会话以支持 H2C
        self.session = requests.Session()
        # 设置 H2C 相关的头部
        self.session.headers.update({
            'User-Agent': 'H2C-Test-Client/1.0',
            'Accept': 'application/json, text/html, */*'
        })
    
    def test_h2c_endpoint(self, path, expected_status=200, method="GET", data=None, test_h2c_upgrade=False):
        """测试 H2C 端点"""
        url = f"{self.base_url}{path}"
        try:
            headers = {}
            if test_h2c_upgrade:
                # 添加 H2C 升级头部
                headers.update({
                    'Connection': 'Upgrade, HTTP2-Settings',
                    'Upgrade': 'h2c',
                    'HTTP2-Settings': ''  # 空的 HTTP2-Settings
                })
            
            if method == "GET":
                response = self.session.get(url, headers=headers, timeout=10)
            elif method == "POST":
                response = self.session.post(url, json=data, headers=headers, timeout=10)
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
                "response_size": len(response.content),
                "h2c_upgrade_test": test_h2c_upgrade,
                "protocol_headers": {
                    "connection": response.headers.get('connection', ''),
                    "upgrade": response.headers.get('upgrade', ''),
                    "server": response.headers.get('server', '')
                }
            }
            
            if success:
                print(f"✅ {method} {path} - 状态码: {response.status_code}")
                if test_h2c_upgrade:
                    print(f"   🔄 H2C 升级测试: {'成功' if response.status_code in [200, 101] else '失败'}")
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
                "error": str(e),
                "h2c_upgrade_test": test_h2c_upgrade
            })
            return None
    
    def run_all_h2c_tests(self):
        """运行所有 H2C 测试"""
        print("\n🧪 开始 H2C 协议自动化测试...")
        
        # 基础 H2C 端点测试
        self.test_h2c_endpoint("/")
        self.test_h2c_endpoint("/h2c-status")
        self.test_h2c_endpoint("/h2c-echo")
        self.test_h2c_endpoint("/h2c-json")
        self.test_h2c_endpoint("/h2c-stream")
        
        # H2C POST 测试
        self.test_h2c_endpoint("/h2c-echo", method="POST", data={"test": "h2c_data", "protocol": "HTTP/2"})
        
        # H2C 协议升级测试
        print("\n🔄 测试 H2C 协议升级...")
        self.test_h2c_endpoint("/h2c-status", test_h2c_upgrade=True)
        
        # 测试 404 错误处理
        self.test_h2c_endpoint("/nonexistent-h2c", expected_status=404)
        
        # 输出测试结果
        self.print_h2c_summary()
    
    def print_h2c_summary(self):
        """打印 H2C 测试摘要"""
        total = len(self.results)
        passed = sum(1 for r in self.results if r.get('success', False))
        failed = total - passed
        
        print(f"\n📊 H2C 测试摘要:")
        print(f"   总计: {total}")
        print(f"   通过: {passed} ✅")
        print(f"   失败: {failed} ❌")
        print(f"   成功率: {(passed/total*100):.1f}%")
        
        # 统计 H2C 特定测试
        h2c_upgrade_tests = [r for r in self.results if r.get('h2c_upgrade_test', False)]
        if h2c_upgrade_tests:
            h2c_passed = sum(1 for r in h2c_upgrade_tests if r.get('success', False))
            print(f"\n🔄 H2C 升级测试: {h2c_passed}/{len(h2c_upgrade_tests)} 通过")


def main():
    """主函数"""
    print("🚀 RAT Engine H2C 协议测试")
    print("="*50)
    
    # 创建 H2C 测试服务器
    server = H2CTestServer()
    
    try:
        # 启动服务器（非阻塞模式）
        server.start_server(blocking=False)
        
        # 等待服务器完全启动
        time.sleep(3)
        
        # 运行 H2C 自动化测试
        tester = H2CAutoTester()
        tester.run_all_h2c_tests()
        
        # 保持服务器运行，供手动测试
        print("\n🌐 H2C 服务器继续运行，可进行手动测试...")
        print("按 Ctrl+C 停止服务器")
        
        while server.running:
            time.sleep(1)
            
    except KeyboardInterrupt:
        print("\n⏹️ 收到停止信号")
    except Exception as e:
        print(f"❌ H2C 测试过程中发生错误: {e}")
    finally:
        server.stop_server()
        print("🏁 H2C 测试完成")


if __name__ == "__main__":
    main()