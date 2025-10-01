#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
动态路由测试示例
使用 PyO3 绑定的 RatApp 创建服务器并测试动态路由功能

支持配置化测试，可以选择性地测试特定的路由端点：
- 静态路由测试
- 动态路由测试
- 边界情况测试
- 404错误测试
"""

import time
import threading
import requests
import signal
import sys
from enum import Flag, auto
from rat_engine import RatApp, HttpRequest, HttpResponse, HttpMethod

# 测试功能枚举
class TestFeature(Flag):
    """测试功能枚举 - 使用 Flag 支持组合选择"""
    NONE = 0
    HOME = auto()           # 主页测试
    HEALTH = auto()         # 健康检查测试
    API_STATUS = auto()     # API状态测试
    USER_ROUTES = auto()    # 用户相关路由测试
    ITEM_ROUTES = auto()    # 物品相关路由测试
    MULTI_PARAM = auto()    # 多参数路由测试
    EDGE_CASES = auto()     # 边界情况测试
    NOT_FOUND = auto()      # 404错误测试
    
    # 预定义组合
    STATIC = HOME | HEALTH | API_STATUS           # 静态路由测试
    DYNAMIC = USER_ROUTES | ITEM_ROUTES | MULTI_PARAM  # 动态路由测试
    ERROR_HANDLING = NOT_FOUND                    # 错误处理测试
    ALL = HOME | HEALTH | API_STATUS | USER_ROUTES | ITEM_ROUTES | MULTI_PARAM | EDGE_CASES | NOT_FOUND  # 所有测试

# 配置开关
AUTO_TEST_ENABLED = True  # 设置为 False 可关闭自动测试
TEST_DELAY = 2  # 测试延迟秒数
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8083
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

# 测试配置 - 可以通过修改这里来选择要运行的测试
# 示例配置:
# TEST_FEATURES = TestFeature.STATIC          # 只测试静态路由
# TEST_FEATURES = TestFeature.DYNAMIC         # 只测试动态路由
# TEST_FEATURES = TestFeature.USER_ROUTES | TestFeature.ITEM_ROUTES  # 只测试用户和物品路由
TEST_FEATURES = TestFeature.ALL             # 测试所有功能

def create_dynamic_routes_server():
    """创建带有动态路由的服务器"""
    print("🚀 创建带动态路由的 RatApp...")
    app = RatApp(name="dynamic_routes_test")
    
    # 静态路由
    @app.html("/")
    def home(request_data):
        return "<h1>动态路由测试服务器</h1><p>服务器运行正常</p>"
    
    @app.json("/health")
    def health(request_data):
        return {"status": "ok", "message": "服务器健康"}
    
    @app.json("/api/v1/status")
    def api_status(request_data):
        return {"api_version": "v1", "status": "running"}
    
    # 动态路由 - 用户相关
    @app.json("/users/<id>")
    def get_user(request_data, *path_args):
        print(f"🔍 [DEBUG-PYTHON] get_user 收到的 request_data: {request_data}")
        print(f"🔍 [DEBUG-PYTHON] request_data 类型: {type(request_data)}")
        print(f"🔍 [DEBUG-PYTHON] path_args: {path_args}")
        if hasattr(request_data, 'get'):
            path_params = request_data.get('path_params', {})
            print(f"🔍 [DEBUG-PYTHON] path_params: {path_params}")
            print(f"🔍 [DEBUG-PYTHON] path_params 类型: {type(path_params)}")
        user_id = request_data.get('path_params', {}).get('id', 'unknown')
        print(f"🔍 [DEBUG-PYTHON] 提取的 user_id: {user_id}")
        return {
            "user_id": user_id,
            "name": f"用户{user_id}",
            "status": "active",
            "message": f"获取用户 {user_id} 的信息"
        }
    
    @app.json("/users/<id>/profile")
    def get_user_profile(request_data, *path_args):
        print(f"🔍 [DEBUG-PYTHON] get_user_profile 收到的 request_data: {request_data}")
        print(f"🔍 [DEBUG-PYTHON] path_args: {path_args}")
        path_params = request_data.get('path_params', {})
        print(f"🔍 [DEBUG-PYTHON] path_params: {path_params}")
        user_id = request_data.get('path_params', {}).get('id', 'unknown')
        print(f"🔍 [DEBUG-PYTHON] 提取的 user_id: {user_id}")
        return {
            "user_id": user_id,
            "profile": {
                "name": f"用户{user_id}",
                "email": f"user{user_id}@example.com",
                "created_at": "2024-01-01"
            },
            "message": f"获取用户 {user_id} 的个人资料"
        }
    
    # 动态路由 - API 相关
    @app.json("/api/v1/items/<id>")
    def get_item(request_data, *path_args):
        print(f"🔍 [DEBUG-PYTHON] get_item path_args: {path_args}")
        item_id = request_data.get('path_params', {}).get('id', 'unknown')
        return {
            "item_id": item_id,
            "name": f"物品{item_id}",
            "price": 99.99,
            "message": f"获取物品 {item_id} 的信息"
        }
    
    # 多层动态路由
    @app.json("/api/v1/users/<user_id>/posts/<post_id>")
    def get_user_post(request_data, *path_args):
        print(f"🔍 [DEBUG-PYTHON] get_user_post path_args: {path_args}")
        path_params = request_data.get('path_params', {})
        user_id = path_params.get('user_id', 'unknown')
        post_id = path_params.get('post_id', 'unknown')
        return {
            "user_id": user_id,
            "post_id": post_id,
            "title": f"用户{user_id}的帖子{post_id}",
            "content": "这是一个测试帖子",
            "message": f"获取用户 {user_id} 的帖子 {post_id}"
        }
    
    # 通配符路由（用于测试未匹配的路径）
    @app.json("/*")
    def catch_all(request_data):
        path = request_data.get('path', '/unknown')
        return {
            "error": "路径未找到",
            "path": path,
            "message": f"请求的路径 {path} 不存在"
        }, 404
    
    return app

class DynamicRouteTester:
    """动态路由测试器"""
    
    def __init__(self, base_url: str):
        self.base_url = base_url
        self.session = requests.Session()
        self.session.timeout = 10
    
    def get_test_cases(self) -> dict:
        """获取所有测试用例，按功能分组"""
        return {
            TestFeature.HOME: {
                "name": "首页",
                "url": f"{self.base_url}/",
                "expected_status": 200,
                "content_type": "text/html"
            },
            TestFeature.HEALTH: {
                "name": "健康检查",
                "url": f"{self.base_url}/health",
                "expected_status": 200,
                "expected_json": {"status": "ok"}
            },
            TestFeature.API_STATUS: {
                "name": "API状态",
                "url": f"{self.base_url}/api/v1/status",
                "expected_status": 200,
                "expected_json": {"api_version": "v1"}
            },
            TestFeature.USER_ROUTES: [
                {
                    "name": "获取用户123",
                    "url": f"{self.base_url}/users/123",
                    "expected_status": 200,
                    "expected_json": {"user_id": "123"}
                },
                {
                    "name": "获取用户456的资料",
                    "url": f"{self.base_url}/users/456/profile",
                    "expected_status": 200,
                    "expected_json": {"user_id": "456"}
                }
            ],
            TestFeature.ITEM_ROUTES: {
                "name": "获取物品789",
                "url": f"{self.base_url}/api/v1/items/789",
                "expected_status": 200,
                "expected_json": {"item_id": "789"}
            },
            TestFeature.MULTI_PARAM: {
                "name": "获取用户101的帖子202",
                "url": f"{self.base_url}/api/v1/users/101/posts/202",
                "expected_status": 200,
                "expected_json": {"user_id": "101", "post_id": "202"}
            },
            TestFeature.EDGE_CASES: [
                {
                    "name": "特殊字符用户ID",
                    "url": f"{self.base_url}/users/user-123",
                    "expected_status": 200,
                    "expected_json": {"user_id": "user-123"}
                },
                {
                    "name": "数字字母混合ID",
                    "url": f"{self.base_url}/users/abc123",
                    "expected_status": 200,
                    "expected_json": {"user_id": "abc123"}
                }
            ],
            TestFeature.NOT_FOUND: {
                "name": "不存在的路径",
                "url": f"{self.base_url}/nonexistent/path",
                "expected_status": 404,
                "expected_json": {"error": "路径未找到"}
            }
        }
    
    def test_single_case(self, test_case: dict) -> bool:
        """测试单个用例"""
        print(f"\n🧪 测试: {test_case['name']}")
        print(f"   URL: {test_case['url']}")
        
        try:
            response = self.session.get(test_case['url'], timeout=5)
            
            # 检查状态码
            if response.status_code == test_case['expected_status']:
                print(f"   ✅ 状态码正确: {response.status_code}")
                
                # 检查内容类型
                if 'content_type' in test_case:
                    content_type = response.headers.get('Content-Type', '')
                    if test_case['content_type'] in content_type:
                        print(f"   ✅ Content-Type 正确: {content_type}")
                    else:
                        print(f"   ⚠️  Content-Type 不匹配: 期望 {test_case['content_type']}, 实际 {content_type}")
                
                # 检查 JSON 响应
                if 'expected_json' in test_case:
                    try:
                        json_data = response.json()
                        expected = test_case['expected_json']
                        
                        # 检查期望的字段是否存在
                        all_fields_match = True
                        for key, value in expected.items():
                            if key not in json_data or json_data[key] != value:
                                all_fields_match = False
                                print(f"   ❌ JSON 字段不匹配: {key} 期望 {value}, 实际 {json_data.get(key)}")
                                break
                        
                        if all_fields_match:
                            print(f"   ✅ JSON 响应正确")
                            print(f"   📄 响应数据: {json_data}")
                            return True
                        
                    except Exception as json_err:
                        print(f"   ❌ JSON 解析失败: {json_err}")
                        print(f"   📄 原始响应: {response.text[:200]}...")
                        return False
                else:
                    # 非 JSON 响应，只要状态码正确就算成功
                    print(f"   📄 响应内容: {response.text[:100]}...")
                    return True
                    
            else:
                print(f"   ❌ 状态码错误: 期望 {test_case['expected_status']}, 实际 {response.status_code}")
                print(f"   📄 错误响应: {response.text[:200]}...")
                return False
                
        except Exception as e:
            print(f"   ❌ 请求失败: {e}")
            return False
        
        return False
    
    def run_selected_tests(self, features: TestFeature) -> bool:
        """运行选定的测试"""
        print(f"\n🧪 开始动态路由测试 (配置: {features.name if hasattr(features, 'name') else '自定义组合'})...")
        
        test_cases = self.get_test_cases()
        selected_tests = []
        
        # 根据配置选择测试用例
        for feature in TestFeature:
            if feature != TestFeature.NONE and feature != TestFeature.ALL and \
               feature != TestFeature.STATIC and feature != TestFeature.DYNAMIC and \
               feature != TestFeature.ERROR_HANDLING and (features & feature):
                if feature in test_cases:
                    case_data = test_cases[feature]
                    if isinstance(case_data, list):
                        selected_tests.extend([(feature, case) for case in case_data])
                    else:
                        selected_tests.append((feature, case_data))
        
        if not selected_tests:
            print("❌ 没有选择任何测试用例")
            return False
        
        print(f"📊 将运行 {len(selected_tests)} 个测试用例")
        
        success_count = 0
        total_tests = len(selected_tests)
        
        # 执行测试
        for feature, test_case in selected_tests:
            print(f"\n🎯 [{feature.name}] ", end="")
            if self.test_single_case(test_case):
                success_count += 1
            time.sleep(0.5)
        
        # 测试总结
        print(f"\n{'='*60}")
        print(f"🎯 测试完成: {success_count}/{total_tests} 通过")
        
        if success_count == total_tests:
            print("🎉 所有选定的动态路由测试都通过了！")
            return True
        elif success_count > 0:
            print("⚠️  部分测试通过，请检查失败的测试项。")
            return False
        else:
            print("❌ 所有测试都失败了，请检查服务器配置。")
            return False

def test_dynamic_routes():
    """测试动态路由功能（保持向后兼容）"""
    return test_dynamic_routes_with_config(TestFeature.ALL)

def test_dynamic_routes_with_config(features: TestFeature) -> bool:
    """使用指定配置测试动态路由功能"""
    # 创建服务器
    app = create_dynamic_routes_server()
    
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
    tester = DynamicRouteTester(SERVER_URL)
    return tester.run_selected_tests(features)

def print_test_configuration_help():
    """打印测试配置帮助信息"""
    print("\n📖 测试配置说明:")
    print("-" * 30)
    print("可以通过修改 TEST_FEATURES 变量来选择要运行的测试:")
    print("")
    print("🔹 单个功能测试:")
    print("   TEST_FEATURES = TestFeature.HOME         # 只测试主页")
    print("   TEST_FEATURES = TestFeature.HEALTH       # 只测试健康检查")
    print("   TEST_FEATURES = TestFeature.USER_ROUTES  # 只测试用户路由")
    print("   TEST_FEATURES = TestFeature.ITEM_ROUTES  # 只测试物品路由")
    print("")
    print("🔹 组合功能测试:")
    print("   TEST_FEATURES = TestFeature.STATIC       # 静态路由 (主页+健康检查+API状态)")
    print("   TEST_FEATURES = TestFeature.DYNAMIC      # 动态路由 (用户+物品+多参数)")
    print("   TEST_FEATURES = TestFeature.ERROR_HANDLING  # 错误处理 (404测试)")
    print("")
    print("🔹 自定义组合:")
    print("   TEST_FEATURES = TestFeature.USER_ROUTES | TestFeature.ITEM_ROUTES  # 用户+物品路由")
    print("   TEST_FEATURES = TestFeature.STATIC | TestFeature.EDGE_CASES       # 静态路由+边界测试")
    print("   TEST_FEATURES = TestFeature.HOME | TestFeature.NOT_FOUND          # 主页+404测试")
    print("")
    print("🔹 所有测试:")
    print("   TEST_FEATURES = TestFeature.ALL          # 运行所有测试 (默认)")
    print("")
    print("🔹 禁用测试:")
    print("   AUTO_TEST_ENABLED = False                # 完全禁用自动测试")
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
    
    print("🚀 RAT Engine 动态路由测试")
    print("=" * 50)
    print("📦 支持配置化测试功能:")
    print("   • 可选择性测试特定路由端点")
    print("   • 支持静态路由、动态路由、边界测试等")
    print("   • 减少日志输出，提高测试效率")
    print("-" * 50)
    
    # 显示当前测试配置
    if AUTO_TEST_ENABLED:
        print("\n⚙️  当前测试配置:")
        if TEST_FEATURES == TestFeature.ALL:
            print("   🔄 运行所有测试功能")
        elif TEST_FEATURES == TestFeature.STATIC:
            print("   🔹 静态路由测试 (主页 + 健康检查 + API状态)")
        elif TEST_FEATURES == TestFeature.DYNAMIC:
            print("   🌊 动态路由测试 (用户 + 物品 + 多参数)")
        elif TEST_FEATURES == TestFeature.ERROR_HANDLING:
            print("   ❌ 错误处理测试 (404测试)")
        else:
            # 显示自定义组合
            selected_features = []
            for feature in TestFeature:
                if feature != TestFeature.NONE and feature != TestFeature.ALL and \
                   feature != TestFeature.STATIC and feature != TestFeature.DYNAMIC and \
                   feature != TestFeature.ERROR_HANDLING and (TEST_FEATURES & feature):
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
    
    try:
        if AUTO_TEST_ENABLED:
            print(f"⏳ 等待 {TEST_DELAY} 秒后开始自动测试...")
            time.sleep(TEST_DELAY)
            
            # 运行自动测试
            success = test_dynamic_routes_with_config(TEST_FEATURES)
            
            if success:
                print("\n✅ 所有测试通过，演示完成！")
            else:
                print("\n❌ 部分测试失败，请检查服务器状态")
            
            print("\n🔚 自动测试完成，正在自动关闭服务器...")
            # 自动测试完成后直接返回，不再保持服务器运行
            return 0 if success else 1
        else:
            print("\n🔧 自动测试已禁用")
            print(f"🌐 服务器地址: {SERVER_URL}")
            print("\n按 Ctrl+C 停止服务器")
            
            # 只有在禁用自动测试时才保持服务器运行
            success = test_dynamic_routes_with_config(TestFeature.ALL)
            
            if success:
                print("\n🎉 动态路由测试成功完成！")
            else:
                print("\n💥 动态路由测试失败！")
            
            # 等待一下再退出
            print("\n⏳ 等待 2 秒后退出...")
            time.sleep(2)
            return 0 if success else 1
            
    except KeyboardInterrupt:
        print("\n🛑 用户中断测试")
        return 0
    except Exception as e:
        print(f"\n💥 测试过程中发生错误: {e}")
        import traceback
        traceback.print_exc()
        return 1

if __name__ == "__main__":
    main()