#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
简化的 Tuple 返回值测试

验证核心的 tuple 返回值功能：
- (content, status_code) 格式
- (content, status_code, headers) 格式
"""

import sys
import os
import time
import json
import httpx

# 添加项目根目录到 Python 路径
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from rat_engine import RatApp

def create_simple_test_app() -> RatApp:
    """创建简化测试应用"""
    app = RatApp("simple_tuple_test")
    
    # 测试 1: 字典 + 400 状态码
    @app.json("/api/error")
    def error_response(request_data):
        """返回错误响应和状态码"""
        return {
            'success': False,
            'error': '无效的 JSON 数据'
        }, 400
    
    # 测试 2: 字符串 + content_type + 404 状态码
    @app.custom("/api/notfound")
    def not_found_response(request_data):
        """返回文本、content_type和状态码"""
        return "资源未找到", "text/plain; charset=utf-8", 404
    
    # 测试 3: 内容 + 状态码 + 自定义头部
    @app.json("/api/created")
    def created_response(request_data):
        """返回内容、状态码和自定义头部"""
        return {
            'message': '创建成功',
            'id': 12345
        }, 201, {
            'Location': '/api/resource/12345',
            'X-Custom-Header': 'test-value'
        }
    
    # 测试 4: 正常成功响应（对比）
    @app.json("/api/success")
    def success_response(request_data):
        """正常成功响应"""
        return {
            'success': True,
            'message': '操作成功'
        }
    
    # 测试 5: HTML + 状态码
    @app.html("/api/server-error")
    def server_error_response(request_data):
        """返回 HTML 和状态码"""
        return "<h1>服务器内部错误</h1><p>请稍后重试</p>", 500
    
    return app

def test_simple_tuple_responses():
    """测试简化的 tuple 返回值功能"""
    print("🧪 开始简化 Tuple 返回值测试")
    
    # 创建应用
    app = create_simple_test_app()
    
    # 启动服务器
    print("🚀 启动测试服务器...")
    import threading
    server_thread = threading.Thread(
        target=lambda: app.run(host="127.0.0.1", port=8898, debug=False, blocking=True),
        daemon=True
    )
    server_thread.start()
    
    # 等待服务器启动
    time.sleep(3)
    
    base_url = "http://127.0.0.1:8898"
    
    test_cases = [
        {
            'name': '字典 + 400 状态码',
            'url': f'{base_url}/api/error',
            'expected_status': 400,
            'expected_content_type': 'application/json'
        },
        {
            'name': '文本 + 404 状态码',
            'url': f'{base_url}/api/notfound',
            'expected_status': 404,
            'expected_content_type': 'text/plain'
        },
        {
            'name': '内容 + 状态码 + 自定义头部',
            'url': f'{base_url}/api/created',
            'expected_status': 201,
            'expected_headers': {
                'Location': '/api/resource/12345',
                'X-Custom-Header': 'test-value'
            }
        },
        {
            'name': '正常成功响应',
            'url': f'{base_url}/api/success',
            'expected_status': 200,
            'expected_content_type': 'application/json'
        },
        {
            'name': 'HTML + 500 状态码',
            'url': f'{base_url}/api/server-error',
            'expected_status': 500,
            'expected_content_type': 'text/html'
        }
    ]
    
    success_count = 0
    total_count = len(test_cases)
    
    try:
        with httpx.Client(timeout=10) as client:
            for i, test_case in enumerate(test_cases, 1):
                print(f"\n📋 测试 {i}/{total_count}: {test_case['name']}")
                
                try:
                    response = client.get(test_case['url'])
                    
                    print(f"   📊 状态码: {response.status_code}")
                    print(f"   📄 Content-Type: {response.headers.get('content-type', 'N/A')}")
                    print(f"   📝 响应内容: {response.text[:100]}{'...' if len(response.text) > 100 else ''}")
                    
                    # 检查状态码
                    if response.status_code == test_case['expected_status']:
                        print(f"   ✅ 状态码正确: {response.status_code}")
                        status_ok = True
                    else:
                        print(f"   ❌ 状态码错误: 期望 {test_case['expected_status']}, 实际 {response.status_code}")
                        status_ok = False
                    
                    # 检查 Content-Type
                    content_type_ok = True
                    if 'expected_content_type' in test_case:
                        actual_ct = response.headers.get('content-type', '')
                        expected_ct = test_case['expected_content_type']
                        if expected_ct in actual_ct:
                            print(f"   ✅ Content-Type 正确: {actual_ct}")
                        else:
                            print(f"   ❌ Content-Type 错误: 期望包含 {expected_ct}, 实际 {actual_ct}")
                            content_type_ok = False
                    
                    # 检查自定义头部
                    headers_ok = True
                    if 'expected_headers' in test_case:
                        for header_name, expected_value in test_case['expected_headers'].items():
                            actual_value = response.headers.get(header_name)
                            if actual_value == expected_value:
                                print(f"   ✅ 头部 {header_name} 正确: {actual_value}")
                            else:
                                print(f"   ❌ 头部 {header_name} 错误: 期望 {expected_value}, 实际 {actual_value}")
                                headers_ok = False
                    
                    if status_ok and content_type_ok and headers_ok:
                        print(f"   🎉 测试通过")
                        success_count += 1
                    else:
                        print(f"   💥 测试失败")
                        
                except Exception as e:
                    print(f"   ❌ 请求失败: {e}")
                    
    except Exception as e:
        print(f"❌ 测试过程中发生错误: {e}")
    
    print(f"\n📊 测试结果: {success_count}/{total_count} 通过")
    
    if success_count == total_count:
        print("🎉 所有测试通过！Tuple 返回值支持功能正常工作")
        return True
    else:
        print(f"💥 有 {total_count - success_count} 个测试失败")
        return False

if __name__ == "__main__":
    success = test_simple_tuple_responses()
    sys.exit(0 if success else 1)