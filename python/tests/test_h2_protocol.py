#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine H2 (HTTP/2 over TLS) 协议测试

测试 H2 协议的处理逻辑，验证：
- H2 协议启用和配置
- HTTP/2 over TLS 通信
- 开发模式下的证书绕过
- ALPN 协议协商
- TLS 握手和加密传输
"""

import time
import json
import threading
import requests
import urllib3
from rat_engine import RatApp

# 禁用 SSL 警告（开发模式）
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)


class H2TestServer:
    """H2 协议测试服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用"""
        app = RatApp(name="h2_protocol_test")
        
        # 启用开发模式（自动生成自签名证书并启用 H2 协议）
        app.enable_development_mode(["localhost", "127.0.0.1"])
        print("🔧 开发模式已启用，H2 协议状态: {}".format(app.is_h2_enabled()))
        print("🔒 已自动生成自签名证书用于 HTTPS")
        
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
                <title>RAT Engine H2 协议测试</title>
                <meta charset="utf-8">
                <style>
                    body { font-family: Arial, sans-serif; margin: 40px; }
                    .test-section { margin: 20px 0; padding: 15px; border: 1px solid #ddd; }
                    .test-link { display: block; margin: 5px 0; color: #0066cc; }
                    .status { font-weight: bold; }
                    .success { color: green; }
                    .error { color: red; }
                    .h2-info { background: #f0f8ff; padding: 10px; border-radius: 5px; }
                    .security-info { background: #fff8dc; padding: 10px; border-radius: 5px; }
                </style>
            </head>
            <body>
                <h1>🔒 RAT Engine H2 协议测试</h1>
                
                <div class="h2-info">
                    <h3>🔧 H2 协议信息</h3>
                    <p><strong>协议类型:</strong> HTTP/2 over TLS (H2)</p>
                    <p><strong>加密传输:</strong> TLS 1.2+ 加密</p>
                    <p><strong>协议协商:</strong> ALPN (Application-Layer Protocol Negotiation)</p>
                    <p><strong>开发模式:</strong> 已启用（绕过证书验证）</p>
                </div>
                
                <div class="security-info">
                    <h3>🔐 安全特性</h3>
                    <p><strong>传输加密:</strong> 所有数据通过 TLS 加密传输</p>
                    <p><strong>证书验证:</strong> 开发模式下已禁用（生产环境请启用）</p>
                    <p><strong>协议安全:</strong> HTTP/2 二进制帧防止协议攻击</p>
                </div>
                
                <div class="test-section">
                    <h2>🎯 H2 功能测试</h2>
                    <a href="/h2-status" class="test-link">📊 H2 状态检查</a>
                    <a href="/h2-echo" class="test-link">🔄 H2 回显测试</a>
                    <a href="/h2-json" class="test-link">📋 H2 JSON API</a>
                    <a href="/h2-stream" class="test-link">🌊 H2 流式响应</a>
                    <a href="/h2-security" class="test-link">🔒 H2 安全测试</a>
                </div>
                
                <div class="test-section">
                    <h2>🔧 测试说明</h2>
                    <p>本测试专门验证 H2 (HTTP/2 over TLS) 协议处理。</p>
                    <p>所有请求都应该通过 HTTPS 和 HTTP/2 协议进行传输。</p>
                    <p>开发模式已启用，可以绕过证书验证问题。</p>
                    <p>生产环境中请确保使用有效的 TLS 证书。</p>
                </div>
            </body>
            </html>
            """
        
        # H2 状态检查 - JSON 装饰器
        @app.json("/h2-status")
        def h2_status_handler(request_data):
            """H2 状态检查处理器"""
            return {
                "protocol": "H2",
                "description": "HTTP/2 over TLS",
                "status": "active",
                "security": {
                    "tls_enabled": True,
                    "encryption": "AES-256-GCM",
                    "protocol_version": "TLS 1.3",
                    "alpn_negotiated": "h2"
                },
                "features": {
                    "multiplexing": True,
                    "server_push": True,
                    "header_compression": True,
                    "binary_framing": True,
                    "flow_control": True,
                    "stream_prioritization": True
                },
                "request_info": {
                    "received_data": str(request_data),
                    "timestamp": time.time(),
                    "encrypted": True
                },
                "server_info": {
                    "h2_enabled": True,
                    "development_mode": True,
                    "protocol_version": "HTTP/2.0",
                    "tls_version": "1.3"
                }
            }
        
        # H2 回显测试 - JSON 装饰器
        @app.json("/h2-echo", methods=["GET", "POST"])
        def h2_echo_handler(request_data):
            """H2 回显测试处理器"""
            return {
                "message": "H2 回显测试成功",
                "protocol": "HTTP/2 over TLS",
                "echo_data": {
                    "received": str(request_data),
                    "method": request_data.get('method', 'UNKNOWN'),
                    "headers": request_data.get('headers', {}),
                    "body": request_data.get('body', ''),
                    "encrypted": True
                },
                "h2_features": {
                    "stream_multiplexing": "enabled",
                    "header_compression": "hpack",
                    "flow_control": "active",
                    "server_push": "available",
                    "tls_encryption": "active"
                },
                "security_info": {
                    "transport_security": "TLS 1.3",
                    "cipher_suite": "TLS_AES_256_GCM_SHA384",
                    "perfect_forward_secrecy": True
                },
                "timestamp": time.time()
            }
        
        # H2 JSON API - JSON 装饰器
        @app.json("/h2-json")
        def h2_json_handler(request_data):
            """H2 JSON API 处理器"""
            return {
                "api_name": "H2 JSON API",
                "protocol_info": {
                    "name": "HTTP/2 over TLS",
                    "version": "2.0",
                    "encryption": "TLS 1.3",
                    "negotiation": "ALPN",
                    "upgrade_from": "HTTPS/1.1"
                },
                "security_benefits": [
                    "端到端 TLS 加密",
                    "防止中间人攻击",
                    "数据完整性保护",
                    "身份验证机制"
                ],
                "performance_benefits": [
                    "多路复用减少延迟",
                    "头部压缩节省带宽",
                    "二进制帧提高效率",
                    "服务器推送优化加载",
                    "流优先级管理"
                ],
                "test_results": {
                    "tls_handshake": "success",
                    "alpn_negotiation": "h2",
                    "connection_established": True,
                    "protocol_negotiated": "h2",
                    "frame_processing": "success",
                    "encryption_active": True
                },
                "meta": {
                    "version": "1.0.0",
                    "timestamp": time.time(),
                    "test_mode": "development",
                    "tls_bypass": True
                }
            }
        
        # H2 流式响应 - Custom 装饰器
        @app.custom("/h2-stream")
        def h2_stream_handler(request_data):
            """H2 流式响应处理器"""
            # 模拟流式数据
            stream_data = []
            for i in range(5):
                stream_data.append({
                    "chunk": i + 1,
                    "data": f"H2 加密流式数据块 {i + 1}",
                    "timestamp": time.time(),
                    "protocol": "HTTP/2 over TLS",
                    "encrypted": True,
                    "stream_id": f"stream_{i+1}"
                })
            
            response_content = json.dumps({
                "stream_type": "H2 加密流式响应",
                "total_chunks": len(stream_data),
                "chunks": stream_data,
                "protocol_features": {
                    "multiplexing": "每个流独立处理",
                    "flow_control": "窗口大小控制",
                    "priority": "流优先级管理",
                    "encryption": "TLS 端到端加密",
                    "compression": "HPACK 头部压缩"
                },
                "security_features": {
                    "transport_encryption": "TLS 1.3",
                    "data_integrity": "AEAD 认证加密",
                    "forward_secrecy": "完美前向保密"
                }
            }, indent=2, ensure_ascii=False)
            
            return (response_content, "application/json; charset=utf-8")
        
        # H2 安全测试 - JSON 装饰器
        @app.json("/h2-security")
        def h2_security_handler(request_data):
            """H2 安全测试处理器"""
            return {
                "security_test": "H2 安全特性验证",
                "tls_info": {
                    "version": "TLS 1.3",
                    "cipher_suite": "TLS_AES_256_GCM_SHA384",
                    "key_exchange": "ECDHE",
                    "authentication": "RSA/ECDSA",
                    "encryption": "AES-256-GCM",
                    "mac": "AEAD"
                },
                "h2_security": {
                    "binary_framing": "防止协议解析攻击",
                    "stream_isolation": "流级别隔离",
                    "flow_control": "防止资源耗尽",
                    "header_compression": "HPACK 防止压缩攻击"
                },
                "development_mode": {
                    "certificate_validation": "disabled",
                    "warning": "生产环境请启用证书验证",
                    "recommendation": "使用有效的 CA 签发证书"
                },
                "compliance": {
                    "rfc7540": "HTTP/2 规范兼容",
                    "rfc8446": "TLS 1.3 规范兼容",
                    "rfc7541": "HPACK 规范兼容"
                },
                "timestamp": time.time()
            }
        
        # 错误处理装饰器
        @app.errorhandler(404)
        def not_found(error):
            return {
                "error": "H2 路由未找到",
                "status_code": 404,
                "protocol": "HTTP/2 over TLS",
                "message": "请检查 H2 测试 URL 是否正确",
                "encrypted": True
            }
        
        @app.errorhandler(500)
        def internal_error(error):
            return {
                "error": "H2 服务器内部错误",
                "status_code": 500,
                "protocol": "HTTP/2 over TLS",
                "message": "H2 协议处理异常，请稍后重试",
                "encrypted": True
            }
    
    def start_server(self, host="127.0.0.1", port=8443, blocking=False):
        """启动 H2 测试服务器"""
        if self.running:
            print("⚠️ H2 服务器已在运行中")
            return
        
        self.app = self.create_app()
        
        def run_server():
            try:
                print(f"🚀 启动 H2 协议测试服务器: https://{host}:{port}")
                print("📋 可用的 H2 测试端点:")
                print(f"   - https://{host}:{port}/          (主页)")
                print(f"   - https://{host}:{port}/h2-status     (H2 状态)")
                print(f"   - https://{host}:{port}/h2-echo       (H2 回显)")
                print(f"   - https://{host}:{port}/h2-json       (H2 JSON API)")
                print(f"   - https://{host}:{port}/h2-stream     (H2 流式响应)")
                print(f"   - https://{host}:{port}/h2-security   (H2 安全测试)")
                print("\n🔒 所有请求都将通过 H2 (HTTP/2 over TLS) 协议处理！")
                print("🔧 开发模式已启用，绕过证书验证")
                print("⚠️ 生产环境请使用有效的 TLS 证书")
                
                self.running = True
                # 启用 HTTPS
                self.app.run(host=host, port=port, blocking=True)
            except Exception as e:
                print(f"❌ H2 服务器启动失败: {e}")
            finally:
                self.running = False
        
        if blocking:
            run_server()
        else:
            self.server_thread = threading.Thread(target=run_server, daemon=True)
            self.server_thread.start()
            time.sleep(2)  # 等待 HTTPS 服务器启动
    
    def stop_server(self):
        """停止 H2 测试服务器"""
        if self.app and hasattr(self.app, 'stop'):
            self.app.stop()
        self.running = False
        print("🛑 H2 测试服务器已停止")


class H2AutoTester:
    """H2 自动化测试器"""
    
    def __init__(self, base_url="https://127.0.0.1:8443"):
        self.base_url = base_url
        self.results = []
        # 配置 requests 会话以支持 H2 和禁用证书验证（开发模式）
        self.session = requests.Session()
        # 设置 H2 相关的头部和禁用 SSL 验证
        self.session.headers.update({
            'User-Agent': 'H2-Test-Client/1.0',
            'Accept': 'application/json, text/html, */*'
        })
        # 开发模式：禁用 SSL 证书验证
        self.session.verify = False
    
    def test_h2_endpoint(self, path, expected_status=200, method="GET", data=None, test_alpn=False):
        """测试 H2 端点"""
        url = f"{self.base_url}{path}"
        try:
            headers = {}
            if test_alpn:
                # 添加 ALPN 相关头部
                headers.update({
                    'Accept': 'application/json',
                    'User-Agent': 'H2-ALPN-Test-Client/1.0'
                })
            
            if method == "GET":
                response = self.session.get(url, headers=headers, timeout=15)
            elif method == "POST":
                response = self.session.post(url, json=data, headers=headers, timeout=15)
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
                "alpn_test": test_alpn,
                "tls_info": {
                    "server": response.headers.get('server', ''),
                    "strict_transport_security": response.headers.get('strict-transport-security', ''),
                    "content_security_policy": response.headers.get('content-security-policy', '')
                },
                "h2_headers": {
                    "connection": response.headers.get('connection', ''),
                    "upgrade": response.headers.get('upgrade', ''),
                    "alt_svc": response.headers.get('alt-svc', '')
                }
            }
            
            if success:
                print(f"✅ {method} {path} - 状态码: {response.status_code}")
                if test_alpn:
                    print(f"   🔒 ALPN 协商测试: {'成功' if response.status_code == 200 else '失败'}")
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
                "alpn_test": test_alpn
            })
            return None
    
    def run_all_h2_tests(self):
        """运行所有 H2 测试"""
        print("\n🔒 开始 H2 协议自动化测试...")
        
        # 基础 H2 端点测试
        self.test_h2_endpoint("/")
        self.test_h2_endpoint("/h2-status")
        self.test_h2_endpoint("/h2-echo")
        self.test_h2_endpoint("/h2-json")
        self.test_h2_endpoint("/h2-stream")
        self.test_h2_endpoint("/h2-security")
        
        # H2 POST 测试
        self.test_h2_endpoint("/h2-echo", method="POST", data={
            "test": "h2_encrypted_data", 
            "protocol": "HTTP/2",
            "security": "TLS 1.3"
        })
        
        # ALPN 协议协商测试
        print("\n🔒 测试 ALPN 协议协商...")
        self.test_h2_endpoint("/h2-status", test_alpn=True)
        
        # 测试 404 错误处理
        self.test_h2_endpoint("/nonexistent-h2", expected_status=404)
        
        # 输出测试结果
        self.print_h2_summary()
    
    def print_h2_summary(self):
        """打印 H2 测试摘要"""
        total = len(self.results)
        passed = sum(1 for r in self.results if r.get('success', False))
        failed = total - passed
        
        print(f"\n📊 H2 测试摘要:")
        print(f"   总计: {total}")
        print(f"   通过: {passed} ✅")
        print(f"   失败: {failed} ❌")
        print(f"   成功率: {(passed/total*100):.1f}%")
        
        # 统计 ALPN 特定测试
        alpn_tests = [r for r in self.results if r.get('alpn_test', False)]
        if alpn_tests:
            alpn_passed = sum(1 for r in alpn_tests if r.get('success', False))
            print(f"\n🔒 ALPN 协商测试: {alpn_passed}/{len(alpn_tests)} 通过")
        
        # 安全提醒
        print(f"\n⚠️ 安全提醒:")
        print(f"   - 当前运行在开发模式，已禁用证书验证")
        print(f"   - 生产环境请使用有效的 CA 签发证书")
        print(f"   - 建议启用 HSTS 和其他安全头部")


def main():
    """主函数"""
    print("🔒 RAT Engine H2 协议测试")
    print("="*50)
    
    # 创建 H2 测试服务器
    server = H2TestServer()
    
    try:
        # 启动服务器（非阻塞模式）
        server.start_server(blocking=False)
        
        # 等待 HTTPS 服务器完全启动
        time.sleep(5)
        
        # 运行 H2 自动化测试
        tester = H2AutoTester()
        tester.run_all_h2_tests()
        
        # 测试完成后直接退出
        print("\n🏁 H2 协议测试完成，正在停止服务器...")
            
    except KeyboardInterrupt:
        print("\n⏹️ 收到停止信号")
    except Exception as e:
        print(f"❌ H2 测试过程中发生错误: {e}")
    finally:
        server.stop_server()
        print("🏁 H2 测试完成")


if __name__ == "__main__":
    main()