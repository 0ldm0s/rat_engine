#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
验证生成的 1x1 像素图片
"""

import os
import sys
from PIL import Image
import requests

def verify_local_image():
    """验证本地生成的图片文件"""
    image_path = "test_1x1_pixel.png"
    
    if not os.path.exists(image_path):
        print(f"❌ 图片文件不存在: {image_path}")
        return False
    
    try:
        with Image.open(image_path) as img:
            print(f"✅ 成功打开图片: {image_path}")
            print(f"   尺寸: {img.size}")
            print(f"   模式: {img.mode}")
            print(f"   格式: {img.format}")
            
            # 获取像素值
            if img.size == (1, 1):
                pixel = img.getpixel((0, 0))
                print(f"   像素值: {pixel}")
                print(f"   文件大小: {os.path.getsize(image_path)} 字节")
                return True
            else:
                print(f"❌ 图片尺寸不正确，期望 (1, 1)，实际 {img.size}")
                return False
                
    except Exception as e:
        print(f"❌ 打开图片失败: {e}")
        return False

def verify_server_image():
    """验证服务器返回的图片"""
    url = "http://127.0.0.1:8081/image"
    
    try:
        response = requests.get(url, timeout=5)
        
        if response.status_code == 200:
            print(f"✅ 服务器图片请求成功")
            print(f"   Content-Type: {response.headers.get('Content-Type')}")
            print(f"   Content-Length: {len(response.content)} 字节")
            
            # 尝试解析图片
            try:
                from io import BytesIO
                img = Image.open(BytesIO(response.content))
                print(f"   图片尺寸: {img.size}")
                print(f"   图片模式: {img.mode}")
                print(f"   图片格式: {img.format}")
                
                if img.size == (1, 1):
                    pixel = img.getpixel((0, 0))
                    print(f"   像素值: {pixel}")
                    return True
                else:
                    print(f"❌ 服务器图片尺寸不正确")
                    return False
                    
            except Exception as e:
                print(f"❌ 解析服务器图片失败: {e}")
                return False
        else:
            print(f"❌ 服务器请求失败，状态码: {response.status_code}")
            return False
            
    except Exception as e:
        print(f"❌ 请求服务器图片失败: {e}")
        return False

def test_other_endpoints():
    """测试其他端点"""
    base_url = "http://127.0.0.1:8081"
    
    endpoints = [
        ("/api/json", "JSON API"),
        ("/download", "文件下载"),
        ("/html-test", "HTML 测试")
    ]
    
    for endpoint, name in endpoints:
        try:
            response = requests.get(f"{base_url}{endpoint}", timeout=5)
            if response.status_code == 200:
                print(f"✅ {name} - 状态码: {response.status_code}")
                content_type = response.headers.get('Content-Type', 'N/A')
                print(f"   Content-Type: {content_type}")
                
                if 'json' in content_type:
                    try:
                        data = response.json()
                        print(f"   JSON 数据: {list(data.keys()) if isinstance(data, dict) else type(data)}")
                    except:
                        pass
            else:
                print(f"❌ {name} - 状态码: {response.status_code}")
        except Exception as e:
            print(f"❌ {name} - 错误: {e}")

def main():
    print("🔍 验证 RAT Engine 装饰器测试结果")
    print("=" * 40)
    
    print("\n📁 本地图片验证:")
    local_ok = verify_local_image()
    
    print("\n🌐 服务器图片验证:")
    server_ok = verify_server_image()
    
    print("\n🧪 其他端点测试:")
    test_other_endpoints()
    
    print("\n📊 验证结果:")
    print(f"   本地图片: {'✅ 通过' if local_ok else '❌ 失败'}")
    print(f"   服务器图片: {'✅ 通过' if server_ok else '❌ 失败'}")
    
    if local_ok and server_ok:
        print("\n🎉 所有验证通过！")
    else:
        print("\n⚠️ 部分验证失败，请检查服务器状态")

if __name__ == "__main__":
    main()