//! RAT Engine 独立HTTP客户端单元测试
//!
//! 测试基于reqwest的独立HTTP客户端的核心功能

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[cfg(feature = "reqwest")]
    #[tokio::test]
    async fn test_independent_client_creation() {
        // 测试客户端创建
        let client = rat_engine::RatIndependentHttpClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent("test-agent/1.0")
            .build();

        assert!(client.is_ok());
    }

    #[cfg(feature = "reqwest")]
    #[tokio::test]
    async fn test_basic_http_request() {
        // 创建客户端
        let client = rat_engine::RatIndependentHttpClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create client");

        // 发送GET请求
        let response = client.get("https://httpbin.org/get").await;

        assert!(response.is_ok());

        let resp = response.unwrap();
        assert_eq!(resp.status.as_u16(), 200);
        assert!(resp.body.len() > 0);
    }

    #[cfg(feature = "reqwest")]
    #[tokio::test]
    async fn test_compression_support() {
        // 创建支持压缩的客户端
        let client = rat_engine::RatIndependentHttpClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .supported_compressions(vec!["gzip".to_string(), "deflate".to_string()])
            .auto_decompress(true)
            .build()
            .expect("Failed to create client");

        // 测试压缩请求
        let response = client.get("https://httpbin.org/gzip").await;

        assert!(response.is_ok());

        let resp = response.unwrap();
        assert_eq!(resp.status.as_u16(), 200);
        assert!(resp.body.len() > 0);
    }

    #[test]
    fn test_feature_gate() {
        // 测试特性门控
        #[cfg(feature = "reqwest")]
        {
            assert!(true);
        }

        #[cfg(not(feature = "reqwest"))]
        {
            // 当没有reqwest特性时，应该无法访问相关类型
            // 这里只是确保代码能编译
            assert!(true);
        }
    }
}