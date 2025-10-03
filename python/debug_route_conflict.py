#!/usr/bin/env python3
"""
调试路由冲突问题的独立测试
专注于测试两个特定问题：
1. /mixed/456/docs/manual.pdf 应该匹配 mixed_file_path 而不是 mixed_params
2. /negative/-456.78 应该匹配 negative_float 而不是 negative_int
"""

import requests
import json
import time
import threading
from rat_engine import RatApp, HttpRequest, HttpResponse, HttpMethod
import rat_engine_py

SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8999  # 使用不同端口避免冲突
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

def create_test_app():
    """创建测试应用"""
    rat_engine_py.rat_startup_log("🚀 创建路由冲突调试应用...")
    app = RatApp(name="route_conflict_debug")

    # 启用debug日志
    app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)

    # mixed 路由1：整数+字符串+浮点数
    @app.json("/mixed/<int:user_id>/<str:category>/<float:price>")
    def handle_mixed_params(request_data):
        return {
            "route": "mixed_params",
            "user_id": request_data.get('path_params', {}).get('user_id'),
            "category": request_data.get('path_params', {}).get('category'),
            "price": request_data.get('path_params', {}).get('price'),
            "description": "整数+字符串+浮点数"
        }

    # mixed 路由2：整数+路径
    @app.json("/mixed/<int:user_id>/<path:file_path>")
    def handle_mixed_file_path(request_data):
        return {
            "route": "mixed_file_path",
            "user_id": request_data.get('path_params', {}).get('user_id'),
            "file_path": request_data.get('path_params', {}).get('file_path'),
            "description": "整数+路径"
        }

    # negative 路由1：负整数
    @app.json("/negative/<int:value>")
    def handle_negative_int(request_data):
        return {
            "route": "negative_int",
            "value": request_data.get('path_params', {}).get('value'),
            "description": "负整数"
        }

    # negative 路由2：负浮点数
    @app.json("/negative/<float:value>")
    def handle_negative_float(request_data):
        return {
            "route": "negative_float",
            "value": request_data.get('path_params', {}).get('value'),
            "description": "负浮点数"
        }

    return app

def test_route(description, url, expected_route):
    """测试单个路由"""
    try:
        print(f"🧪 测试: {description}")
        print(f"   URL: {url}")

        response = requests.get(url, timeout=10)

        if response.status_code == 200:
            data = response.json()
            actual_route = data.get('route', 'unknown')

            print(f"   📄 响应数据: {json.dumps(data, ensure_ascii=False)}")

            if actual_route == expected_route:
                print(f"   ✅ 正确匹配: {actual_route}")
                return True
            else:
                print(f"   ❌ 错误匹配: 期望 {expected_route}, 实际 {actual_route}")
                return False
        else:
            print(f"   ❌ 状态码错误: {response.status_code}")
            return False

    except requests.RequestException as e:
        print(f"   ❌ 请求失败: {e}")
        return False

def main():
    """主函数"""
    print("🚀 启动路由冲突调试测试")
    print("=" * 60)

    # 创建应用
    app = create_test_app()

    # 启动服务器
    print(f"📡 启动服务器在端口 {SERVER_PORT}...")

    def run_server():
        app.run(host=SERVER_HOST, port=SERVER_PORT)

    server_thread = threading.Thread(target=run_server, daemon=True)
    server_thread.start()

    # 等待服务器启动
    print("⏳ 等待服务器启动...")
    time.sleep(3)

    # 测试用例
    test_cases = [
        # mixed 路由冲突测试
        ("mixed参数路由", f"{SERVER_URL}/mixed/123/electronics/299.99", "mixed_params"),
        ("mixed文件路径路由", f"{SERVER_URL}/mixed/456/docs/manual.pdf", "mixed_file_path"),

        # negative 路由冲突测试
        ("负整数路由", f"{SERVER_URL}/negative/-123", "negative_int"),
        ("负浮点数路由", f"{SERVER_URL}/negative/-456.78", "negative_float"),
    ]

    print("🧪 开始路由冲突调试测试...")
    passed = 0
    total = len(test_cases)

    for description, url, expected_route in test_cases:
        print()
        if test_route(description, url, expected_route):
            passed += 1
        print("-" * 40)

    print(f"\n📊 测试结果: {passed}/{total} 通过")

    if passed == total:
        print("🎉 所有测试通过！")
    else:
        print(f"⚠️  {total - passed} 个测试失败")

    return 0 if passed == total else 1

if __name__ == "__main__":
    import sys
    sys.exit(main())