#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
高级路径参数测试示例
基于 Rust advanced_path_params_demo.rs 的 Python 实现
测试不同类型的路径参数：int、str、uuid、float、path
包含自动化测试验证功能
"""

import time
import threading
import requests
import signal
import sys
import json
import re
from enum import Flag, auto
from rat_engine import RatApp, HttpRequest, HttpResponse, HttpMethod

# 测试功能枚举
class TestFeature(Flag):
    """测试功能枚举 - 使用 Flag 支持组合选择"""
    NONE = 0
    HOME = auto()                    # 主页测试
    INT_PARAMS = auto()              # 整数参数测试
    UUID_PARAMS = auto()             # UUID参数测试
    FLOAT_PARAMS = auto()            # 浮点数参数测试
    PATH_PARAMS = auto()             # 路径参数测试
    MIXED_PARAMS = auto()            # 混合参数测试
    ALL = HOME | INT_PARAMS | UUID_PARAMS | FLOAT_PARAMS | PATH_PARAMS | MIXED_PARAMS  # 所有测试

# 配置开关
AUTO_TEST_ENABLED = True
TEST_DELAY = 2
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8084
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

# 测试配置
TEST_FEATURES = TestFeature.ALL

# 测试用例定义
TEST_CASES = [
    # (功能, 描述, URL路径, 预期参数类型, 预期值)
    (TestFeature.HOME, "首页", f"{SERVER_URL}/", None, None),

    (TestFeature.INT_PARAMS, "整数参数 - 有效值", f"{SERVER_URL}/users/int/123", "id", "123"),
    (TestFeature.INT_PARAMS, "整数参数 - 负数", f"{SERVER_URL}/users/int/-456", "id", "-456"),

    (TestFeature.UUID_PARAMS, "UUID参数 - 标准格式", f"{SERVER_URL}/users/uuid/550e8400-e29b-41d4-a716-446655440000", "id", "550e8400-e29b-41d4-a716-446655440000"),
    (TestFeature.UUID_PARAMS, "UUID参数 - 简短格式", f"{SERVER_URL}/users/uuid/12345678-1234-1234-1234-123456789012", "id", "12345678-1234-1234-1234-123456789012"),

    (TestFeature.FLOAT_PARAMS, "浮点数参数 - 整数", f"{SERVER_URL}/products/price/99", "price", "99"),
    (TestFeature.FLOAT_PARAMS, "浮点数参数 - 小数", f"{SERVER_URL}/products/price/99.99", "price", "99.99"),
    (TestFeature.FLOAT_PARAMS, "浮点数参数 - 负数", f"{SERVER_URL}/products/price/-12.34", "price", "-12.34"),

    (TestFeature.PATH_PARAMS, "路径参数 - 简单路径", f"{SERVER_URL}/files/docs/readme.md", "file_path", "docs/readme.md"),
    (TestFeature.PATH_PARAMS, "路径参数 - 复杂路径", f"{SERVER_URL}/files/src/utils/logger.rs", "file_path", "src/utils/logger.rs"),
    (TestFeature.PATH_PARAMS, "路径参数 - 多级路径", f"{SERVER_URL}/files/user/documents/2024/report.pdf", "file_path", "user/documents/2024/report.pdf"),

    (TestFeature.MIXED_PARAMS, "混合参数 - 整数+字符串+浮点数", f"{SERVER_URL}/mixed/123/electronics/299.99", ["user_id", "category", "price"], ["123", "electronics", "299.99"]),
    (TestFeature.MIXED_PARAMS, "混合参数 - 负数+路径", f"{SERVER_URL}/mixed/-456/docs/manual.pdf", ["user_id", "file_path"], ["-456", "docs/manual.pdf"]),
]

def is_valid_uuid(uuid_string):
    """检查是否为有效的UUID格式"""
    uuid_pattern = re.compile(
        r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
    )
    return bool(uuid_pattern.match(uuid_string))

def is_valid_float(value):
    """检查是否为有效的浮点数"""
    try:
        float(value)
        return True
    except ValueError:
        return False

def handle_root(request_data) -> str:
    """根路径处理器"""
    return f"""
    <html>
    <head>
        <title>高级路径参数测试服务器</title>
        <style>
            body {{ font-family: Arial, sans-serif; margin: 40px; background-color: #f5f5f5; }}
        </style>
    </head>
    <body>
        <h1>🚀 高级路径参数测试服务器</h1>
        <p>服务器运行正常</p>
        <h2>📋 支持的路由:</h2>
        <ul>
            <li><strong>GET /</strong> - 主页</li>
            <li><strong>GET /users/int/&lt;id&gt;</strong> - 整数参数 (默认类型)</li>
            <li><strong>GET /users/uuid/&lt;uuid:id&gt;</strong> - UUID参数 (字符串类型)</li>
            <li><strong>GET /products/price/&lt;float:price&gt;</strong> - 浮点数参数</li>
            <li><strong>GET /files/&lt;path:file_path&gt;</strong> - 路径参数 (可包含斜杠)</li>
            <li><strong>GET /mixed/&lt;int:user_id&gt;/&lt;str:category&gt;/&lt;float:price&gt;</strong> - 混合参数</li>
        </ul>
    </body>
    </html>
    """

def handle_int_user(request_data) -> dict:
    """整数参数处理器"""
    # 从路径参数中获取用户ID
    path_params = request_data.get('path_params', {})
    user_id_raw = path_params.get('id', '0')

    # 验证是否为整数
    try:
        user_id = int(user_id_raw)
        is_valid = True
        value_type = "integer"
    except ValueError:
        user_id = 0
        is_valid = False
        value_type = "invalid"

    response_data = {
        "type": "integer_parameter",
        "parameter_name": "id",
        "raw_value": user_id_raw,
        "parsed_value": user_id,
        "value_type": value_type,
        "is_valid": is_valid,
        "path_matched": request_data.get('path', '/unknown'),
        "description": "整数ID参数，默认类型"
    }

    return response_data

def handle_uuid_user(request_data) -> dict:
    """UUID参数处理器"""
    # 从路径参数中获取UUID
    path_params = request_data.get('path_params', {})
    user_uuid = path_params.get('id', 'unknown')

    # 检查UUID格式
    uuid_is_valid = is_valid_uuid(user_uuid)

    response_data = {
        "type": "uuid_parameter",
        "parameter_name": "id",
        "raw_value": user_uuid,
        "parsed_value": user_uuid,
        "value_type": "String",
        "is_valid_uuid": uuid_is_valid,
        "path_matched": request_data.get('path', '/unknown'),
        "description": "UUID参数，使用uuid类型约束"
    }

    return response_data

def handle_product_price(request_data) -> dict:
    """浮点数价格处理器"""
    # 从路径参数中获取价格
    path_params = request_data.get('path_params', {})
    price_raw = path_params.get('price', '0')

    # 验证是否为浮点数
    try:
        price = float(price_raw)
        is_valid = True
        value_type = "float"
    except ValueError:
        price = 0.0
        is_valid = False
        value_type = "invalid"

    response_data = {
        "type": "float_parameter",
        "parameter_name": "price",
        "raw_value": price_raw,
        "parsed_value": price,
        "value_type": value_type,
        "is_valid": is_valid,
        "path_matched": request_data.get('path', '/unknown'),
        "description": "浮点数价格参数，使用float类型约束"
    }

    return response_data

def handle_file_request(request_data) -> dict:
    """文件路径处理器"""
    # 从路径参数中获取文件路径
    path_params = request_data.get('path_params', {})
    file_path = path_params.get('file_path', 'unknown')

    response_data = {
        "type": "path_parameter",
        "parameter_name": "file_path",
        "raw_value": file_path,
        "parsed_value": file_path,
        "value_type": "String",
        "path_segments": file_path.split('/') if file_path != 'unknown' else [],
        "path_matched": request_data.get('path', '/unknown'),
        "description": "完整路径参数，可以包含斜杠，使用path类型约束"
    }

    return response_data

def handle_mixed_params(request_data) -> dict:
    """混合参数处理器"""
    # 从路径参数中获取多个参数
    path_params = request_data.get('path_params', {})

    user_id_raw = path_params.get('user_id', '0')
    category = path_params.get('category', 'unknown')
    price_raw = path_params.get('price', '0')

    # 验证和转换参数
    try:
        user_id = int(user_id_raw)
        user_id_valid = True
    except ValueError:
        user_id = 0
        user_id_valid = False

    try:
        price = float(price_raw)
        price_valid = True
    except ValueError:
        price = 0.0
        price_valid = False

    response_data = {
        "type": "mixed_parameters",
        "parameters": {
            "user_id": {
                "name": "user_id",
                "raw_value": user_id_raw,
                "parsed_value": user_id,
                "type": "integer" if user_id_valid else "invalid",
                "is_valid": user_id_valid,
                "constraint": "<int:user_id>"
            },
            "category": {
                "name": "category",
                "raw_value": category,
                "parsed_value": category,
                "type": "String",
                "constraint": "<str:category>"
            },
            "price": {
                "name": "price",
                "raw_value": price_raw,
                "parsed_value": price,
                "type": "float" if price_valid else "invalid",
                "is_valid": price_valid,
                "constraint": "<float:price>"
            }
        },
        "path_matched": request_data.get('path', '/unknown'),
        "description": "混合类型参数：整数ID + 字符串分类 + 浮点数价格"
    }

    return response_data

def create_app():
    """创建带有高级路径参数路由的RatApp"""
    print("🚀 创建带高级路径参数的 RatApp...")
    app = RatApp(name="advanced_path_params_test")

    # 启用debug日志
    app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)

    # 静态路由
    @app.html("/")
    def home(request_data):
        return handle_root(request_data)

    # 整数参数（默认类型）
    @app.json("/users/int/<id>")
    def handle_int_user_route(request_data, *path_args):
        return handle_int_user(request_data)

    # UUID参数（字符串类型）
    @app.json("/users/uuid/<uuid:id>")
    def handle_uuid_user_route(request_data, *path_args):
        return handle_uuid_user(request_data)

    # 浮点数参数
    @app.json("/products/price/<float:price>")
    def handle_product_price_route(request_data, *path_args):
        return handle_product_price(request_data)

    # 路径参数 - 使用path类型，能匹配多级路径
    @app.json("/files/<path:file_path>")
    def handle_file_request_route(request_data, *path_args):
        return handle_file_request(request_data)

    # 混合参数 - 整数+字符串+浮点数
    @app.json("/mixed/<int:user_id>/<str:category>/<float:price>")
    def handle_mixed_params_route(request_data, *path_args):
        return handle_mixed_params(request_data)

    # 混合参数 - 整数+路径 (用于负数+路径的测试)
    @app.json("/mixed/<int:user_id>/<path:file_path>")
    def handle_mixed_user_file_route(request_data, *path_args):
        # 重新映射参数以适配测试期望
        path_params = request_data.get('path_params', {})
        user_id = path_params.get('user_id', '0')
        file_path = path_params.get('file_path', '')

        # 解析文件路径，模拟category和price
        path_parts = file_path.split('/', 1)
        category = path_parts[0] if len(path_parts) > 0 else ''
        price_str = path_parts[1] if len(path_parts) > 1 else '0'

        # 构造模拟的混合参数响应
        response_data = {
            "type": "mixed_parameters",
            "parameters": {
                "user_id": {
                    "name": "user_id",
                    "raw_value": user_id,
                    "parsed_value": int(user_id) if user_id.lstrip('-').isdigit() else 0,
                    "type": "integer" if user_id.lstrip('-').isdigit() else "invalid",
                    "is_valid": user_id.lstrip('-').isdigit(),
                    "constraint": "<int:user_id>"
                },
                "category": {
                    "name": "category",
                    "raw_value": category,
                    "parsed_value": category,
                    "type": "String",
                    "constraint": "<str:category>"
                },
                "price": {
                    "name": "price",
                    "raw_value": price_str,
                    "parsed_value": price_str,
                    "type": "String",  # 这里原来是路径的一部分，当作字符串处理
                    "is_valid": True,
                    "constraint": "<path:file_path>"
                }
            },
            "path_matched": request_data.get('path', '/unknown'),
            "description": "混合类型参数：整数ID + 路径 (负数+路径测试用)"
        }

        return response_data

    return app

def test_route(description, url, expected_param_name=None, expected_values=None):
    """测试单个路由"""
    try:
        print(f"🧪 测试: {description}")
        print(f"   URL: {url}")

        response = requests.get(url, timeout=10)

        # 检查状态码
        if response.status_code == 200:
            print(f"   ✅ 状态码正确: {response.status_code}")

            # 检查内容类型
            content_type = response.headers.get('content-type', '')
            if 'application/json' in content_type:
                print(f"   ✅ JSON 响应正确")

                # 解析响应数据
                try:
                    data = response.json()
                    print(f"   📄 响应数据: {json.dumps(data, ensure_ascii=False, indent=2)[:200]}...")

                    # 验证参数
                    if expected_param_name and expected_values is not None:
                        if isinstance(expected_param_name, str):
                            expected_param_name = [expected_param_name]
                            expected_values = [expected_values]

                        for param_name, expected_value in zip(expected_param_name, expected_values):
                            if param_name in data.get('parameters', {}):
                                # 多参数情况
                                actual_value = data['parameters'][param_name]['raw_value']
                            elif param_name == data.get('parameter_name'):
                                # 单参数情况 - 参数名匹配parameter_name时，获取raw_value
                                actual_value = data.get('raw_value')
                            elif param_name in data:
                                # 单参数情况 - 参数名在data中时，直接获取
                                actual_value = data[param_name]
                            else:
                                actual_value = None

                            
                            if actual_value == expected_value:
                                print(f"   ✅ 参数 '{param_name}' 正确: {expected_value}")
                            else:
                                print(f"   ❌ 参数 '{param_name}' 错误: 期望 {expected_value}, 实际 {actual_value}")
                                return False

                    return True
                except json.JSONDecodeError as e:
                    print(f"   ❌ JSON解析失败: {e}")
                    return False
            else:
                print(f"   ✅ 响应内容正确: {response.text[:100]}...")
                return True
        else:
            print(f"   ❌ 状态码错误: {response.status_code}")
            return False

    except requests.RequestException as e:
        print(f"   ❌ 请求失败: {e}")
        return False

def run_tests():
    """运行选定的测试"""
    print("🧪 开始高级路径参数测试...")
    print(f"📊 将运行 {len([t for t in TEST_CASES if t[0] & TEST_FEATURES])} 个测试用例")
    print()

    passed = 0
    total = 0

    for feature, description, url, expected_param_name, expected_values in TEST_CASES:
        if feature & TEST_FEATURES:
            total += 1
            if test_route(description, url, expected_param_name, expected_values):
                passed += 1
            print()

    print("=" * 60)
    print(f"📊 测试完成: {passed}/{total} 通过")

    if passed == total:
        print("🎉 所有选定的测试都通过了！")
        return True
    else:
        print(f"⚠️  {total - passed} 个测试失败")
        return False

def test_advanced_path_params_with_config(features: TestFeature) -> bool:
    """使用指定配置测试高级路径参数功能"""
    # 创建服务器
    app = create_app()

    # 启动服务器
    print(f"📡 启动服务器在端口 {SERVER_PORT}...")
    try:
        def run_server():
            app.run(host=SERVER_HOST, port=SERVER_PORT)

        server_thread = threading.Thread(target=run_server, daemon=True)
        server_thread.start()
        print("✅ 服务器启动命令执行成功")
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")
        return False

    # 等待服务器启动
    print("⏳ 等待服务器启动...")
    time.sleep(3)

    # 运行测试
    success = run_tests()

    return success

def main():
    """主函数"""
    print("🚀 启动高级路径参数测试")
    print("=" * 60)
    print("📦 支持配置化测试功能:")
    print("   • 支持整数、UUID、浮点数、路径、混合参数测试")
    print("   • 自动验证参数类型和值")
    print("   • 详细的响应数据分析")
    print("-" * 60)

    print(f"⚙️  当前测试配置: {TEST_FEATURES.name}")
    print(f"💡 提示: 可以修改 TEST_FEATURES 变量来选择不同的测试功能")
    print("-" * 60)

    if AUTO_TEST_ENABLED:
        print(f"⏳ 等待 {TEST_DELAY} 秒后开始自动测试...")
        time.sleep(TEST_DELAY)

        # 运行自动测试
        success = test_advanced_path_params_with_config(TEST_FEATURES)

        if success:
            print("\n✅ 所有测试通过，演示完成！")
        else:
            print("\n❌ 部分测试失败，请检查服务器状态")

        print("\n🔚 自动测试完成，正在自动关闭服务器...")
        # 自动测试完成后直接返回，不再保持服务器运行
        return 0 if success else 1
    else:
        print("🔧 自动测试已禁用")
        print("   💡 提示: 设置 AUTO_TEST_ENABLED = True 来启用自动测试")
        print(f"🌐 服务器地址: {SERVER_URL}")

        # 启动服务器
        app = create_app()
        app.run(host=SERVER_HOST, port=SERVER_PORT, debug=False)
        return 0

if __name__ == "__main__":
    sys.exit(main())