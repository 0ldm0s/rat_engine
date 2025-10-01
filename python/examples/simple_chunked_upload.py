#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine 简化版大文件分块上传示例

特性：
- 基于 Base64 编码的分块上传
- SSE 实时进度推送
- 文件完整性验证
- 现代化 Web UI
- 简化的实现逻辑
"""

import os
import sys
import json
import time
import base64
import hashlib
import tempfile
import uuid
from pathlib import Path
from datetime import datetime
from typing import Dict, Optional

try:
    from rat_engine import RatApp
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    sys.exit(1)

# 配置
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8087
UPLOAD_DIR = Path(__file__).parent / "uploads"  # 保存到脚本同级目录
CHUNK_SIZE = 64 * 1024  # 64KB 分块（适合 Base64 传输）
MAX_FILE_SIZE = 50 * 1024 * 1024  # 50MB 最大文件大小

# 全局状态
upload_sessions: Dict[str, Dict] = {}
sse_connections: Dict[str, list] = {}
sse_messages: Dict[str, list] = {}  # 存储待发送的 SSE 消息

# 创建应用
app = RatApp(name="simple_chunked_upload")
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
    # print(f"[API/INIT] 收到上传请求: {request_data}")
    try:
        # 从请求体中获取数据
        if 'body' not in request_data:
            # print("[API/INIT] 错误: 请求缺少body字段")
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
            # print(f"[API/INIT] 错误: 无效的文件信息 filename={filename} size={file_size}")
            return {"error": "无效的文件信息"}
        
        # print(f"[API/INIT] 验证文件信息: name={filename} size={file_size} hash={file_hash}")
        if file_size > MAX_FILE_SIZE:
            return {"error": f"文件过大，最大支持 {MAX_FILE_SIZE//1024//1024}MB"}
        
        # 生成唯一会话ID（使用UUID确保唯一性）
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
    """处理文件分块上传"""
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
            'type': 'progress',
            'progress': session['progress'],
            'chunk_index': chunk_index,
            'received_chunks': len(session['received_chunks']),
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
        
# SSE 进度推送
@app.sse("/api/progress/<session_id>")
def progress_stream(request_data, session_id):
    # session_id 现在直接作为参数传递
    
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
            'progress': session['progress'],
            'completed': session['completed']
        }
        yield 'data: ' + json.dumps(initial_message) + '\n\n'
        
        # 如果已经完成，直接发送完成消息并退出
        if session.get('completed', False):
            yield 'data: ' + json.dumps({
                'type': 'completed',
                'session_id': session_id,
                'filename': session['filename'],
                'file_size': session['file_size'],
                'download_url': f'/api/download/{session_id}',
                'progress': 100.0
            }) + '\n\n'
            return
        
        # 保持连接活跃并处理消息队列
        start_time = time.time()
        last_heartbeat = time.time()
        
        while time.time() - start_time < 300:  # 5分钟超时
            time.sleep(0.1)  # 更频繁检查消息
            
            # 处理待发送的消息
            if session_id in sse_messages and sse_messages[session_id]:
                message = sse_messages[session_id].pop(0)
                # 双重验证：确保消息确实属于当前会话
                if message.get('session_id') == session_id:
                    yield 'data: ' + json.dumps(message) + '\n\n'
                    # 如果是完成消息，立即退出
                    if message.get('type') == 'completed':
                        return
                else:
                    print(f"⚠️ 检测到会话ID不匹配的消息，已丢弃: {message}")
            
            # 检查会话是否已完成
            if session.get('completed', False):
                # 发送完成消息并退出
                yield 'data: ' + json.dumps({
                    'type': 'completed',
                    'session_id': session_id,
                    'filename': session['filename'],
                    'file_size': session['file_size'],
                    'download_url': f'/api/download/{session_id}',
                    'progress': 100.0
                }) + '\n\n'
                return
            
            # 每10秒发送一次心跳（减少频率）
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
        
# 获取上传状态
@app.json("/api/status/<session_id>")
def get_status(request_data, session_id):
    # session_id 现在直接作为参数传递
    
    if session_id not in upload_sessions:
        return {"error": "会话不存在"}
    
    session = upload_sessions[session_id]
    return {
        "session_id": session_id,
        "filename": session['filename'],
        "file_size": session['file_size'],
        "progress": session['progress'],
        "received_chunks": len(session['received_chunks']),
        "total_chunks": session['total_chunks'],
        "completed": session['completed'],
        "created_at": session['created_at'].isoformat()
    }

# 下载文件
@app.file("/api/download/<session_id>")
def download_file(request_data, session_id):
    # session_id 现在直接作为参数传递
    
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
        # print(f"🔍 文件哈希验证: {'✅ 通过' if hash_match else '❌ 失败'}")
    
    # 广播完成消息
    _broadcast_progress(session_id, {
        'type': 'completed',
        'filename': session['filename'],
        'file_size': session['file_size'],
        'download_url': f'/api/download/{session_id}',
        'progress': 100.0
    })
        
    # print(f"✅ 文件上传完成: {session['filename']} ({_format_size(session['file_size'])})")
    # print(f"📂 保存位置: {final_file.absolute()}")
    
    # 延迟清理会话（给客户端时间接收完成消息）
    import threading
    threading.Timer(10.0, _cleanup_session, args=[session_id]).start()

def _cleanup_session(session_id: str):
    """清理会话资源"""
    try:
        # 清理上传会话
        if session_id in upload_sessions:
            session = upload_sessions.pop(session_id)
            print(f"🧹 清理会话: {session_id} ({session.get('filename', 'unknown')})")
        
        # 清理SSE连接
        if session_id in sse_connections:
            sse_connections.pop(session_id)
        
        # 清理消息队列
        if session_id in sse_messages:
            sse_messages.pop(session_id)
            
    except Exception as e:
        print(f"⚠️ 清理会话时出错: {e}")
    
def _calculate_hash(file_path: Path) -> str:
    """计算文件SHA-256哈希"""
    hash_sha256 = hashlib.sha256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            hash_sha256.update(chunk)
    return hash_sha256.hexdigest()

def _broadcast_progress(session_id: str, message: dict):
    """广播进度消息"""
    # 验证会话ID是否有效，防止恶意或错误的会话ID
    if session_id not in upload_sessions:
        print(f"⚠️ 尝试向无效会话发送消息: {session_id}")
        return
        
    # print(f"📡 [{session_id}] {message.get('type', 'unknown')}: {message}")
    
    # 确保消息包含会话ID，防止消息被错误路由
    message['session_id'] = session_id
    
    # 将消息添加到对应会话的消息队列
    if session_id not in sse_messages:
        sse_messages[session_id] = []
    sse_messages[session_id].append(message)
    
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
    <title>RAT Engine - 简化版分块上传</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: linear-gradient(135deg, #74b9ff 0%, #0984e3 100%);
            min-height: 100vh;
            padding: 20px;
        }
        
        .container {
            max-width: 700px;
            margin: 0 auto;
            background: white;
            border-radius: 15px;
            box-shadow: 0 15px 35px rgba(0,0,0,0.1);
            overflow: hidden;
        }
        
        .header {
            background: linear-gradient(135deg, #74b9ff 0%, #0984e3 100%);
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
            border-color: #74b9ff;
            background: #f8fbff;
        }
        
        .upload-zone.dragover {
            border-color: #0984e3;
            background: #e3f2fd;
            transform: scale(1.02);
        }
        
        .upload-icon { font-size: 3em; margin-bottom: 15px; color: #ddd; }
        .upload-text { font-size: 1.1em; color: #666; margin-bottom: 10px; }
        .upload-hint { color: #999; font-size: 0.9em; }
        
        .file-input { display: none; }
        
        .btn {
            background: linear-gradient(135deg, #74b9ff 0%, #0984e3 100%);
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
            box-shadow: 0 8px 15px rgba(116, 185, 255, 0.3);
        }
        
        .btn:disabled {
            opacity: 0.6;
            cursor: not-allowed;
            transform: none;
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
            background: linear-gradient(90deg, #74b9ff 0%, #0984e3 100%);
            width: 0%;
            transition: width 0.3s ease;
        }
        
        .progress-text {
            text-align: center;
            color: #666;
            font-size: 1em;
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
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🚀 RAT Engine</h1>
            <p>简化版大文件分块上传演示</p>
        </div>
        
        <div class="content">
            <div class="upload-zone" id="uploadZone">
                <div class="upload-icon">📁</div>
                <div class="upload-text">点击选择文件或拖拽到此处</div>
                <div class="upload-hint">最大支持 {{MAX_FILE_SIZE_MB}}MB 文件</div>
                <input type="file" class="file-input" id="fileInput">
            </div>
            
            <div class="file-info" id="fileInfo">
                <h3>📄 选中文件</h3>
                <p><strong>文件名:</strong> <span id="fileName"></span></p>
                <p><strong>大小:</strong> <span id="fileSize"></span></p>
                <p><strong>类型:</strong> <span id="fileType"></span></p>
                <button class="btn" id="uploadBtn">开始上传</button>
            </div>
            
            <div class="progress-section" id="progressSection">
                <div class="progress-bar">
                    <div class="progress-fill" id="progressFill"></div>
                </div>
                <div class="progress-text" id="progressText">准备中...</div>
            </div>
            
            <div class="log-section" id="logSection"></div>
        </div>
    </div>

    <script>
        class SimpleUploader {
            constructor() {
                this.file = null;
                this.sessionId = null;
                this.chunkSize = {{CHUNK_SIZE}};
                this.eventSource = null;
                this.uploading = false;
                
                this.initEvents();
            }
            
            initEvents() {
                const zone = document.getElementById('uploadZone');
                const input = document.getElementById('fileInput');
                const btn = document.getElementById('uploadBtn');
                
                zone.onclick = () => input.click();
                
                zone.ondragover = (e) => {
                    e.preventDefault();
                    zone.classList.add('dragover');
                };
                
                zone.ondragleave = () => zone.classList.remove('dragover');
                
                zone.ondrop = (e) => {
                    e.preventDefault();
                    zone.classList.remove('dragover');
                    this.handleFile(e.dataTransfer.files[0]);
                };
                
                input.onchange = (e) => this.handleFile(e.target.files[0]);
                btn.onclick = () => this.startUpload();
            }
            
            handleFile(file) {
                if (!file) return;
                
                if (file.size > {{MAX_FILE_SIZE}}) {
                    this.log('文件过大，超过限制', 'error');
                    return;
                }
                
                this.file = file;
                this.showFileInfo();
            }
            
            showFileInfo() {
                document.getElementById('fileName').textContent = this.file.name;
                document.getElementById('fileSize').textContent = this.formatSize(this.file.size);
                document.getElementById('fileType').textContent = this.file.type || '未知';
                document.getElementById('fileInfo').style.display = 'block';
            }
            
            async startUpload() {
                if (!this.file || this.uploading) return;
                
                this.uploading = true;
                this.uploadCompleted = false;
                document.getElementById('uploadBtn').disabled = true;
                document.getElementById('progressSection').style.display = 'block';
                
                try {
                    this.log('开始计算文件哈希...', 'info');
                    const fileHash = await this.calculateHash(this.file);
                    
                    this.log('初始化上传会话...', 'info');
                    const initResp = await fetch('/api/init', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({
                            filename: this.file.name,
                            file_size: this.file.size,
                            file_hash: fileHash
                        })
                    });
                    
                    const initData = await initResp.json();
                    if (initData.error) {
                        this.log(initData.error, 'error');
                        return;
                    }
                    
                    this.sessionId = initData.session_id;
                    this.chunkSize = initData.chunk_size;
                    
                    this.log(`会话创建成功: ${this.sessionId}`, 'success');
                    this.log(`总分块数: ${initData.total_chunks}`, 'info');
                    
                    // 开始 SSE
                    this.startSSE();
                    
                    // 开始上传
                    await this.uploadChunks();
                    
                } catch (error) {
                    this.log(`上传失败: ${error.message}`, 'error');
                } finally {
                    if (!this.uploadCompleted) {
                        this.uploading = false;
                        document.getElementById('uploadBtn').disabled = false;
                    }
                }
            }
            
            async calculateHash(file) {
                const buffer = await file.arrayBuffer();
                const hashBuffer = await crypto.subtle.digest('SHA-256', buffer);
                const hashArray = Array.from(new Uint8Array(hashBuffer));
                return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
            }
            
            async uploadChunks() {
                const totalChunks = Math.ceil(this.file.size / this.chunkSize);
                
                for (let i = 0; i < totalChunks; i++) {
                    const start = i * this.chunkSize;
                    const end = Math.min(start + this.chunkSize, this.file.size);
                    const chunk = this.file.slice(start, end);
                    
                    await this.uploadChunk(i, chunk);
                    
                    // 小延迟，避免过快请求
                    await new Promise(resolve => setTimeout(resolve, 10));
                }
            }
            
            async uploadChunk(index, chunk) {
                const buffer = await chunk.arrayBuffer();
                const base64Data = btoa(String.fromCharCode(...new Uint8Array(buffer)));
                
                const response = await fetch('/api/chunk', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        session_id: this.sessionId,
                        chunk_index: index,
                        chunk_data: base64Data
                    })
                });
                
                const data = await response.json();
                if (data.error) {
                    throw new Error(data.error);
                }
                
                return data;
            }
            
            startSSE() {
                if (this.eventSource) {
                    this.eventSource.close();
                }
                
                // 初始化重试计数器(只在第一次调用时)
                if (this.sseRetryCount === undefined) {
                    this.sseRetryCount = 0;
                }
                
                this.eventSource = new EventSource(`/api/progress/${this.sessionId}`);
                this.sseCompleted = false;
                
                this.eventSource.onmessage = (event) => {
                    try {
                        const data = JSON.parse(event.data);
                        this.handleSSEMessage(data);
                    } catch (error) {
                        console.error('SSE 解析错误:', error);
                    }
                };
                
                this.eventSource.onerror = (error) => {
                    console.error('SSE 错误:', error);
                    // 如果已经完成，不要重连
                    if (this.sseCompleted) {
                        this.closeSSE();
                        return;
                    }
                    
                    // 关闭当前连接
                    this.eventSource.close();
                    
                    // 如果连接失败，等待一段时间后重试（最多3次）
                    if (this.sseRetryCount < 3) {
                        this.sseRetryCount++;
                        this.log(`SSE 重连尝试 ${this.sseRetryCount}/3`, 'info');
                        setTimeout(() => {
                            if (!this.sseCompleted) {
                                this.startSSE();
                            }
                        }, 2000 * this.sseRetryCount); // 递增延迟
                    } else {
                        this.log('SSE 连接失败，已停止重试', 'error');
                        this.closeSSE();
                    }
                };
                
                this.eventSource.onopen = () => {
                    // 连接成功时重置重试计数
                    this.sseRetryCount = 0;
                    this.log('SSE 连接成功', 'success');
                };
            }
            
            handleSSEMessage(data) {
                switch (data.type) {
                    case 'init':
                        this.log('SSE 连接建立', 'success');
                        if (data.completed) {
                            // 如果初始状态就是完成的，直接处理
                            this.updateProgress(100);
                            document.getElementById('progressText').textContent = '已完成';
                            this.log('🎉 文件已完成上传！', 'success');
                            if (data.download_url) {
                                this.log(`<a href="${data.download_url}" target="_blank">点击下载文件</a>`, 'success');
                            }
                            this.sseCompleted = true;
                            this.uploadCompleted = true;
                            this.uploading = false;
                            clearInterval(this.progressInterval);
                            this.closeSSE();
                        }
                        break;
                    case 'progress':
                        this.updateProgress(data.progress);
                        this.log(`上传进度: ${data.received_chunks}/${data.total_chunks} 分块`, 'info');
                        if (data.progress >= 100) {
                            this.closeSSE();
                        }
                        break;
                    case 'completed':
                        this.updateProgress(100);
                        this.log('🎉 上传完成！', 'success');
                        this.log(`<a href="${data.download_url}" target="_blank">点击下载文件</a>`, 'success');
                        this.sseCompleted = true;
                        this.uploadCompleted = true;
                        this.uploading = false;
                        // 确保所有定时器和连接都被清理
                        if (this.progressInterval) {
                            clearInterval(this.progressInterval);
                            this.progressInterval = null;
                        }
                        // 延迟关闭SSE连接，确保所有消息处理完成
                        setTimeout(() => {
                            this.closeSSE();
                            // 重置上传状态，防止重复上传
                            this.file = null;
                            this.sessionId = null;
                        }, 500);
                        break;
                    case 'heartbeat':
                        // 心跳，不显示
                        break;
                    default:
                        console.log('未知SSE消息类型:', data.type, data);
                }
            }
            
            closeSSE() {
                if (this.eventSource) {
                    try {
                        this.eventSource.close();
                    } catch (e) {
                        console.log('SSE关闭异常:', e);
                    }
                    this.eventSource = null;
                    this.log('SSE 连接已关闭', 'info');
                }
                if (this.progressInterval) {
                    clearInterval(this.progressInterval);
                    this.progressInterval = null;
                }
                // 重置重试计数器，为下次连接做准备
                this.sseRetryCount = 0;
                document.getElementById('progressText').textContent = '已完成';
                
                // 强制清理所有上传状态
                this.uploading = false;
                this.uploadCompleted = true;
                this.sseCompleted = true;
            }
            
            updateProgress(percent) {
                document.getElementById('progressFill').style.width = `${percent}%`;
                document.getElementById('progressText').textContent = `进度: ${percent.toFixed(1)}%`;
            }
            
            log(message, type = 'info') {
                const logSection = document.getElementById('logSection');
                const entry = document.createElement('div');
                entry.className = `log-entry log-${type}`;
                entry.innerHTML = `[${new Date().toLocaleTimeString()}] ${message}`;
                logSection.appendChild(entry);
                logSection.scrollTop = logSection.scrollHeight;
            }
            
            formatSize(bytes) {
                const units = ['B', 'KB', 'MB', 'GB'];
                let size = bytes;
                let unitIndex = 0;
                
                while (size >= 1024 && unitIndex < units.length - 1) {
                    size /= 1024;
                    unitIndex++;
                }
                
                return `${size.toFixed(1)} ${units[unitIndex]}`;
            }
        }
        
        // 启动上传器
        new SimpleUploader();
    </script>
</body>
</html>
        """
    return template_engine.render_string(template, {
        'CHUNK_SIZE': CHUNK_SIZE,
        'MAX_FILE_SIZE': MAX_FILE_SIZE,
        'MAX_FILE_SIZE_MB': MAX_FILE_SIZE // 1024 // 1024
    })
    
def main():
    try:
        app.run(
            host=SERVER_HOST,
            port=SERVER_PORT,
            blocking=True,
            shutdown_timeout=5
        )
        print("\n✅ 服务器启动成功，等待请求...")
    except KeyboardInterrupt:
        print("\n🛑 收到终止信号，正在优雅停止服务器...")
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()