#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine 大 TXT 文件逐行读取演示

特性：
- 大 TXT 文件上传
- 逐行实时读取和输出
- SSE 流式传输
- 现代化 Web UI
- 支持大文件处理
"""

import os
import sys
import json
import time
import base64
import hashlib
import tempfile
import uuid
import threading
from pathlib import Path
from datetime import datetime
from typing import Dict, Optional, Generator

try:
    from rat_engine import RatApp
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    sys.exit(1)

# 配置
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8089
UPLOAD_DIR = Path(__file__).parent / "txt_uploads"  # 保存到脚本同级目录
CHUNK_SIZE = 32 * 1024  # 32KB 分块（适合 Base64 传输）
MAX_FILE_SIZE = 100 * 1024 * 1024  # 100MB 最大文件大小
READ_BUFFER_SIZE = 8192  # 读取缓冲区大小

# 全局状态
upload_sessions: Dict[str, Dict] = {}
reading_sessions: Dict[str, Dict] = {}
sse_connections: Dict[str, list] = {}
sse_messages: Dict[str, list] = {}  # 存储待发送的 SSE 消息

# 创建应用
app = RatApp(name="txt_line_reader")
# 注意：日志配置已移除，现在通过 RatEngineBuilder 处理

# 初始化模板引擎
from rat_engine.templates import TemplateEngine
template_engine = TemplateEngine(auto_escape=True, cache=True)

# 创建上传目录
UPLOAD_DIR.mkdir(exist_ok=True)
print(f"📁 上传目录: {UPLOAD_DIR.absolute()}")

# 主页
@app.html("/")
def upload_page(request_data):
    """文件上传页面"""
    return _get_upload_page()

# 初始化上传
@app.json("/api/init", methods=["POST"])
def init_upload(request_data):
    """初始化上传会话"""
    try:
        # 从请求体中获取数据
        if 'body' not in request_data:
            return {
                'status': 'error',
                'message': '缺少请求体数据'
            }
        
        body_data = request_data['body']
        
        try:
            # 解析 JSON 数据
            if isinstance(body_data, str):
                data = json.loads(body_data)
            else:
                data = json.loads(body_data.decode('utf-8'))
            
            filename = data.get('filename')
            file_size = data.get('file_size')
            file_hash = data.get('file_hash')
            
        except (json.JSONDecodeError, UnicodeDecodeError, AttributeError) as e:
            return {
                'status': 'error',
                'message': f'JSON 解析失败: {str(e)}'
            }
        
        filename = data.get('filename')
        file_size = int(data.get('file_size', 0))
        file_hash = data.get('file_hash', '')
        
        if not filename or file_size <= 0:
            return {"error": "无效的文件信息"}
        
        # 检查文件类型（只允许txt文件）
        if not filename.lower().endswith('.txt'):
            return {"error": "只支持 .txt 文件"}
        
        if file_size > MAX_FILE_SIZE:
            return {"error": f"文件过大，最大支持 {MAX_FILE_SIZE//1024//1024}MB"}
        
        # 生成唯一会话ID
        session_id = str(uuid.uuid4()).replace('-', '')[:16]
        
        # 计算分块数量
        total_chunks = (file_size + CHUNK_SIZE - 1) // CHUNK_SIZE
        
        # 创建会话
        upload_sessions[session_id] = {
            'filename': filename,
            'file_size': file_size,
            'file_hash': file_hash,
            'total_chunks': total_chunks,
            'received_chunks': {},
            'temp_file': UPLOAD_DIR / f"{session_id}.tmp",
            'created_at': datetime.now(),
            'completed': False,
            'progress': 0.0
        }
        
        # 创建临时文件
        with open(upload_sessions[session_id]['temp_file'], 'wb') as f:
            f.write(b'\x00' * file_size)
        
        return {
            "session_id": session_id,
            "chunk_size": CHUNK_SIZE,
            "total_chunks": total_chunks
        }
        
    except Exception as e:
        return {"error": f"初始化失败: {str(e)}"}

# 上传分块
@app.json("/api/chunk", methods=["POST"])
def upload_chunk(request_data):
    """处理文件分块上传并实时输出文本内容"""
    try:
        data = json.loads(request_data.get('body', '{}'))
        session_id = data.get('session_id')
        chunk_index = int(data.get('chunk_index', -1))
        chunk_data_b64 = data.get('chunk_data', '')
        
        if session_id not in upload_sessions:
            return {"error": "无效的会话ID"}
        
        session = upload_sessions[session_id]
        
        if chunk_index < 0 or chunk_index >= session['total_chunks']:
            return {"error": "无效的分块索引"}
        
        # 解码分块数据
        try:
            chunk_data = base64.b64decode(chunk_data_b64)
        except Exception:
            return {"error": "分块数据解码失败"}
        
        # 写入分块
        with open(session['temp_file'], 'r+b') as f:
            f.seek(chunk_index * CHUNK_SIZE)
            f.write(chunk_data)
        
        # 更新会话状态
        session['received_chunks'][chunk_index] = len(chunk_data)
        session['progress'] = len(session['received_chunks']) / session['total_chunks'] * 100
        
        # 广播进度
        _broadcast_progress(session_id, {
            'type': 'upload_progress',
            'progress': session['progress'],
            'chunk_index': chunk_index,
            'uploaded_chunks': len(session['received_chunks']),
            'total_chunks': session['total_chunks']
        })
        
        # 检查是否完成
        if len(session['received_chunks']) == session['total_chunks']:
            _complete_upload(session_id)
        
        return {
            "success": True,
            "progress": session['progress'],
            "completed": session['completed']
        }
        
    except Exception as e:
        return {"error": f"上传分块失败: {str(e)}"}

# 开始逐行读取
@app.json("/api/start_reading/<session_id>", methods=["POST"])
def start_reading(request_data, session_id):
    """开始逐行读取文件"""
    try:
        if session_id not in upload_sessions:
            return {"error": "无效的会话ID"}
            
        session = upload_sessions[session_id]
        
        # 确保文件已上传完成
        if not session['completed']:
            return {"error": "文件上传未完成"}
            
        # 检查是否已经在读取
        if session_id in reading_sessions:
            return {"error": "文件正在读取中"}
        
        # 创建读取会话
        reading_sessions[session_id] = {
            'status': 'reading',
            'current_line': 0,
            'total_lines': 0,
            'started_at': datetime.now(),
            'completed': False
        }
        
        # 在后台线程中开始读取
        threading.Thread(target=_read_file_lines, args=(session_id,), daemon=True).start()
        
        return {
            "success": True,
            "message": "开始逐行读取文件"
        }
    except Exception as e:
        return {"error": f"开始读取失败: {str(e)}"}

# SSE 进度推送
@app.sse("/api/progress/<session_id>")
def progress_stream(request_data, session_id):
    """SSE 数据流处理"""
    if not session_id or session_id not in upload_sessions:
        yield json.dumps({"error": "无效的会话ID"})
        return
    
    # 注册连接
    if session_id not in sse_connections:
        sse_connections[session_id] = []
    
    connection_id = f"conn_{len(sse_connections[session_id])}_{time.time()}"
    sse_connections[session_id].append(connection_id)
            
    try:
        session = upload_sessions[session_id]
        
        # 发送初始状态
        initial_message = {
            'type': 'init',
            'session_id': session_id,
            'filename': session['filename'],
            'upload_progress': session['progress'],
            'upload_completed': session['completed']
        }
        yield 'data: ' + json.dumps(initial_message) + '\n\n'
        
        # 保持连接活跃并处理消息队列
        start_time = time.time()
        last_heartbeat = time.time()
        
        while time.time() - start_time < 600:  # 10分钟超时
            time.sleep(0.1)  # 更频繁检查消息
            
            # 处理待发送的消息
            if session_id in sse_messages and sse_messages[session_id]:
                message = sse_messages[session_id].pop(0)
                # 双重验证：确保消息确实属于当前会话
                if message.get('session_id') == session_id:
                    yield 'data: ' + json.dumps(message) + '\n\n'
                    # 如果是上传完成或读取完成消息，根据情况处理
                    if message.get('type') == 'upload_completed':
                        # 上传完成后继续保持连接，等待用户开始读取
                        pass
                    elif message.get('type') == 'reading_completed':
                        # 读取完成后立即退出
                        return
                else:
                    print(f"⚠️ 检测到会话ID不匹配的消息，已丢弃: {message}")
            
            # 检查读取是否已完成
            if session_id in reading_sessions and reading_sessions[session_id].get('completed', False):
                # 发送读取完成消息并退出
                yield 'data: ' + json.dumps({
                    'type': 'reading_completed',
                    'session_id': session_id,
                    'total_lines': reading_sessions[session_id]['total_lines'],
                    'message': '文件读取完成'
                }) + '\n\n'
                return
            
            # 每10秒发送一次心跳
            current_time = time.time()
            if current_time - last_heartbeat >= 10:
                yield 'data: ' + json.dumps({
                    'type': 'heartbeat',
                    'timestamp': current_time
                }) + '\n\n'
                last_heartbeat = current_time
                
    except Exception as e:
        print(f"SSE 连接错误: {e}")
    finally:
        # 清理连接
        if session_id in sse_connections and connection_id in sse_connections[session_id]:
            sse_connections[session_id].remove(connection_id)
            # 如果没有更多连接，清理消息队列
            if not sse_connections[session_id]:
                sse_messages.pop(session_id, None)

# 获取状态
@app.json("/api/status/<session_id>")
def get_status(request_data, session_id):
    """获取会话状态"""
    if session_id not in upload_sessions:
        return {"error": "会话不存在"}
    
    upload_session = upload_sessions[session_id]
    reading_session = reading_sessions.get(session_id, {})
    
    return {
        "session_id": session_id,
        "filename": upload_session['filename'],
        "file_size": upload_session['file_size'],
        "upload_progress": upload_session['progress'],
        "upload_completed": upload_session['completed'],
        "reading_status": reading_session.get('status', 'not_started'),
        "current_line": reading_session.get('current_line', 0),
        "total_lines": reading_session.get('total_lines', 0),
        "reading_completed": reading_session.get('completed', False),
        "created_at": upload_session['created_at'].isoformat()
    }

# 下载文件
@app.file("/api/download/<session_id>")
def download_file(request_data, session_id):
    """下载上传的文件"""
    if session_id not in upload_sessions:
        return "会话不存在"
    
    session = upload_sessions[session_id]
    if not session['completed']:
        return "文件未完成上传"
    
    final_file = UPLOAD_DIR / session['filename']
    if final_file.exists():
        return str(final_file)
    else:
        return "文件不存在"

def _complete_upload(session_id: str):
    """完成上传处理"""
    session = upload_sessions[session_id]
    
    # 防止重复完成
    if session.get('completed', False):
        return
        
    temp_file = session['temp_file']
    final_file = UPLOAD_DIR / session['filename']
    
    # 移动文件
    temp_file.rename(final_file)
    session['completed'] = True
    session['final_file'] = final_file
    
    # 验证文件哈希
    if session['file_hash']:
        actual_hash = _calculate_hash(final_file)
        hash_match = actual_hash.lower() == session['file_hash'].lower()
        print(f"🔍 文件哈希验证: {'✅ 通过' if hash_match else '❌ 失败'}")
    
    # 广播完成消息
    _broadcast_progress(session_id, {
        'type': 'upload_completed',
        'filename': session['filename'],
        'file_size': session['file_size'],
        'download_url': f'/api/download/{session_id}',
        'progress': 100.0
    })
        
    print(f"✅ 文件上传完成: {session['filename']} ({_format_size(session['file_size'])})")
    print(f"📂 保存位置: {final_file.absolute()}")

def _read_file_lines(session_id: str):
    """在后台线程中逐行读取文件"""
    try:
        session = upload_sessions[session_id]
        reading_session = reading_sessions[session_id]
        final_file = session['final_file']
        
        print(f"📖 开始逐行读取文件: {final_file}")
        
        # 首先计算总行数
        total_lines = 0
        with open(final_file, 'r', encoding='utf-8', errors='ignore') as f:
            for _ in f:
                total_lines += 1
        
        reading_session['total_lines'] = total_lines
        
        # 广播开始读取消息
        _broadcast_progress(session_id, {
            'type': 'reading_started',
            'total_lines': total_lines,
            'message': f'开始读取文件，共 {total_lines} 行'
        })
        
        # 逐行读取并发送
        current_line = 0
        with open(final_file, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                current_line += 1
                reading_session['current_line'] = current_line
                
                # 移除行尾换行符
                line_content = line.rstrip('\n\r')
                
                # 广播当前行内容
                _broadcast_progress(session_id, {
                    'type': 'line_content',
                    'line_number': current_line,
                    'line_content': line_content,
                    'total_lines': total_lines,
                    'progress': (current_line / total_lines) * 100
                })
                
                # 控制读取速度，避免过快
                time.sleep(0.1)  # 每行间隔100ms
                
                # 检查是否需要停止（可以添加停止机制）
                if reading_session.get('stop_requested', False):
                    break
        
        # 标记读取完成
        reading_session['completed'] = True
        reading_session['status'] = 'completed'
        
        # 广播读取完成消息
        _broadcast_progress(session_id, {
            'type': 'reading_completed',
            'total_lines': total_lines,
            'message': f'文件读取完成，共处理 {current_line} 行'
        })
        
        print(f"✅ 文件读取完成: {session['filename']} ({current_line} 行)")
        
    except Exception as e:
        print(f"❌ 读取文件时出错: {e}")
        reading_session['status'] = 'error'
        _broadcast_progress(session_id, {
            'type': 'reading_error',
            'error': str(e),
            'message': f'读取文件时出错: {str(e)}'
        })

def _calculate_hash(file_path: Path) -> str:
    """计算文件SHA-256哈希"""
    hash_sha256 = hashlib.sha256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            hash_sha256.update(chunk)
    return hash_sha256.hexdigest()

def _broadcast_progress(session_id: str, message: dict):
    """广播进度消息"""
    # 验证会话ID是否有效
    if session_id not in upload_sessions:
        print(f"⚠️ 尝试向无效会话发送消息: {session_id}")
        return
        
    # 确保消息包含会话ID
    message['session_id'] = session_id
    message['timestamp'] = datetime.now().isoformat()
    
    # 将消息添加到对应会话的消息队列
    if session_id not in sse_messages:
        sse_messages[session_id] = []
    sse_messages[session_id].append(message)
    
    # 限制消息队列大小，避免内存泄漏
    if len(sse_messages[session_id]) > 1000:
        sse_messages[session_id] = sse_messages[session_id][-500:]  # 保留最新的500条消息

def _format_size(size: int) -> str:
    """格式化文件大小"""
    for unit in ['B', 'KB', 'MB', 'GB']:
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} TB"

def _get_upload_page() -> str:
    """使用模板引擎渲染上传页面"""
    template = """
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RAT Engine - 大 TXT 文件逐行读取</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }
        
        .container {
            max-width: 900px;
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
        
        .upload-zone {
            border: 2px dashed #ddd;
            border-radius: 10px;
            padding: 40px 20px;
            text-align: center;
            margin-bottom: 25px;
            transition: all 0.3s ease;
            cursor: pointer;
        }
        
        .upload-zone:hover {
            border-color: #667eea;
            background: #f8fbff;
        }
        
        .upload-zone.dragover {
            border-color: #764ba2;
            background: #f3e5f5;
            transform: scale(1.02);
        }
        
        .upload-icon { font-size: 3em; margin-bottom: 15px; color: #ddd; }
        .upload-text { font-size: 1.1em; color: #666; margin-bottom: 10px; }
        .upload-hint { color: #999; font-size: 0.9em; }
        
        .file-input { display: none; }
        
        .btn {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            border: none;
            padding: 10px 25px;
            border-radius: 20px;
            cursor: pointer;
            font-size: 1em;
            transition: all 0.3s ease;
            margin: 5px;
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
        
        .btn.success {
            background: linear-gradient(135deg, #4CAF50 0%, #45a049 100%);
        }
        
        .file-info {
            background: #f8f9fa;
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 20px;
            display: none;
        }
        
        .file-info h3 { color: #333; margin-bottom: 10px; }
        .file-info p { color: #666; margin: 5px 0; }
        
        .progress-section {
            display: none;
            margin-top: 25px;
        }
        
        .progress-bar {
            width: 100%;
            height: 15px;
            background: #f0f0f0;
            border-radius: 8px;
            overflow: hidden;
            margin-bottom: 10px;
        }
        
        .progress-fill {
            height: 100%;
            background: linear-gradient(90deg, #667eea 0%, #764ba2 100%);
            width: 0%;
            transition: width 0.3s ease;
        }
        
        .progress-text {
            text-align: center;
            color: #666;
            font-size: 1em;
        }
        
        .reading-section {
            display: none;
            margin-top: 25px;
        }
        
        .line-display {
            background: #f8f9fa;
            border-radius: 8px;
            padding: 15px;
            margin-bottom: 15px;
            max-height: 400px;
            overflow-y: auto;
            font-family: 'Courier New', monospace;
            font-size: 0.9em;
            border: 1px solid #e9ecef;
        }
        
        .line-item {
            padding: 3px 0;
            border-bottom: 1px solid #eee;
            word-wrap: break-word;
        }
        
        .line-item:last-child {
            border-bottom: none;
        }
        
        .line-number {
            color: #666;
            font-weight: bold;
            margin-right: 10px;
            min-width: 50px;
            display: inline-block;
        }
        
        .line-content {
            color: #333;
        }
        
        .log-section {
            margin-top: 20px;
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
            justify-content: space-around;
            background: #f8f9fa;
            padding: 15px;
            border-radius: 8px;
            margin-bottom: 15px;
        }
        
        .stat-item {
            text-align: center;
        }
        
        .stat-value {
            font-size: 1.5em;
            font-weight: bold;
            color: #667eea;
        }
        
        .stat-label {
            color: #666;
            font-size: 0.9em;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📖 RAT Engine</h1>
            <p>大 TXT 文件逐行读取演示</p>
        </div>
        
        <div class="content">
            <div class="upload-zone" id="uploadZone">
                <div class="upload-icon">📄</div>
                <div class="upload-text">点击选择 TXT 文件或拖拽到此处</div>
                <div class="upload-hint">最大支持 100MB TXT 文件</div>
                <input type="file" class="file-input" id="fileInput" accept=".txt">
            </div>
            
            <div class="file-info" id="fileInfo">
                <h3>📄 选中文件</h3>
                <p><strong>文件名:</strong> <span id="fileName"></span></p>
                <p><strong>大小:</strong> <span id="fileSize"></span></p>
                <p><strong>类型:</strong> <span id="fileType"></span></p>
                <button class="btn" id="uploadBtn">开始上传</button>
            </div>
            
            <div class="progress-section" id="progressSection">
                <h3>📤 上传进度</h3>
                <div class="progress-bar">
                    <div class="progress-fill" id="progressFill"></div>
                </div>
                <div class="progress-text" id="progressText">准备中...</div>
                <button class="btn success" id="startReadingBtn" style="display: none;">开始逐行读取</button>
            </div>
            
            <div class="reading-section" id="readingSection">
                <h3>📖 文件内容</h3>
                <div class="stats" id="readingStats">
                    <div class="stat-item">
                        <div class="stat-value" id="currentLineNum">0</div>
                        <div class="stat-label">当前行</div>
                    </div>
                    <div class="stat-item">
                        <div class="stat-value" id="totalLinesNum">0</div>
                        <div class="stat-label">总行数</div>
                    </div>
                    <div class="stat-item">
                        <div class="stat-value" id="readingProgress">0%</div>
                        <div class="stat-label">读取进度</div>
                    </div>
                </div>
                <div class="line-display" id="lineDisplay"></div>
            </div>
            
            <div class="log-section" id="logSection"></div>
        </div>
    </div>

    <script>
        class TxtLineReader {
            constructor() {
                this.file = null;
                this.sessionId = null;
                this.chunkSize = 32768; // 32KB
                this.eventSource = null;
                this.uploading = false;
                this.reading = false;
                this.lineCount = 0;
                
                this.initEvents();
            }
            
            initEvents() {
                const zone = document.getElementById('uploadZone');
                const input = document.getElementById('fileInput');
                const uploadBtn = document.getElementById('uploadBtn');
                const startReadingBtn = document.getElementById('startReadingBtn');
                
                zone.onclick = () => input.click();
                
                zone.ondragover = (e) => {
                    e.preventDefault();
                    zone.classList.add('dragover');
                };
                
                zone.ondragleave = () => zone.classList.remove('dragover');
                
                zone.ondrop = (e) => {
                    e.preventDefault();
                    zone.classList.remove('dragover');
                    if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
                        this.handleFile(e.dataTransfer.files[0]);
                    }
                };
                
                input.onchange = (e) => {
                    if (e.target.files && e.target.files.length > 0) {
                        this.handleFile(e.target.files[0]);
                    }
                };
                
                uploadBtn.onclick = () => this.startUpload();
                startReadingBtn.onclick = () => this.startReading();
            }
            
            handleFile(file) {
                // 检查文件类型
                if (!file.name.toLowerCase().endsWith('.txt')) {
                    this.log('error', '只支持 .txt 文件');
                    return;
                }
                
                this.file = file;
                this.showFileInfo(file);
            }
            
            showFileInfo(file) {
                document.getElementById('fileName').textContent = file.name;
                document.getElementById('fileSize').textContent = this.formatSize(file.size);
                document.getElementById('fileType').textContent = file.type || 'text/plain';
                document.getElementById('fileInfo').style.display = 'block';
            }
            
            async startUpload() {
                if (!this.file || this.uploading) return;
                
                this.uploading = true;
                document.getElementById('uploadBtn').disabled = true;
                document.getElementById('progressSection').style.display = 'block';
                
                try {
                    // 计算文件哈希
                    this.log('info', '正在计算文件哈希...');
                    const fileHash = await this.calculateHash(this.file);
                    
                    // 初始化上传
                    const initResponse = await fetch('/api/init', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                            filename: this.file.name,
                            file_size: this.file.size,
                            file_hash: fileHash
                        })
                    });
                    
                    const initData = await initResponse.json();
                    if (initData.error) {
                        throw new Error(initData.error);
                    }
                    
                    this.sessionId = initData.session_id;
                    this.chunkSize = initData.chunk_size;
                    
                    // 启动SSE
                    this.startSSE();
                    
                    // 开始分块上传
                    await this.uploadChunks(initData.total_chunks);
                    
                } catch (error) {
                    this.log('error', `上传失败: ${error.message}`);
                    this.uploading = false;
                    document.getElementById('uploadBtn').disabled = false;
                }
            }
            
            async uploadChunks(totalChunks) {
                // 提前显示读取区域
                document.getElementById('readingSection').style.display = 'block';
                
                for (let i = 0; i < totalChunks; i++) {
                    const start = i * this.chunkSize;
                    const end = Math.min(start + this.chunkSize, this.file.size);
                    const chunk = this.file.slice(start, end);
                    
                    // 实时读取并显示文本内容
                    const text = await this.readChunkAsText(chunk); 
                    const lines = text.split('\\n').map(line => line.replace(/\\r/g, ''));
                    let lineNumber = this.lineCount;
                    
                    for (const line of lines) {
                        if (line.trim()) {
                            this.displayLine(++lineNumber, line);
                        }
                    }
                    this.lineCount = lineNumber;
                    
                    const chunkData = await this.fileToBase64(chunk);
                    
                    const response = await fetch('/api/chunk', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                            session_id: this.sessionId,
                            chunk_index: i,
                            chunk_data: chunkData
                        })
                    });
                    
                    const result = await response.json();
                    if (result.error) {
                        throw new Error(result.error);
                    }
                }
            }
            
            startSSE() {
                this.eventSource = new EventSource(`/api/progress/${this.sessionId}`);
                
                this.eventSource.onmessage = (event) => {
                    const data = JSON.parse(event.data);
                    this.handleSSEMessage(data);
                };
                
                this.eventSource.onerror = (error) => {
                    this.log('error', 'SSE连接错误');
                    this.disconnectSSE();
                };
            }
            
            handleSSEMessage(data) {
                switch (data.type) {
                    case 'init':
                        this.log('info', '连接已建立');
                        break;
                    case 'upload_progress':
                        this.updateUploadProgress(data.progress, data.uploaded_chunks, data.total_chunks);
                        break;
                    case 'upload_completed':
                        this.log('success', '文件上传完成');
                        this.uploading = false;
                        document.getElementById('startReadingBtn').style.display = 'inline-block';
                        break;
                    case 'reading_started':
                        this.log('info', `开始读取文件，共 ${data.total_lines} 行`);
                        document.getElementById('totalLinesNum').textContent = data.total_lines;
                        document.getElementById('readingSection').style.display = 'block';
                        break;
                    case 'line_content':
                        this.displayLine(data.line_number, data.line_content);
                        this.updateReadingProgress(data.line_number, data.total_lines, data.progress);
                        break;
                    case 'reading_completed':
                        this.log('success', `文件读取完成，共处理 ${data.total_lines} 行`);
                        this.reading = false;
                        this.disconnectSSE();
                        break;
                    case 'reading_error':
                        this.log('error', data.message);
                        this.reading = false;
                        break;
                    case 'heartbeat':
                        // 心跳消息，保持连接
                        break;
                }
            }
            
            updateUploadProgress(progress, uploaded, total) {
                document.getElementById('progressFill').style.width = `${progress}%`;
                document.getElementById('progressText').textContent = 
                    `上传进度: ${progress.toFixed(1)}% (${uploaded}/${total} 块)`;
            }
            
            updateReadingProgress(current, total, progress) {
                document.getElementById('currentLineNum').textContent = current;
                document.getElementById('readingProgress').textContent = `${progress.toFixed(1)}%`;
            }
            
            displayLine(lineNumber, content) {
                const lineDisplay = document.getElementById('lineDisplay');
                const lineItem = document.createElement('div');
                lineItem.className = 'line-item';
                lineItem.innerHTML = `
                    <span class="line-number">${lineNumber}:</span>
                    <span class="line-content">${this.escapeHtml(content)}</span>
                `;
                lineDisplay.appendChild(lineItem);
                
                // 自动滚动到底部
                lineDisplay.scrollTop = lineDisplay.scrollHeight;
                
                // 限制显示的行数，避免内存问题
                const maxLines = 1000;
                if (lineDisplay.children.length > maxLines) {
                    lineDisplay.removeChild(lineDisplay.firstChild);
                }
            }
            
            async startReading() {
                if (!this.sessionId || this.reading) return;
                
                this.reading = true;
                document.getElementById('startReadingBtn').disabled = true;
                
                try {
                    const response = await fetch(`/api/start_reading/${this.sessionId}`, {
                        method: 'POST'
                    });
                    
                    if (!response.ok) {
                        const text = await response.text();
                        throw new Error(text || 'Unknown error');
                    }
                    
                    const result = await response.json();
                    if (result.error) {
                        throw new Error(result.error);
                    }
                    
                    this.log('info', '开始逐行读取文件...');
                    
                } catch (error) {
                    this.log('error', `开始读取失败: ${error.message}`);
                    this.reading = false;
                    document.getElementById('startReadingBtn').disabled = false;
                }
            }
            
            disconnectSSE() {
                if (this.eventSource) {
                    this.eventSource.close();
                    this.eventSource = null;
                }
            }
            
            async calculateHash(file) {
                const buffer = await file.arrayBuffer();
                const hashBuffer = await crypto.subtle.digest('SHA-256', buffer);
                const hashArray = Array.from(new Uint8Array(hashBuffer));
                return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
            }
            
            readChunkAsText(chunk) {
                return new Promise((resolve, reject) => {
                    const reader = new FileReader();
                    reader.onload = (e) => {
                        try {
                            let text = e.target.result;
                            // 移除可能的UTF-8 BOM字符
                            if (text.charCodeAt(0) === 0xFEFF) {
                                text = text.substring(1);
                            }
                            // 确保正确转义换行符
                            // text = text.replaceAll('\\r\\n', '\\r\\n').replaceAll('\\n', '\\n');
                            resolve(text);
                        } catch (error) {
                            reject(`文本处理错误: ${error}`);
                        }
                    };
                    reader.onerror = () => reject('文件读取失败');
                    reader.readAsText(chunk, 'UTF-8');
                });
            }
            
            async fileToBase64(file) {
                return new Promise((resolve, reject) => {
                    const reader = new FileReader();
                    reader.onload = () => {
                        const base64 = reader.result.split(',')[1];
                        resolve(base64);
                    };
                    reader.onerror = reject;
                    reader.readAsDataURL(file);
                });
            }
            
            formatSize(bytes) {
                const sizes = ['B', 'KB', 'MB', 'GB'];
                if (bytes === 0) return '0 B';
                const i = Math.floor(Math.log(bytes) / Math.log(1024));
                return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${sizes[i]}`;
            }
            
            escapeHtml(text) {
                const div = document.createElement('div');
                div.textContent = text;
                return div.innerHTML;
            }
            
            log(type, message) {
                const logSection = document.getElementById('logSection');
                const entry = document.createElement('div');
                entry.className = `log-entry log-${type}`;
                entry.textContent = `[${new Date().toLocaleTimeString()}] ${message}`;
                logSection.appendChild(entry);
                logSection.scrollTop = logSection.scrollHeight;
            }
        }
        
        // 初始化应用
        new TxtLineReader();
    </script>
</body>
</html>
    """
    
    return template

def main():
    """主函数"""
    try:
        print(f"🚀 启动 RAT Engine TXT 逐行读取服务器...")
        print(f"📍 服务地址: http://{SERVER_HOST}:{SERVER_PORT}")
        print(f"📁 上传目录: {UPLOAD_DIR.absolute()}")
        print(f"📋 功能特性:")
        print(f"   • 大 TXT 文件分块上传")
        print(f"   • 逐行实时读取和显示")
        print(f"   • SSE 流式传输")
        print(f"   • 现代化 Web UI")
        print(f"\n🎯 使用方法:")
        print(f"   1. 打开浏览器访问: http://{SERVER_HOST}:{SERVER_PORT}")
        print(f"   2. 选择或拖拽 TXT 文件")
        print(f"   3. 点击开始上传")
        print(f"   4. 上传完成后点击开始逐行读取")
        print(f"   5. 观察文件内容逐行显示")
        print("\n" + "="*50)
        
        app.run(
            host=SERVER_HOST,
            port=SERVER_PORT,
            blocking=True,
            debug=True
        )
        
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()