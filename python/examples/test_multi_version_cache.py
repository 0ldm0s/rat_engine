#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
专门用于测试多版本缓存功能的脚本
严格参考 cache_compression_performance_test.rs 的配置和调用方式
"""

import requests
import time
import json
import threading
from rat_engine import RatApp, HttpResponse

def create_cache_test_app():
    """创建专门用于缓存测试的应用，严格参考示例配置"""
    app = RatApp()

    # 设置debug级别日志以查看缓存详细信息
    app.configure_logging(level="debug", enable_access_log=True, enable_error_log=True)

    # 严格按照 cache_compression_performance_test.rs 的配置启用缓存
    print("🐍 [Python] 开始配置多版本缓存，严格参考示例...")

    app.enable_cache(
        # L1 配置 - 与示例完全一致
        max_memory=64 * 1024 * 1024,  # 64MB
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
        large_value_threshold=512,  # 512字节

        # L2 配置 - 与示例完全一致
        enable_l2_cache=True,
        data_dir="./cache_l2",
        clear_on_startup=False,
        max_disk_size=1024 * 1024 * 1024,  # 1GB
        write_buffer_size=64 * 1024 * 1024,  # 64MB
        max_write_buffer_number=3,
        block_cache_size=32 * 1024 * 1024,  # 32MB
        background_threads=2,
        enable_lz4=False,  # 示例中禁用L2压缩
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
        smart_flush_accumulated_bytes_threshold=4 * 1024 * 1024,  # 4MB
        cache_warmup_strategy="Recent",
        l2_write_strategy="always",
        l2_write_threshold=256,
        l2_write_ttl_threshold=300,

        # 多版本缓存配置 - 与示例完全一致
        enable_precompression=True,
        supported_encodings=["br", "gzip", "deflate", "identity"],
        precompression_threshold=1024,  # 1KB
        enable_stats=True,
        enable_smart_precompression=True,
    )
    print("✅ [Python] 多版本缓存配置完成")

    # 小数据路由 (测试多版本缓存基础功能)
    @app.html("/small-data", methods=["GET"])
    def small_data(request_data):
        """小数据端点，测试多版本缓存基础功能"""
        # 模拟数据库查询延迟
        time.sleep(0.1)  # 100ms延迟

        response = HttpResponse.html("小数据响应: " + "测试数据" * 20)
        response.set_header("Cache-Control", "public, max-age=300")
        return response

    # 大数据路由 (测试多版本缓存的编码协商优势)
    @app.html("/large-data", methods=["GET"])
    def large_data(request_data):
        """大数据端点，测试多版本缓存编码协商"""
        # 模拟复杂计算延迟
        time.sleep(0.5)  # 500ms延迟

        # 生成大量数据，确保超过压缩阈值
        large_text = "这是大量数据用于测试多版本缓存和压缩功能。" * 1000
        response = HttpResponse.html(large_text)
        response.set_header("Cache-Control", "public, max-age=600")
        return response

    # 动态数据路由 (测试不缓存场景)
    @app.json("/dynamic-data", methods=["GET"])
    def dynamic_data(request_data):
        """动态数据端点，不缓存，用于对比"""
        import time
        timestamp = int(time.time())

        data = {
            "message": "动态数据",
            "timestamp": timestamp,
            "data": "这是不会被缓存的数据" * 100
        }

        response = HttpResponse.json(data)
        response.set_header("Cache-Control", "no-cache, must-revalidate")
        return response

    return app

def test_cache_functionality():
    """测试缓存功能"""
    base_url = "http://localhost:3000"

    print("=" * 60)
    print("🧪 开始多版本缓存功能测试")
    print("=" * 60)

    test_cases = [
        {
            "name": "小数据缓存测试",
            "url": "/small-data",
            "requests": 3,
            "encoding": "gzip, deflate, br",
            "expect_cache": True
        },
        {
            "name": "大数据缓存测试",
            "url": "/large-data",
            "requests": 3,
            "encoding": "gzip, deflate, br, lz4",
            "expect_cache": True
        },
        {
            "name": "动态数据测试（不缓存）",
            "url": "/dynamic-data",
            "requests": 2,
            "encoding": "gzip",
            "expect_cache": False
        }
    ]

    client = requests.Session()
    client.headers.update({
        "User-Agent": "RAT-Engine-Cache-Test/1.0"
    })

    for test_case in test_cases:
        print(f"\n📋 {test_case['name']}")
        print("-" * 40)

        response_times = []
        response_sizes = []
        cache_headers = []

        for i in range(test_case['requests']):
            print(f"  🔄 第{i+1}次请求: {test_case['url']}")

            # 设置Accept-Encoding头
            headers = {"Accept-Encoding": test_case['encoding']}

            start_time = time.time()
            response = client.get(base_url + test_case['url'], headers=headers, timeout=10)
            elapsed_time = (time.time() - start_time) * 1000

            response_times.append(elapsed_time)
            response_sizes.append(len(response.content))

            # 检查缓存相关头部
            cache_status = response.headers.get("x-cache", "MISS")
            cache_type = response.headers.get("x-cache-type", "UNKNOWN")
            cache_headers.append((cache_status, cache_type))

            content_encoding = response.headers.get("content-encoding", "none")

            print(f"    ✅ 状态码: {response.status_code}")
            print(f"    ⏱️  响应时间: {elapsed_time:.2f}ms")
            print(f"    📦 响应大小: {len(response.content)} bytes")
            print(f"    🗜️  压缩编码: {content_encoding}")
            print(f"    🎯 缓存状态: {cache_status} (类型: {cache_type})")

            # 请求间隔
            if i < test_case['requests'] - 1:
                time.sleep(0.5)

        # 分析结果
        avg_time = sum(response_times) / len(response_times)
        cache_hits = sum(1 for status, _ in cache_headers if status == "HIT")

        print(f"\n  📊 测试结果:")
        print(f"    - 平均响应时间: {avg_time:.2f}ms")
        print(f"    - 缓存命中率: {cache_hits}/{test_case['requests']} ({cache_hits/test_case['requests']*100:.1f}%)")

        if test_case['expect_cache'] and cache_hits > 0:
            print(f"    ✅ 缓存功能正常工作")
        elif not test_case['expect_cache'] and cache_hits == 0:
            print(f"    ✅ 正确地不缓存动态数据")
        else:
            print(f"    ⚠️  缓存行为不符合预期")

def main():
    """主函数"""
    print("🚀 启动多版本缓存测试服务器...")

    # 创建应用
    app = create_cache_test_app()

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
        # 运行缓存功能测试
        test_cache_functionality()

        print("\n" + "=" * 60)
        print("🏁 多版本缓存测试完成")
        print("=" * 60)

        print("\n📋 测试总结:")
        print("   🎯 小数据端点: 应该展示多版本缓存的基础功能")
        print("   🎯 大数据端点: 应该展示编码协商和预压缩优势")
        print("   🎯 动态数据端点: 应该正确地不缓存数据")
        print("   🔍 查看日志中的多版本缓存管理器输出")

    except KeyboardInterrupt:
        print("\n🛑 测试被用户中断")
    except Exception as e:
        print(f"\n❌ 测试过程中发生错误: {e}")
        import traceback
        traceback.print_exc()
    finally:
        print("\n👋 测试结束")

if __name__ == "__main__":
    main()