//! 直接测试rat_memcache的L1缓存性能
//!
//! 这个测试绕过所有中间件，直接测试rat_memcache的L1缓存性能

use rat_memcache::{RatMemCacheBuilder, CacheOptions};
use rat_memcache::config::{L1Config, TtlConfig, PerformanceConfig, LoggingConfig};
use rat_memcache::types::EvictionStrategy;
use bytes::Bytes;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 直接测试rat_memcache L1缓存性能");
    println!("📋 测试目标: 验证L1缓存是否真的能提供微秒级响应");

    // 创建纯 L1 缓存配置（不使用L2）
    let cache = RatMemCacheBuilder::new()
        .l1_config(L1Config {
            max_memory: 64 * 1024 * 1024, // 64MB
            max_entries: 1000,
            eviction_strategy: EvictionStrategy::Lru,
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
            large_value_threshold: 102400, // 100KB - 确保测试数据主要存在L1
        })
                .build()
        .await?;

    println!("✅ 缓存创建成功（L1+L2，但高阈值确保主要使用L1）");

    // 测试数据1: 小数据（应该存L1）
    let small_key = "small_data";
    let small_value = Bytes::from("这是一些小数据，应该存储在L1缓存中");

    println!("\n📝 测试1: 小数据L1缓存性能");
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
    let iterations = 1000;

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

        // 每100次输出一次进度
        if (i + 1) % 100 == 0 {
            println!("     🔄 已完成 {} 次GET操作", i + 1);
        }
    }

    let avg_get_time = total_get_time / iterations;
    println!("     📊 平均GET耗时: {:?} ({}次迭代)", avg_get_time, iterations);

    // 性能判断标准：如果L1缓存正常，平均GET时间应该非常短（< 50微秒）
    if avg_get_time.as_micros() < 50 {
        println!("     ✅ L1缓存性能测试通过：响应时间正常（< 50μs）");
    } else if avg_get_time.as_micros() < 500 {
        println!("     ⚠️  L1缓存性能一般：响应时间偏慢（> 50μs）");
    } else {
        println!("     ❌ L1缓存性能测试失败：响应时间过慢（> 500μs）");
    }

    // 测试数据2: 中等大小数据（测试L1缓存边界）
    let medium_key = "medium_data";
    let medium_value = Bytes::from("A".repeat(8000)); // 8KB（小于10KB阈值）

    println!("\n📝 测试2: 中等数据L1缓存性能");
    println!("   键: {}", medium_key);
    println!("   值大小: {} 字节", medium_value.len());

    // 测试set操作
    let set_start = Instant::now();
    cache.set(medium_key.to_string(), medium_value.clone()).await?;
    let set_duration = set_start.elapsed();
    println!("   ⏱️  SET操作耗时: {:?}", set_duration);

    // 测试get操作（多次）
    let mut total_get_time = std::time::Duration::new(0, 0);
    let iterations = 1000;

    for i in 0..iterations {
        let get_start = Instant::now();
        let retrieved = cache.get(medium_key).await?;
        let get_duration = get_start.elapsed();
        total_get_time += get_duration;

        if i == 0 {
            match retrieved {
                Some(value) => {
                    println!("     ✅ 首次GET成功，大小: {} 字节", value.len());
                    println!("     🔍 数据一致性: {}", value == medium_value);
                }
                None => {
                    println!("     ❌ 首次GET失败: 数据未找到");
                    break;
                }
            }
        }
    }

    let avg_get_time = total_get_time / iterations;
    println!("     📊 平均GET耗时: {:?} ({}次迭代)", avg_get_time, iterations);

    if avg_get_time.as_micros() < 50 {
        println!("     ✅ L1缓存性能测试通过：响应时间正常（< 50μs）");
    } else if avg_get_time.as_micros() < 500 {
        println!("     ⚠️  L1缓存性能一般：响应时间偏慢（> 50μs）");
    } else {
        println!("     ❌ L1缓存性能测试失败：响应时间过慢（> 500μs）");
    }

    // 验证缓存统计
    println!("\n📊 缓存统计信息:");
    let l1_stats = cache.get_l1_stats().await;
    println!("   📈 L1缓存条目数: {}", l1_stats.entry_count);
    println!("   📈 L1缓存内存使用: {} 字节", l1_stats.memory_usage);
    println!("   📈 L1缓存内存利用率: {:.2}%", l1_stats.memory_utilization * 100.0);
    println!("   📈 L1缓存条目利用率: {:.2}%", l1_stats.entry_utilization * 100.0);

    // 确认没有L2缓存
    #[cfg(feature = "melange-storage")]
    {
        let l2_stats = cache.get_l2_stats().await;
        println!("   📈 L2缓存写入次数: {}", l2_stats.writes);
        if l2_stats.writes == 0 {
            println!("   ✅ 确认：没有L2缓存写入操作");
        } else {
            println!("   ⚠️  警告：检测到L2缓存写入操作（{}次）", l2_stats.writes);
        }
    }

    println!("\n✅ 直接L1缓存性能测试完成");
    println!("📋 结论:");
    println!("   - 如果L1缓存性能正常，问题可能在CacheVersionManager中间件");
    println!("   - 如果L1缓存性能也很慢，问题可能在rat_memcache本身或环境因素");

    Ok(())
}