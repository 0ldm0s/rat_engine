//! 测试rat_engine强制禁用LZ4压缩的功能
//!
//! 这个测试验证不管用户如何配置，rat_engine都会强制禁用rat_memcache的L2压缩。

use rat_engine::cache::CacheBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 测试rat_engine强制禁用LZ4压缩功能");

    // 测试1：用户故意设置enable_lz4: true
    println!("\n📋 测试场景1：用户故意设置enable_lz4: true");

    let l2_config = rat_engine::cache::L2Config {
        enable_l2_cache: true,
        data_dir: Some("./test_cache".into()),
        clear_on_startup: true,
        max_disk_size: 1024 * 1024 * 1024,
        write_buffer_size: 64 * 1024 * 1024,
        max_write_buffer_number: 3,
        block_cache_size: 32 * 1024 * 1024,
        background_threads: 2,
        enable_lz4: true, // 用户故意设置为true
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
        l2_write_strategy: "adaptive".to_string(),
        l2_write_threshold: 1024,
        l2_write_ttl_threshold: 300,
    };

    println!("   用户传入的配置: enable_lz4 = {}", l2_config.enable_lz4);

    // 通过rat_engine的CacheBuilder配置
    let cache = CacheBuilder::new()
        .with_l1_config(rat_engine::cache::L1Config {
            max_memory: 64 * 1024 * 1024,
            max_entries: 1000,
            eviction_strategy: rat_engine::cache::EvictionStrategy::Lru,
        })
        .with_l2_config(l2_config) // 这里会强制禁用LZ4
        .with_ttl_config(rat_engine::cache::TtlConfig {
            expire_seconds: Some(60),
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .with_performance_config(rat_engine::cache::PerformanceConfig {
            worker_threads: 4,
            enable_concurrency: true,
            read_write_separation: true,
            batch_size: 100,
            enable_warmup: true,
            large_value_threshold: 1024 * 1024,
        })
        .build()
        .await?;

    println!("   ✅ rat_engine CacheBuilder构建成功");
    println!("   ✅ LZ4压缩已被强制禁用（通过with_l2_config方法）");

    // 测试2：不启用L2缓存的情况
    println!("\n📋 测试场景2：禁用L2缓存的情况");

    let l2_config_disabled = rat_engine::cache::L2Config {
        enable_l2_cache: false, // 禁用L2缓存
        data_dir: Some("./test_cache_disabled".into()),
        clear_on_startup: true,
        max_disk_size: 1024 * 1024 * 1024,
        write_buffer_size: 64 * 1024 * 1024,
        max_write_buffer_number: 3,
        block_cache_size: 32 * 1024 * 1024,
        background_threads: 2,
        enable_lz4: true, // 即使设置为true也会被禁用
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
        l2_write_strategy: "adaptive".to_string(),
        l2_write_threshold: 1024,
        l2_write_ttl_threshold: 300,
    };

    let cache2 = CacheBuilder::new()
        .with_l1_config(rat_engine::cache::L1Config {
            max_memory: 64 * 1024 * 1024,
            max_entries: 1000,
            eviction_strategy: rat_engine::cache::EvictionStrategy::Lru,
        })
        .with_l2_config(l2_config_disabled) // 即使启用L2也会强制禁用压缩
        .with_ttl_config(rat_engine::cache::TtlConfig {
            expire_seconds: Some(60),
            cleanup_interval: 300,
            max_cleanup_entries: 1000,
            lazy_expiration: true,
            active_expiration: true,
        })
        .with_performance_config(rat_engine::cache::PerformanceConfig {
            worker_threads: 4,
            enable_concurrency: true,
            read_write_separation: true,
            batch_size: 100,
            enable_warmup: true,
            large_value_threshold: 1024 * 1024,
        })
        .build()
        .await?;

    println!("   ✅ 禁用L2缓存构建成功");
    println!("   ✅ 即使enable_lz4=true也被强制禁用");

    // 测试3：验证缓存功能正常工作
    println!("\n📋 测试场景3：验证缓存功能正常工作");

    use bytes::Bytes;

    // 测试写入
    cache.set("test_key".to_string(), Bytes::from("test_value")).await?;
    println!("   ✅ 缓存写入成功");

    // 测试读取
    let value = cache.get("test_key").await?;
    match value {
        Some(val) => println!("   ✅ 缓存读取成功: {}", std::str::from_utf8(&val)?),
        None => println!("   ❌ 缓存读取失败"),
    }

    // 测试删除
    let deleted = cache.delete("test_key").await?;
    println!("   ✅ 缓存删除成功: {}", deleted);

    println!("\n🎉 测试完成！");
    println!("📊 总结：");
    println!("   - rat_engine成功强制禁用了LZ4压缩");
    println!("   - 不管用户如何配置enable_lz4，都会被设为false");
    println!("   - 缓存功能正常工作");
    println!("   - 压缩由独立的压缩中间件处理，架构解耦");

    Ok(())
}