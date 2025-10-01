#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
压缩功能测试
使用 RatApp 创建服务器并测试压缩功能

支持配置化测试，可以选择性地测试特定的压缩功能：
- 小文本测试（不压缩）
- 大文本测试（压缩）
- JSON数据测试（压缩）
"""

import time
import threading
import requests
import signal
import sys
from enum import Flag, auto
from rat_engine import RatApp, CompressionConfig

# 测试功能枚举
class TestFeature(Flag):
    """测试功能枚举 - 使用 Flag 支持组合选择"""
    NONE = 0
    SMALL_TEXT = auto()     # 小文本测试（不压缩）
    LARGE_TEXT = auto()      # 大文本测试（压缩）
    JSON_DATA = auto()       # JSON数据测试（压缩）
    
    # 预定义组合
    ALL = SMALL_TEXT | LARGE_TEXT | JSON_DATA  # 所有测试

# 配置开关
AUTO_TEST_ENABLED = True  # 设置为 False 可关闭自动测试
TEST_DELAY = 2  # 测试延迟秒数
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8084
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

# 测试配置 - 可以通过修改这里来选择要运行的测试
TEST_FEATURES = TestFeature.ALL  # 测试所有功能

def create_compression_server():
    """创建带有压缩功能的服务器"""
    print("🚀 创建带压缩功能的 RatApp...")
    app = RatApp(name="compression_test")
    
    # 启用压缩功能
    app.enable_compression(
        min_size=1024,  # 最小压缩大小（字节）
        level=6,        # 压缩级别（1-9）
        enable_gzip=True,
        enable_deflate=True,
        enable_brotli=True,
        enable_zstd=True,
        enable_lz4=False,
    )
    
    # 定义处理函数 - 小文本（可能不会被压缩）
    @app.html("/hello")
    def handle_hello(request_data):
        return "<h1>Hello, World!</h1>"
    
    # 定义一个返回大量文本的处理函数，用于测试压缩效果
    @app.html("/large-text")
    def handle_large_text(request_data):
        # 生成大量文本
        text = "这是一段可以被压缩的重复文本。" * 1000
        return text
    
    # 定义一个返回 JSON 的处理函数
    @app.json("/json")
    def handle_json(request_data):
        data = {
            "message": "Hello, World!",
            "timestamp": time.time(),
            "data": [i for i in range(100)],
            "nested": {
                "key1": "value1",
                "key2": "value2",
                "key3": "value3",
            }
        }
        return data
    
    return app

class CompressionTester:
    """压缩功能测试器"""
    
    def __init__(self, base_url: str):
        self.base_url = base_url
        self.session = requests.Session()
        self.session.timeout = 10
    
    def get_test_cases(self) -> dict:
        """获取所有测试用例，按功能分组"""
        return {
            TestFeature.SMALL_TEXT: {
                "name": "小文本（不压缩）",
                "url": f"{self.base_url}/hello",
                "expected_status": 200,
                "content_type": "text/html",
                "check_compression": False
            },
            TestFeature.LARGE_TEXT: {
                "name": "大文本（压缩）",
                "url": f"{self.base_url}/large-text",
                "expected_status": 200,
                "content_type": "text/html",
                "check_compression": True
            },
            TestFeature.JSON_DATA: {
                "name": "JSON数据（压缩）",
                "url": f"{self.base_url}/json",
                "expected_status": 200,
                "content_type": "application/json",
                "check_compression": True
            }
        }
    
    def test_single_case(self, test_case: dict) -> bool:
        """测试单个用例"""
        print(f"\n🧪 测试: {test_case['name']}")
        print(f"   URL: {test_case['url']}")
        
        try:
            # 设置请求头，表示接受压缩
            headers = {
                'Accept-Encoding': 'gzip, deflate, br, zstd'
            }
            
            response = self.session.get(test_case['url'], headers=headers, timeout=5)
            
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
                
                # 检查压缩
                if test_case.get('check_compression', False):
                    content_encoding = response.headers.get('Content-Encoding', '')
                    if content_encoding:
                        print(f"   ✅ 压缩正确: {content_encoding}")
                    else:
                        print(f"   ❌ 未检测到压缩，Content-Encoding 头不存在")
                        return False
                else:
                    content_encoding = response.headers.get('Content-Encoding', '')
                    if content_encoding:
                        print(f"   ℹ️ 检测到压缩: {content_encoding} (不要求压缩)")
                    else:
                        print(f"   ✅ 未压缩，符合预期")
                
                # 检查响应内容
                try:
                    if 'application/json' in response.headers.get('Content-Type', ''):
                        json_data = response.json()
                        print(f"   ✅ JSON 响应正确")
                        print(f"   📄 响应数据: {str(json_data)[:100]}...")
                    else:
                        print(f"   ✅ 响应内容正确")
                        print(f"   📄 响应内容: {response.text[:100]}...")
                    return True
                except Exception as content_err:
                    print(f"   ❌ 响应内容解析失败: {content_err}")
                    return False
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
        print(f"\n🧪 开始压缩功能测试 (配置: {features.name if hasattr(features, 'name') else '自定义组合'})...")
        
        test_cases = self.get_test_cases()
        selected_tests = []
        
        # 根据配置选择测试用例
        for feature in TestFeature:
            if feature != TestFeature.NONE and feature != TestFeature.ALL and (features & feature):
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
            print("🎉 所有选定的压缩功能测试都通过了！")
            return True
        elif success_count > 0:
            print("⚠️  部分测试通过，请检查失败的测试项。")
            return False
        else:
            print("❌ 所有测试都失败了，请检查服务器配置。")
            return False

def test_compression():
    """测试压缩功能（保持向后兼容）"""
    return test_compression_with_config(TestFeature.ALL)

def test_compression_with_config(features: TestFeature) -> bool:
    """使用指定配置测试压缩功能"""
    # 创建服务器
    app = create_compression_server()
    
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
    tester = CompressionTester(SERVER_URL)
    return tester.run_selected_tests(features)

def signal_handler(signum, frame):
    """信号处理器"""
    print("\n\n🛑 接收到停止信号，正在关闭服务器...")
    sys.exit(0)

def main():
    """主函数"""
    # 注册信号处理器
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    print("🚀 RAT Engine 压缩功能测试")
    print("=" * 50)
    print("📦 支持配置化测试功能:")
    print("   • 可选择性测试特定压缩功能")
    print("   • 支持小文本、大文本、JSON数据测试")
    print("   • 自动检测压缩是否正确应用")
    print("-" * 50)
    
    # 显示当前测试配置
    if AUTO_TEST_ENABLED:
        print("\n⚙️  当前测试配置:")
        if TEST_FEATURES == TestFeature.ALL:
            print("   🔄 运行所有测试功能")
        else:
            # 显示自定义组合
            selected_features = []
            for feature in TestFeature:
                if feature != TestFeature.NONE and feature != TestFeature.ALL and (TEST_FEATURES & feature):
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
            success = test_compression_with_config(TEST_FEATURES)
            
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
            success = test_compression_with_config(TestFeature.ALL)
            
            if success:
                print("\n🎉 压缩功能测试成功完成！")
            else:
                print("\n💥 压缩功能测试失败！")
            
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