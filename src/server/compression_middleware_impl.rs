//! 压缩中间件实现模块
//! 
//! 这个模块提供了压缩中间件的具体实现。

use crate::server::compression_middleware::CompressionMiddleware;
use hyper::{Request, Response, body::Bytes};
use http_body_util::combinators::BoxBody;
use std::error::Error;
use std::sync::Arc;

/// 压缩中间件实现
pub struct CompressionMiddlewareImpl {
    /// 内部中间件实例
    middleware: Arc<CompressionMiddleware>,
}

impl CompressionMiddlewareImpl {
    /// 创建新的压缩中间件实现
    pub fn new(middleware: CompressionMiddleware) -> Self {
        Self {
            middleware: Arc::new(middleware),
        }
    }
    
    /// 处理请求和响应
    pub async fn process<B>(
        &self,
        req: &Request<B>,
        res: Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>,
    ) -> Result<Response<BoxBody<Bytes, Box<dyn Error + Send + Sync>>>, hyper::Error> {
        // 应用压缩
        self.middleware.process(req, res).await
    }
}