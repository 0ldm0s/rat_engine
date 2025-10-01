//! Python 压缩模块
//! 
//! 提供压缩配置和相关功能的 Python 绑定

use pyo3::prelude::*;
use pyo3::types::{PyList, PyDict};
use std::collections::HashSet;
use crate::compression::{CompressionType, CompressionConfig};

/// Python 压缩配置类
#[pyclass(name = "CompressionConfig")]
#[derive(Clone)]
pub struct PyCompressionConfig {
    /// 内部压缩配置
    pub config: CompressionConfig,
}

impl PyCompressionConfig {
}

#[pymethods]
#[allow(clippy::new_ret_no_self)]
impl PyCompressionConfig {
    /// 创建新的压缩配置
    #[new]
    #[pyo3(signature = (
        min_size=1024,
        level=6,
        enable_gzip=true,
        enable_deflate=true,
        enable_brotli=true,
        enable_zstd=true,
        enable_lz4=false,
        excluded_content_types=None,
        excluded_extensions=None,
        enable_smart_compression=false
    ))]
    fn new(
        min_size: usize,
        level: u32,
        enable_gzip: bool,
        enable_deflate: bool,
        enable_brotli: bool,
        enable_zstd: bool,
        enable_lz4: bool,
        excluded_content_types: Option<Vec<String>>,
        excluded_extensions: Option<Vec<String>>,
        enable_smart_compression: bool,
    ) -> Self {
        let mut enabled_algorithms = Vec::new();
        if enable_gzip { enabled_algorithms.push(CompressionType::Gzip); }
        if enable_deflate { enabled_algorithms.push(CompressionType::Deflate); }
        if enable_brotli { enabled_algorithms.push(CompressionType::Brotli); }
        if enable_zstd { enabled_algorithms.push(CompressionType::Zstd); }
        if enable_lz4 { enabled_algorithms.push(CompressionType::Lz4); }

        let config = CompressionConfig {
            enabled_algorithms,
            min_size,
            level,
            excluded_content_types: excluded_content_types.unwrap_or_default().into_iter().collect(),
            excluded_extensions: excluded_extensions.unwrap_or_default().into_iter().collect(),
            enable_smart_compression,
        };

        Self { config }
    }
    
    /// 启用 Gzip 压缩
    fn enable_gzip(&mut self) -> PyResult<()> {
        if !self.config.enabled_algorithms.contains(&CompressionType::Gzip) {
            self.config.enabled_algorithms.push(CompressionType::Gzip);
        }
        Ok(())
    }

    /// 启用 Deflate 压缩
    fn enable_deflate(&mut self) -> PyResult<()> {
        if !self.config.enabled_algorithms.contains(&CompressionType::Deflate) {
            self.config.enabled_algorithms.push(CompressionType::Deflate);
        }
        Ok(())
    }

    /// 启用 Brotli 压缩
    fn enable_brotli(&mut self) -> PyResult<()> {
        if !self.config.enabled_algorithms.contains(&CompressionType::Brotli) {
            self.config.enabled_algorithms.push(CompressionType::Brotli);
        }
        Ok(())
    }

    /// 启用 Zstd 压缩
    fn enable_zstd(&mut self) -> PyResult<()> {
        if !self.config.enabled_algorithms.contains(&CompressionType::Zstd) {
            self.config.enabled_algorithms.push(CompressionType::Zstd);
        }
        Ok(())
    }

    /// 启用 LZ4 压缩
    fn enable_lz4(&mut self) -> PyResult<()> {
        if !self.config.enabled_algorithms.contains(&CompressionType::Lz4) {
            self.config.enabled_algorithms.push(CompressionType::Lz4);
        }
        Ok(())
    }
    
    /// 禁用 Gzip 压缩
    fn disable_gzip(&mut self) -> PyResult<()> {
        self.config.enabled_algorithms.retain(|&alg| alg != CompressionType::Gzip);
        Ok(())
    }

    /// 禁用 Deflate 压缩
    fn disable_deflate(&mut self) -> PyResult<()> {
        self.config.enabled_algorithms.retain(|&alg| alg != CompressionType::Deflate);
        Ok(())
    }

    /// 禁用 Brotli 压缩
    fn disable_brotli(&mut self) -> PyResult<()> {
        self.config.enabled_algorithms.retain(|&alg| alg != CompressionType::Brotli);
        Ok(())
    }

    /// 禁用 Zstd 压缩
    fn disable_zstd(&mut self) -> PyResult<()> {
        self.config.enabled_algorithms.retain(|&alg| alg != CompressionType::Zstd);
        Ok(())
    }

    /// 禁用 LZ4 压缩
    fn disable_lz4(&mut self) -> PyResult<()> {
        self.config.enabled_algorithms.retain(|&alg| alg != CompressionType::Lz4);
        Ok(())
    }
    
    /// 设置最小压缩大小
    fn min_size(&mut self, size: usize) -> PyResult<()> {
        self.config.min_size = size;
        Ok(())
    }

    /// 设置压缩级别
    fn level(&mut self, level: u32) -> PyResult<()> {
        self.config.level = level;
        Ok(())
    }

    /// 获取启用的压缩算法列表
    #[getter]
    fn enabled_algorithms(&self) -> Vec<String> {
        self.config.enabled_algorithms.iter()
            .map(|alg| alg.name().to_string())
            .collect()
    }

    /// 获取智能压缩状态
    #[getter]
    fn smart_compression(&self) -> bool {
        self.config.enable_smart_compression
    }

    /// 设置排除的内容类型
    fn exclude_content_types(&mut self, content_types: Vec<String>) -> PyResult<()> {
        self.config.excluded_content_types = content_types.into_iter().collect();
        Ok(())
    }

    /// 设置排除的文件扩展名
    fn exclude_extensions(&mut self, extensions: Vec<String>) -> PyResult<()> {
        self.config.excluded_extensions = extensions.into_iter().collect();
        Ok(())
    }
    
    /// 获取当前配置的字符串表示
    fn __repr__(&self) -> String {
        let algorithms: Vec<String> = self.config.enabled_algorithms.iter()
            .map(|alg| alg.name().to_string())
            .collect();

        format!(
            "CompressionConfig(min_size={}, level={}, algorithms=[{}], smart_compression={})",
            self.config.min_size,
            self.config.level,
            algorithms.join(", "),
            self.config.enable_smart_compression
        )
    }
}

/// 注册压缩模块
pub fn register_compression_module(_py: Python, parent_module: &PyModule) -> PyResult<()> {
    // 直接将压缩类注册到根模块，而不是创建子模块
    parent_module.add_class::<PyCompressionConfig>()?;
    parent_module.add_class::<PyCompressionType>()?;
    
    Ok(())
}

/// Python 压缩类型枚举
#[pyclass(name = "CompressionType")]
#[derive(Clone)]
pub struct PyCompressionType {
    pub compression_type: CompressionType,
}

#[pymethods]
impl PyCompressionType {
    #[classattr]
    fn NONE() -> Self {
        Self { compression_type: CompressionType::None }
    }
    
    #[classattr]
    fn GZIP() -> Self {
        Self { compression_type: CompressionType::Gzip }
    }
    
    #[classattr]
    fn DEFLATE() -> Self {
        Self { compression_type: CompressionType::Deflate }
    }
    
    #[cfg(feature = "compression-br")]
    #[classattr]
    fn BROTLI() -> Self {
        Self { compression_type: CompressionType::Brotli }
    }

    #[cfg(feature = "compression-zstd")]
    #[classattr]
    fn ZSTD() -> Self {
        Self { compression_type: CompressionType::Zstd }
    }
    
    #[classattr]
    fn LZ4() -> Self {
        Self { compression_type: CompressionType::Lz4 }
    }
    
    /// 获取压缩类型名称
    fn name(&self) -> &'static str {
        self.compression_type.name()
    }
    
    /// 获取 HTTP 头部值
    fn header_value(&self) -> &'static str {
        self.compression_type.header_value()
    }
    
    /// 获取当前压缩类型的字符串表示
    fn __repr__(&self) -> String {
        format!("CompressionType.{}", self.name().to_uppercase())
    }
    
    /// 获取当前压缩类型的字符串表示
    fn __str__(&self) -> String {
        self.name().to_string()
    }
}