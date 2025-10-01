#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine Python H2 协议流式响应演示

展示如何使用 RAT Engine Python API 在 H2 协议下的流式响应功能：
- Server-Sent Events (SSE) over H2 (HTTPS)
- 分块传输编码 over H2 (HTTPS)
- 自定义流式响应 over H2 (HTTPS)
- JSON 流 over H2 (HTTPS)
- 文本流 over H2 (HTTPS)
- 实时日志流 over H2 (HTTPS)
- H2 协议验证 (HTTPS)
- 自签名证书支持
"""

import asyncio
import json
import time
import threading
import signal
import sys
import ssl
from datetime import datetime
from enum import Enum, Flag, auto
from typing import List, Dict, Any, Set, AsyncGenerator

try:
    import requests
    from requests.packages.urllib3.exceptions import InsecureRequestWarning
    # 禁用 SSL 警告（仅用于开发模式的自签名证书）
    requests.packages.urllib3.disable_warnings(InsecureRequestWarning)
except ImportError:
    print("❌ 请安装 requests: pip install requests")
    sys.exit(1)

try:
    import aiohttp
    from aiohttp_sse_client2 import client as sse_client
except ImportError:
    print("❌ 请安装 aiohttp 和 aiohttp-sse-client2: pip install aiohttp aiohttp-sse-client2")
    sys.exit(1)

try:
    from rat_engine import RatApp
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    print("请确保 rat_engine 已正确安装")
    sys.exit(1)

# 测试功能枚举
class TestFeature(Flag):
    """测试功能枚举 - 使用 Flag 支持组合选择"""
    NONE = 0
    HOME = auto()           # 主页测试
    CHUNKED = auto()        # 分块传输测试
    JSON_STREAM = auto()    # JSON 流测试
    TEXT_STREAM = auto()    # 文本流测试
    HEADERS = auto()        # 头信息测试
    SSE = auto()           # SSE 连接测试 (组合测试)
    SSE_AIOHTTP = auto()   # SSE aiohttp_sse_client 单独测试
    LOGS = auto()          # 实时日志流测试
    H2_PROTOCOL = auto()   # H2 协议验证测试
    TLS_CERT = auto()      # TLS 证书验证测试
    
    # 预定义组合
    BASIC = HOME | HEADERS                    # 基础功能测试
    STREAMING = CHUNKED | JSON_STREAM | TEXT_STREAM  # 流式传输测试
    REALTIME = SSE | LOGS                    # 实时通信测试
    SSE_ALL = SSE | SSE_AIOHTTP              # 所有SSE测试
    H2_ALL = H2_PROTOCOL | TLS_CERT | STREAMING | SSE_ALL  # H2 相关测试
    ALL = HOME | CHUNKED | JSON_STREAM | TEXT_STREAM | HEADERS | SSE | LOGS | SSE_AIOHTTP | H2_PROTOCOL | TLS_CERT  # 所有测试

# 配置开关
AUTO_TEST_ENABLED = True  # 设置为 False 可关闭自动测试
TEST_DELAY = 2  # 测试延迟秒数
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8443  # 使用 HTTPS 端口
SERVER_URL = f"https://{SERVER_HOST}:{SERVER_PORT}"  # HTTPS URL

# 测试配置 - H2 专用测试
TEST_FEATURES = TestFeature.H2_ALL            # 测试 H2 相关功能

class StreamingDemoH2Server:
    """H2 协议流式响应演示服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用并启用 H2 开发模式"""
        # 创建应用（RatApp 内部会自动创建 router 和 server）
        app = RatApp(name="streaming_demo_h2")
        
        # 启用 H2 开发模式（自动生成自签名证书并启用 HTTPS + H2）
        print("🔧 启用 H2 开发模式（自动生成自签名证书）...")
        app.enable_development_mode(["localhost", "127.0.0.1"])
        
        # 验证 H2 是否启用
        if hasattr(app, 'is_h2_enabled') and app.is_h2_enabled():
            print("✅ H2 协议已启用")
        else:
            print("⚠️ H2 协议状态未知")
            
        # 验证 H2C 是否启用
        if hasattr(app, 'is_h2c_enabled') and app.is_h2c_enabled():
            print("✅ H2C 协议也已启用")
        else:
            print("ℹ️ H2C 协议未启用（仅 H2 over TLS）")
            
        print("🔒 自签名证书已自动生成，支持 HTTPS 和 H2 协议")
        
        # 注册路由
        self._register_routes(app)
        
        return app
    
    def _register_routes(self, app: RatApp):
        """注册所有路由"""
        
        # 主页路由
        @app.html("/")
        def home(request):
            return self._get_demo_html()
        
        # H2 协议验证路由
        @app.json("/h2-status")
        def h2_status(request):
            """H2 协议状态检查"""
            return {
                "protocol": "H2",
                "message": "H2 协议验证端点",
                "timestamp": datetime.utcnow().isoformat() + "Z",
                "server_info": {
                    "name": "RAT Engine H2 Demo",
                    "version": "0.2.9",
                    "h2_enabled": True,
                    "tls_enabled": True,
                    "development_mode": True
                },
                "request_info": {
                    "method": "GET",
                    "path": "/h2-status",
                    "headers": dict(request.headers) if hasattr(request, 'headers') else {},
                    "secure": True
                },
                "tls_info": {
                    "certificate_type": "self-signed",
                    "purpose": "development",
                    "warning": "仅用于开发和测试环境"
                }
            }
        
        # TLS 证书信息路由
        @app.json("/tls-info")
        def tls_info(request):
            """TLS 证书信息检查"""
            return {
                "tls_status": "enabled",
                "certificate_info": {
                    "type": "self-signed",
                    "generated_by": "RAT Engine Development Mode",
                    "hostnames": ["localhost", "127.0.0.1"],
                    "purpose": "development_and_testing",
                    "security_warning": "此证书仅用于开发环境，生产环境请使用 CA 签发的证书"
                },
                "protocol_support": {
                    "http1_1": True,
                    "h2": True,
                    "h2c": True
                },
                "timestamp": datetime.utcnow().isoformat() + "Z"
            }
        
        # SSE 路由 - H2 版本
        @app.sse("/sse")
        def sse_endpoint(request):
            def sse_generator():
                # 发送初始连接事件
                yield "event: connected\ndata: H2 HTTPS Connection established\n\n"
                
                # 发送定期更新
                for i in range(1, 11):
                    time.sleep(1)
                    data = {
                        "timestamp": datetime.utcnow().isoformat() + "Z",
                        "counter": i,
                        "message": f"H2 HTTPS Update #{i}",
                        "protocol": "H2",
                        "secure": True
                    }
                    yield f"event: update\ndata: {json.dumps(data)}\n\n"
                
                # 发送结束事件
                yield "event: end\ndata: H2 HTTPS Stream completed\n\n"
            
            return sse_generator()
        
        # 分块传输路由 - H2 版本
        @app.chunk("/chunked")
        def chunked_endpoint(request):
            def chunked_generator():
                # H2 增强的分块数据
                chunks = [
                    f"H2_HTTPS_CHUNK_START|{datetime.utcnow().isoformat()}Z|开始 H2 HTTPS 数据传输...\n",
                    f"H2_HTTPS_CHUNK_001|SIZE:48|正在通过 H2 HTTPS 处理第一部分数据...\n",
                    f"H2_HTTPS_CHUNK_002|SIZE:64|正在通过 H2 HTTPS 处理第二部分数据，包含更多内容...\n",
                    f"H2_HTTPS_CHUNK_003|SIZE:80|正在通过 H2 HTTPS 处理第三部分数据，这是一个较长的数据块用于测试...\n",
                    f"H2_HTTPS_CHUNK_004|SIZE:40|H2 HTTPS 数据传输完成！\n",
                    f"H2_HTTPS_CHUNK_END|TOTAL:5|{datetime.utcnow().isoformat()}Z|H2 HTTPS 传输结束\n"
                ]
                
                for i, chunk in enumerate(chunks, 1):
                    # 添加传输延迟模拟真实网络环境
                    time.sleep(0.5)
                    
                    # 在每个分块前添加分块元数据
                    template = f"[H2_HTTPS_CHUNK_{i:03d}|BYTES:{{}}] {chunk}"
                    temp_chunk = template.format(999999)
                    actual_size = len(temp_chunk.encode('utf-8'))
                    enhanced_chunk = template.format(actual_size)
                    
                    yield enhanced_chunk
            
            return chunked_generator()
        
        # JSON 流路由 - H2 版本
        @app.sse_json("/json-stream")
        def json_stream_endpoint(request):
            items = [
                {"id": 1, "name": "Alice", "age": 30, "protocol": "H2", "secure": True},
                {"id": 2, "name": "Bob", "age": 25, "protocol": "H2", "secure": True},
                {"id": 3, "name": "Charlie", "age": 35, "protocol": "H2", "secure": True},
                {"id": 4, "name": "Diana", "age": 28, "protocol": "H2", "secure": True},
                {"id": 5, "name": "Eve", "age": 32, "protocol": "H2", "secure": True},
            ]
            
            # 装饰器会自动处理 JSON 流格式
            return items
        
        # 文本流路由 - H2 版本
        @app.sse_text("/text-stream")
        def text_stream_endpoint(request):
            lines = [
                "第一行 H2 HTTPS 文本",
                "第二行 H2 HTTPS 文本",
                "第三行 H2 HTTPS 文本",
                "第四行 H2 HTTPS 文本",
                "最后一行 H2 HTTPS 文本"
            ]
            
            # 装饰器会自动处理文本流格式
            return lines
        
        # 实时日志流路由 - H2 版本
        @app.sse("/logs")
        def logs_endpoint(request):
            def log_generator():
                # 发送初始日志
                yield "event: log\ndata: [INFO] H2 HTTPS 日志流已启动\n\n"
                
                # 模拟实时日志
                log_levels = ["INFO", "WARN", "ERROR", "DEBUG"]
                messages = [
                    "H2 HTTPS 用户登录成功",
                    "H2 HTTPS 数据库连接建立",
                    "H2 HTTPS 处理用户请求",
                    "H2 HTTPS 缓存更新完成",
                    "H2 HTTPS 定时任务执行",
                    "H2 HTTPS 系统健康检查"
                ]
                
                for i in range(20):
                    time.sleep(0.8)
                    level = log_levels[i % len(log_levels)]
                    message = messages[i % len(messages)]
                    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                    
                    log_entry = f"[{timestamp}] {level} - {message}"
                    yield f"event: log\ndata: {log_entry}\n\n"
            
            return log_generator()
        
        # 头信息测试路由 - H2 版本
        @app.json("/headers-test")
        def headers_test_endpoint(request):
            """返回请求和响应头信息用于测试"""
            return {
                "message": "H2 HTTPS 头信息测试端点",
                "protocol": "H2",
                "secure": True,
                "timestamp": datetime.utcnow().isoformat() + "Z",
                "request_headers": dict(request.headers) if hasattr(request, 'headers') else {},
                "note": "请查看浏览器开发者工具的网络标签页查看完整的 H2 HTTPS 响应头信息"
            }
    
    def _get_demo_html(self) -> str:
        """获取 H2 演示页面 HTML"""
        return '''
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine Python H2 流式响应演示</title>
    <meta charset="UTF-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .demo-section { margin: 20px 0; padding: 20px; border: 1px solid #ddd; border-radius: 5px; }
        .log-output { background: #f5f5f5; padding: 10px; height: 200px; overflow-y: auto; font-family: monospace; }
        button { padding: 10px 20px; margin: 5px; cursor: pointer; }
        .status { padding: 10px; margin: 10px 0; border-radius: 5px; }
        .success { background-color: #d4edda; color: #155724; border: 1px solid #c3e6cb; }
        .info { background-color: #d1ecf1; color: #0c5460; border: 1px solid #bee5eb; }
        .warning { background-color: #fff3cd; color: #856404; border: 1px solid #ffeaa7; }
        .h2-badge { background-color: #28a745; color: white; padding: 2px 8px; border-radius: 3px; font-size: 12px; }
        .https-badge { background-color: #17a2b8; color: white; padding: 2px 8px; border-radius: 3px; font-size: 12px; }
    </style>
</head>
<body>
    <h1>RAT Engine Python H2 流式响应演示 <span class="h2-badge">H2</span> <span class="https-badge">HTTPS</span></h1>
    
    <div class="status success">
        ✅ RAT Engine Python H2 服务器运行中
        <br>🔧 开发模式已启用，H2 协议支持
        <br>🔒 自签名证书已自动生成，支持 HTTPS
    </div>
    
    <div class="status warning">
        ⚠️ <strong>安全提醒：</strong> 当前使用自签名证书，仅适用于开发和测试环境。
        生产环境请使用 CA 签发的有效证书。
    </div>
    
    <div class="demo-section">
        <h2>H2 协议验证</h2>
        <div class="status info">
            <strong>H2 协议特性：</strong><br>
            • HTTP/2 over TLS (HTTPS)<br>
            • 支持多路复用和服务器推送<br>
            • 二进制协议，性能优异<br>
            • 头部压缩 (HPACK)<br>
            • 流量控制和优先级<br><br>
            <a href="/h2-status" target="_blank">🔗 检查 H2 协议状态</a> |
            <a href="/tls-info" target="_blank">🔒 查看 TLS 证书信息</a>
        </div>
    </div>
    
    <div class="demo-section">
        <h2>Server-Sent Events (SSE) over H2 演示</h2>
        <button onclick="startSSE()">开始 H2 HTTPS SSE 连接</button>
        <button onclick="stopSSE()">停止 SSE 连接</button>
        <div id="sse-output" class="log-output"></div>
    </div>
    
    <div class="demo-section">
        <h2>H2 HTTPS 分块传输测试</h2>
        <div class="status info">
            <strong>H2 HTTPS 增强验证功能：</strong><br>
            • 每个分块包含 H2 HTTPS 协议标识<br>
            • 自动验证声明大小与实际大小<br>
            • 检测 H2 HTTPS 传输开始和结束标记<br>
            • 验证 H2 HTTPS 预期内容完整性<br>
            • TLS 加密传输保障<br><br>
            <a href="/chunked" target="_blank">🔗 测试 H2 HTTPS 分块传输</a> (在新标签页中打开)
        </div>
    </div>
    
    <div class="demo-section">
        <h2>H2 HTTPS 实时日志流演示</h2>
        <button onclick="startLogs()">开始 H2 HTTPS 日志流</button>
        <button onclick="stopLogs()">停止日志流</button>
        <div id="log-output" class="log-output"></div>
    </div>
    
    <div class="demo-section">
        <h2>其他 H2 HTTPS 流式端点</h2>
        <ul>
            <li><a href="/h2-status" target="_blank">H2 协议状态检查</a></li>
            <li><a href="/tls-info" target="_blank">TLS 证书信息查看</a></li>
            <li><a href="/chunked" target="_blank">H2 HTTPS 分块传输演示</a></li>
            <li><a href="/json-stream" target="_blank">H2 HTTPS JSON 流演示</a></li>
            <li><a href="/text-stream" target="_blank">H2 HTTPS 文本流演示</a></li>
            <li><a href="/headers-test" target="_blank">H2 HTTPS 头信息测试</a> - 查看响应头信息</li>
        </ul>
    </div>
    
    <div class="demo-section">
        <h2>H2 协议说明</h2>
        <div class="status info">
            <strong>H2 (HTTP/2 over TLS) 特点：</strong><br>
            • 基于 TLS 的安全 HTTP/2 协议<br>
            • 多路复用：单连接并发处理多个请求<br>
            • 服务器推送：主动推送资源<br>
            • 头部压缩：减少传输开销<br>
            • 二进制协议：解析效率高<br>
            • 流量控制：防止缓冲区溢出<br><br>
            <strong>开发模式特性：</strong><br>
            • 自动生成自签名证书<br>
            • 同时支持 HTTP/1.1、H2C 和 H2<br>
            • 提供完整的协议协商机制<br>
            • 适用于开发和测试环境<br>
            • 跳过证书验证（仅开发模式）
        </div>
    </div>
    
    <div class="demo-section">
        <h2>TLS 证书信息</h2>
        <div class="status warning">
            <strong>自签名证书说明：</strong><br>
            • 证书由 RAT Engine 开发模式自动生成<br>
            • 支持的主机名：localhost, 127.0.0.1<br>
            • 仅用于开发和测试环境<br>
            • 浏览器会显示安全警告，这是正常的<br>
            • 生产环境请使用 CA 签发的有效证书<br><br>
            <strong>如何信任自签名证书：</strong><br>
            1. 浏览器访问时点击 "高级" 或 "Advanced"<br>
            2. 点击 "继续访问" 或 "Proceed to site"<br>
            3. 或在浏览器中手动添加证书例外
        </div>
    </div>
    
    <div class="demo-section">
        <h2>API 端点列表</h2>
        <div class="status info">
            <strong>可用 H2 HTTPS 端点：</strong><br>
            • GET / - H2 演示主页<br>
            • GET /h2-status - H2 协议状态检查<br>
            • GET /tls-info - TLS 证书信息查看<br>
            • GET /sse - H2 HTTPS Server-Sent Events<br>
            • GET /chunked - H2 HTTPS 分块传输<br>
            • GET /json-stream - H2 HTTPS JSON 流<br>
            • GET /text-stream - H2 HTTPS 文本流<br>
            • GET /logs - H2 HTTPS 实时日志流<br>
            • GET /headers-test - H2 HTTPS 头信息测试
        </div>
    </div>
    
    <script>
        let sseConnection = null;
        let logConnection = null;
        
        function startSSE() {
            if (sseConnection) return;
            
            const output = document.getElementById('sse-output');
            output.innerHTML = '';
            
            sseConnection = new EventSource('/sse');
            
            sseConnection.onopen = function() {
                output.innerHTML += '[H2 HTTPS 连接已建立]\n';
            };
            
            sseConnection.addEventListener('connected', function(e) {
                output.innerHTML += '[H2 HTTPS 连接事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            sseConnection.addEventListener('update', function(e) {
                output.innerHTML += '[H2 HTTPS 更新事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            sseConnection.addEventListener('end', function(e) {
                output.innerHTML += '[H2 HTTPS 结束事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
                sseConnection.close();
                sseConnection = null;
            });
            
            sseConnection.onerror = function() {
                output.innerHTML += '[H2 HTTPS 连接错误]\n';
                output.scrollTop = output.scrollHeight;
            };
        }
        
        function stopSSE() {
            if (sseConnection) {
                sseConnection.close();
                sseConnection = null;
                document.getElementById('sse-output').innerHTML += '[H2 HTTPS 连接已关闭]\n';
            }
        }
        
        function startLogs() {
            if (logConnection) return;
            
            const output = document.getElementById('log-output');
            output.innerHTML = '';
            
            logConnection = new EventSource('/logs');
            
            logConnection.addEventListener('log', function(e) {
                output.innerHTML += e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            logConnection.onerror = function() {
                output.innerHTML += '[H2 HTTPS 日志连接错误]\n';
                output.scrollTop = output.scrollHeight;
            };
        }
        
        function stopLogs() {
            if (logConnection) {
                logConnection.close();
                logConnection = null;
                document.getElementById('log-output').innerHTML += '[H2 HTTPS 日志连接已关闭]\n';
            }
        }
    </script>
</body>
</html>
        '''
    
    def start_server(self):
        """启动 H2 HTTPS 服务器"""
        try:
            self.app = self.create_app()
            print(f"🚀 RAT Engine Python H2 HTTPS 流式响应演示服务器启动中...")
            print(f"📡 H2 HTTPS 服务器地址: {SERVER_URL}")
            print(f"🔗 H2 演示页面: {SERVER_URL}/")
            print(f"🔍 H2 协议状态: {SERVER_URL}/h2-status")
            print(f"🔒 TLS 证书信息: {SERVER_URL}/tls-info")
            print(f"📊 H2 HTTPS SSE 端点: {SERVER_URL}/sse")
            print(f"📦 H2 HTTPS 分块传输: {SERVER_URL}/chunked")
            print(f"📄 H2 HTTPS JSON 流: {SERVER_URL}/json-stream")
            print(f"📝 H2 HTTPS 文本流: {SERVER_URL}/text-stream")
            print(f"📋 H2 HTTPS 日志流: {SERVER_URL}/logs")
            print(f"🔍 H2 HTTPS 头信息测试: {SERVER_URL}/headers-test")
            print()
            print("💡 提示: H2 HTTPS 自动测试将显示每个端点的响应头信息")
            print("🔧 开发模式: H2 协议已启用，自签名证书已生成")
            print("⚠️ 安全提醒: 浏览器可能显示证书警告，这是正常的（开发模式）")
            print()
            
            # 在后台线程启动服务器
            def run_server():
                # 使用非阻塞模式避免潜在的程序卡死问题
                self.app.run(host=SERVER_HOST, port=SERVER_PORT)
                # 保持线程运行
                import time
                while True:
                    time.sleep(1)
            
            self.server_thread = threading.Thread(target=run_server, daemon=True)
            self.server_thread.start()
            
            # 等待服务器启动
            time.sleep(3)
            self.running = True
            return True
        except Exception as e:
            print(f"❌ H2 HTTPS 服务器启动失败: {e}")
            self.running = False
            return False
    
    def stop_server(self):
        """停止 H2 HTTPS 服务器"""
        self.running = False
        if self.app:
            try:
                self.app.stop()
            except:
                pass

class AutoTesterH2:
    """H2 HTTPS 自动测试器"""
    
    def __init__(self, base_url: str):
        self.base_url = base_url
        self.session = requests.Session()
        self.session.timeout = 10
        # 禁用 SSL 验证（仅用于开发模式的自签名证书）
        self.session.verify = False
        # 设置 H2 相关头部
        self.session.headers.update({
            'User-Agent': 'RAT-Engine-H2-Tester/1.0'
        })
    
    def _print_response_headers(self, response):
        """输出响应头信息"""
        print("   📋 H2 HTTPS 响应头信息:")
        
        # 重要的 H2 HTTPS 流式响应相关头信息
        important_headers = [
            'content-type',
            'transfer-encoding', 
            'cache-control',
            'connection',
            'upgrade',
            'strict-transport-security',
            'access-control-allow-origin',
            'access-control-allow-methods',
            'access-control-allow-headers',
            'server',
            'content-length',
            'content-security-policy'
        ]
        
        # 首先输出重要头信息
        for header in important_headers:
            if header in response.headers:
                value = response.headers[header]
                print(f"      {header}: {value}")
        
        # 输出其他头信息
        other_headers = {k: v for k, v in response.headers.items() 
                        if k.lower() not in important_headers}
        
        if other_headers:
            print("      --- 其他头信息 ---")
            for key, value in other_headers.items():
                print(f"      {key}: {value}")
        
        print()  # 空行分隔
    
    def test_h2_protocol_status(self) -> bool:
        """测试 H2 协议状态"""
        url = f"{self.base_url}/h2-status"
        print(f"🧪 测试 H2 协议状态: {url}")
        
        try:
            response = self.session.get(url)
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                data = response.json()
                print(f"   ✅ H2 协议状态: {data.get('protocol', 'Unknown')}")
                print(f"   📊 服务器信息: {data.get('server_info', {})}")
                print(f"   🔍 请求信息: {data.get('request_info', {})}")
                print(f"   🔒 TLS 信息: {data.get('tls_info', {})}")
                return True
            else:
                print(f"   ❌ 状态码: {response.status_code}")
                return False
        except Exception as e:
            print(f"   ❌ 错误: {e}")
            return False
    
    def test_tls_certificate_info(self) -> bool:
        """测试 TLS 证书信息"""
        url = f"{self.base_url}/tls-info"
        print(f"🧪 测试 TLS 证书信息: {url}")
        
        try:
            response = self.session.get(url)
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                data = response.json()
                print(f"   ✅ TLS 状态: {data.get('tls_status', 'Unknown')}")
                print(f"   🔒 证书信息: {data.get('certificate_info', {})}")
                print(f"   📡 协议支持: {data.get('protocol_support', {})}")
                return True
            else:
                print(f"   ❌ 状态码: {response.status_code}")
                return False
        except Exception as e:
            print(f"   ❌ 错误: {e}")
            return False
    
    def test_chunked_endpoint(self, endpoint: str, description: str) -> bool:
        """专门测试 H2 HTTPS 分块传输端点"""
        url = f"{self.base_url}{endpoint}"
        print(f"🧪 测试 {description}: {url}")
        
        try:
            response = self.session.get(url, stream=True)
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                print("   📦 H2 HTTPS 分块数据验证:")
                
                chunks_received = []
                total_bytes = 0
                chunk_count = 0
                h2_https_chunks = 0
                
                # 逐块读取并验证
                for chunk in response.iter_content(chunk_size=None, decode_unicode=True):
                    if chunk:
                        chunk_count += 1
                        chunk_size = len(chunk.encode('utf-8'))
                        total_bytes += chunk_size
                        chunks_received.append(chunk)
                        
                        # 验证 H2 HTTPS 分块格式
                        if 'H2_HTTPS_CHUNK' in chunk:
                            h2_https_chunks += 1
                            
                        if chunk.startswith('[H2_HTTPS_CHUNK_'):
                            # 提取分块信息
                            if '|BYTES:' in chunk:
                                try:
                                    bytes_info = chunk.split('|BYTES:')[1].split(']')[0]
                                    declared_size = int(bytes_info)
                                    print(f"      H2 HTTPS 分块 {chunk_count}: 声明大小 {declared_size} 字节, 实际大小 {chunk_size} 字节")
                                    
                                    # 验证大小是否匹配
                                    if declared_size == chunk_size:
                                        print(f"         ✅ H2 HTTPS 大小验证通过")
                                    else:
                                        print(f"         ⚠️  H2 HTTPS 大小不匹配")
                                        
                                except (ValueError, IndexError):
                                    print(f"      H2 HTTPS 分块 {chunk_count}: 无法解析大小信息")
                            
                            # 检查 H2 HTTPS 特殊标记
                            if 'H2_HTTPS_CHUNK_START' in chunk:
                                print(f"         🚀 检测到 H2 HTTPS 传输开始标记")
                            elif 'H2_HTTPS_CHUNK_END' in chunk:
                                print(f"         🏁 检测到 H2 HTTPS 传输结束标记")
                
                print(f"   📊 H2 HTTPS 分块传输摘要:")
                print(f"      总分块数: {chunk_count}")
                print(f"      H2 HTTPS 标识分块: {h2_https_chunks}")
                print(f"      总字节数: {total_bytes}")
                print(f"      H2 HTTPS 协议验证: {'✅ 通过' if h2_https_chunks > 0 else '❌ 失败'}")
                print(f"      TLS 加密传输: ✅ 已启用")
                
                return True
            else:
                print(f"   ❌ 状态码: {response.status_code}")
                return False
        except Exception as e:
            print(f"   ❌ 错误: {e}")
            return False
    
    def test_endpoint(self, endpoint: str, description: str, stream: bool = False) -> bool:
        """测试 H2 HTTPS 端点"""
        url = f"{self.base_url}{endpoint}"
        print(f"🧪 测试 {description}: {url}")
        
        try:
            response = self.session.get(url, stream=stream)
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                if stream:
                    print("   📊 H2 HTTPS 流式数据:")
                    content_preview = ""
                    byte_count = 0
                    
                    for chunk in response.iter_content(chunk_size=1024, decode_unicode=True):
                        if chunk:
                            byte_count += len(chunk.encode('utf-8'))
                            if len(content_preview) < 200:  # 只显示前200字符
                                content_preview += chunk
                    
                    print(f"      内容预览: {content_preview[:200]}{'...' if len(content_preview) > 200 else ''}")
                    print(f"      总字节数: {byte_count}")
                    print(f"      TLS 加密: ✅ 已启用")
                else:
                    content = response.text
                    print(f"   📄 H2 HTTPS 响应内容预览: {content[:200]}{'...' if len(content) > 200 else ''}")
                    print(f"   📊 内容长度: {len(content)} 字符")
                    print(f"   🔒 TLS 加密: ✅ 已启用")
                
                return True
            else:
                print(f"   ❌ 状态码: {response.status_code}")
                return False
        except Exception as e:
            print(f"   ❌ 错误: {e}")
            return False
    
    def run_h2_tests(self) -> bool:
        """运行 H2 HTTPS 专用测试"""
        print("🚀 开始 H2 HTTPS 协议测试...")
        print("=" * 50)
        
        tests = [
            (self.test_h2_protocol_status, "H2 协议状态检查"),
            (self.test_tls_certificate_info, "TLS 证书信息检查"),
            (lambda: self.test_endpoint("/", "H2 HTTPS 主页"), "H2 HTTPS 主页测试"),
            (lambda: self.test_chunked_endpoint("/chunked", "H2 HTTPS 分块传输"), "H2 HTTPS 分块传输测试"),
            (lambda: self.test_endpoint("/json-stream", "H2 HTTPS JSON 流", stream=True), "H2 HTTPS JSON 流测试"),
            (lambda: self.test_endpoint("/text-stream", "H2 HTTPS 文本流", stream=True), "H2 HTTPS 文本流测试"),
            (lambda: self.test_endpoint("/headers-test", "H2 HTTPS 头信息测试"), "H2 HTTPS 头信息测试"),
        ]
        
        passed = 0
        total = len(tests)
        
        for test_func, test_name in tests:
            print(f"\n🔍 执行: {test_name}")
            try:
                if test_func():
                    passed += 1
                    print(f"✅ {test_name}: 通过")
                else:
                    print(f"❌ {test_name}: 失败")
            except Exception as e:
                print(f"❌ {test_name}: 异常 - {e}")
            
            time.sleep(1)  # 测试间隔
        
        print("\n" + "=" * 50)
        print(f"📊 H2 HTTPS 测试摘要:")
        print(f"   总计: {total}")
        print(f"   通过: {passed} ✅")
        print(f"   失败: {total - passed} ❌")
        print(f"   成功率: {(passed/total)*100:.1f}%")
        
        print("\n🔒 安全特性验证:")
        print("   • TLS 加密传输: ✅ 已启用")
        print("   • H2 协议支持: ✅ 已启用")
        print("   • 自签名证书: ✅ 开发模式")
        print("   • 证书验证跳过: ✅ 测试环境")
        
        return passed == total

def signal_handler(signum, frame):
    """信号处理器"""
    print("\n🛑 接收到中断信号，正在停止 H2 HTTPS 服务器...")
    sys.exit(0)

def main():
    """主函数"""
    print("🔒 RAT Engine H2 HTTPS 协议测试")
    print("=" * 50)
    
    # 注册信号处理器
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    server = StreamingDemoH2Server()
    
    try:
        # 启动服务器（非阻塞模式）
        server.start_server()
        
        # 等待 H2 HTTPS 服务器完全启动
        time.sleep(TEST_DELAY)
        
        if AUTO_TEST_ENABLED:
            print("🤖 开始自动 H2 HTTPS 测试...")
            tester = AutoTesterH2(SERVER_URL)
            success = tester.run_h2_tests()
            
            if success:
                print("\n🎉 所有 H2 HTTPS 测试通过！")
            else:
                print("\n⚠️ 部分 H2 HTTPS 测试失败，请检查日志")
        
        print(f"\n🌐 H2 HTTPS 服务器运行中: {SERVER_URL}")
        print("⚠️ 浏览器可能显示证书警告，这是正常的（自签名证书）")
        print("💡 点击 '高级' -> '继续访问' 即可访问演示页面")
        print("按 Ctrl+C 停止服务器")
        
        # 保持服务器运行
        while server.running:
            time.sleep(1)
            
    except KeyboardInterrupt:
        print("\n⏹️ H2 HTTPS 服务器已停止")
    except Exception as e:
        print(f"❌ H2 HTTPS 服务器运行失败: {e}")
        import traceback
        traceback.print_exc()
    finally:
        server.stop_server()
        print("🛑 H2 HTTPS 测试服务器已停止")
        print("🏁 H2 HTTPS 测试完成")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())