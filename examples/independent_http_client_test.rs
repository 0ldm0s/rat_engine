//! RAT Engine 独立HTTP客户端测试示例
//!
//! 这个示例展示了如何使用基于reqwest的独立HTTP客户端进行：
//! - 标准HTTP协议验证
//! - 压缩协议协商测试
//! - SSE功能验证
//! - 外部服务测试

use std::time::Duration;
use anyhow::Result;

#[cfg(feature = "reqwest")]
#[tokio::main]
async fn main() -> Result<()> {

    println!("🚀 RAT Engine 独立HTTP客户端测试示例");
    println!("========================================");

    // 创建独立HTTP客户端
    let client = rat_engine::RatIndependentHttpClientBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("rat-engine-independent-test/1.0")
        .supported_compressions(["gzip", "deflate", "br"].iter().map(|s| s.to_string()))
        .auto_decompress(true)
        .build()?;

    println!("✅ 独立HTTP客户端创建成功");

    // 运行测试套件
    run_compression_tests(&client).await?;
    run_sse_tests(&client).await?;
    run_external_service_tests(&client).await?;

    println!("\n🎉 所有测试完成！");
    Ok(())
}

#[cfg(feature = "reqwest")]
async fn run_compression_tests(client: &rat_engine::RatIndependentHttpClient) -> Result<()> {
    println!("\n🧪 压缩协议协商测试");
    println!("======================");

    // 测试支持压缩的外部服务
    let test_urls = vec![
        ("https://httpbin.org/gzip", "HTTPBin GZIP测试"),
        ("https://httpbin.org/deflate", "HTTPBin DEFLATE测试"),
        ("https://www.github.com", "GitHub（支持Brotli）"),
        ("https://httpbin.org/brotli", "HTTPBin Brotli测试"),
    ];

    for (url, description) in test_urls {
        println!("\n📊 测试: {}", description);
        println!("   URL: {}", url);

        // 测试压缩支持
        match client.test_compression(url).await {
            Ok(result) => {
                println!("   📈 压缩测试结果:");

                for (algorithm, item) in &result.results {
                    if item.supported {
                        let ratio = if item.original_size > 0 {
                            (item.compressed_size as f64 / item.original_size as f64) * 100.0
                        } else {
                            0.0
                        };

                        println!("     - {}: 支持 (压缩率: {:.1}%, 耗时: {}ms)",
                               algorithm, ratio, item.response_time_ms);
                    } else {
                        println!("     - {}: 不支持", algorithm);
                    }
                }
            }
            Err(e) => {
                println!("   ❌ 压缩测试失败: {}", e);
            }
        }

        // 标准请求测试
        match client.get(url).await {
            Ok(response) => {
                println!("   ✅ 标准请求成功:");
                println!("     - 状态码: {}", response.status);
                println!("     - 压缩算法: {:?}", response.compression_algorithm);
                println!("     - 原始大小: {}字节", response.original_size);
                println!("     - 解压后大小: {}字节", response.body.len());
                println!("     - 请求耗时: {}ms", response.request_time_ms);

                if let Some(content_type) = response.content_type() {
                    println!("     - Content-Type: {:?}", content_type);
                }
            }
            Err(e) => {
                println!("   ❌ 标准请求失败: {}", e);
            }
        }
    }

    Ok(())
}

#[cfg(feature = "reqwest")]
async fn run_sse_tests(client: &rat_engine::RatIndependentHttpClient) -> Result<()> {
    println!("\n🔄 SSE功能验证测试");
    println!("==================");

    // 使用httpbin.org的SSE测试端点
    let sse_url = "https://sse-eventstream.herokuapp.com/events";

    println!("📊 测试SSE连接");
    println!("   URL: {}", sse_url);

    match client.connect_sse(sse_url).await {
        Ok(mut sse_stream) => {
            println!("   ✅ SSE连接成功");

            let mut event_count = 0;
            let start_time = std::time::Instant::now();

            // 接收事件
            while let Ok(Some(event)) = sse_stream.next_event().await {
                event_count += 1;

                println!("   📥 事件 #{}:", event_count);
                if let Some(id) = &event.id {
                    println!("     - ID: {}", id);
                }
                if let Some(event_type) = &event.event_type {
                    println!("     - 类型: {}", event_type);
                }
                println!("     - 数据: {}", event.data);
                if let Some(retry) = &event.retry {
                    println!("     - 重试: {}ms", retry);
                }

                // 限制接收事件数量
                if event_count >= 5 {
                    break;
                }
            }

            let elapsed = start_time.elapsed();
            println!("   📊 SSE统计:");
            println!("     - 接收事件数: {}", event_count);
            println!("     - 总耗时: {:?}", elapsed);
        }
        Err(e) => {
            println!("   ❌ SSE连接失败: {}", e);
        }
    }

    Ok(())
}

#[cfg(feature = "reqwest")]
async fn run_external_service_tests(client: &rat_engine::RatIndependentHttpClient) -> Result<()> {
    println!("\n🌐 外部服务测试");
    println!("=================");

    // 测试各种外部服务
    let test_cases = vec![
        ("https://api.github.com/rate_limit", "GitHub API限流"),
        ("https://httpbin.org/json", "HTTPBin JSON响应"),
        ("https://httpbin.org/headers", "HTTPBin请求头"),
        ("https://httpbin.org/status/200", "HTTP状态码测试"),
    ];

    for (url, description) in test_cases {
        println!("\n📊 测试: {}", description);
        println!("   URL: {}", url);

        let start_time = std::time::Instant::now();

        match client.get(url).await {
            Ok(response) => {
                let elapsed = start_time.elapsed();

                println!("   ✅ 请求成功:");
                println!("     - 状态码: {}", response.status);
                println!("     - 响应大小: {}字节", response.body.len());
                println!("     - 请求耗时: {}ms", elapsed.as_millis());

                if response.was_compressed {
                    println!("     - 压缩算法: {:?}", response.compression_algorithm);
                    let ratio = (response.body.len() as f64 / response.original_size as f64) * 100.0;
                    println!("     - 压缩率: {:.1}%", ratio);
                }

                // 尝试解析JSON（如果适用）
                if let Some(content_type) = response.content_type() {
                    if let Ok(content_str) = content_type.to_str() {
                        if content_str.contains("application/json") {
                            if let Ok(json_text) = response.text() {
                                if json_text.len() < 500 {
                                    println!("     - JSON预览: {}", json_text);
                                } else {
                                    println!("     - JSON预览: {} bytes (数据过大)", json_text.len());
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ❌ 请求失败: {}", e);
            }
        }

        // 避免请求过于频繁
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Ok(())
}

#[cfg(not(feature = "reqwest"))]
fn main() -> Result<()> {
    println!("❌ 需要启用 reqwest feature 才能运行此示例");
    println!("请使用: cargo run --example independent_http_client_test --features reqwest");
    Ok(())
}