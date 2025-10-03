#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine @app.file 装饰器验证示例

此脚本演示如何：
1. 创建和启动 RAT Engine 服务器
2. 注册 @app.file 装饰器处理图片文件
3. 使用 PIL 生成测试图片
4. 自动验证装饰器功能是否正常工作
"""

import os
import sys
import io
import time
import threading
from PIL import Image, ImageDraw, ImageFont
import requests

# 添加 rat 模块路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

try:
    from rat_engine import RatApp
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    print("请确保 rat_engine 已正确安装")
    sys.exit(1)


class ImageFileServer:
    """图片文件服务器类"""

    def __init__(self, host="127.0.0.1", port=8081):
        self.host = host
        self.port = port
        self.app = RatApp(name="image_test_server")
        self.test_images = {}
        self.server_running = False

    def create_test_images(self):
        """使用 PIL 创建测试图片文件"""
        print("🔧 开始创建测试图片")

        # 1x1 像素红色 PNG
        red_img = Image.new('RGB', (1, 1), color='red')
        red_path = 'test_red_1x1.png'
        red_img.save(red_path, 'PNG')
        self.test_images['red_1x1'] = red_path

        # 2x2 像素蓝色 JPEG
        blue_img = Image.new('RGB', (2, 2), color='blue')
        blue_path = 'test_blue_2x2.jpg'
        blue_img.save(blue_path, 'JPEG')
        self.test_images['blue_2x2'] = blue_path

        # 3x3 像素绿色 GIF
        green_img = Image.new('RGB', (3, 3), color='green')
        green_path = 'test_green_3x3.gif'
        green_img.save(green_path, 'GIF')
        self.test_images['green_3x3'] = green_path

        # 100x100 像素紫色 PNG（带文字）
        purple_img = Image.new('RGB', (100, 100), color='purple')
        draw = ImageDraw.Draw(purple_img)
        try:
            font = ImageFont.truetype("/System/Library/Fonts/Arial.ttf", 20)
        except:
            font = ImageFont.load_default()
        draw.text((25, 40), "100x100", fill='white', font=font)
        purple_path = 'test_purple_100x100.png'
        purple_img.save(purple_path, 'PNG')
        self.test_images['purple_100x100'] = purple_path

        print(f"✅ 创建了 {len(self.test_images)} 个测试图片文件")
        return self.test_images

    def register_routes(self):
        """注册所有路由"""
        print("🔧 注册路由")

        # 主页 - 显示测试链接
        @self.app.html("/")
        def home(request_data):
            return """
            <!DOCTYPE html>
            <html>
            <head>
                <title>RAT Engine @app.file 装饰器测试</title>
                <meta charset="utf-8">
                <style>
                    body { font-family: Arial, sans-serif; margin: 40px; }
                    .test-link { display: block; margin: 10px 0; padding: 10px; background: #f0f0f0; text-decoration: none; border-radius: 5px; }
                    .test-link:hover { background: #e0e0e0; }
                </style>
            </head>
            <body>
                <h1>🖼️ @app.file 装饰器测试</h1>
                <p>测试 RAT Engine 的文件装饰器功能：</p>
                <a href="/image/red" class="test-link">🔴 1x1 红色 PNG</a>
                <a href="/image/blue" class="test-link">🔵 2x2 蓝色 JPEG</a>
                <a href="/image/green" class="test-link">🟢 3x3 绿色 GIF</a>
                <a href="/image/purple" class="test-link">🟣 100x100 紫色 PNG（带文字）</a>
                <a href="/api/test-info" class="test-link">📋 测试信息 API</a>
            </body>
            </html>
            """

        # 红色 1x1 PNG 图片
        @self.app.file("/image/red")
        def red_image(request_data):
            """返回 1x1 红色 PNG 图片"""
            return self.test_images['red_1x1'], "image/png"

        # 蓝色 2x2 JPEG 图片
        @self.app.file("/image/blue")
        def blue_image(request_data):
            """返回 2x2 蓝色 JPEG 图片"""
            return self.test_images['blue_2x2'], "image/jpeg"

        # 绿色 3x3 GIF 图片
        @self.app.file("/image/green")
        def green_image(request_data):
            """返回 3x3 绿色 GIF 图片"""
            return self.test_images['green_3x3'], "image/gif"

        # 紫色 100x100 PNG 图片
        @self.app.file("/image/purple")
        def purple_image(request_data):
            """返回 100x100 紫色 PNG 图片"""
            return self.test_images['purple_100x100'], "image/png"

        # 传统图片路由（兼容性）
        @self.app.file("/image")
        def default_image(request_data):
            """默认返回 1x1 红色图片"""
            return self.test_images['red_1x1'], "image/png"

        # 测试信息 API
        @self.app.json("/api/test-info")
        def test_info(request_data):
            """返回测试信息"""
            return {
                'server': 'RAT Engine @app.file 装饰器测试服务器',
                'images': {
                    'red_1x1': {'path': self.test_images['red_1x1'], 'type': 'PNG'},
                    'blue_2x2': {'path': self.test_images['blue_2x2'], 'type': 'JPEG'},
                    'green_3x3': {'path': self.test_images['green_3x3'], 'type': 'GIF'},
                    'purple_100x100': {'path': self.test_images['purple_100x100'], 'type': 'PNG'}
                },
                'endpoints': [
                    '/image/red',
                    '/image/blue',
                    '/image/green',
                    '/image/purple',
                    '/image',
                    '/api/test-info'
                ]
            }

        print("✅ 路由注册完成")

    def start_server(self, blocking=False):
        """启动服务器"""
        try:
            # 创建测试图片
            self.create_test_images()

            # 注册路由
            self.register_routes()

            url = f"http://{self.host}:{self.port}"
            print(f"🚀 启动 @app.file 装饰器测试服务器")
            print(f"📍 地址: {url}")
            print(f"📋 测试页面: {url}/")
            print("=" * 50)

            self.server_running = True

            if blocking:
                # 阻塞模式启动
                self.app.run(host=self.host, port=self.port, debug=True, blocking=True)
            else:
                # 非阻塞模式启动（在后台线程中运行）
                server_thread = threading.Thread(
                    target=self.app.run,
                    kwargs={'host': self.host, 'port': self.port, 'debug': True, 'blocking': True}
                )
                server_thread.daemon = True
                server_thread.start()

                # 等待服务器启动
                time.sleep(2)

                return server_thread

        except Exception as e:
            print(f"❌ 服务器启动失败: {e}")
            return None


def validate_image_response(image_data: bytes, expected_format: str = None, expected_size: tuple = None) -> dict:
    """验证图片响应"""
    result = {
        'valid': False,
        'format': None,
        'size': len(image_data),
        'dimensions': None,
        'mode': None,
        'error': None,
        'pixel': None
    }

    try:
        if len(image_data) == 0:
            result['error'] = "接收到空的图片数据"
            return result

        img_buffer = io.BytesIO(image_data)
        with Image.open(img_buffer) as img:
            result['format'] = img.format
            result['dimensions'] = img.size
            result['mode'] = img.mode
            img.verify()

        # 重新打开获取详细信息
        img_buffer = io.BytesIO(image_data)
        with Image.open(img_buffer) as img:
            if img.size == (1, 1):
                result['pixel'] = img.getpixel((0, 0))

            if expected_format and result['format'].upper() != expected_format.upper():
                result['error'] = f"格式不匹配，期望: {expected_format}, 实际: {result['format']}"
                return result

            if expected_size and result['dimensions'] != expected_size:
                result['error'] = f"尺寸不匹配，期望: {expected_size}, 实际: {result['dimensions']}"
                return result

        result['valid'] = True

    except Exception as e:
        result['error'] = f"图片验证失败: {str(e)}"

    return result


def test_server_endpoints(base_url="http://127.0.0.1:8081"):
    """测试服务器端点"""
    print("🧪 测试 @app.file 装饰器端点", flush=True)
    print("-" * 40, flush=True)

    test_cases = [
        {
            'url': f'{base_url}/image/red',
            'name': '红色 1x1 PNG',
            'expected_format': 'PNG',
            'expected_size': (1, 1),
            'expected_pixel': (255, 0, 0)
        },
        {
            'url': f'{base_url}/image/blue',
            'name': '蓝色 2x2 JPEG',
            'expected_format': 'JPEG',
            'expected_size': (2, 2)
        },
        {
            'url': f'{base_url}/image/green',
            'name': '绿色 3x3 GIF',
            'expected_format': 'GIF',
            'expected_size': (3, 3)
        },
        {
            'url': f'{base_url}/image/purple',
            'name': '紫色 100x100 PNG',
            'expected_format': 'PNG',
            'expected_size': (100, 100)
        },
        {
            'url': f'{base_url}/image',
            'name': '默认图片（红色 1x1）',
            'expected_format': 'PNG',
            'expected_size': (1, 1)
        }
    ]

    success_count = 0
    total_count = len(test_cases)

    for i, test_case in enumerate(test_cases, 1):
        print(f"\n📸 测试 {i}/{total_count}: {test_case['name']}")
        print(f"   URL: {test_case['url']}")

        try:
            response = requests.get(test_case['url'], timeout=5)

            if response.status_code == 200:
                print(f"   ✅ HTTP 请求成功")

                # 检查响应头
                content_type = response.headers.get('Content-Type', '')
                print(f"   📋 Content-Type: {content_type}")

                # 验证图片
                result = validate_image_response(
                    response.content,
                    expected_format=test_case.get('expected_format'),
                    expected_size=test_case.get('expected_size')
                )

                if result['valid']:
                    print(f"   ✅ 图片验证通过")
                    print(f"      📐 尺寸: {result['dimensions']}")
                    print(f"      🎨 格式: {result['format']}")
                    print(f"      💾 大小: {result['size']} 字节")

                    if 'pixel' in result and 'expected_pixel' in test_case:
                        expected_pixel = test_case['expected_pixel']
                        actual_pixel = result['pixel']

                        if actual_pixel == expected_pixel:
                            print(f"      🔴 像素值: {actual_pixel} ✅")
                        else:
                            print(f"      🔴 像素值: {actual_pixel} (期望: {expected_pixel}) ⚠️")

                    success_count += 1
                else:
                    print(f"   ❌ 图片验证失败: {result['error']}")
            else:
                print(f"   ❌ HTTP 请求失败 (状态码: {response.status_code})")

        except requests.exceptions.ConnectionError:
            print(f"   ❌ 连接失败 - 服务器可能未运行")
            return False
        except Exception as e:
            print(f"   ❌ 请求异常: {e}")

    print(f"\n📊 测试结果: {success_count}/{total_count} 通过", flush=True)
    return success_count == total_count


def main():
    """主函数 - 简单直接的自动验证"""
    print("🖼️  RAT Engine @app.file 装饰器验证示例")
    print("=" * 50)

    try:
        # 创建并启动服务器
        server = ImageFileServer(host="127.0.0.1", port=8081)

        # 非阻塞模式启动服务器
        server_thread = server.start_server(blocking=False)

        if server_thread:
            # 等待服务器完全启动
            print("⏳ 等待服务器启动...")
            time.sleep(3)

            # 自动运行测试
            success = test_server_endpoints("http://127.0.0.1:8081")

            if success:
                print("\n🎉 所有测试通过！@app.file 装饰器功能正常！", flush=True)
            else:
                print("\n⚠️ 部分测试失败", flush=True)

            print("\n✅ 验证完成，自动退出", flush=True)
            print(f"💡 如需继续访问服务器，可手动运行: python3 {__file__}", flush=True)
            sys.exit(0 if success else 1)
        else:
            print("❌ 服务器启动失败")

    except Exception as e:
        print(f"\n❌ 发生错误: {e}")


if __name__ == "__main__":
    main()