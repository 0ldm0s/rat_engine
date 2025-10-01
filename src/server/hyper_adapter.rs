use hyper::body::Incoming;
use http_body_util::{Full, combinators::BoxBody};
use hyper::body::Bytes;
use hyper::service::Service;
use hyper::{Request, Response};
use crate::server::router::Router;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::convert::Infallible;
use std::net::SocketAddr;

pub struct HyperAdapter {
    router: Arc<Router>,
}

impl HyperAdapter {
    pub fn new(router: Arc<Router>) -> Self {
        HyperAdapter { router }
    }

    pub async fn handle_request(
        &self,
        req: Request<Incoming>,
        remote_addr: Option<SocketAddr>,
    ) -> Result<Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>, hyper::Error> {
        // 记录请求信息
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let start = std::time::Instant::now();
        let client_ip = remote_addr.map(|addr| addr.ip().to_string()).unwrap_or_else(|| "unknown".to_string());
        
        crate::utils::logger::debug!("🔍 [HyperAdapter] 收到请求: {} {}", method, path);
        crate::utils::logger::debug!("🔍 [HyperAdapter] 请求头: {:?}", req.headers());
        
        // 处理请求
        crate::utils::logger::debug!("🔍 [HyperAdapter] 开始路由处理...");
        let router_start = std::time::Instant::now();
        let response = self.router.handle_hyper_request(req, remote_addr).await;
        let total_duration = start.elapsed();
        
        match &response {
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                // 统计信息日志（info 级别，生产环境可见）
                crate::utils::logger::info!(
                    "📊 {} {} {} {} {}",
                    client_ip,
                    method,
                    path,
                    status_code,
                    crate::utils::logger::format_duration(total_duration)
                );

                crate::utils::logger::debug!("🔍 [HyperAdapter] 路由处理成功，总耗时: {}", crate::utils::logger::format_duration(total_duration));
                crate::utils::logger::debug!("🔍 [HyperAdapter] 响应状态码: {}", resp.status());
                crate::utils::logger::debug!("🔍 [HyperAdapter] 响应头: {:?}", resp.headers());
                crate::utils::logger::debug!("🔍 [HyperAdapter] 响应体类型: Full<Bytes>");
                crate::utils::logger::debug!("🔍 [HyperAdapter] 准备返回响应给 Hyper...");
            },
            Err(e) => {
                // 错误访问日志 - error级别
                crate::utils::logger::error!(
                    "❌ {} {} {} ERROR {} - {}",
                    client_ip,
                    method,
                    path,
                    crate::utils::logger::format_duration(total_duration),
                    e
                );

                crate::utils::logger::debug!("🔍 [HyperAdapter] 路由处理失败，耗时: {}", crate::utils::logger::format_duration(total_duration));
                crate::utils::logger::debug!("🔍 [HyperAdapter] 错误详情: {:?}", e);
            },
        }
        
        crate::utils::logger::debug!("🔍 [HyperAdapter] handle_request 方法即将返回");
        
        response
    }
}

impl Service<Request<Incoming>> for HyperAdapter {
    type Response = Response<BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        crate::utils::logger::debug!("🔍 [Service] call 方法被调用，请求: {} {}", req.method(), req.uri().path());
        let adapter = self.clone();
        Box::pin(async move {
            crate::utils::logger::debug!("🔍 [Service] 开始异步处理请求");
            let result = adapter.handle_request(req, None).await;
            match &result {
                Ok(resp) => crate::utils::logger::debug!("🔍 [Service] handle_request 返回成功，状态码: {}", resp.status()),
                Err(e) => crate::utils::logger::debug!("🔍 [Service] handle_request 返回错误: {:?}", e),
            }
            crate::utils::logger::debug!("🔍 [Service] call 方法即将返回结果");
            result
        })
    }
}

impl Clone for HyperAdapter {
    fn clone(&self) -> Self {
        HyperAdapter {
            router: Arc::clone(&self.router),
        }
    }
}