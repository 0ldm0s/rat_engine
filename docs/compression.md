# RAT Engine 压缩功能

本文档介绍如何在 RAT Engine 中使用内容压缩功能。

## 功能概述

RAT Engine 的压缩功能支持以下压缩算法：

- **Gzip**：最广泛支持的压缩算法，兼容性最好
- **Deflate**：类似 Gzip 但没有头和校验和，体积稍小
- **Brotli**：Google 开发的压缩算法，压缩率高但压缩速度较慢
- **Zstd**：Facebook 开发的压缩算法，压缩率和速度平衡较好
- **Lz4**：超快速压缩算法，压缩率较低但速度极快

压缩功能可以显著减少网络传输的数据量，提高网站加载速度，降低带宽使用。

## 启用压缩功能

### 1. 在 Cargo.toml 中启用 compression feature

```toml
[dependencies]
rat_engine = { version = "0.1.0", features = ["compression"] }
```

如果需要缓存支持，还可以启用 cache feature：

```toml
[dependencies]
rat_engine = { version = "0.1.0", features = ["compression", "cache"] }
```

### 2. 创建压缩配置

```rust
use rat_engine::{CompressionConfig, CompressionType};

let compression_config = CompressionConfig::new()
    .enable_gzip()       // 启用 Gzip 压缩
    .enable_deflate()    // 启用 Deflate 压缩
    .enable_brotli()     // 启用 Brotli 压缩
    .enable_zstd()       // 启用 Zstd 压缩
    .enable_lz4()        // 启用 Lz4 压缩
    .min_size(1024)      // 设置最小压缩大小为 1KB
    .compression_level(6) // 设置压缩级别（1-9，值越大压缩率越高但速度越慢）
    .exclude_content_type(vec!["image/jpeg", "image/png"]) // 排除不需要压缩的内容类型
    .exclude_extension(vec!["jpg", "png", "gif"]); // 排除不需要压缩的文件扩展名
```

### 3. 创建压缩中间件

```rust
use rat_engine::CompressionMiddleware;

// 创建普通压缩中间件
let compression_middleware = CompressionMiddleware::new(compression_config);
```

如果启用了缓存功能，可以创建带缓存的压缩中间件：

```rust
#[cfg(feature = "cache")]
let compression_middleware = {
    use rat_memcache::MemCache;
    
    // 创建内存缓存，设置最大缓存项数为 100，每项最大大小为 1MB
    let cache = MemCache::new(100, 1024 * 1024);
    
    // 创建带缓存的压缩中间件
    CompressionMiddleware::with_cache(compression_config, cache)
};
```

### 4. 应用压缩中间件到路由器

```rust
use rat_engine::CompressionExt;

// 应用压缩中间件到路由器
let router = router.with_compression(compression_middleware);
```

## 工作原理

1. 当客户端发送请求时，压缩中间件会检查请求头中的 `Accept-Encoding` 字段，确定客户端支持的压缩算法。
2. 中间件会根据配置和客户端支持的算法，选择最佳的压缩算法。
3. 如果响应体大小超过配置的最小压缩大小，且内容类型和文件扩展名不在排除列表中，中间件会对响应体进行压缩。
4. 压缩后的响应会添加 `Content-Encoding` 头，指明使用的压缩算法。
5. 如果启用了缓存功能，中间件会缓存压缩后的结果，以便后续相同的响应可以直接使用缓存，提高性能。

## 性能考虑

- **Gzip** 和 **Deflate** 是最通用的压缩算法，几乎所有浏览器都支持。
- **Brotli** 提供更高的压缩率，但压缩速度较慢，适合静态内容。
- **Zstd** 在压缩率和速度之间取得了很好的平衡，但浏览器支持较少。
- **Lz4** 是最快的压缩算法，但压缩率较低，适合需要快速压缩的场景。

对于静态内容或频繁访问的动态内容，建议启用缓存功能，可以显著提高性能。

## 示例

RAT Engine 提供了两个示例，展示如何使用压缩功能：

- `examples/compression_example.rs`：基本的压缩中间件示例
- `examples/compression_with_cache_example.rs`：带缓存的压缩中间件示例

运行示例：

```bash
# 运行基本压缩示例
cargo run --example compression_example --features="compression"

# 运行带缓存的压缩示例
cargo run --example compression_with_cache_example --features="compression,cache"
```