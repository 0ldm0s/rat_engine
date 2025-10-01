//! 直接测试rat_memcache的L2缓存性能
//!
//! 这个测试绕过所有中间件，直接测试rat_memcache的L2缓存性能
//! 禁用L2压缩，符合当前架构模式（压缩由中间件处理）

use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use rat_memcache::config::{L1Config, L2Config, TtlConfig, PerformanceConfig};
use rat_memcache::types::EvictionStrategy;
use bytes::Bytes;
use std::time::Instant;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 直接测试rat_memcache L2缓存性能");
    println!("📋 测试目标: 验证L2缓存的性能表现（禁用L2压缩）");
    println!("🏗️  架构模式: 压缩由中间件处理，L2仅存储原始数据");

    // 清理旧的缓存数据
    let cache_dir = "./test_l2_cache";
    if let Err(e) = fs::remove_dir_all(cache_dir) {
        if !e.to_string().contains("No such file or directory") {
            println!("⚠️ 清理{}目录失败: {}", cache_dir, e);
        }
    }

    // 创建强制使用L2缓存的配置
    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 64 * 1024 * 1024, // 64MB - 正常L1大小
            max_entries: 1000,             // 正常L1条目数
            eviction_strategy: EvictionStrategy::Lru,
        })
        .l2_config(L2Config {
            enable_l2_cache: true,
            data_dir: Some(std::path::PathBuf::from(cache_dir)),
            clear_on_startup: true,
            max_disk_size: 1024 * 1024 * 1024, // 1GB
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            block_cache_size: 32 * 1024 * 1024, // 32MB
            enable_lz4: false, // 🔴 禁用L2压缩，符合架构模式
            compression_threshold: 256,
            compression_max_threshold: 1024 * 1024,
            compression_level: 1, // 最低压缩级别（虽然禁用，但保留配置）
            background_threads: 2,
            ..Default::default() // 使用默认值填充其他字段
        })
        .ttl_config(TtlConfig {
            expire_seconds: Some(60),
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .performance_config(PerformanceConfig {
            worker_threads: 2,
            enable_concurrency: true,
            read_write_separation: false,
            batch_size: 100,
            enable_warmup: false,
            large_value_threshold: 512, // 512字节 - 低阈值，强制大一点的数据进入L2
        })
          .build()
        .await?;

    println!("✅ 缓存创建成功（正常L1 + L2，低large_value_threshold强制数据进入L2）");

    // 测试数据1: 小数据（小于512字节，应该存在L1）
    let small_key = "small_l1_data";
    let small_value = Bytes::from("这是小数据，应该存储在L1缓存中，因为小于512字节阈值");

    println!("\n📝 测试1: 小数据L1缓存性能（验证L1正常工作）");
    println!("   键: {}", small_key);
    println!("   值大小: {} 字节", small_value.len());

    // 测试set操作
    let set_start = Instant::now();
    cache.set(small_key.to_string(), small_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 测试get操作（多次）
    println!("   🔄 执行多次GET操作测试L1缓存性能...");
    let mut total_get_time = std::time::Duration::new(0, 0);
    let iterations = 100;

    for i in 0..iterations {
        let get_start = Instant::now();
        let retrieved = cache.get(small_key).await?;
        let get_duration = get_start.elapsed();
        total_get_time += get_duration;

        if i == 0 {
            match retrieved {
                Some(value) => {
                    println!("     ✅ 首次GET成功，大小: {} 字节", value.len());
                    println!("     🔍 数据一致性: {}", value == small_value);
                }
                None => {
                    println!("     ❌ 首次GET失败: 数据未找到");
                    break;
                }
            }
        }

        // 每20次输出一次进度
        if (i + 1) % 20 == 0 {
            println!("     🔄 已完成 {} 次GET操作", i + 1);
        }
    }

    let avg_get_time = total_get_time / iterations;
    println!("     📊 平均GET耗时: {:?} ({}次迭代)", avg_get_time, iterations);

    // L1缓存的预期性能标准
    if avg_get_time.as_micros() < 50 {
        println!("     ✅ L1缓存性能优秀：响应时间 < 50μs");
    } else if avg_get_time.as_micros() < 100 {
        println!("     ✅ L1缓存性能正常：响应时间 < 100μs");
    } else if avg_get_time.as_millis() < 10 {
        println!("     ⚠️  L1缓存性能一般：响应时间 < 10ms");
    } else {
        println!("     ❌ L1缓存性能异常：响应时间 > 10ms");
    }

    // 测试数据2: 大数据（大于512字节阈值，强制进入L2）
    let large_key = "large_l2_data";
    let large_value = Bytes::from("A".repeat(20000)); // 20KB（超过512字节阈值，强制进入L2）

    println!("\n📝 测试2: 大数据L2缓存性能（超过large_value_threshold）");
    println!("   键: {}", large_key);
    println!("   值大小: {} 字节", large_value.len());

    // 测试set操作
    let set_start = Instant::now();
    cache.set(large_key.to_string(), large_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 测试get操作（多次）
    println!("   🔄 执行多次GET操作测试L2缓存性能...");
    let mut total_get_time = std::time::Duration::new(0, 0);
    let iterations = 50; // 减少迭代次数，因为L2操作较慢

    for i in 0..iterations {
        let get_start = Instant::now();
        let retrieved = cache.get(large_key).await?;
        let get_duration = get_start.elapsed();
        total_get_time += get_duration;

        if i == 0 {
            match retrieved {
                Some(value) => {
                    println!("     ✅ 首次GET成功，大小: {} 字节", value.len());
                    println!("     🔍 数据一致性: {}", value == large_value);
                }
                None => {
                    println!("     ❌ 首次GET失败: 数据未找到");
                    break;
                }
            }
        }

        // 每10次输出一次进度
        if (i + 1) % 10 == 0 {
            println!("     🔄 已完成 {} 次GET操作", i + 1);
        }
    }

    let avg_get_time = total_get_time / iterations;
    println!("     📊 平均GET耗时: {:?} ({}次迭代)", avg_get_time, iterations);

    // L2缓存的预期性能标准（比L1慢，但应该合理）
    if avg_get_time.as_millis() < 10 {
        println!("     ✅ L2缓存性能优秀：响应时间 < 10ms");
    } else if avg_get_time.as_millis() < 50 {
        println!("     ✅ L2缓存性能正常：响应时间 < 50ms");
    } else if avg_get_time.as_millis() < 100 {
        println!("     ⚠️  L2缓存性能一般：响应时间 50-100ms");
    } else {
        println!("     ❌ L2缓存性能异常：响应时间 > 100ms");
    }

    // 验证缓存统计
    println!("\n📊 缓存统计信息:");
    let l1_stats = cache.get_l1_stats().await;
    let l2_stats = cache.get_l2_stats().await;

    println!("   📈 L1缓存条目数: {}", l1_stats.entry_count);
    println!("   📈 L1缓存内存使用: {} 字节", l1_stats.memory_usage);
    println!("   📈 L1缓存内存利用率: {:.2}%", l1_stats.memory_utilization * 100.0);
    println!("   📈 L2缓存写入次数: {}", l2_stats.writes);
    println!("   📈 L2缓存读取次数: {}", l2_stats.reads);
    println!("   📈 L2缓存命中率: {:.2}%", l2_stats.hit_rate() * 100.0);

    // 确认数据确实存储在L2
    if l2_stats.writes > 0 {
        println!("   ✅ 确认：数据已写入L2缓存");
    } else {
        println!("   ⚠️  警告：未检测到L2缓存写入操作");
    }

    println!("\n✅ L1+L2分层缓存性能测试完成");
    println!("📋 结论:");
    println!("   - L1缓存：小于large_value_threshold的数据，应该微秒级响应");
    println!("   - L2缓存：大于large_value_threshold的数据，由melange_db处理");
    println!("   - 当前架构：L2禁用压缩，压缩由中间件处理，避免重复压缩");
    println!("   - 如果L2缓存性能异常，需要检查melange_db配置或存储层");

    Ok(())
}