#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
RAT Engine 缓存配置模块

提供缓存配置的 Python 接口，自动将 Python 配置转换为 JSON 字符串传递给 Rust 层。
基于 cache_compression_performance_test.rs 的实际配置字段。
"""

import json
from typing import Dict, Any, Optional


def build_cache_config(**kwargs) -> str:
    """
    构建缓存配置 JSON 字符串

    Args:
        **kwargs: 缓存配置参数，必须包含以下必需字段：

            L1 配置 (必需):
            - max_memory: L1 缓存最大内存（字节）
            - max_entries: L1 缓存最大条目数
            - eviction_strategy: 淘汰策略（"Lru", "Lfu", "Fifo"）

            TTL 配置 (必需):
            - expire_seconds: 默认过期时间（秒）
            - cleanup_interval: 清理间隔（秒）
            - max_cleanup_entries: 最大清理条目数
            - lazy_expiration: 是否启用惰性过期
            - active_expiration: 是否启用主动过期

            性能配置 (必需):
            - worker_threads: 工作线程数
            - enable_concurrency: 是否启用并发
            - read_write_separation: 是否读写分离
            - batch_size: 批处理大小
            - enable_warmup: 是否启用预热
            - large_value_threshold: 大数据阈值

  
            L2 配置 (可选):
            - enable_l2_cache: 是否启用 L2 缓存
            - data_dir: L2 数据目录
            - clear_on_startup: 启动时清理
            - max_disk_size: 最大磁盘大小
            - write_buffer_size: 写入缓冲区大小
            - max_write_buffer_number: 最大写入缓冲区数量
            - block_cache_size: 块缓存大小
            - background_threads: 后台线程数
            - enable_lz4: 是否启用 LZ4 压缩
            - compression_threshold: 压缩阈值
            - compression_max_threshold: 最大压缩阈值
            - compression_level: 压缩级别
            - zstd_compression_level: ZSTD 压缩级别
            - cache_size_mb: 缓存大小（MB）
            - max_file_size_mb: 最大文件大小（MB）
            - smart_flush_enabled: 智能刷新
            - smart_flush_base_interval_ms: 智能刷新基础间隔
            - smart_flush_min_interval_ms: 智能刷新最小间隔
            - smart_flush_max_interval_ms: 智能刷新最大间隔
            - smart_flush_write_rate_threshold: 智能刷新写入速率阈值
            - smart_flush_accumulated_bytes_threshold: 智能刷新累积字节阈值
            - cache_warmup_strategy: 缓存预热策略
            - l2_write_strategy: L2 写入策略
            - l2_write_threshold: L2 写入阈值
            - l2_write_ttl_threshold: L2 写入 TTL 阈值

            多版本缓存配置 (可选):
            - enable_precompression: 是否启用预压缩
            - supported_encodings: 支持的编码列表
            - precompression_threshold: 预压缩阈值
            - enable_stats: 是否启用统计
            - max_entries: 最大条目数
            - enable_smart_precompression: 是否启用智能预压缩

    Returns:
        str: JSON 配置字符串

    Raises:
        ValueError: 缺少必需配置字段时抛出
    """
    # 验证必需的 L1 配置字段
    required_l1_fields = ["max_memory", "max_entries", "eviction_strategy"]
    for field in required_l1_fields:
        if field not in kwargs:
            raise ValueError(f"缺少必需的 L1 配置字段: {field}")

    # 验证必需的 TTL 配置字段
    required_ttl_fields = ["expire_seconds", "cleanup_interval", "max_cleanup_entries",
                           "lazy_expiration", "active_expiration"]
    for field in required_ttl_fields:
        if field not in kwargs:
            raise ValueError(f"缺少必需的 TTL 配置字段: {field}")

    # 验证必需的性能配置字段
    required_perf_fields = ["worker_threads", "enable_concurrency", "read_write_separation",
                            "batch_size", "enable_warmup", "large_value_threshold"]
    for field in required_perf_fields:
        if field not in kwargs:
            raise ValueError(f"缺少必需的性能配置字段: {field}")


    # 构建配置字典
    config = {
        "l1": {
            "max_memory": kwargs["max_memory"],
            "max_entries": kwargs["max_entries"],
            "eviction_strategy": kwargs["eviction_strategy"],
        },
        "ttl": {
            "expire_seconds": kwargs["expire_seconds"],
            "cleanup_interval": kwargs["cleanup_interval"],
            "max_cleanup_entries": kwargs["max_cleanup_entries"],
            "lazy_expiration": kwargs["lazy_expiration"],
            "active_expiration": kwargs["active_expiration"],
        },
        "performance": {
            "worker_threads": kwargs["worker_threads"],
            "enable_concurrency": kwargs["enable_concurrency"],
            "read_write_separation": kwargs["read_write_separation"],
            "batch_size": kwargs["batch_size"],
            "enable_warmup": kwargs["enable_warmup"],
            "large_value_threshold": kwargs["large_value_threshold"],
        },
    }

    # 添加 L2 配置（如果启用）
    if kwargs.get("enable_l2_cache", False):
        required_l2_fields = ["data_dir", "clear_on_startup", "max_disk_size", "write_buffer_size",
                              "max_write_buffer_number", "block_cache_size", "background_threads",
                              "enable_lz4", "compression_threshold", "compression_max_threshold",
                              "compression_level", "cache_size_mb", "max_file_size_mb",
                              "smart_flush_enabled", "smart_flush_base_interval_ms",
                              "smart_flush_min_interval_ms", "smart_flush_max_interval_ms",
                              "smart_flush_write_rate_threshold", "smart_flush_accumulated_bytes_threshold",
                              "cache_warmup_strategy", "l2_write_strategy", "l2_write_threshold",
                              "l2_write_ttl_threshold"]

        for field in required_l2_fields:
            if field not in kwargs:
                raise ValueError(f"启用 L2 缓存时缺少必需字段: {field}")

        l2_config = {
            "enable_l2_cache": True,
            "data_dir": kwargs["data_dir"],
            "clear_on_startup": kwargs["clear_on_startup"],
            "max_disk_size": kwargs["max_disk_size"],
            "write_buffer_size": kwargs["write_buffer_size"],
            "max_write_buffer_number": kwargs["max_write_buffer_number"],
            "block_cache_size": kwargs["block_cache_size"],
            "background_threads": kwargs["background_threads"],
            "enable_lz4": kwargs["enable_lz4"],
            "compression_threshold": kwargs["compression_threshold"],
            "compression_max_threshold": kwargs["compression_max_threshold"],
            "compression_level": kwargs["compression_level"],
            "zstd_compression_level": kwargs.get("zstd_compression_level"),
            "cache_size_mb": kwargs["cache_size_mb"],
            "max_file_size_mb": kwargs["max_file_size_mb"],
            "smart_flush_enabled": kwargs["smart_flush_enabled"],
            "smart_flush_base_interval_ms": kwargs["smart_flush_base_interval_ms"],
            "smart_flush_min_interval_ms": kwargs["smart_flush_min_interval_ms"],
            "smart_flush_max_interval_ms": kwargs["smart_flush_max_interval_ms"],
            "smart_flush_write_rate_threshold": kwargs["smart_flush_write_rate_threshold"],
            "smart_flush_accumulated_bytes_threshold": kwargs["smart_flush_accumulated_bytes_threshold"],
            "cache_warmup_strategy": kwargs["cache_warmup_strategy"],
            "l2_write_strategy": kwargs["l2_write_strategy"],
            "l2_write_threshold": kwargs["l2_write_threshold"],
            "l2_write_ttl_threshold": kwargs["l2_write_ttl_threshold"],
        }
        config["l2"] = l2_config

    # 添加多版本缓存配置（如果启用）
    if kwargs.get("enable_precompression", False):
        required_version_fields = ["supported_encodings", "precompression_threshold",
                                   "enable_stats", "enable_smart_precompression"]

        for field in required_version_fields:
            if field not in kwargs:
                raise ValueError(f"启用预压缩时缺少必需字段: {field}")

        version_config = {
            "enable_precompression": kwargs["enable_precompression"],
            "supported_encodings": kwargs["supported_encodings"],
            "precompression_threshold": kwargs["precompression_threshold"],
            "enable_stats": kwargs["enable_stats"],
            "enable_smart_precompression": kwargs["enable_smart_precompression"],
        }
        config["version_manager"] = version_config

    # 转换为 JSON 字符串
    return json.dumps(config, ensure_ascii=False)