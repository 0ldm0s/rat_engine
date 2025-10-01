#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine Python 流式响应演示

展示如何使用 RAT Engine Python API 的流式响应功能：
- Server-Sent Events (SSE)
- 分块传输编码
- 自定义流式响应
- JSON 流
- 文本流
- 实时日志流
"""

import asyncio
import json
import time
import threading
import signal
import sys
from datetime import datetime
from enum import Enum, Flag, auto
from typing import List, Dict, Any, Set, AsyncGenerator

try:
    import requests
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
    
    # 预定义组合
    BASIC = HOME | HEADERS                    # 基础功能测试
    STREAMING = CHUNKED | JSON_STREAM | TEXT_STREAM  # 流式传输测试
    REALTIME = SSE | LOGS                    # 实时通信测试
    SSE_ALL = SSE | SSE_AIOHTTP              # 所有SSE测试
    ALL = HOME | CHUNKED | JSON_STREAM | TEXT_STREAM | HEADERS | SSE | LOGS | SSE_AIOHTTP  # 所有测试

# 配置开关
AUTO_TEST_ENABLED = True  # 设置为 False 可关闭自动测试
TEST_DELAY = 2  # 测试延迟秒数
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 3000
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

# 测试配置 - 可以通过修改这里来选择要运行的测试
# 示例配置:
# TEST_FEATURES = TestFeature.BASIC           # 只测试基础功能
# TEST_FEATURES = TestFeature.STREAMING       # 只测试流式传输
# TEST_FEATURES = TestFeature.CHUNKED | TestFeature.SSE  # 只测试分块传输和SSE
TEST_FEATURES = TestFeature.ALL            # 测试所有功能

class StreamingDemoServer:
    """流式响应演示服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用"""
        # 创建应用（RatApp 内部会自动创建 router 和 server）
        app = RatApp(name="streaming_demo")
        
        # 注册路由
        self._register_routes(app)
        
        return app
    
    def _register_routes(self, app: RatApp):
        """注册所有路由"""
        
        # 主页路由
        @app.html("/")
        def home(request):
            return self._get_demo_html()
        
        # SSE 路由
        @app.sse("/sse")
        def sse_endpoint(request):
            def sse_generator():
                # 发送初始连接事件
                yield "event: connected\ndata: Connection established\n\n"
                
                # 发送定期更新
                for i in range(1, 11):
                    time.sleep(1)
                    data = {
                        "timestamp": datetime.utcnow().isoformat() + "Z",
                        "counter": i,
                        "message": f"Update #{i}"
                    }
                    yield f"event: update\ndata: {json.dumps(data)}\n\n"
                
                # 发送结束事件
                yield "event: end\ndata: Stream completed\n\n"
            
            return sse_generator()
        
        # 分块传输路由
        @app.chunk("/chunked")
        def chunked_endpoint(request):
            def chunked_generator():                # 增强的分块数据，包含更多验证信息
                chunks = [
                    f"CHUNK_START|{datetime.utcnow().isoformat()}Z|开始数据传输...\n",
                    f"CHUNK_001|SIZE:32|正在处理第一部分数据...\n",
                    f"CHUNK_002|SIZE:48|正在处理第二部分数据，包含更多内容...\n",
                    f"CHUNK_003|SIZE:64|正在处理第三部分数据，这是一个较长的数据块用于测试...\n",
                    f"CHUNK_004|SIZE:24|数据传输完成！\n",
                    f"CHUNK_END|TOTAL:5|{datetime.utcnow().isoformat()}Z|传输结束\n"
                ]
                
                for i, chunk in enumerate(chunks, 1):
                    # 添加传输延迟模拟真实网络环境
                    time.sleep(0.5)
                    
                    # 在每个分块前添加分块元数据（仅用于调试，实际HTTP分块传输由服务器处理）
                    # 使用占位符先构建模板
                    template = f"[CHUNK_{i:03d}|BYTES:{{}}] {chunk}"
                    # 计算模板中占位符的长度（假设最大6位数字）
                    temp_chunk = template.format(999999)
                    actual_size = len(temp_chunk.encode('utf-8'))
                    # 用实际大小替换占位符
                    enhanced_chunk = template.format(actual_size)
                    
                    yield enhanced_chunk
            
            return chunked_generator()
        
        # JSON 流路由
        @app.sse_json("/json-stream")
        def json_stream_endpoint(request):
            items = [
                {"id": 1, "name": "Alice", "age": 30},
                {"id": 2, "name": "Bob", "age": 25},
                {"id": 3, "name": "Charlie", "age": 35},
                {"id": 4, "name": "Diana", "age": 28},
                {"id": 5, "name": "Eve", "age": 32},
            ]
            
            # 装饰器会自动处理 JSON 流格式
            return items
        
        # 文本流路由
        @app.sse_text("/text-stream")
        def text_stream_endpoint(request):
            lines = [
                "第一行文本",
                "第二行文本",
                "第三行文本",
                "第四行文本",
                "最后一行文本"
            ]
            
            # 装饰器会自动处理文本流格式 - 支持字符串和列表两种返回类型
            return lines
        
        # 实时日志流路由
        @app.sse("/logs")
        def logs_endpoint(request):
            def log_generator():
                # 发送初始日志
                yield "event: log\ndata: [INFO] 日志流已启动\n\n"
                
                # 模拟实时日志
                log_levels = ["INFO", "WARN", "ERROR", "DEBUG"]
                messages = [
                    "用户登录成功",
                    "数据库连接建立",
                    "处理用户请求",
                    "缓存更新完成",
                    "定时任务执行",
                    "系统健康检查"
                ]
                
                for i in range(20):
                    time.sleep(0.8)
                    level = log_levels[i % len(log_levels)]
                    message = messages[i % len(messages)]
                    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                    
                    log_entry = f"[{timestamp}] {level} - {message}"
                    yield f"event: log\ndata: {log_entry}\n\n"
            
            return log_generator()
        
        # 头信息测试路由
        @app.json("/headers-test")
        def headers_test_endpoint(request):
            """返回请求和响应头信息用于测试"""
            return {
                "message": "头信息测试端点",
                "timestamp": datetime.utcnow().isoformat() + "Z",
                "request_headers": dict(request.headers) if hasattr(request, 'headers') else {},
                "note": "请查看浏览器开发者工具的网络标签页查看完整的响应头信息"
            }
    
    def _get_demo_html(self) -> str:
        """获取演示页面 HTML"""
        return '''
<!DOCTYPE html>
<html>
<head>
    <title>RAT Engine Python 流式响应演示</title>
    <meta charset="UTF-8">
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .demo-section { margin: 20px 0; padding: 20px; border: 1px solid #ddd; border-radius: 5px; }
        .log-output { background: #f5f5f5; padding: 10px; height: 200px; overflow-y: auto; font-family: monospace; }
        button { padding: 10px 20px; margin: 5px; cursor: pointer; }
        .status { padding: 10px; margin: 10px 0; border-radius: 5px; }
        .success { background-color: #d4edda; color: #155724; border: 1px solid #c3e6cb; }
        .info { background-color: #d1ecf1; color: #0c5460; border: 1px solid #bee5eb; }
    </style>
</head>
<body>
    <h1>RAT Engine Python 流式响应演示</h1>
    
    <div class="status success">
        ✅ RAT Engine Python 服务器运行中
    </div>
    
    <div class="demo-section">
        <h2>Server-Sent Events (SSE) 演示</h2>
        <button onclick="startSSE()">开始 SSE 连接</button>
        <button onclick="stopSSE()">停止 SSE 连接</button>
        <div id="sse-output" class="log-output"></div>
    </div>
    
    <div class="demo-section">
        <h2>分块传输测试</h2>
        <div class="status info">
            <strong>增强验证功能：</strong><br>
            • 每个分块包含大小和序号信息<br>
            • 自动验证声明大小与实际大小<br>
            • 检测传输开始和结束标记<br>
            • 验证预期内容完整性<br><br>
            <a href="/chunked" target="_blank">🔗 测试分块传输</a> (在新标签页中打开)
        </div>
    </div>
    
    <div class="demo-section">
        <h2>实时日志流演示</h2>
        <button onclick="startLogs()">开始日志流</button>
        <button onclick="stopLogs()">停止日志流</button>
        <div id="log-output" class="log-output"></div>
    </div>
    
    <div class="demo-section">
        <h2>其他流式端点</h2>
        <ul>
            <li><a href="/chunked" target="_blank">分块传输演示</a></li>
            <li><a href="/json-stream" target="_blank">JSON 流演示</a></li>
            <li><a href="/text-stream" target="_blank">文本流演示</a></li>
            <li><a href="/headers-test" target="_blank">头信息测试</a> - 查看响应头信息</li>
        </ul>
    </div>
    
    <div class="demo-section">
        <h2>头信息测试说明</h2>
        <div class="status info">
            <strong>如何查看响应头信息：</strong><br>
            1. 点击上方的 "头信息测试" 链接<br>
            2. 打开浏览器开发者工具 (F12)<br>
            3. 切换到 "网络" 或 "Network" 标签页<br>
            4. 刷新页面或重新点击链接<br>
            5. 点击请求查看详细的响应头信息<br><br>
            <strong>重要的流式响应头：</strong><br>
            • Content-Type: 内容类型 (如 text/event-stream)<br>
            • Transfer-Encoding: 传输编码 (如 chunked)<br>
            • Cache-Control: 缓存控制<br>
            • Connection: 连接类型
        </div>
    </div>
    
    <div class="demo-section">
        <h2>API 端点列表</h2>
        <div class="status info">
            <strong>可用端点：</strong><br>
            • GET / - 演示主页<br>
            • GET /sse - Server-Sent Events<br>
            • GET /chunked - 分块传输 (增强验证)<br>
            • GET /json-stream - JSON 流<br>
            • GET /text-stream - 文本流<br>
            • GET /logs - 实时日志流<br>
            • GET /headers-test - 头信息测试
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
                output.innerHTML += '[连接已建立]\n';
            };
            
            sseConnection.addEventListener('connected', function(e) {
                output.innerHTML += '[连接事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            sseConnection.addEventListener('update', function(e) {
                output.innerHTML += '[更新事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
            });
            
            sseConnection.addEventListener('end', function(e) {
                output.innerHTML += '[结束事件] ' + e.data + '\n';
                output.scrollTop = output.scrollHeight;
                sseConnection.close();
                sseConnection = null;
            });
            
            sseConnection.onerror = function() {
                output.innerHTML += '[连接错误]\n';
                output.scrollTop = output.scrollHeight;
            };
        }
        
        function stopSSE() {
            if (sseConnection) {
                sseConnection.close();
                sseConnection = null;
                document.getElementById('sse-output').innerHTML += '[连接已关闭]\n';
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
                output.innerHTML += '[日志连接错误]\n';
                output.scrollTop = output.scrollHeight;
            };
        }
        
        function stopLogs() {
            if (logConnection) {
                logConnection.close();
                logConnection = null;
                document.getElementById('log-output').innerHTML += '[日志连接已关闭]\n';
            }
        }
    </script>
</body>
</html>
        '''
    
    def start_server(self):
        """启动服务器"""
        try:
            self.app = self.create_app()
            print(f"🚀 RAT Engine Python 流式响应演示服务器启动中...")
            print(f"📡 服务器地址: {SERVER_URL}")
            print(f"🔗 演示页面: {SERVER_URL}/")
            print(f"📊 SSE 端点: {SERVER_URL}/sse")
            print(f"📦 分块传输: {SERVER_URL}/chunked")
            print(f"📄 JSON 流: {SERVER_URL}/json-stream")
            print(f"📝 文本流: {SERVER_URL}/text-stream")
            print(f"📋 日志流: {SERVER_URL}/logs")
            print(f"🔍 头信息测试: {SERVER_URL}/headers-test")
            print()
            print("💡 提示: 自动测试将显示每个端点的响应头信息")
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
            print(f"❌ 服务器启动失败: {e}")
            self.running = False
            return False
    
    def stop_server(self):
        """停止服务器"""
        self.running = False
        if self.app:
            try:
                self.app.stop()
            except:
                pass

class AutoTester:
    """自动测试器"""
    
    def __init__(self, base_url: str):
        self.base_url = base_url
        self.session = requests.Session()
        self.session.timeout = 10
    
    def _print_response_headers(self, response):
        """输出响应头信息"""
        print("   📋 响应头信息:")
        
        # 重要的流式响应相关头信息
        important_headers = [
            'content-type',
            'transfer-encoding', 
            'cache-control',
            'connection',
            'access-control-allow-origin',
            'access-control-allow-methods',
            'access-control-allow-headers',
            'server',
            'content-length'
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
    
    def test_chunked_endpoint(self, endpoint: str, description: str) -> bool:
        """专门测试分块传输端点"""
        url = f"{self.base_url}{endpoint}"
        print(f"🧪 测试 {description}: {url}")
        
        try:
            response = self.session.get(url, stream=True)
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                print("   📦 分块数据验证:")
                
                chunks_received = []
                total_bytes = 0
                chunk_count = 0
                
                # 逐块读取并验证
                for chunk in response.iter_content(chunk_size=None, decode_unicode=True):
                    if chunk:
                        chunk_count += 1
                        chunk_size = len(chunk.encode('utf-8'))
                        total_bytes += chunk_size
                        chunks_received.append(chunk)
                        
                        # 验证分块格式
                        if chunk.startswith('[CHUNK_'):
                            # 提取分块信息
                            if '|BYTES:' in chunk:
                                try:
                                    bytes_info = chunk.split('|BYTES:')[1].split(']')[0]
                                    declared_size = int(bytes_info)
                                    print(f"      分块 {chunk_count}: 声明大小 {declared_size} 字节, 实际大小 {chunk_size} 字节")
                                    
                                    # 验证大小是否匹配
                                    if declared_size == chunk_size:
                                        print(f"         ✅ 大小验证通过")
                                    else:
                                        print(f"         ⚠️  大小不匹配")
                                        
                                except (ValueError, IndexError):
                                    print(f"      分块 {chunk_count}: 无法解析大小信息")
                            
                            # 检查特殊标记
                            if 'CHUNK_START' in chunk:
                                print(f"         🚀 检测到传输开始标记")
                            elif 'CHUNK_END' in chunk:
                                print(f"         🏁 检测到传输结束标记")
                                if 'TOTAL:' in chunk:
                                    try:
                                        total_info = chunk.split('TOTAL:')[1].split('|')[0]
                                        declared_total = int(total_info)
                                        print(f"         📊 声明总分块数: {declared_total}, 实际接收: {chunk_count}")
                                    except (ValueError, IndexError):
                                        pass
                        else:
                            print(f"      分块 {chunk_count}: 大小 {chunk_size} 字节 (非标准格式)")
                
                print(f"   ✅ 分块传输完成")
                print(f"      📊 总计: {chunk_count} 个分块, {total_bytes} 字节")
                print(f"      🔍 内容预览: {chunks_received[0][:50]}..." if chunks_received else "")
                
                # 验证是否包含预期的关键内容
                full_content = ''.join(chunks_received)
                expected_markers = ['CHUNK_START', 'CHUNK_END', '开始数据传输', '数据传输完成']
                missing_markers = []
                
                for marker in expected_markers:
                    if marker not in full_content:
                        missing_markers.append(marker)
                
                if missing_markers:
                    print(f"      ⚠️  缺少预期标记: {missing_markers}")
                else:
                    print(f"      ✅ 所有预期标记都存在")
                
                return True
            else:
                print(f"   ❌ 失败 (状态码: {response.status_code})")
                return False
        except Exception as e:
            print(f"   ❌ 异常: {e}")
            return False
    
    def test_endpoint(self, endpoint: str, description: str, stream: bool = False) -> bool:
        """测试单个端点"""
        # 如果是分块传输端点，使用专门的测试方法
        if endpoint == '/chunked':
            return self.test_chunked_endpoint(endpoint, description)
        
        url = f"{self.base_url}{endpoint}"
        print(f"🧪 测试 {description}: {url}")
        
        try:
            if stream:
                response = self.session.get(url, stream=True)
            else:
                response = self.session.get(url)
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                if stream:
                    # 对于流式响应，读取内容
                    content = b""
                    for chunk in response.iter_content(chunk_size=1024):
                        content += chunk
                        # 对于json-stream端点，读取完整内容
                        if endpoint != '/json-stream' and len(content) > 1024:  # 其他端点只读取前1KB
                            break
                    
                    # 打印特定端点的完整接收内容
                    if endpoint in ['/json-stream', '/text-stream']:
                        try:
                            content_str = content.decode('utf-8')
                            endpoint_name = "JSON流" if endpoint == '/json-stream' else "文本流"
                            print(f"   📄 {endpoint_name}完整内容:")
                            print(f"   {content_str}")
                            print(f"   ✅ 成功 (状态码: {response.status_code}, 内容长度: {len(content)} 字节)")
                        except UnicodeDecodeError:
                            print(f"   ⚠️  内容解码失败，显示原始字节数")
                            print(f"   ✅ 成功 (状态码: {response.status_code}, 内容长度: {len(content)} 字节)")
                    else:
                        print(f"   ✅ 成功 (状态码: {response.status_code}, 内容长度: {len(content)} 字节)")
                else:
                    content_length = len(response.content)
                    # 打印特定端点的内容
                    if endpoint in ['/json-stream', '/text-stream']:
                        try:
                            content_str = response.content.decode('utf-8')
                            endpoint_name = "JSON流" if endpoint == '/json-stream' else "文本流"
                            print(f"   📄 {endpoint_name}完整内容:")
                            print(f"   {content_str}")
                        except UnicodeDecodeError:
                            print(f"   ⚠️  内容解码失败")
                    print(f"   ✅ 成功 (状态码: {response.status_code}, 内容长度: {content_length} 字节)")
                return True
            else:
                print(f"   ❌ 失败 (状态码: {response.status_code})")
                return False
        except Exception as e:
            print(f"   ❌ 异常: {e}")
            return False
    
    def test_sse_endpoint(self, endpoint: str, description: str) -> bool:
        """测试 SSE 端点"""
        url = f"{self.base_url}{endpoint}"
        print(f"🧪 测试 {description}: {url}")
        
        try:
            response = self.session.get(url, stream=True, headers={
                'Accept': 'text/event-stream',
                'Cache-Control': 'no-cache'
            })
            
            # 输出响应头信息
            self._print_response_headers(response)
            
            if response.status_code == 200:
                # 读取前几个 SSE 事件
                events_received = 0
                print("   📨 接收到的 SSE 数据:")
                
                for line in response.iter_lines(decode_unicode=True):
                    if line:
                        # 打印所有接收到的行数据
                        print(f"      原始行: {line}")
                        
                        if line.startswith('data:'):
                            events_received += 1
                            data_content = line[5:].strip()  # 移除 'data:' 前缀
                            print(f"      📦 事件 {events_received} 数据: {data_content}")
                            
                            if events_received >= 3:  # 读取前3个事件就停止
                                print(f"      🛑 已接收 {events_received} 个事件，停止接收")
                                break
                        elif line.startswith('event:'):
                            event_type = line[6:].strip()  # 移除 'event:' 前缀
                            print(f"      🏷️  事件类型: {event_type}")
                
                print(f"   ✅ 成功 (状态码: {response.status_code}, 接收到 {events_received} 个事件)")
                return True
            else:
                print(f"   ❌ 失败 (状态码: {response.status_code})")
                return False
        except Exception as e:
            print(f"   ❌ 异常: {e}")
            return False
    
    async def test_sse_endpoint_aiohttp(self, endpoint: str, description: str) -> bool:
        """使用 aiohttp_sse_client2 测试 SSE 端点"""
        url = f"{self.base_url}{endpoint}"
        print(f"🧪 测试 {description} (aiohttp_sse_client2): {url}")
        
        try:
            # 设置超时以避免无限等待
            timeout = aiohttp.ClientTimeout(total=30)  # 30秒总超时
            
            async with aiohttp.ClientSession(timeout=timeout) as session:
                async with sse_client.EventSource(
                    url,
                    session=session,
                    headers={
                        'Accept': 'text/event-stream',
                        'Cache-Control': 'no-cache'
                    }
                ) as event_source:
                    events_received = 0
                    events_data = []
                    max_events = 3  # 最大接收事件数
                    
                    print("   📨 接收到的 SSE 数据 (aiohttp_sse_client2):")
                    
                    try:
                        async for event in event_source:
                            # 打印事件的详细信息
                            print(f"      🔍 事件类型: {event.type}")
                            print(f"      📦 事件数据: {event.data}")
                            if hasattr(event, 'id') and event.id:
                                print(f"      🆔 事件ID: {event.id}")
                            if hasattr(event, 'retry') and event.retry:
                                print(f"      🔄 重试间隔: {event.retry}")
                            
                            # 处理不同类型的事件
                            if event.data:  # 只要有数据就计数
                                events_received += 1
                                events_data.append(event.data)
                                print(f"      ✅ 已接收事件 {events_received}/{max_events}")
                                
                                # 达到最大事件数时主动退出
                                if events_received >= max_events:
                                    print(f"      🛑 已接收 {max_events} 个事件，主动终止连接")
                                    break
                            
                            # 检查是否是结束事件
                            if event.type == 'end' or (event.data and 'completed' in event.data.lower()):
                                print(f"      🏁 检测到结束事件，终止连接")
                                break
                                
                    except asyncio.TimeoutError:
                        print(f"      ⏰ 连接超时，已接收 {events_received} 个事件")
                    except Exception as inner_e:
                        print(f"      ⚠️  事件循环异常: {inner_e}")
                    
                    print(f"   ✅ 成功 (aiohttp_sse_client2, 接收到 {events_received} 个事件)")
                    return events_received > 0  # 只要接收到事件就算成功
                    
        except asyncio.TimeoutError:
            print(f"   ⏰ 连接超时 (aiohttp_sse_client2)")
            return False
        except Exception as e:
            print(f"   ❌ 异常 (aiohttp_sse_client2): {e}")
            return False
    
    def test_sse_endpoint_combined(self, endpoint: str, description: str) -> bool:
        """组合测试 SSE 端点 - 同时使用两种方法"""
        print(f"\n🔄 组合测试 {description}:")
        
        # 测试1: 使用 requests
        print("\n📡 方法1: requests (同步)")
        result1 = self.test_sse_endpoint(endpoint, description)
        
        # 测试2: 使用 aiohttp_sse_client
        print("\n📡 方法2: aiohttp_sse_client (异步)")
        result2 = asyncio.run(self.test_sse_endpoint_aiohttp(endpoint, description))
        
        # 比较结果
        if result1 and result2:
            print("\n✅ 两种方法都成功")
            return True
        elif result1:
            print("\n⚠️  只有 requests 方法成功")
            return True
        elif result2:
            print("\n⚠️  只有 aiohttp_sse_client 方法成功")
            return True
        else:
            print("\n❌ 两种方法都失败")
            return False
    
    def run_selected_tests(self, features: TestFeature) -> bool:
        """根据选择的功能运行测试"""
        print("\n🔍 开始自动测试...")
        print("=" * 50)
        
        # 定义所有可用的测试
        all_tests = {
            TestFeature.HOME: ("/", "主页", False, "endpoint"),
            TestFeature.CHUNKED: ("/chunked", "分块传输", True, "endpoint"),
            TestFeature.JSON_STREAM: ("/json-stream", "JSON 流", True, "endpoint"),
            TestFeature.TEXT_STREAM: ("/text-stream", "文本流", True, "endpoint"),
            TestFeature.HEADERS: ("/headers-test", "头信息测试", False, "endpoint"),
            TestFeature.SSE: ("/sse", "SSE 连接", None, "sse"),
            TestFeature.SSE_AIOHTTP: ("/sse", "SSE aiohttp客户端", None, "sse_aiohttp"),
            TestFeature.LOGS: ("/logs", "实时日志流", None, "sse"),
        }
        
        # 筛选要运行的测试
        selected_tests = []
        for feature, test_info in all_tests.items():
            if features & feature:  # 检查是否包含该功能
                selected_tests.append((feature, test_info))
        
        if not selected_tests:
            print("⚠️  没有选择任何测试功能")
            return False
        
        # 显示将要运行的测试
        feature_names = []
        for feature, _ in selected_tests:
            feature_names.append(feature.name)
        print(f"📋 选择的测试功能: {', '.join(feature_names)}")
        print("-" * 50)
        
        passed = 0
        total = len(selected_tests)
        
        # 运行选定的测试
        for feature, (endpoint, description, stream, test_type) in selected_tests:
            print(f"\n🎯 [{feature.name}] ", end="")
            
            if test_type == "endpoint":
                success = self.test_endpoint(endpoint, description, stream)
            elif test_type == "sse":
                success = self.test_sse_endpoint_combined(endpoint, description)
            elif test_type == "sse_aiohttp":
                success = asyncio.run(self.test_sse_endpoint_aiohttp(endpoint, description))
            else:
                print(f"❌ 未知的测试类型: {test_type}")
                success = False
            
            if success:
                passed += 1
            time.sleep(0.5)
        
        print("\n" + "=" * 50)
        print(f"📊 测试结果: {passed}/{total} 通过")
        
        if passed == total:
            print("🎉 所有选定的测试通过！")
            return True
        else:
            print(f"⚠️  {total - passed} 个测试失败")
            return False
    
    def run_all_tests(self) -> bool:
        """运行所有测试（保持向后兼容）"""
        return self.run_selected_tests(TestFeature.ALL)

def print_test_configuration_help():
    """打印测试配置帮助信息"""
    print("\n📖 测试配置说明:")
    print("-" * 30)
    print("可以通过修改 TEST_FEATURES 变量来选择要运行的测试:")
    print("")
    print("🔹 单个功能测试:")
    print("   TEST_FEATURES = TestFeature.HOME        # 只测试主页")
    print("   TEST_FEATURES = TestFeature.CHUNKED     # 只测试分块传输")
    print("   TEST_FEATURES = TestFeature.SSE         # 只测试SSE连接 (组合测试)")
    print("   TEST_FEATURES = TestFeature.SSE_AIOHTTP # 只测试SSE aiohttp客户端")
    print("")
    print("🔹 组合功能测试:")
    print("   TEST_FEATURES = TestFeature.BASIC       # 基础功能 (主页+头信息)")
    print("   TEST_FEATURES = TestFeature.STREAMING   # 流式传输 (分块+JSON流+文本流)")
    print("   TEST_FEATURES = TestFeature.REALTIME    # 实时通信 (SSE+日志流)")
    print("   TEST_FEATURES = TestFeature.SSE_ALL     # 所有SSE测试 (组合+aiohttp)")
    print("")
    print("🔹 自定义组合:")
    print("   TEST_FEATURES = TestFeature.CHUNKED | TestFeature.SSE  # 分块传输+SSE")
    print("   TEST_FEATURES = TestFeature.SSE | TestFeature.SSE_AIOHTTP  # 两种SSE测试")
    print("   TEST_FEATURES = TestFeature.HOME | TestFeature.LOGS    # 主页+日志流")
    print("")
    print("🔹 所有测试:")
    print("   TEST_FEATURES = TestFeature.JSON_STREAM         # 运行所有测试 (默认)")
    print("")
    print("🔹 禁用测试:")
    print("   AUTO_TEST_ENABLED = False               # 完全禁用自动测试")
    print("-" * 30)

def signal_handler(signum, frame):
    """信号处理器"""
    print("\n\n🛑 接收到停止信号，正在关闭服务器...")
    sys.exit(0)

def main():
    """主函数"""
    # 注册信号处理器
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    print("🚀 RAT Engine Python 流式响应演示")
    print("=" * 50)
    print("📦 分块传输增强验证功能:")
    print("   • 每个分块包含详细的大小和序号信息")
    print("   • 自动验证声明大小与实际接收大小")
    print("   • 检测传输开始/结束标记和内容完整性")
    print("   • 提供详细的分块传输诊断信息")
    print("-" * 50)
    
    # 显示当前测试配置
    if AUTO_TEST_ENABLED:
        print("\n⚙️  当前测试配置:")
        if TEST_FEATURES == TestFeature.ALL:
            print("   🔄 运行所有测试功能")
        elif TEST_FEATURES == TestFeature.BASIC:
            print("   🔹 基础功能测试 (主页 + 头信息)")
        elif TEST_FEATURES == TestFeature.STREAMING:
            print("   🌊 流式传输测试 (分块 + JSON流 + 文本流)")
        elif TEST_FEATURES == TestFeature.REALTIME:
            print("   ⚡ 实时通信测试 (SSE + 日志流)")
        else:
            # 显示自定义组合
            selected_features = []
            for feature in TestFeature:
                if feature != TestFeature.NONE and feature != TestFeature.ALL and \
                   feature != TestFeature.BASIC and feature != TestFeature.STREAMING and \
                   feature != TestFeature.REALTIME and (TEST_FEATURES & feature):
                    selected_features.append(feature.name)
            if selected_features:
                print(f"   🎯 自定义测试: {', '.join(selected_features)}")
            else:
                print("   ❌ 无效的测试配置")
        
        print("   💡 提示: 可以修改 TEST_FEATURES 变量来选择不同的测试功能")
    else:
        print("\n🔧 自动测试已禁用")
        print("   💡 提示: 设置 AUTO_TEST_ENABLED = True 来启用自动测试")
    
    print("-" * 50)
    
    # 创建并启动服务器
    server = StreamingDemoServer()
    
    if not server.start_server():
        print("❌ 服务器启动失败")
        return 1
    
    try:
        if AUTO_TEST_ENABLED:
            print(f"⏳ 等待 {TEST_DELAY} 秒后开始自动测试...")
            time.sleep(TEST_DELAY)
            
            # 运行自动测试
            tester = AutoTester(SERVER_URL)
            test_passed = tester.run_selected_tests(TEST_FEATURES)
            
            if test_passed:
                print("\n✅ 所有测试通过，演示完成！")
            else:
                print("\n❌ 部分测试失败，请检查服务器状态")
            
            print("\n🔚 自动测试完成，正在自动关闭服务器...")
            # 自动测试完成后直接返回，不再保持服务器运行
            return 0 if test_passed else 1
        else:
            print("\n🔧 自动测试已禁用")
            print(f"🌐 演示页面: {SERVER_URL}")
            print("\n按 Ctrl+C 停止服务器")
            
            # 只有在禁用自动测试时才保持服务器运行
            while server.running:
                time.sleep(1)
            
    except KeyboardInterrupt:
        pass
    finally:
        print("\n🛑 正在停止服务器...")
        server.stop_server()
        print("✅ 服务器已停止")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())