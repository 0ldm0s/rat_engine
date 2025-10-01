#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
简单的requests测试脚本
用于排查rat_engine服务器的缓存和压缩功能问题
"""

import requests
import time
import json
from rat_engine import RatApp, HttpResponse
import threading

def create_test_app():
    """创建测试应用"""
    app = RatApp()

    # 启用压缩功能
    app.enable_compression(
        min_size=512,
        level=6,
        enable_gzip=True,
        enable_deflate=True,
        enable_brotli=True,
        enable_zstd=True,
        enable_lz4=True,
    )

    # 设置debug日志级别以查看缓存详细信息
    app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)

    # 启用缓存功能 - 按照cache_compression_performance_test.rs的配置
    app.enable_cache(
        # L1 配置 - 与示例完全一致
        max_memory=64 * 1024 * 1024,
        max_entries=1000,
        eviction_strategy="Lru",

        # TTL 配置 - 与示例完全一致
        expire_seconds=60,
        cleanup_interval=300,
        max_cleanup_entries=1000,
        lazy_expiration=True,
        active_expiration=True,

        # 性能配置 - 与示例完全一致
        worker_threads=4,
        enable_concurrency=True,
        read_write_separation=True,
        batch_size=100,
        enable_warmup=True,
        large_value_threshold=512,

        # L2 配置 - 与示例完全一致
        enable_l2_cache=True,
        data_dir="./cache_l2",
        clear_on_startup=False,
        max_disk_size=1024 * 1024 * 1024,
        write_buffer_size=64 * 1024 * 1024,
        max_write_buffer_number=3,
        block_cache_size=32 * 1024 * 1024,
        background_threads=2,
        enable_lz4=False,
        compression_threshold=256,
        compression_max_threshold=1024 * 1024,
        compression_level=6,
        cache_size_mb=512,
        max_file_size_mb=1024,
        smart_flush_enabled=True,
        smart_flush_base_interval_ms=100,
        smart_flush_min_interval_ms=20,
        smart_flush_max_interval_ms=500,
        smart_flush_write_rate_threshold=10000,
        smart_flush_accumulated_bytes_threshold=4 * 1024 * 1024,
        cache_warmup_strategy="Recent",
        l2_write_strategy="always",
        l2_write_threshold=256,
        l2_write_ttl_threshold=300,

        # 多版本缓存配置 - 与示例完全一致
        enable_precompression=True,
        supported_encodings=["br", "gzip", "deflate", "identity"],
        precompression_threshold=1024,
        enable_stats=True,
        enable_smart_precompression=True,
    )

    # 调试：检查enable_cache调用过程
    print("🐍 [Python] 准备调用enable_cache...")
    try:
        app.enable_cache(
            max_memory=64 * 1024 * 1024,
            max_entries=1000,
            eviction_strategy="Lru",
            expire_seconds=60,
            cleanup_interval=300,
            max_cleanup_entries=1000,
            lazy_expiration=True,
            active_expiration=True,
            worker_threads=4,
            enable_concurrency=True,
            read_write_separation=True,
            batch_size=100,
            enable_warmup=True,
            large_value_threshold=512,
            enable_l2_cache=True,
            data_dir="./cache_l2",
            clear_on_startup=False,
            max_disk_size=1024 * 1024 * 1024,
            write_buffer_size=64 * 1024 * 1024,
            max_write_buffer_number=3,
            block_cache_size=32 * 1024 * 1024,
            background_threads=2,
            enable_lz4=False,
            compression_threshold=256,
            compression_max_threshold=1024 * 1024,
            compression_level=6,
            cache_size_mb=512,
            max_file_size_mb=1024,
            smart_flush_enabled=True,
            smart_flush_base_interval_ms=100,
            smart_flush_min_interval_ms=20,
            smart_flush_max_interval_ms=500,
            smart_flush_write_rate_threshold=10000,
            smart_flush_accumulated_bytes_threshold=4 * 1024 * 1024,
            cache_warmup_strategy="Recent",
            l2_write_strategy="always",
            l2_write_threshold=256,
            l2_write_ttl_threshold=300,
            enable_precompression=True,
            supported_encodings=["br", "gzip", "deflate", "identity"],
            precompression_threshold=1024,
            enable_stats=True,
            enable_smart_precompression=True,
        )
        print("✅ [Python] enable_cache调用成功")
    except Exception as e:
        print(f"❌ [Python] enable_cache调用失败: {e}")
        import traceback
        traceback.print_exc()
    
    @app.html("/test_text", methods=["GET"])
    def test_text(request_data):
        """返回文本内容用于测试"""
        response = HttpResponse.html("This is a test response for cache and compression testing. " * 50)
        response.set_header("Cache-Control", "public, max-age=60")
        return response
    
    @app.json("/test_json", methods=["GET"])
    def test_json(request_data):
        """返回JSON内容用于测试"""
        # 生成更大的JSON数据以更好地展示缓存和压缩效果
        large_data = []
        for i in range(1000):
            large_data.append({
                "id": i,
                "name": f"Item {i}",
                "description": f"This is a detailed description for item {i} " * 10,
                "metadata": {
                    "created": "2024-01-01",
                    "tags": [f"tag{j}" for j in range(5)],
                    "stats": {"views": i * 10, "likes": i * 2}
                }
            })

        response = HttpResponse.json({
            "message": "This is a JSON response for testing",
            "data": large_data,
            "cached": True,
            "total_items": len(large_data)
        })
        response.set_header("Cache-Control", "public, max-age=60")
        return response
    
    @app.json("/test_large", methods=["GET"])
    def test_large(request_data):
        """返回大量数据用于测试压缩"""
        large_text = "This is a large response. " * 1000
        return {"data": large_text, "size": len(large_text)}
    
    return app

def test_with_requests():
    """使用requests库测试服务器"""
    base_url = "http://localhost:3000"
    
    print("=" * 60)
    print("🧪 开始使用requests库测试rat_engine服务器")
    print("=" * 60)
    
    # 测试不同的Accept-Encoding头
    test_cases = [
        {
            "name": "无压缩",
            "headers": {"Accept-Encoding": "identity"},
            "endpoint": "/test_text"
        },
        {
            "name": "支持gzip",
            "headers": {"Accept-Encoding": "gzip"},
            "endpoint": "/test_text"
        },
        {
            "name": "支持多种压缩",
            "headers": {"Accept-Encoding": "gzip, deflate, br, lz4, zstd"},
            "endpoint": "/test_text"
        },
        {
            "name": "JSON测试",
            "headers": {"Accept-Encoding": "gzip"},
            "endpoint": "/test_json"
        }
    ]
    
    for i, test_case in enumerate(test_cases, 1):
        print(f"\n📋 测试 {i}: {test_case['name']}")
        print("-" * 40)
        
        url = base_url + test_case['endpoint']
        headers = test_case['headers']
        
        try:
            # 第一次请求（冷缓存）
            print(f"🔄 第一次请求: {url}")
            print(f"📤 请求头: {headers}")
            
            first_start_time = time.time()
            response1 = requests.get(url, headers=headers, timeout=10)
            first_end_time = time.time()
            
            print(f"✅ 响应状态: {response1.status_code}")
            print(f"⏱️  响应时间: {(first_end_time - first_start_time)*1000:.2f}ms")
            print(f"📦 响应大小: {len(response1.content)} bytes")
            print(f"🗜️  Content-Encoding: {response1.headers.get('Content-Encoding', 'none')}")
            print(f"📋 Content-Type: {response1.headers.get('Content-Type', 'none')}")
            
            # 检查缓存相关头部
            cache_headers = ['X-Cache-Status', 'X-Cache-Key', 'X-Compression-Used', 'Cache-Control']
            for header in cache_headers:
                if header in response1.headers:
                    print(f"🎯 {header}: {response1.headers[header]}")

            # 第二次请求（热缓存）
            print(f"\n🔄 第二次请求（测试缓存）: {url}")

            start_time = time.time()
            response2 = requests.get(url, headers=headers, timeout=10)
            end_time = time.time()

            print(f"✅ 响应状态: {response2.status_code}")
            print(f"⏱️  响应时间: {(end_time - start_time)*1000:.2f}ms")
            print(f"📦 响应大小: {len(response2.content)} bytes")
            print(f"🗜️  Content-Encoding: {response2.headers.get('Content-Encoding', 'none')}")

            # 检查缓存相关头部
            for header in cache_headers:
                if header in response2.headers:
                    print(f"🎯 {header}: {response2.headers[header]}")

            # 计算性能提升
            time_improvement = ((end_time - start_time) - (first_end_time - first_start_time)) / (first_end_time - first_start_time) * 100
            if time_improvement < 0:
                print(f"🚀 性能提升: {abs(time_improvement):.1f}% 更快")
            else:
                print(f"📉 性能变化: {time_improvement:.1f}% 更慢")
            
            # 验证内容一致性
            if response1.content == response2.content:
                print("✅ 内容一致性检查: 通过")
            else:
                print("❌ 内容一致性检查: 失败")
            
            # 显示部分响应内容（如果是文本）
            if test_case['endpoint'] == '/test_text':
                content_preview = response1.text[:100] + "..." if len(response1.text) > 100 else response1.text
                print(f"📄 响应内容预览: {content_preview}")
            elif test_case['endpoint'] == '/test_json':
                try:
                    json_data = response1.json()
                    print(f"📄 JSON响应: message={json_data.get('message', 'N/A')}, data_length={len(json_data.get('data', []))}")
                except:
                    print("📄 JSON解析失败")
                    
        except requests.exceptions.Timeout:
            print("❌ 请求超时")
        except requests.exceptions.ConnectionError:
            print("❌ 连接错误")
        except Exception as e:
            print(f"❌ 请求失败: {e}")
    
    print("\n" + "=" * 60)
    print("🏁 requests测试完成")
    print("=" * 60)

def main():
    """主函数"""
    print("🚀 启动rat_engine测试服务器...")
    
    # 创建应用
    app = create_test_app()
    
    # 在单独线程中启动服务器
    server_thread = threading.Thread(
        target=lambda: app.run(host="127.0.0.1", port=3000),
        daemon=True
    )
    server_thread.start()
    
    # 等待服务器启动
    print("⏳ 等待服务器启动...")
    time.sleep(2)
    
    try:
        # 运行测试
        test_with_requests()
    except KeyboardInterrupt:
        print("\n🛑 测试被用户中断")
    except Exception as e:
        print(f"\n❌ 测试过程中发生错误: {e}")
    finally:
        print("\n👋 测试结束")

if __name__ == "__main__":
    main()