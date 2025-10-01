//! 压缩中间件模块
//! 
//! 这个模块提供了用于HTTP响应压缩的中间件功能。

use crate::compression::{CompressionConfig, Compressor};
use hyper::{Request, Response, body::Bytes};
use http_body_util::combinators::BoxBody;
use std::error::Error;
use std::sync::Arc;

/// 压缩中间件结构体
pub struct CompressionMiddleware {
    /// 压缩器实例
    compressor: Arc<Compressor>,
}

impl CompressionMiddleware {
    /// 创建新的压缩中间件
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            compressor: Arc::new(Compressor::new(config)),
        }
    }
    
    /// 处理请求和响应
    pub async fn process<B>(
        &self,
        _req: &Request<B>,
        res: Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>,
    ) -> Result<Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>, hyper::Error> {
        // 获取Accept-Encoding头
        let accept_encoding = _req.headers()
            .get("accept-encoding")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
            
        // 获取请求路径
        let path = _req.uri().path();
        let file_ext = path.split('.')
            .last()
            .unwrap_or("");
            
        // 应用压缩
        self.compressor.compress_response(res, accept_encoding, file_ext).await
    }
}