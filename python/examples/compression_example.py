#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
压缩功能示例

演示如何在 RAT Engine 中启用压缩功能
"""

import time
import json
import sys
import threading
import requests
from enum import Flag, auto
from rat_engine import RatApp

# 创建应用实例
app = RatApp(name="compression_example")

# 启用压缩功能
app.enable_compression(
    min_size=1024,  # 最小压缩大小（字节）
    level=6,        # 压缩级别（1-9）
    enable_gzip=True,
    enable_deflate=True,
    enable_brotli=True,
    enable_zstd=True,
    enable_lz4=False,
    # 可以自定义排除的内容类型和文件扩展名
    # excluded_content_types=["image/jpeg", "image/png"],
    # excluded_extensions=["jpg", "png"],
)

# 也可以使用 CompressionConfig 类创建配置（取消注释以使用此方法）
"""
# 导入压缩类型枚举（可选）
from rat_engine.compression import CompressionType

# 创建压缩配置对象
from rat_engine.compression import CompressionConfig
compression_config = CompressionConfig(
    min_size=1024,
    level=6,
    enable_gzip=True,
    enable_deflate=True,
    enable_brotli=True,
    enable_zstd=True,
    enable_lz4=False,
    # excluded_content_types=["image/jpeg", "image/png"],
    # excluded_extensions=["jpg", "png"],
)

# 使用配置对象启用压缩
app.enable_compression(compression_config)

# 也可以使用链式调用方式配置
# compression_config = CompressionConfig()\
#     .min_size(1024)\
#     .level(6)\
#     .enable_gzip()\
#     .enable_brotli()\
#     .enable_zstd()\
#     .disable_lz4()\
#     .exclude_content_types(["image/jpeg", "image/png"])\
#     .exclude_extensions(["jpg", "png"])
# app.enable_compression(compression_config)
"""

# 测试功能枚举
class TestFeature(Flag):
    """测试功能枚举 - 使用 Flag 支持组合选择"""
    NONE = 0
    SMALL_TEXT = auto()     # 小文本测试（不压缩）
    LARGE_TEXT = auto()      # 大文本测试（压缩）
    JSON_DATA = auto()       # JSON数据测试（压缩）
    
    # 预定义组合
    ALL = SMALL_TEXT | LARGE_TEXT | JSON_DATA  # 所有测试

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
    # 生成足够大的数据以触发压缩（超过 1024 字节）
    data = {
        "message": "Hello, World! 这是一个测试压缩功能的 JSON 响应",
        "timestamp": time.time(),
        "data": [i for i in range(500)],  # 增加数据量
        "nested": {
            "key1": "value1" * 20,  # 增加字符串长度
            "key2": "value2" * 20,
            "key3": "value3" * 20,
            "description": "这是一个用于测试压缩功能的大型 JSON 对象，包含重复的文本内容以便更好地展示压缩效果。" * 5
        },
        "additional_data": {
            "items": [{"id": i, "name": f"item_{i}", "description": f"这是第 {i} 个项目的描述信息"} for i in range(50)]
        }
    }
    return data

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
                        # 添加调试信息
                        print(f"   🔍 响应大小: {len(response.content)} 字节")
                        print(f"   🔍 响应编码: {response.encoding}")
                        
                        # 尝试解析 JSON
                        json_data = response.json()
                        print(f"   ✅ JSON 响应正确")
                        
                        # 显示部分数据
                        if isinstance(json_data, dict):
                            keys = list(json_data.keys())[:3]
                            preview = {k: str(json_data[k])[:50] + "..." if len(str(json_data[k])) > 50 else json_data[k] for k in keys}
                            print(f"   📄 响应数据预览: {preview}")
                        else:
                            print(f"   📄 响应数据: {str(json_data)[:100]}...")
                    else:
                        print(f"   ✅ 响应内容正确")
                        print(f"   📄 响应内容: {response.text[:100]}...")
                    return True
                except Exception as content_err:
                    print(f"   ❌ 响应内容解析失败: {content_err}")
                    print(f"   🔍 原始响应内容 (前100字节): {response.content[:100]}")
                    print(f"   🔍 响应头: {dict(response.headers)}")
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

def test_compression(features: TestFeature = TestFeature.ALL, port: int = 8000) -> bool:
    """测试压缩功能"""
    server_url = f"http://127.0.0.1:{port}"
    
    # 等待服务器启动
    print("⏳ 等待服务器启动...")
    time.sleep(2)
    
    # 运行测试
    tester = CompressionTester(server_url)
    return tester.run_selected_tests(features)

def main():
    """主函数 - 启动Web应用"""
    # 解析命令行参数
    import argparse
    parser = argparse.ArgumentParser(description="RAT Engine 压缩功能示例")
    parser.add_argument("--test", action="store_true", help="运行自动测试")
    parser.add_argument("--port", type=int, default=8000, help="服务器端口")
    args = parser.parse_args()
    
    port = args.port
    
    print("🚀 启动压缩功能演示服务器...")
    print(f"📍 访问地址: http://127.0.0.1:{port}")
    print("📋 可用路由:")
    print("   👋 Hello: /hello - 小文本（可能不会被压缩）")
    print("   📝 大文本: /large-text - 大文本（会被压缩）")
    print("   📊 JSON数据: /json - JSON 数据（会被压缩）")
    print("\n提示：可以使用浏览器开发者工具查看响应头中的 Content-Encoding 字段来确认是否启用了压缩")
    print("\n" + "="*50)
    
    # 启动服务器
    if args.test:
        # 非阻塞模式启动服务器，然后运行测试
        server_thread = threading.Thread(
            target=lambda: app.run(host="127.0.0.1", port=port, debug=True, blocking=True),
            daemon=True
        )
        server_thread.start()
        
        # 运行测试
        success = test_compression(TestFeature.ALL, port)
        sys.exit(0 if success else 1)
    else:
        try:
            # 启动服务器（阻塞模式，优雅处理退出信号）
            app.run(host="127.0.0.1", port=port, debug=True, blocking=True)
        except Exception as e:
            print(f"❌ 服务器启动失败: {e}")

if __name__ == "__main__":
    main()