#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
SPA (单页应用) 功能演示

本示例展示如何在 RAT Engine 中配置和使用 SPA 支持。
SPA 支持允许前端路由在服务器端正确处理，当访问不存在的路由时，
会自动回退到指定的 fallback 文件（通常是 index.html）。
"""

import os
import sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from rat_engine import RatApp, HttpResponse

# 创建应用实例
app = RatApp("spa_demo")

# 配置 SPA 支持
print("🔧 配置 SPA 支持...")
app.enable_spa("/index.html")
print("✅ SPA 支持已启用，回退路径: /index.html")

# 创建一个简单的 index.html 内容
index_html_content = """
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SPA Demo - RAT Engine</title>
    <style>
        body {
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            margin: 0;
            padding: 20px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            min-height: 100vh;
        }
        .container {
            max-width: 800px;
            margin: 0 auto;
            background: rgba(255, 255, 255, 0.1);
            padding: 30px;
            border-radius: 15px;
            backdrop-filter: blur(10px);
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.1);
        }
        h1 {
            text-align: center;
            margin-bottom: 30px;
            font-size: 2.5em;
        }
        .nav {
            display: flex;
            justify-content: center;
            gap: 20px;
            margin-bottom: 30px;
        }
        .nav a {
            color: white;
            text-decoration: none;
            padding: 10px 20px;
            background: rgba(255, 255, 255, 0.2);
            border-radius: 25px;
            transition: all 0.3s ease;
        }
        .nav a:hover {
            background: rgba(255, 255, 255, 0.3);
            transform: translateY(-2px);
        }
        .content {
            background: rgba(255, 255, 255, 0.1);
            padding: 20px;
            border-radius: 10px;
            margin-bottom: 20px;
        }
        .feature {
            margin: 15px 0;
            padding: 15px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 8px;
            border-left: 4px solid #4CAF50;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🚀 RAT Engine SPA Demo</h1>
        
        <div class="nav">
            <a href="/">首页</a>
            <a href="/about">关于</a>
            <a href="/contact">联系</a>
            <a href="/products">产品</a>
            <a href="/api/status">API 状态</a>
        </div>
        
        <div class="content">
            <h2>🎯 SPA 功能演示</h2>
            <p>这是一个单页应用 (SPA) 演示页面。无论你访问什么路径，都会回退到这个页面。</p>
            
            <div class="feature">
                <h3>✨ 当前路径</h3>
                <p id="current-path">正在获取...</p>
            </div>
            
            <div class="feature">
                <h3>🔄 SPA 路由测试</h3>
                <p>尝试访问以下路径，它们都会回退到这个页面：</p>
                <ul>
                    <li><a href="/dashboard" style="color: #4CAF50;">/dashboard</a></li>
                    <li><a href="/user/profile" style="color: #4CAF50;">/user/profile</a></li>
                    <li><a href="/settings/advanced" style="color: #4CAF50;">/settings/advanced</a></li>
                    <li><a href="/non-existent-route" style="color: #4CAF50;">/non-existent-route</a></li>
                </ul>
            </div>
            
            <div class="feature">
                <h3>🛠️ API 端点</h3>
                <p>以下是可用的 API 端点：</p>
                <ul>
                    <li><a href="/api/status" style="color: #2196F3;">/api/status</a> - 服务器状态</li>
                    <li><a href="/api/spa-info" style="color: #2196F3;">/api/spa-info</a> - SPA 配置信息</li>
                </ul>
            </div>
        </div>
    </div>
    
    <script>
        // 显示当前路径
        document.getElementById('current-path').textContent = window.location.pathname;
        
        // 简单的客户端路由处理
        window.addEventListener('popstate', function(event) {
            document.getElementById('current-path').textContent = window.location.pathname;
        });
        
        // 拦截导航链接点击
        document.addEventListener('click', function(e) {
            if (e.target.tagName === 'A' && e.target.href.startsWith(window.location.origin)) {
                const href = e.target.getAttribute('href');
                if (href.startsWith('/api/')) {
                    // API 请求不拦截
                    return;
                }
                e.preventDefault();
                history.pushState(null, '', href);
                document.getElementById('current-path').textContent = href;
            }
        });
    </script>
</body>
</html>
"""

# 提供 index.html
@app.html("/index.html")
def serve_index(request):
    return index_html_content

# API 端点 - 服务器状态
@app.json("/api/status")
def api_status(request):
    return {
        "status": "running",
        "message": "RAT Engine SPA Demo 正在运行",
        "spa_enabled": True,
        "fallback_path": "/index.html"
    }

# API 端点 - SPA 信息
@app.json("/api/spa-info")
def api_spa_info(request):
    return {
        "spa_config": {
            "enabled": True,
            "fallback_path": "/index.html",
            "description": "当访问不存在的路由时，会自动回退到 /index.html"
        },
        "supported_routes": [
            "/",
            "/about",
            "/contact",
            "/products",
            "/dashboard",
            "/user/profile",
            "/settings/advanced",
            "/any-other-route"
        ],
        "api_endpoints": [
            "/api/status",
            "/api/spa-info"
        ]
    }

# 根路径也返回 index.html
@app.html("/")
def serve_root(request):
    return index_html_content

if __name__ == "__main__":
    print("\n" + "="*60)
    print("🚀 RAT Engine SPA Demo 启动中...")
    print("="*60)
    print("📍 访问地址: http://127.0.0.1:3000")
    print("📋 功能说明:")
    print("   • SPA 支持已启用，回退路径: /index.html")
    print("   • 访问任何不存在的路由都会回退到主页")
    print("   • API 端点不受 SPA 影响")
    print("\n🔗 测试链接:")
    print("   • 主页: http://127.0.0.1:3000/")
    print("   • SPA 路由: http://127.0.0.1:3000/dashboard")
    print("   • API 状态: http://127.0.0.1:3000/api/status")
    print("   • SPA 信息: http://127.0.0.1:3000/api/spa-info")
    print("\n💡 提示: 按 Ctrl+C 停止服务器")
    print("="*60 + "\n")
    
    try:
        app.run(host="127.0.0.1", port=3000, blocking=True)
    except KeyboardInterrupt:
        print("\n👋 服务器已停止")