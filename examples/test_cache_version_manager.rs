//! 测试CacheVersionManager的预压缩机制
//!
//! 验证预压缩是否正确工作，以及压缩后的数据是否正确存储

use rat_engine::server::cache_version_manager::{CacheVersionManager, CacheVersionManagerConfig};
use rat_engine::cache::CacheBuilder;
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    rat_engine::require_features!("cache-full", "compression-full");

    println!("🧪 测试CacheVersionManager预压缩机制");

    // 创建缓存实例
    let cache = CacheBuilder::new()
        .with_l1_config(rat_engine::cache::L1Config {
            max_memory: 64 * 1024 * 1024, // 64MB
            max_entries: 1000,
            eviction_strategy: rat_engine::cache::EvictionStrategy::Lru,
        })
        .with_l2_config(rat_engine::cache::L2Config {
            enable_l2_cache: false, // 禁用L2，专注于L1测试
            data_dir: None,
            clear_on_startup: false,
            max_disk_size: 1024 * 1024 * 1024,
            write_buffer_size: 64 * 1024 * 1024,
            max_write_buffer_number: 3,
            block_cache_size: 32 * 1024 * 1024,
            background_threads: 2,
            enable_lz4: false,
            compression_threshold: 256,
            compression_max_threshold: 1024 * 1024,
            compression_level: 6,
            zstd_compression_level: None,
            cache_size_mb: 512,
            max_file_size_mb: 1024,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 500,
            smart_flush_write_rate_threshold: 10000,
            smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024,
            cache_warmup_strategy: rat_engine::cache::CacheWarmupStrategy::Recent,
            l2_write_strategy: "always".to_string(),
            l2_write_threshold: 256,
            l2_write_ttl_threshold: 300,
        })
        .with_ttl_config(rat_engine::cache::TtlConfig {
            expire_seconds: Some(60),
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .with_performance_config(rat_engine::cache::PerformanceConfig {
            worker_threads: 2,
            enable_concurrency: true,
            read_write_separation: false,
            batch_size: 100,
            enable_warmup: false,
            large_value_threshold: 1024, // 1KB
        })
              .with_ttl(60)
        .build()
        .await?;

    // 创建CacheVersionManager配置
    let config = CacheVersionManagerConfig {
        enable_precompression: true,
        supported_encodings: vec!["gzip".to_string(), "deflate".to_string(), "identity".to_string()],
        precompression_threshold: 500, // 500字节以上预压缩
        enable_stats: true,
        enable_smart_precompression: false, // 禁用智能预压缩，强制预压缩
    };

    // 创建CacheVersionManager
    let mut version_manager = CacheVersionManager::with_cache_and_config(cache.clone(), config, Some(60));

    // 创建测试数据（足够大以触发预压缩）
    let test_data = Bytes::from("A".repeat(1000)); // 1000字节
    let base_key = "test_key";

    println!("📊 测试数据大小: {} 字节", test_data.len());
    println!("🔧 预压缩阈值: 500 字节");

    // 步骤1：存储原始数据
    println!("\n📝 步骤1：存储原始数据 (identity编码)");
    if let Err(e) = version_manager.handle_cache_storage(base_key, "application/json", test_data.clone(), "identity", None).await {
        println!("❌ 存储失败: {}", e);
        return Err(e);
    }

    // 等待一下让预压缩完成
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 步骤2：检查各个编码版本是否存在
    println!("\n📝 步骤2：检查预压缩版本是否存在");

    for encoding in &["identity", "gzip", "deflate"] {
        let encoded_key = format!("{}:{}", base_key, encoding);
        match cache.get(&encoded_key).await {
            Ok(Some(data)) => {
                println!("   ✅ 找到编码版本 {}: {} 字节", encoding, data.len());

                // 验证数据内容
                if *encoding == "identity" {
                    if data == test_data {
                        println!("      ✅ identity版本数据正确");
                    } else {
                        println!("      ❌ identity版本数据错误");
                    }
                } else {
                    // 非identity版本应该是压缩后的数据
                    if data.len() < test_data.len() {
                        println!("      ✅ {}版本数据已压缩 (压缩率: {:.1}%)",
                            encoding,
                            ((test_data.len() - data.len()) as f64 / test_data.len() as f64) * 100.0
                        );
                    } else {
                        println!("      ❌ {}版本数据未压缩或压缩失败", encoding);
                    }
                }
            }
            Ok(None) => {
                println!("   ❌ 未找到编码版本 {}", encoding);
            }
            Err(e) => {
                println!("   ❌ 获取编码版本 {} 失败: {}", encoding, e);
            }
        }
    }

    // 步骤3：测试缓存查找
    println!("\n📝 步骤3：测试缓存查找");

    // 测试查找gzip版本
    if let Some(result) = version_manager.handle_cache_lookup(base_key, "gzip").await {
        println!("   ✅ gzip缓存查找成功: {} 字节", result.data.len());
        println!("   🏷️  返回的编码: {}", result.encoding);

        if result.data.len() < test_data.len() {
            println!("   ✅ 返回的是压缩数据");
        } else {
            println!("   ❌ 返回的是未压缩数据（问题！）");
        }
    } else {
        println!("   ❌ gzip缓存查找失败");
    }

    // 测试查找identity版本
    if let Some(result) = version_manager.handle_cache_lookup(base_key, "identity").await {
        println!("   ✅ identity缓存查找成功: {} 字节", result.data.len());
        println!("   🏷️  返回的编码: {}", result.encoding);

        if result.data == test_data {
            println!("   ✅ 返回的是原始数据");
        } else {
            println!("   ❌ 返回的数据不匹配原始数据");
        }
    } else {
        println!("   ❌ identity缓存查找失败");
    }

    println!("\n✅ CacheVersionManager预压缩机制测试完成");

    Ok(())
}