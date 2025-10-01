//! HTTP 转换工具模块
//! 提供 hyper 和内部 HTTP 类型之间的转换功能

use std::collections::HashMap;
use hyper::{Request, Response, body::Incoming};
use hyper::body::Bytes;
use http_body_util::Full;
use http_body_util::BodyExt;
// 已移除 serialization 模块的引用

use super::{HttpRequest, HttpResponse};

/// 将 hyper::Request 转换为 HttpRequest
pub async fn convert_hyper_to_http_request(req: Request<Incoming>) -> Result<HttpRequest, hyper::Error> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_string = req.uri().query().unwrap_or("").to_string();
    
    // 收集请求头
    let mut headers = HashMap::new();
    for (name, value) in req.headers() {
        if let Ok(value_str) = value.to_str() {
            headers.insert(name.to_string(), value_str.to_string());
        }
    }
    
    // 读取请求体
    let body_bytes = req.into_body().collect().await?.to_bytes();
    let body = String::from_utf8_lossy(&body_bytes).to_string();
    
    Ok(HttpRequest {
        method,
        path,
        query_string,
        headers,
        body: body.into_bytes(),
        remote_addr: "127.0.0.1:0".to_string(),
        real_ip: "127.0.0.1".to_string(),
        path_params: std::collections::HashMap::new(),
    })
}

/// 将 HttpResponse 转换为 hyper::Response
pub fn convert_http_to_hyper_response(response: HttpResponse) -> Response<Full<Bytes>> {
    let mut builder = Response::builder()
        .status(response.status as u16);
    
    // 添加响应头
    for (key, value) in response.headers {
        builder = builder.header(key, value);
    }
    
    // 设置响应体
    builder
        .body(Full::new(Bytes::from(response.body)))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(500)
                .body(Full::new(Bytes::from("Internal Server Error")))
                .unwrap()
        })
}

/// 序列化响应为字节数组（使用 JSON 序列化）
pub fn serialize_response(response: HttpResponse) -> Vec<u8> {
    serde_json::to_vec(&response)
        .unwrap_or_else(|_| {
            // 降级处理：返回错误响应的 JSON 编码
            let error_response = HttpResponse::error("Serialization failed".to_string(), Some(500));
            serde_json::to_vec(&error_response)
                .unwrap_or_else(|_| b"Internal Server Error".to_vec())
        })
}

/// 创建错误响应（使用 bincode 高性能序列化）
pub fn create_error_response(status: u16, message: &str) -> Vec<u8> {
    let response = HttpResponse::error(message.to_string(), Some(status));
    serialize_response(response)
}