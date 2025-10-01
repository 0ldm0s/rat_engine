use rat_engine::compression::{Compressor, CompressionType, CompressionConfig};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // 检查必需的特性
    rat_engine::require_features!("compression-zstd");
    // 创建测试数据
    let test_data = "这是一段测试数据，用于验证 zstd 压缩和解压功能是否正常工作。".repeat(100).into_bytes();
    println!("原始数据大小: {} 字节", test_data.len());
    
    // 创建压缩配置
    let config = CompressionConfig::new().with_all_algorithms();
    // 创建压缩器
    let compressor = Compressor::new(config);
    
    // 压缩数据
    let compressed = compressor.compress(&test_data, CompressionType::Zstd)?;
    println!("压缩后数据大小: {} 字节", compressed.len());
    println!("压缩率: {:.2}%", (1.0 - (compressed.len() as f64 / test_data.len() as f64)) * 100.0);
    
    // 解压数据
    let decompressed = compressor.decompress(&compressed, CompressionType::Zstd)?;
    println!("解压后数据大小: {} 字节", decompressed.len());
    
    // 验证解压后的数据是否与原始数据相同
    if decompressed == test_data {
        println!("✅ 测试通过: 解压后的数据与原始数据相同");
    } else {
        println!("❌ 测试失败: 解压后的数据与原始数据不同");
    }
    
    Ok(())
}