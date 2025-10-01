#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
多版本缓存管理器测试脚本

此脚本用于验证多版本缓存管理器功能是否正常工作
"""

import time
import threading
import requests
import json
from rat_engine import RatApp

def create_test_app():
    """创建测试应用"""
    app = RatApp("version_manager_test")
    
    # 启用缓存
    app.enable_cache_production(default_ttl=60)  # 1分钟缓存
    
    # 启用压缩
    app.enable_compression(min_size=50)
    
    # 启用多版本缓存管理器
    app.enable_version_manager(
        max_encoding_versions=3,
        enable_precompression=True,
        hot_encoding_threshold=0.1,
        store_original_data=True,
        cleanup_age_threshold=300,
        cleanup_idle_threshold=180
    )
    
    print("✅ 多版本缓存管理器配置完成")
    print("   - 最大编码版本: 3")
    print("   - 预压缩: 启用")
    print("   - 热点阈值: 0.1")
    print("   - 存储原始数据: 启用")
    
    @app.json("/test", methods=["GET"])
    def test_endpoint(request_data):
        """测试端点"""
        return {
            "message": "Hello from multi-version cache!",
            "timestamp": time.time(),
            "data": "This is test data for compression and caching. " * 10
        }
    
    @app.html("/", methods=["GET"])
    def index(request_data):
        """首页"""
        return """
        <h1>多版本缓存管理器测试</h1>
        <p>测试端点: <a href="/test">/test</a></p>
        <p>多版本缓存管理器已启用，支持多种编码格式的缓存。</p>
        """
    
    return app

def test_compression_protocols(base_url):
    """测试不同压缩协议"""
    print("\n🧪 开始自动测试不同压缩协议")
    print("=" * 60)
    
    # 测试不同的编码格式
    encodings = [
        ("无压缩", {}),
        ("gzip", {"Accept-Encoding": "gzip"}),
        ("brotli", {"Accept-Encoding": "br"}),
        ("deflate", {"Accept-Encoding": "deflate"}),
        ("多种格式", {"Accept-Encoding": "gzip, deflate, br"}),
    ]
    
    for encoding_name, headers in encodings:
        print(f"\n📡 测试 {encoding_name} 编码...")
        try:
            # 发送请求
            response = requests.get(f"{base_url}/test", headers=headers, timeout=10)
            
            # 显示响应信息
            print(f"   状态码: {response.status_code}")
            print(f"   响应大小: {len(response.content)} 字节")
            
            # 显示相关响应头
            relevant_headers = [
                "content-encoding", "content-length", "content-type",
                "cache-control", "x-cache-status", "x-compression-ratio"
            ]
            
            print("   响应头部:")
            for header in relevant_headers:
                if header in response.headers:
                    print(f"     {header}: {response.headers[header]}")
            
            # 验证JSON响应
            if response.headers.get("content-type", "").startswith("application/json"):
                data = response.json()
                print(f"   消息: {data.get('message', 'N/A')}")
                print(f"   时间戳: {data.get('timestamp', 'N/A')}")
            
            print("   ✅ 请求成功")
            
        except requests.exceptions.RequestException as e:
            print(f"   ❌ 请求失败: {e}")
        except json.JSONDecodeError as e:
            print(f"   ⚠️  JSON解析失败: {e}")
        
        # 短暂延迟，观察缓存效果
        time.sleep(1)

def run_test():
    """运行测试"""
    print("🚀 启动多版本缓存管理器测试")
    print("=" * 50)
    
    app = create_test_app()
    base_url = "http://127.0.0.1:8081"
    
    # 在后台线程中启动服务器
    def start_server():
        try:
            app.run(host="127.0.0.1", port=8081, debug=False)
        except Exception as e:
            print(f"❌ 服务器启动失败: {e}")
    
    server_thread = threading.Thread(target=start_server, daemon=True)
    server_thread.start()
    
    # 等待服务器启动
    print("⏳ 等待服务器启动...")
    time.sleep(3)
    
    # 验证服务器是否启动成功
    try:
        response = requests.get(base_url, timeout=5)
        print("✅ 服务器已启动并响应正常")
    except requests.exceptions.RequestException:
        print("❌ 服务器启动失败或无法访问")
        return
    
    print(f"🌐 服务器地址: {base_url}")
    print(f"🔧 测试端点: {base_url}/test")
    
    # 自动测试不同压缩协议
    test_compression_protocols(base_url)
    
    print("\n" + "=" * 60)
    print("🔄 重复测试以观察缓存效果...")
    
    # 再次测试，观察缓存命中情况
    test_compression_protocols(base_url)
    
    print("\n" + "=" * 60)
    print("✅ 自动测试完成")
    print("💡 提示: 服务器仍在运行，您可以手动访问以下地址:")
    print(f"   首页: {base_url}")
    print(f"   API: {base_url}/test")
    print("\n按 Ctrl+C 停止服务器")
    
    try:
        # 保持主线程运行
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n👋 测试结束")

if __name__ == "__main__":
    run_test()