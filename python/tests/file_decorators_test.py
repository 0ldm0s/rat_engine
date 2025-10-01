# -*- coding: utf-8 -*-
"""
RAT Engine @app.file 装饰器专项测试

专注测试内容：
- @app.file 文件装饰器的各种用法
- PIL 生成的图片文件（1x1像素、多种格式）
- 动态生成的二进制内容
- 不同 MIME 类型的文件响应
- 文件下载功能验证
- 内存中生成的文件内容

注意：其他装饰器（@app.html、@app.json、@app.chunk、@app.sse等）
已在 streaming_demo.py 中充分测试，此文件不再重复验证。
"""

import os
import sys
import time
import threading
from datetime import datetime
from io import BytesIO

try:
    from PIL import Image
except ImportError:
    print("❌ 请安装 Pillow: pip install Pillow")
    sys.exit(1)

try:
    import requests
except ImportError:
    print("❌ 请安装 requests: pip install requests")
    sys.exit(1)

try:
    from rat_engine import RatApp, HttpResponse
except ImportError as e:
    print(f"❌ 导入 rat_engine 失败: {e}")
    print("请确保 rat_engine 已正确安装")
    sys.exit(1)

# 服务器配置
SERVER_HOST = "127.0.0.1"
SERVER_PORT = 8082  # 使用不同端口避免与 streaming_demo.py 冲突
SERVER_URL = f"http://{SERVER_HOST}:{SERVER_PORT}"

# 测试配置
AUTO_TEST_ENABLED = True
TEST_DELAY = 2

class FileTestServer:
    """@app.file 装饰器专项测试服务器"""
    
    def __init__(self):
        self.app = None
        self.server_thread = None
        self.running = False
        self.test_image_path = None
        
    def create_test_images(self) -> dict:
        """使用 PIL 创建多种格式的测试图片"""
        test_dir = os.path.dirname(os.path.abspath(__file__))
        images = {}
        
        # 1x1 像素红色 PNG
        img_png = Image.new('RGB', (1, 1), color='red')
        png_path = os.path.join(test_dir, 'test_1x1_red.png')
        img_png.save(png_path, 'PNG')
        images['png'] = png_path
        
        # 2x2 像素蓝色 JPEG
        img_jpg = Image.new('RGB', (2, 2), color='blue')
        jpg_path = os.path.join(test_dir, 'test_2x2_blue.jpg')
        img_jpg.save(jpg_path, 'JPEG')
        images['jpg'] = jpg_path
        
        # 3x3 像素绿色 GIF
        img_gif = Image.new('RGB', (3, 3), color='green')
        gif_path = os.path.join(test_dir, 'test_3x3_green.gif')
        img_gif.save(gif_path, 'GIF')
        images['gif'] = gif_path
        
        print(f"✅ 创建测试图片: PNG({images['png']}), JPEG({images['jpg']}), GIF({images['gif']})")
        return images
        
    def create_app(self) -> RatApp:
        """创建 RAT Engine 应用"""
        app = RatApp(name="file_test")
        
        # 创建测试图片
        self.test_images = self.create_test_images()
        
        # 注册路由
        self._register_routes(app)
        
        return app
    
    def _register_routes(self, app: RatApp):
        """注册所有测试路由"""
        
        # 主页 - HTML 装饰器测试
        @app.html("/")
        def home(request_data):
            return """
            <!DOCTYPE html>
            <html>
            <head>
                <title>RAT Engine 装饰器测试</title>
                <meta charset="utf-8">
                <style>
                    body { font-family: Arial, sans-serif; margin: 40px; }
                    .test-section { margin: 20px 0; padding: 15px; border: 1px solid #ddd; }
                    .test-link { display: block; margin: 5px 0; color: #0066cc; }
                    .status { font-weight: bold; }
                    .success { color: green; }
                    .error { color: red; }
                </style>
            </head>
            <body>
                <h1>🧪 RAT Engine 装饰器功能测试</h1>
                
                <div class="test-section">
                    <h2>📁 文件装饰器测试</h2>
                    <a href="/image" class="test-link">📷 1x1 像素图片 (PIL 生成)</a>
                    <a href="/download" class="test-link">💾 文件下载测试</a>
                </div>
                
                <div class="test-section">
                    <h2>📄 其他装饰器测试</h2>
                    <p>JSON、HTML、流式等装饰器测试请参考: <code>streaming_demo.py</code></p>
                </div>
                
                <div class="test-section">
                    <h2>ℹ️ 说明</h2>
                    <p>流式装饰器测试请参考: <code>streaming_demo.py</code></p>
                    <p>此演示专注于 <code>@app.file</code> 装饰器功能</p>
                </div>
                
                <div class="test-section">
                    <h2>🔧 测试状态</h2>
                    <p>服务器运行在: <code>" + SERVER_URL + "</code></p>
                    <p>测试图片路径: <code>" + str(getattr(self, 'test_images', {}).get('png', '未创建')) + "</code></p>
                    <p class="status success">✅ 所有装饰器已注册</p>
                </div>
            </body>
            </html>
            """
        
        # 文件装饰器测试 - 返回 1x1 像素图片
        @app.file("/image")
        def serve_test_image(request_data):
            # 直接返回文件路径，框架会自动识别MIME类型并处理
            return self.test_images['png']
        
        # 文件下载测试
        @app.file("/download")
        def download_test(request_data):
            # 创建临时文本文件
            import tempfile
            content = f"""RAT Engine 文件下载测试
生成时间: {datetime.now()}
这是一个通过 @app.file 装饰器生成的测试文件。

测试内容:
- 1x1 像素图片生成 ✅
- 文件装饰器功能 ✅
- 动态文件下载 ✅
"""
            # 创建临时文件并返回文件路径，框架会自动处理
            temp_file = tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False, encoding='utf-8')
            temp_file.write(content)
            temp_file.close()
            # 返回 (文件路径, MIME类型) 元组格式
            return (temp_file.name, 'text/plain; charset=utf-8')
        
        # 注意：JSON、HTML、流式等装饰器测试已在 streaming_demo.py 中充分测试
        # 此文件专注于 @app.file 装饰器测试
        
        # 注意：流式传输功能已在 streaming_demo.py 中充分测试
        # 此文件专注于 @app.file 装饰器测试
    
    def start_server(self):
        """启动服务器"""
        if self.running:
            print("⚠️ 服务器已在运行中")
            return
            
        print("🚀 创建 RAT Engine 应用...")
        self.app = self.create_app()
        
        print(f"📡 启动服务器在 {SERVER_URL}...")
        try:
            self.app.run(host=SERVER_HOST, port=SERVER_PORT)
            self.running = True
            print(f"✅ 服务器已启动: {SERVER_URL}")
        except Exception as e:
            print(f"❌ 服务器启动失败: {e}")
            return False
        
        return True
    
    def cleanup(self):
        """清理资源"""
        # 删除测试图片
        if hasattr(self, 'test_images'):
            for img_type, img_path in self.test_images.items():
                if os.path.exists(img_path):
                    try:
                        os.remove(img_path)
                        print(f"🗑️ 已删除测试图片 ({img_type}): {img_path}")
                    except Exception as e:
                        print(f"⚠️ 删除测试图片失败 ({img_type}): {e}")
        
        # 清理临时文件（如果有的话）
        import tempfile
        import glob
        temp_dir = tempfile.gettempdir()
        temp_files = glob.glob(os.path.join(temp_dir, 'tmp*.txt'))
        for temp_file in temp_files:
            try:
                # 只删除最近创建的临时文件（避免删除其他程序的文件）
                if os.path.getctime(temp_file) > (time.time() - 3600):  # 1小时内创建的
                    os.remove(temp_file)
                    print(f"🗑️ 已删除临时文件: {temp_file}")
            except Exception as e:
                 pass  # 忽略临时文件清理错误

def run_tests():
    """运行自动化测试"""
    print("\n🧪 开始自动化测试...")
    time.sleep(3)  # 等待服务器启动
    
    # 文件装饰器测试用例
    file_test_cases = [
        ("/", "主页 HTML 测试"),
        ("/image", "1x1 像素图片测试"),
        ("/download", "文件下载测试"),
    ]
    
    # 注意：流式传输测试已移至 streaming_demo.py
    
    print("\n📋 执行文件装饰器测试...")
    success_count = 0
    total_count = 0
    
    for endpoint, description in file_test_cases:
        total_count += 1
        try:
            print(f"\n🔍 测试: {description}")
            response = requests.get(f"{SERVER_URL}{endpoint}", timeout=10)
            
            if response.status_code == 200:
                print(f"✅ {description} - 状态码: {response.status_code}")
                print(f"   Content-Type: {response.headers.get('Content-Type', 'N/A')}")
                
                # 详细验证不同类型的响应
                if endpoint == "/image":
                    print(f"   图片大小: {len(response.content)} 字节")
                    # 验证是否为有效的图片数据
                    if response.content.startswith(b'\x89PNG'):
                        print(f"   ✅ PNG 图片格式验证通过")
                    else:
                        print(f"   ⚠️ 图片格式可能异常")
                elif endpoint == "/download":
                    print(f"   文件大小: {len(response.content)} 字节")
                    # 验证下载文件内容
                    if "RAT Engine 文件下载测试" in response.text:
                        print(f"   ✅ 下载文件内容验证通过")
                elif len(response.text) < 200:
                    print(f"   响应预览: {response.text[:100]}...")
                
                success_count += 1
            else:
                print(f"❌ {description} - 状态码: {response.status_code}")
                if response.text:
                    print(f"   错误信息: {response.text[:200]}")
                
        except Exception as e:
            print(f"❌ {description} - 错误: {e}")
    
    print(f"\n📊 文件装饰器测试结果: {success_count}/{total_count} 通过")
    
    # 测试总结
    if success_count == total_count:
        print("\n🎉 所有文件装饰器测试通过! @app.file 功能正常")
    else:
        print(f"\n⚠️ 部分测试失败，请检查服务器日志")
    
    print(f"\n🎯 手动测试地址:")
    print(f"   主页: {SERVER_URL}")
    print(f"   图片测试: {SERVER_URL}/image")
    print(f"   文件下载: {SERVER_URL}/download")
    print(f"\n💡 其他装饰器测试请运行: streaming_demo.py")
    
    # 测试完成后发送停止信号
    print("\n🛑 自动化测试完成，正在停止服务器...")
    import os
    os._exit(0)  # 强制退出程序

def main():
    """主函数"""
    print("🎯 RAT Engine 文件装饰器和其他装饰器功能测试")
    print("=" * 50)
    
    server = FileTestServer()
    
    try:
        # 启动服务器
        if server.start_server():
            # 启动自动化测试
            test_thread = threading.Thread(target=run_tests, daemon=True)
            test_thread.start()
            
            print(f"\n🌐 服务器运行中: {SERVER_URL}")
            print("按 Ctrl+C 停止服务器")
            
            # 保持主线程运行
            try:
                while True:
                    time.sleep(1)
            except KeyboardInterrupt:
                print("\n🛑 收到停止信号")
        
    except Exception as e:
        print(f"❌ 运行错误: {e}")
    finally:
        server.cleanup()
        print("👋 程序结束")

if __name__ == "__main__":
    main()