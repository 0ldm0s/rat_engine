//! 特性检查工具模块
//!
//! 提供 compile-time 和 runtime 的特性检查功能
//! 用于在 examples 和 tests 中优雅地处理特性依赖

/// 检查客户端特性是否启用
pub fn check_client_feature() -> Result<(), String> {
    #[cfg(not(feature = "client"))]
    {
        return Err(
            "此功能需要启用客户端特性。请在运行时添加 --features client 参数。\n\
            例如：cargo run --example example_name --features client".to_string()
        );
    }

    #[cfg(feature = "client")]
    {
        Ok(())
    }
}

/// 检查缓存特性是否启用
pub fn check_cache_feature() -> Result<(), String> {
    #[cfg(not(feature = "cache"))]
    {
        return Err(
            "此功能需要启用缓存特性。请在运行时添加 --features cache 参数。\n\
            例如：cargo run --example example_name --features cache".to_string()
        );
    }

    #[cfg(feature = "cache")]
    {
        Ok(())
    }
}

/// 检查完整缓存特性是否启用
pub fn check_cache_full_feature() -> Result<(), String> {
    #[cfg(not(feature = "cache-full"))]
    {
        return Err(
            "此功能需要启用完整缓存特性。请在运行时添加 --features cache-full 参数。\n\
            例如：cargo run --example example_name --features cache-full".to_string()
        );
    }

    #[cfg(feature = "cache-full")]
    {
        Ok(())
    }
}

/// 检查压缩特性是否启用
pub fn check_compression_feature() -> Result<(), String> {
    #[cfg(not(feature = "compression"))]
    {
        return Err(
            "此功能需要启用压缩特性。请在运行时添加 --features compression 参数。\n\
            例如：cargo run --example example_name --features compression".to_string()
        );
    }

    #[cfg(feature = "compression")]
    {
        Ok(())
    }
}

/// 检查完整压缩特性是否启用
pub fn check_compression_full_feature() -> Result<(), String> {
    #[cfg(not(feature = "compression-full"))]
    {
        return Err(
            "此功能需要启用完整压缩特性。请在运行时添加 --features compression-full 参数。\n\
            例如：cargo run --example example_name --features compression-full".to_string()
        );
    }

    #[cfg(feature = "compression-full")]
    {
        Ok(())
    }
}

/// 检查 Brotli 压缩特性是否启用
pub fn check_compression_br_feature() -> Result<(), String> {
    #[cfg(not(feature = "compression-br"))]
    {
        return Err(
            "此功能需要启用 Brotli 压缩特性。请在运行时添加 --features compression-br 参数。\n\
            例如：cargo run --example example_name --features compression-br".to_string()
        );
    }

    #[cfg(feature = "compression-br")]
    {
        Ok(())
    }
}

/// 检查 Zstd 压缩特性是否启用
pub fn check_compression_zstd_feature() -> Result<(), String> {
    #[cfg(not(feature = "compression-zstd"))]
    {
        return Err(
            "此功能需要启用 Zstd 压缩特性。请在运行时添加 --features compression-zstd 参数。\n\
            例如：cargo run --example example_name --features compression-zstd".to_string()
        );
    }

    #[cfg(feature = "compression-zstd")]
    {
        Ok(())
    }
}

/// 检查 TLS 特性是否启用
pub fn check_tls_feature() -> Result<(), String> {
    #[cfg(not(feature = "tls"))]
    {
        return Err(
            "此功能需要启用 TLS 特性。请在运行时添加 --features tls 参数。\n\
            例如：cargo run --example example_name --features tls".to_string()
        );
    }

    #[cfg(feature = "tls")]
    {
        Ok(())
    }
}

/// 检查 ACME 特性是否启用
pub fn check_acme_feature() -> Result<(), String> {
    #[cfg(not(feature = "acme"))]
    {
        return Err(
            "此功能需要启用 ACME 特性。请在运行时添加 --features acme 参数。\n\
            例如：cargo run --example example_name --features acme".to_string()
        );
    }

    #[cfg(feature = "acme")]
    {
        Ok(())
    }
}

/// 检查 Python 特性是否启用
pub fn check_python_feature() -> Result<(), String> {
    #[cfg(not(feature = "python"))]
    {
        return Err(
            "此功能需要启用 Python 特性。请在运行时添加 --features python 参数。\n\
            例如：cargo run --example example_name --features python".to_string()
        );
    }

    #[cfg(feature = "python")]
    {
        Ok(())
    }
}

/// 检查 Reqwest 特性是否启用
pub fn check_reqwest_feature() -> Result<(), String> {
    #[cfg(not(feature = "reqwest"))]
    {
        return Err(
            "此功能需要启用 Reqwest 特性。请在运行时添加 --features reqwest 参数。\n\
            例如：cargo run --example example_name --features reqwest".to_string()
        );
    }

    #[cfg(feature = "reqwest")]
    {
        Ok(())
    }
}

/// 检查特性组合，缺失时退出程序
pub fn check_features_and_exit(required_features: &[&str]) {
    let mut missing_features = Vec::new();

    for &feature in required_features {
        let result = match feature {
            "client" => check_client_feature(),
            "cache" => check_cache_feature(),
            "cache-full" => check_cache_full_feature(),
            "compression" => check_compression_feature(),
            "compression-full" => check_compression_full_feature(),
            "compression-br" => check_compression_br_feature(),
            "compression-zstd" => check_compression_zstd_feature(),
            "tls" => check_tls_feature(),
            "acme" => check_acme_feature(),
            "python" => check_python_feature(),
            "reqwest" => check_reqwest_feature(),
            _ => Err(format!("未知特性: {}", feature)),
        };

        if let Err(e) = result {
            missing_features.push(feature);
        }
    }

    if !missing_features.is_empty() {
        eprintln!("❌ 此示例/测试需要以下特性：");
        for feature in &missing_features {
            eprintln!("   - {}", feature);
        }

        eprintln!("\n🔧 请使用以下命令运行：");
        eprintln!("   cargo run --example <example_name> --features {}", missing_features.join(","));
        eprintln!("   或者");
        eprintln!("   cargo test <test_name> --features {}", missing_features.join(","));

        if missing_features.contains(&"compression-full") {
            eprintln!("\n💡 提示：compression-full 包含 compression、compression-br 和 compression-zstd");
        }
        if missing_features.contains(&"cache-full") {
            eprintln!("\n💡 提示：cache-full 包含 cache 和 L2 存储支持");
        }
        if missing_features.contains(&"python") {
            eprintln!("\n💡 提示：python 特性包含所有功能，是 Python 绑定的完整特性集");
        }

        std::process::exit(1);
    }
    // 所有特性都可用，程序继续执行
}

/// 便捷宏：检查特性并在缺失时退出
#[macro_export]
macro_rules! require_features {
    ($($feature:expr),*) => {
        $crate::utils::feature_check::check_features_and_exit(&[$($feature),*]);
    };
}