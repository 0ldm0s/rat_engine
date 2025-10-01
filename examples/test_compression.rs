//! 简单的压缩器测试
//!
//! 测试各种压缩算法是否正常工作

use rat_engine::compression::{CompressionConfig, CompressionType, Compressor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 测试压缩器是否正常工作");

    // 创建测试数据
    let test_data = "这是一段测试数据，重复很多次来确保有足够的压缩空间。这是一段测试数据，重复很多次来确保有足够的压缩空间。这是一段测试数据，重复很多次来确保有足够的压缩空间。这是一段测试数据，重复很多次来确保有足够的压缩空间。这是一段测试数据，重复很多次来确保有足够的压缩空间。";
    let original_bytes = test_data.as_bytes();

    println!("📊 原始数据大小: {} 字节", original_bytes.len());

    // 创建压缩器
    let compression_config = CompressionConfig::new()
        .with_gzip()
        .enable_smart_compression(false);

    let compressor = Compressor::new(compression_config);

    // 测试各种压缩算法
    let algorithms = vec![
        (CompressionType::Gzip, "gzip"),
        (CompressionType::Deflate, "deflate"),
        (CompressionType::Lz4, "lz4"),
    ];

    for (algorithm, name) in algorithms {
        println!("\n🔧 测试 {} 压缩...", name);

        match compressor.compress(original_bytes, algorithm) {
            Ok(compressed_data) => {
                println!("   ✅ 压缩成功: {} -> {} 字节 (压缩率: {:.1}%)",
                    original_bytes.len(),
                    compressed_data.len(),
                    ((original_bytes.len() - compressed_data.len()) as f64 / original_bytes.len() as f64) * 100.0
                );

                // 验证压缩后的数据确实更小
                if compressed_data.len() >= original_bytes.len() {
                    println!("   ⚠️  警告：压缩后数据没有变小");
                } else {
                    println!("   ✅ 压缩有效：数据确实变小了");
                }

                // 测试解压缩
                match compressor.decompress(&compressed_data, algorithm) {
                    Ok(decompressed_data) => {
                        if decompressed_data == original_bytes {
                            println!("   ✅ 解压缩成功：数据完全匹配");
                        } else {
                            println!("   ❌ 解压缩失败：数据不匹配");
                            println!("      原始长度: {}, 解压长度: {}", original_bytes.len(), decompressed_data.len());
                        }
                    }
                    Err(e) => {
                        println!("   ❌ 解压缩失败: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("   ❌ 压缩失败: {}", e);
            }
        }
    }

    Ok(())
}