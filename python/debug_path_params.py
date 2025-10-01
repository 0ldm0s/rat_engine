#!/usr/bin/env python3
"""
简单的路径参数调试测试
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__)))

from rat_engine import RatApp
import threading
import time
import requests

def create_debug_app():
    """创建调试应用"""
    app = RatApp(name="debug_app")
    
    @app.json("/test/<id>")
    def test_handler(request_data, *args, **kwargs):
        """测试路径参数的处理器"""
        # 🔧 [调试信息] 测试处理器调试 - 如需调试测试处理器问题，可取消注释以下行
        # print(f"🔍 [PYTHON-DEBUG] test_handler 被调用")
        # print(f"🔍 [PYTHON-DEBUG] request_data: {request_data}")
        # print(f"🔍 [PYTHON-DEBUG] request_data 类型: {type(request_data)}")
        # print(f"🔍 [PYTHON-DEBUG] args: {args}")
        # print(f"🔍 [PYTHON-DEBUG] kwargs: {kwargs}")
        
        if isinstance(request_data, dict):
            path_params = request_data.get('path_params', {})
            # 🔧 [调试信息] 路径参数提取调试 - 如需调试路径参数提取问题，可取消注释以下行
            # print(f"🔍 [PYTHON-DEBUG] path_params: {path_params}")
            # print(f"🔍 [PYTHON-DEBUG] path_params 类型: {type(path_params)}")
            
            test_id = path_params.get('id', 'unknown')
            # 🔧 [调试信息] ID 提取调试 - 如需调试 ID 提取问题，可取消注释以下行
            # print(f"🔍 [PYTHON-DEBUG] 提取的 test_id: {test_id}")
            
            return {
                "status": "success",
                "message": f"收到测试请求，ID: {test_id}",
                "test_id": test_id,
                "path_params": path_params,
                "request_data": request_data
            }
        else:
            # 🔧 [调试信息] 数据类型错误调试 - 如需调试数据类型问题，可取消注释以下行
            # print(f"🔍 [PYTHON-DEBUG] request_data 不是字典类型")
            return {
                "status": "error",
                "message": "request_data 不是字典类型",
                "type": str(type(request_data))
            }
    
    return app

def main():
    app = create_debug_app()
    
    # 启动服务器
    def run_server():
        print("🚀 启动调试服务器...")
        app.run(host="127.0.0.1", port=8084, debug=True, blocking=False)
    
    server_thread = threading.Thread(target=run_server, daemon=True)
    server_thread.start()
    
    # 等待服务器启动
    time.sleep(2)
    
    # 测试请求
    test_url = "http://127.0.0.1:8084/test/123"
    print(f"\n🧪 测试请求: {test_url}")
    
    try:
        response = requests.get(test_url, timeout=5)
        print(f"📊 状态码: {response.status_code}")
        print(f"📄 响应内容: {response.text}")
        
        if response.status_code == 200:
            try:
                json_data = response.json()
                print(f"📋 JSON 数据: {json_data}")
            except:
                print("❌ 无法解析 JSON")
        else:
            print(f"❌ 请求失败: {response.status_code}")
            
    except Exception as e:
        print(f"❌ 请求异常: {e}")
    
    print("\n🔚 测试完成")

if __name__ == "__main__":
    main()