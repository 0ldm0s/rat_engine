//! gRPC 压缩模式模块

/// gRPC 压缩模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcCompressionMode {
    /// 禁用压缩（默认）
    Disabled,
    /// 启用 LZ4 压缩
    Lz4,
}

impl GrpcCompressionMode {
    /// 获取压缩算法名称
    pub fn name(&self) -> &'static str {
        match self {
            Self::Disabled => "identity",
            Self::Lz4 => "lz4",
        }
    }

    /// 获取 Accept-Encoding 头部值
    pub fn accept_encoding(&self) -> &'static str {
        match self {
            Self::Disabled => "identity",
            Self::Lz4 => "lz4, identity",
        }
    }
}
