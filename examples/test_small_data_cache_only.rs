use rat_engine::server::{Router, ServerConfig};
use rat_engine::compression::CompressionConfig;
use rat_engine::cache::CacheBuilder;
use rat_engine::server::cache_middleware::CacheMiddleware;
use rat_engine::server::cache_middleware_impl::CacheMiddlewareImpl;
use rat_engine::server::cache_version_manager::{CacheVersionManager, CacheVersionManagerConfig};
use rat_engine::{Method, Response, StatusCode, Full, Bytes};
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use rand::Rng;
use anyhow::Result;

/// 多版本缓存性能测试
///
/// 这个示例展示了多版本缓存（基于CacheVersionManager）的性能表现，
/// 模拟现代主流浏览器的压缩算法协商。
/// 多版本缓存的优势：预压缩+协商，避免重复压缩开销。

#[tokio::main]
async fn main() -> Result<()> {
    rat_engine::require_features!("cache-full", "compression-full");

    println!("🚀 启动多版本缓存性能测试");
    println!("📋 测试目标: 验证多版本缓存的性能表现（模拟现代浏览器协商）");
    println!("🎯 预期结果: 多版本缓存应该比单版本+实时压缩更快（避免重复压缩开销）");

    // 清理旧的缓存数据，避免脏数据污染测试结果
    println!("🧹 清理旧的缓存数据...");
    let cache_dirs = ["./cache_l2", "./test_cache_l2"];
    for cache_dir in &cache_dirs {
        if let Err(e) = tokio::fs::remove_dir_all(cache_dir).await {
            if !e.to_string().contains("No such file or directory") {
                println!("⚠️ 删除{}目录失败: {}", cache_dir, e);
            }
        }
    }
    println!("✅ 缓存数据清理完成");
    
    // 启动服务器
    let addr: std::net::SocketAddr = "127.0.0.1:3001".parse().unwrap();
    println!("🌐 服务器启动在: http://{}", addr);
    
    // 创建路由器
    let mut router = Router::new();
    
    // 创建缓存实例 - 手动配置所有参数
    let cache = match CacheBuilder::new()
        .with_l1_config(rat_engine::cache::L1Config {
            max_memory: 64 * 1024 * 1024, // 64MB
            max_entries: 1000,
            eviction_strategy: rat_engine::cache::EvictionStrategy::Lru,
        })
        .with_l2_config(rat_engine::cache::L2Config {
            enable_l2_cache: true,
            data_dir: Some("./cache_l2".into()),
            clear_on_startup: false,
            max_disk_size: 1024 * 1024 * 1024, // 1GB
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            block_cache_size: 32 * 1024 * 1024, // 32MB
            background_threads: 2,
            enable_lz4: false, // 禁用L2压缩，因为压缩由独立的压缩中间件处理
            compression_threshold: 256, // 256字节以上压缩
            compression_max_threshold: 1024 * 1024, // 最大1MB
            compression_level: 6, // 平衡压缩级别
            zstd_compression_level: None,
            cache_size_mb: 512,
            max_file_size_mb: 1024,
            smart_flush_enabled: true,
            smart_flush_base_interval_ms: 100,
            smart_flush_min_interval_ms: 20,
            smart_flush_max_interval_ms: 500,
            smart_flush_write_rate_threshold: 10000,
            smart_flush_accumulated_bytes_threshold: 4 * 1024 * 1024, // 4MB
            cache_warmup_strategy: rat_engine::cache::CacheWarmupStrategy::Recent,
            l2_write_strategy: "always".to_string(),
            l2_write_threshold: 256, // 降低阈值，确保压缩数据能写入L2
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
            worker_threads: 4,
            enable_concurrency: true,
            read_write_separation: true,
            batch_size: 100,
            enable_warmup: true,
            large_value_threshold: 512, // 512字节 - 小数据存L1，压缩后的大数据存L2
        })
                .with_ttl(60)
        .build().await {
        Ok(cache) => cache,
        Err(e) => {
            eprintln!("创建缓存失败: {}", e);
            return Err(anyhow::anyhow!("创建缓存失败: {}", e));
        }
    };
    
    // 创建现代浏览器支持的编码配置
    let modern_browser_config = CacheVersionManagerConfig {
        enable_precompression: true,
        supported_encodings: vec![
            "br".to_string(),      // Brotli（现代浏览器优先）
            "gzip".to_string(),    // 兼容性好
            "deflate".to_string(), // 传统压缩
            "identity".to_string(), // 不压缩
        ],
        precompression_threshold: 1024, // 1KB以上预压缩
        enable_stats: true, // 启用统计以便观察
            enable_smart_precompression: true, // 启用智能预压缩决策
    };

    // 创建多版本缓存管理器
    let version_manager = CacheVersionManager::with_cache_and_config(cache.clone(), modern_browser_config, Some(60));
    let cache_middleware = Arc::new(CacheMiddlewareImpl::new_multi_version(version_manager));
    router.enable_cache(cache_middleware);

    // 注意：多版本缓存依赖压缩中间件进行预压缩，但我们先测试基础协商功能
    // 实际的预压缩功能后续可以集成压缩中间件
    
    // 添加测试路由

    // 1. 小数据路由 (测试多版本缓存基础功能)
    router.add_route(Method::GET, "/small-data", |_req| {
        Box::pin(async move {
            // 模拟数据库查询延迟
            sleep(Duration::from_millis(100)).await;
            
            let data = serde_json::json!({
                "message": "小数据",
                "size": "small"
            });
            
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Cache-Control", "public, max-age=300")
                .body(Full::new(Bytes::from(data.to_string()))) {
                Ok(response) => Ok(response),
                Err(e) => {
                    eprintln!("构建响应失败: {}", e);
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Full::new(Bytes::from("构建响应失败")))
                        .unwrap())
                }
            }
        })
    });
    
    // 启用压缩中间件
    let compression_config = rat_engine::compression::CompressionConfig {
        enabled_algorithms: vec![
            rat_engine::compression::CompressionType::Gzip,
            rat_engine::compression::CompressionType::Brotli,
        ],
        min_size: 256, // 256字节以上压缩
        level: 6, // 平衡压缩级别
        excluded_content_types: std::collections::HashSet::new(),
        excluded_extensions: std::collections::HashSet::new(),
        enable_smart_compression: true, // 启用智能压缩决策
    };
    router.enable_compression(compression_config);

    // 使用 RatEngine builder 模式启动服务器，配置debug级别日志
    let mut log_config = rat_engine::utils::logger::LogConfig::default();
    log_config.level = rat_engine::utils::logger::LogLevel::Debug; // 设置debug级别日志

    let engine = rat_engine::RatEngine::builder()
        .with_log_config(log_config)
        .router(router)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build engine: {}", e))?;
    
    // 启动服务器（非阻塞）
    let server_handle = tokio::spawn(async move {
        if let Err(e) = engine.start(addr.ip().to_string(), addr.port()).await {
            eprintln!("服务器错误: {}", e);
        }
    });
    
    // 等待服务器启动
    sleep(Duration::from_millis(1000)).await;
    
    // 运行性能测试
    run_performance_tests().await?;
    
    // 停止服务器
    server_handle.abort();
    
    Ok(())
}

/// 运行性能测试
async fn run_performance_tests() -> Result<()> {
    println!("\n🧪 开始性能测试...");
    
    let client = reqwest::Client::new();
    let base_url = "http://127.0.0.1:3001";
    
    // 测试场景
    let test_cases = vec![
        ("小数据 (多版本缓存基础功能)", "/small-data", 5),
    ];
    
    for (name, path, iterations) in test_cases {
        println!("\n📊 测试场景: {}", name);
        println!("   路径: {}", path);
        println!("   开始测试...");
        
        let mut response_times = Vec::new();
        let mut response_sizes = Vec::new();
        let mut cache_statuses = Vec::new();
        let mut compression_encodings = Vec::new();
        
        for i in 1..=iterations {
            println!("   🔄 执行第{}次请求...", i);
            let start_time = std::time::Instant::now();
            
            let response = client
                .get(&format!("{}{}", base_url, path))
                .header("Accept-Encoding", "gzip, deflate, br")
                .send()
                .await?;
            
            let elapsed = start_time.elapsed();
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.bytes().await?;
            
            response_times.push(elapsed.as_millis() as u64);
            response_sizes.push(body.len());
            
            // 获取缓存状态
            let cache_status = headers
                .get("x-cache")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("UNKNOWN");
            cache_statuses.push(cache_status.to_string());

            // 获取缓存类型（多版本还是单版本）
            let cache_type = headers
                .get("x-cache-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("UNKNOWN");

            // 获取压缩编码
            let encoding = headers
                .get("content-encoding")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("none");
            compression_encodings.push(encoding.to_string());

            // 输出多版本缓存特有信息
            if cache_type == "MULTI-VERSION" {
                println!("     - 缓存类型: {}", cache_type);
            }
            
            println!("   第{}次请求结果:", i);
            println!("     - 状态码: {}", status);
            println!("     - 响应时间: {}ms", elapsed.as_millis());
            println!("     - 响应大小: {}字节", body.len());
            println!("     - 缓存状态: {}", cache_status);
            println!("     - 压缩编码: {}", encoding);
            
            // 增加请求间隔，让缓存有时间生效
            if i < iterations {
                println!("   ⏳ 等待{}ms后继续下一次请求...", 500);
                sleep(Duration::from_millis(500)).await;
            }
        }
        
        // 计算统计信息
        let avg_time = response_times.iter().sum::<u64>() as f64 / response_times.len() as f64;
        let avg_size = response_sizes.iter().sum::<usize>() as f64 / response_sizes.len() as f64;
        let cache_hits = cache_statuses.iter().filter(|s| s.as_str() == "HIT").count();
        let compressed_responses = compression_encodings.iter().filter(|e| e.as_str() != "none").count();
        
        println!("\n   📈 统计结果:");
        println!("     - 平均响应时间: {:.2}ms", avg_time);
        println!("     - 平均响应大小: {:.0}字节", avg_size);
        println!("     - 缓存命中率: {}/{} ({:.1}%)", 
                cache_hits, iterations, 
                (cache_hits as f64 / iterations as f64) * 100.0);
        println!("     - 压缩应用率: {}/{} ({:.1}%)", 
                compressed_responses, iterations,
                (compressed_responses as f64 / iterations as f64) * 100.0);
        
        }
    
    println!("\n✅ 性能测试完成!");
    println!("\n📋 测试总结:");
    println!("   🎯 小数据: 应该主要使用缓存策略，减少计算开销");
    println!("   🎯 大数据: 应该使用缓存+压缩策略，优化存储和传输");
    println!("   🎯 动态数据: 应该使用压缩策略，减少传输开销");
    println!("   🎯 原始数据: 应该跳过缓存和压缩，最小化处理开销");
    
    Ok(())
}