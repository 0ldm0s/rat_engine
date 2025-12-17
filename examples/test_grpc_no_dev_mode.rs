use rat_engine::{RatGrpcClientBuilder, RatError};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("测试不设置 development_mode 的情况");

    let result = RatGrpcClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))?
        .request_timeout(Duration::from_secs(30))?
        .max_idle_connections(10)?
        .http_mixed()  // 使用混合模式
        .user_agent("Test Client")?
        .disable_compression()
        .build();  // 不设置 development_mode

    match result {
        Ok(_) => println!("✅ 客户端创建成功（未设置 development_mode）"),
        Err(e) => println!("❌ 错误: {}", e),
    }

    Ok(())
}