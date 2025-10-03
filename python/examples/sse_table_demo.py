#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine SSE 表格实时刷新示例

特性：
- SSE 实时数据推送
- 动态表格更新
- 随机数据生成
- 现代化 Web UI
"""

import os
import sys
import json
import time
import random
import uuid
from datetime import datetime
from typing import Dict, List

try:
    from rat_engine import RatApp
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    sys.exit(1)

# 配置
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8088

# 全局状态
active_sessions: Dict[str, Dict] = {}
sse_connections: Dict[str, list] = {}
sse_messages: Dict[str, list] = {}  # 存储待发送的 SSE 消息
table_data: List[Dict] = []  # 表格数据

# 创建应用
app = RatApp(name="sse_table_demo")

# 配置日志
app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)

# 初始化模板引擎
from rat_engine.templates import TemplateEngine
template_engine = TemplateEngine(auto_escape=True, cache=True)

# 初始化一些示例数据
def _init_sample_data():
    """初始化示例数据"""
    global table_data
    table_data = [
        {"id": 1, "name": "张三", "age": 25, "city": "北京", "score": 85.5, "status": "活跃"},
        {"id": 2, "name": "李四", "age": 30, "city": "上海", "score": 92.3, "status": "活跃"},
        {"id": 3, "name": "王五", "age": 28, "city": "广州", "score": 78.9, "status": "离线"},
        {"id": 4, "name": "赵六", "age": 35, "city": "深圳", "score": 88.7, "status": "活跃"},
        {"id": 5, "name": "钱七", "age": 22, "city": "杭州", "score": 95.2, "status": "活跃"},
    ]

_init_sample_data()

# 主页
@app.html("/")
def table_page(request_data):
    """表格展示页面"""
    return _get_table_page()

# 获取当前表格数据
@app.json("/api/data")
def get_table_data(request_data):
    """获取当前表格数据"""
    return {
        "status": "success",
        "data": table_data,
        "timestamp": datetime.now().isoformat()
    }

# 开始数据生成
@app.json("/api/start", methods=["POST"])
def start_data_generation(request_data):
    """开始随机数据生成"""
    try:
        # 生成唯一会话ID
        session_id = str(uuid.uuid4()).replace('-', '')[:16]
        
        # 创建会话
        active_sessions[session_id] = {
            'created_at': datetime.now(),
            'active': True,
            'generation_count': 0
        }
        
        return {
            "status": "success",
            "session_id": session_id,
            "message": "数据生成已开始"
        }
        
    except Exception as e:
        return {"status": "error", "message": f"启动失败: {str(e)}"}

# 停止数据生成
@app.json("/api/stop/<session_id>", methods=["POST"])
def stop_data_generation(request_data, session_id):
    """停止数据生成"""
    if session_id in active_sessions:
        active_sessions[session_id]['active'] = False
        return {"status": "success", "message": "数据生成已停止"}
    else:
        return {"status": "error", "message": "会话不存在"}

# SSE 数据流
@app.sse("/api/stream/<session_id>")
def data_stream(request_data, session_id):
    """SSE 数据流"""
    if not session_id or session_id not in active_sessions:
        yield json.dumps({"error": "无效的会话ID"})
        return
    
    # 注册连接
    if session_id not in sse_connections:
        sse_connections[session_id] = []
    
    connection_id = f"conn_{len(sse_connections[session_id])}_{time.time()}"
    sse_connections[session_id].append(connection_id)
    
    try:
        session = active_sessions[session_id]
        
        # 发送初始数据
        initial_message = {
            'type': 'init',
            'session_id': session_id,
            'data': table_data,
            'timestamp': datetime.now().isoformat()
        }
        yield 'data: ' + json.dumps(initial_message) + '\n\n'
        
        # 开始数据生成循环
        start_time = time.time()
        last_update = time.time()
        
        while session.get('active', False) and time.time() - start_time < 300:  # 5分钟超时
            time.sleep(0.5)  # 每0.5秒检查一次
            
            # 处理待发送的消息
            if session_id in sse_messages and sse_messages[session_id]:
                message = sse_messages[session_id].pop(0)
                if message.get('session_id') == session_id:
                    yield 'data: ' + json.dumps(message) + '\n\n'
            
            # 每2秒生成新数据
            current_time = time.time()
            if current_time - last_update >= 2:
                _generate_random_data(session_id)
                last_update = current_time
            
            # 每10秒发送心跳
            if int(current_time) % 10 == 0:
                yield 'data: ' + json.dumps({
                    'type': 'heartbeat',
                    'timestamp': current_time
                }) + '\n\n'
                time.sleep(1)  # 避免重复发送心跳
        
        # 发送结束消息
        yield 'data: ' + json.dumps({
            'type': 'end',
            'session_id': session_id,
            'message': '数据生成已结束'
        }) + '\n\n'
        
    except Exception as e:
        print(f"SSE 连接错误: {e}")
    finally:
        # 清理连接
        if session_id in sse_connections and connection_id in sse_connections[session_id]:
            sse_connections[session_id].remove(connection_id)
            if not sse_connections[session_id]:
                sse_messages.pop(session_id, None)

def _generate_random_data(session_id: str):
    """生成随机数据并广播"""
    global table_data
    
    if session_id not in active_sessions:
        return
    
    session = active_sessions[session_id]
    session['generation_count'] += 1
    
    # 随机操作类型
    operations = ['add', 'update', 'delete']
    weights = [0.4, 0.5, 0.1]  # 添加40%，更新50%，删除10%
    operation = random.choices(operations, weights=weights)[0]
    
    names = ["张三", "李四", "王五", "赵六", "钱七", "孙八", "周九", "吴十", "郑十一", "王十二"]
    cities = ["北京", "上海", "广州", "深圳", "杭州", "南京", "成都", "武汉", "西安", "重庆"]
    statuses = ["活跃", "离线", "忙碌"]
    
    if operation == 'add':
        # 添加新记录
        new_id = max([item['id'] for item in table_data], default=0) + 1
        new_record = {
            "id": new_id,
            "name": random.choice(names),
            "age": random.randint(20, 60),
            "city": random.choice(cities),
            "score": round(random.uniform(60, 100), 1),
            "status": random.choice(statuses)
        }
        table_data.append(new_record)
        
        _broadcast_update(session_id, {
            'type': 'add',
            'data': new_record,
            'message': f'添加了新用户: {new_record["name"]}'
        })
        
    elif operation == 'update' and table_data:
        # 更新现有记录
        record = random.choice(table_data)
        old_score = record['score']
        record['score'] = round(random.uniform(60, 100), 1)
        record['status'] = random.choice(statuses)
        
        _broadcast_update(session_id, {
            'type': 'update',
            'data': record,
            'message': f'更新了用户 {record["name"]} 的分数: {old_score} → {record["score"]}'
        })
        
    elif operation == 'delete' and len(table_data) > 3:  # 保持至少3条记录
        # 删除记录
        record = random.choice(table_data)
        table_data.remove(record)
        
        _broadcast_update(session_id, {
            'type': 'delete',
            'data': record,
            'message': f'删除了用户: {record["name"]}'
        })

def _broadcast_update(session_id: str, message: dict):
    """广播更新消息"""
    if session_id not in active_sessions:
        return
    
    # 添加完整的表格数据和时间戳
    message.update({
        'session_id': session_id,
        'table_data': table_data,
        'timestamp': datetime.now().isoformat(),
        'generation_count': active_sessions[session_id]['generation_count']
    })
    
    # 将消息添加到队列
    if session_id not in sse_messages:
        sse_messages[session_id] = []
    sse_messages[session_id].append(message)

def _get_table_page() -> str:
    """生成表格页面HTML"""
    return """
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RAT Engine - SSE 表格实时刷新演示</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }
        
        .container {
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 15px;
            box-shadow: 0 15px 35px rgba(0,0,0,0.1);
            overflow: hidden;
        }
        
        .header {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 25px;
            text-align: center;
        }
        
        .header h1 { font-size: 2em; margin-bottom: 8px; }
        .header p { opacity: 0.9; }
        
        .content { padding: 30px; }
        
        .controls {
            display: flex;
            gap: 15px;
            margin-bottom: 25px;
            align-items: center;
        }
        
        .btn {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            border: none;
            padding: 10px 25px;
            border-radius: 20px;
            cursor: pointer;
            font-size: 1em;
            transition: all 0.3s ease;
        }
        
        .btn:hover {
            transform: translateY(-2px);
            box-shadow: 0 8px 15px rgba(102, 126, 234, 0.3);
        }
        
        .btn:disabled {
            opacity: 0.6;
            cursor: not-allowed;
            transform: none;
        }
        
        .btn.stop {
            background: linear-gradient(135deg, #ff6b6b 0%, #ee5a52 100%);
        }
        
        .status {
            padding: 8px 15px;
            border-radius: 15px;
            font-size: 0.9em;
            font-weight: bold;
        }
        
        .status.active {
            background: #d4edda;
            color: #155724;
        }
        
        .status.inactive {
            background: #f8d7da;
            color: #721c24;
        }
        
        .table-container {
            background: #f8f9fa;
            border-radius: 10px;
            overflow: hidden;
            margin-bottom: 20px;
        }
        
        table {
            width: 100%;
            border-collapse: collapse;
        }
        
        th, td {
            padding: 12px 15px;
            text-align: left;
            border-bottom: 1px solid #dee2e6;
        }
        
        th {
            background: #e9ecef;
            font-weight: 600;
            color: #495057;
        }
        
        tr:hover {
            background: #f1f3f4;
        }
        
        .status-badge {
            padding: 4px 8px;
            border-radius: 12px;
            font-size: 0.8em;
            font-weight: bold;
        }
        
        .status-active { background: #d4edda; color: #155724; }
        .status-offline { background: #f8d7da; color: #721c24; }
        .status-busy { background: #fff3cd; color: #856404; }
        
        .log-section {
            max-height: 200px;
            overflow-y: auto;
            background: #f8f9fa;
            border-radius: 8px;
            padding: 15px;
            font-family: 'Courier New', monospace;
            font-size: 0.85em;
        }
        
        .log-entry {
            margin: 3px 0;
            padding: 2px 0;
        }
        
        .log-info { color: #17a2b8; }
        .log-success { color: #28a745; }
        .log-error { color: #dc3545; }
        .log-warning { color: #ffc107; }
        
        .stats {
            display: flex;
            gap: 20px;
            margin-bottom: 20px;
        }
        
        .stat-card {
            background: #f8f9fa;
            padding: 15px;
            border-radius: 8px;
            text-align: center;
            flex: 1;
        }
        
        .stat-number {
            font-size: 1.5em;
            font-weight: bold;
            color: #667eea;
        }
        
        .stat-label {
            color: #666;
            font-size: 0.9em;
        }
        
        .highlight {
            animation: highlight 1s ease-in-out;
        }
        
        @keyframes highlight {
            0% { background-color: #fff3cd; }
            100% { background-color: transparent; }
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📊 RAT Engine</h1>
            <p>SSE 表格实时刷新演示</p>
        </div>
        
        <div class="content">
            <div class="controls">
                <button class="btn" id="startBtn">🚀 开始生成数据</button>
                <button class="btn stop" id="stopBtn" disabled>⏹️ 停止生成</button>
                <div class="status inactive" id="status">未连接</div>
            </div>
            
            <div class="stats">
                <div class="stat-card">
                    <div class="stat-number" id="recordCount">0</div>
                    <div class="stat-label">记录总数</div>
                </div>
                <div class="stat-card">
                    <div class="stat-number" id="updateCount">0</div>
                    <div class="stat-label">更新次数</div>
                </div>
                <div class="stat-card">
                    <div class="stat-number" id="avgScore">0</div>
                    <div class="stat-label">平均分数</div>
                </div>
            </div>
            
            <div class="table-container">
                <table id="dataTable">
                    <thead>
                        <tr>
                            <th>ID</th>
                            <th>姓名</th>
                            <th>年龄</th>
                            <th>城市</th>
                            <th>分数</th>
                            <th>状态</th>
                        </tr>
                    </thead>
                    <tbody id="tableBody">
                        <!-- 数据将通过 JavaScript 动态填充 -->
                    </tbody>
                </table>
            </div>
            
            <div class="log-section" id="logSection"></div>
        </div>
    </div>

    <script>
        class SSETableDemo {
            constructor() {
                this.sessionId = null;
                this.eventSource = null;
                this.isActive = false;
                this.updateCount = 0;
                
                this.initEvents();
                this.loadInitialData();
            }
            
            initEvents() {
                document.getElementById('startBtn').onclick = () => this.startGeneration();
                document.getElementById('stopBtn').onclick = () => this.stopGeneration();
            }
            
            async loadInitialData() {
                try {
                    const response = await fetch('/api/data');
                    const result = await response.json();
                    if (result.status === 'success') {
                        this.updateTable(result.data);
                        this.updateStats(result.data);
                    }
                } catch (error) {
                    this.log('error', `加载初始数据失败: ${error.message}`);
                }
            }
            
            async startGeneration() {
                try {
                    const response = await fetch('/api/start', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' }
                    });
                    
                    const result = await response.json();
                    if (result.status === 'success') {
                        this.sessionId = result.session_id;
                        this.connectSSE();
                        this.updateUI(true);
                        this.log('success', '数据生成已开始');
                    } else {
                        this.log('error', result.message);
                    }
                } catch (error) {
                    this.log('error', `启动失败: ${error.message}`);
                }
            }
            
            async stopGeneration() {
                if (!this.sessionId) return;
                
                try {
                    const response = await fetch(`/api/stop/${this.sessionId}`, {
                        method: 'POST'
                    });
                    
                    const result = await response.json();
                    this.log('info', result.message);
                } catch (error) {
                    this.log('error', `停止失败: ${error.message}`);
                }
                
                this.disconnectSSE();
                this.updateUI(false);
            }
            
            connectSSE() {
                if (!this.sessionId) return;
                
                this.eventSource = new EventSource(`/api/stream/${this.sessionId}`);
                
                this.eventSource.onmessage = (event) => {
                    try {
                        const data = JSON.parse(event.data);
                        this.handleSSEMessage(data);
                    } catch (error) {
                        this.log('error', `解析SSE消息失败: ${error.message}`);
                    }
                };
                
                this.eventSource.onerror = (error) => {
                    this.log('error', 'SSE连接错误');
                    this.disconnectSSE();
                    this.updateUI(false);
                };
            }
            
            disconnectSSE() {
                if (this.eventSource) {
                    this.eventSource.close();
                    this.eventSource = null;
                }
            }
            
            handleSSEMessage(data) {
                switch (data.type) {
                    case 'init':
                        this.updateTable(data.data);
                        this.updateStats(data.data);
                        this.log('info', '已连接到数据流');
                        break;
                        
                    case 'add':
                    case 'update':
                    case 'delete':
                        this.updateTable(data.table_data);
                        this.updateStats(data.table_data);
                        this.updateCount = data.generation_count || this.updateCount + 1;
                        this.log('success', data.message);
                        break;
                        
                    case 'end':
                        this.log('info', data.message);
                        this.disconnectSSE();
                        this.updateUI(false);
                        break;
                        
                    case 'heartbeat':
                        // 心跳消息，不需要特殊处理
                        break;
                        
                    default:
                        if (data.error) {
                            this.log('error', data.error);
                        }
                }
            }
            
            updateTable(data) {
                const tbody = document.getElementById('tableBody');
                tbody.innerHTML = '';
                
                data.forEach(row => {
                    const tr = document.createElement('tr');
                    tr.innerHTML = `
                        <td>${row.id}</td>
                        <td>${row.name}</td>
                        <td>${row.age}</td>
                        <td>${row.city}</td>
                        <td>${row.score}</td>
                        <td><span class="status-badge status-${row.status === '活跃' ? 'active' : row.status === '离线' ? 'offline' : 'busy'}">${row.status}</span></td>
                    `;
                    
                    // 添加高亮动画
                    tr.classList.add('highlight');
                    setTimeout(() => tr.classList.remove('highlight'), 1000);
                    
                    tbody.appendChild(tr);
                });
            }
            
            updateStats(data) {
                const recordCount = data.length;
                const avgScore = data.length > 0 ? (data.reduce((sum, item) => sum + item.score, 0) / data.length).toFixed(1) : 0;
                
                document.getElementById('recordCount').textContent = recordCount;
                document.getElementById('updateCount').textContent = this.updateCount;
                document.getElementById('avgScore').textContent = avgScore;
            }
            
            updateUI(active) {
                this.isActive = active;
                document.getElementById('startBtn').disabled = active;
                document.getElementById('stopBtn').disabled = !active;
                
                const status = document.getElementById('status');
                status.textContent = active ? '数据生成中...' : '未连接';
                status.className = `status ${active ? 'active' : 'inactive'}`;
            }
            
            log(type, message) {
                const logSection = document.getElementById('logSection');
                const entry = document.createElement('div');
                entry.className = `log-entry log-${type}`;
                entry.textContent = `[${new Date().toLocaleTimeString()}] ${message}`;
                
                logSection.appendChild(entry);
                logSection.scrollTop = logSection.scrollHeight;
                
                // 限制日志条数
                while (logSection.children.length > 50) {
                    logSection.removeChild(logSection.firstChild);
                }
            }
        }
        
        // 初始化应用
        new SSETableDemo();
    </script>
</body>
</html>
    """

def main():
    """主函数 - 启动SSE表格演示应用"""
    print(f"🚀 启动 SSE 表格演示服务器...")
    print(f"📡 地址: http://{SERVER_HOST}:{SERVER_PORT}")
    print(f"💡 功能: 实时表格数据更新演示")
    print(f"⚡ 按 Ctrl+C 停止服务器")
    print("\n" + "="*50)
    
    try:
        # 启动服务器（阻塞模式，优雅处理退出信号）
        app.run(host=SERVER_HOST, port=SERVER_PORT, debug=True, blocking=True)
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")

if __name__ == "__main__":
    main()