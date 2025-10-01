//! ACME 沙盒模式证书申请演示
//! 
//! 这个示例展示如何使用 rat_engine 在沙盒环境中申请 Let's Encrypt 证书
//! 并启动一个 HTTPS 服务器来验证证书的有效性

use rat_engine::{RatEngine, server::{Router, cert_manager::CertManagerBuilder}};
use rat_engine::{Method, Response, StatusCode, Bytes, Full};
use std::time::Duration;
use serde_json::json;
use std::sync::{Arc, RwLock};
use rat_engine::server::config::ServerConfig;
use rat_engine::utils::logger::{LogConfig, LogLevel, LogOutput};
// 配置常量 - 演示用的硬编码配置
const DOMAIN: &str = "gs1.sukiyaki.su";
const EMAIL: &str = "oldmos@gmail.com";
const CLOUDFLARE_API_TOKEN: &str = "_qNrowN18mIYT0qRZFzxRJzDxh2Qw0_qzxJoGhIg";
const PORT: u16 = 8443;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 检查必需的特性
    rat_engine::require_features!("acme", "tls");

    // 初始化加密提供者
    rat_engine::utils::crypto_provider::ensure_crypto_provider_installed();

    println!("🚀 RAT Engine ACME 沙盒模式演示");
    println!("=== 开始ACME证书申请测试 ===");
    println!("================================");
    println!();

    // 使用硬编码的配置常量
    let domain = DOMAIN;
    let email = EMAIL;
    let cloudflare_token = CLOUDFLARE_API_TOKEN;
    let port = PORT;

    println!("📋 配置信息:");
    println!("   域名: {}", domain);
    println!("   邮箱: {}", email);
    println!("   端口: {}", port);
    println!("   Cloudflare Token: {}...{}", 
        &cloudflare_token[..8.min(cloudflare_token.len())],
        if cloudflare_token.len() > 16 { &cloudflare_token[cloudflare_token.len()-8..] } else { "" }
    );
    println!();

    // 创建路由器
    let router = create_demo_router(&domain, &email, &cloudflare_token)?;
    
    // 配置证书目录
    let cert_dir = format!("./acme_certificates_{}", port);
    
    println!("🔐 正在申请 ACME 沙盒证书...");
    println!("⏳ 这可能需要几分钟时间，请耐心等待...");
    println!();

    // 启动服务器
    println!("🚀 启动 HTTPS 服务器: https://{}:{}", domain, port);
    println!("📋 可用端点:");
    println!("   - https://{}:{}/", domain, port);
    println!("   - https://{}:{}/api/status", domain, port);
    println!("   - https://{}:{}/api/cert-info", domain, port);
    println!("   - https://{}:{}/health", domain, port);
    println!();
    println!("🔍 测试命令 (使用本地 IP 绕过 DNS):");
    println!("   curl -k --resolve {}:{}:127.0.0.1 https://{}:{}/health", domain, port, domain, port);
    println!("   curl -k --resolve {}:{}:127.0.0.1 https://{}:{}/api/status", domain, port, domain, port);
    println!();
    println!("🔍 证书检查命令:");
    println!("   openssl s_client -connect 127.0.0.1:{} -servername {} -showcerts", port, domain);
    println!();
    println!("📁 证书存储目录: {}", cert_dir);
    println!("⏸️  按 Ctrl+C 停止服务器");
    println!();

    // 配置debug级别日志
    let log_config = LogConfig {
        enabled: true,
        level: LogLevel::Debug,  // 设置为debug级别
        output: LogOutput::Terminal,
        use_colors: true,
        use_emoji: true,
        show_timestamp: true,
        show_module: true,
    };

    // 使用 RatEngineBuilder 启动服务器，配置ACME证书管理器
    println!("🔧 即将调用cert_manager_acme方法...");
    let engine = RatEngine::builder()
                .with_log_config(log_config)  // 使用debug级别日志配置
        .router(router)
        .cert_manager_acme(
            domain.to_string(),
            email.to_string(),
            cloudflare_token.to_string(),
            cert_dir,
            30,
            false // false: 沙盒环境, true: 生产环境
        ).await?
        .build()?;
    println!("🔧 cert_manager_acme方法调用完成");

    engine.start("0.0.0.0".to_string(), port).await?;

    Ok(())
}

/// 创建演示路由器
fn create_demo_router(
    domain: &str, 
    email: &str, 
    cloudflare_token: &str
) -> Result<Router, Box<dyn std::error::Error + Send + Sync>> {
    let domain = domain.to_string();
    let email = email.to_string();
    let cloudflare_token = cloudflare_token.to_string();
    let mut router = Router::new();

    // 启用 HTTP/2 支持
    router.enable_h2();

    // 主页端点
    let domain_clone = domain.clone();
    router.add_route(Method::GET, "/", move |_req| {
        let domain = domain_clone.clone();
        Box::pin(async move {
            let html_content = format!(r#"
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RAT Engine ACME 演示</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 40px; background: #f5f5f5; }}
        .container {{ max-width: 800px; margin: 0 auto; background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
        .header {{ text-align: center; color: #333; margin-bottom: 30px; }}
        .status {{ background: #e8f5e8; padding: 15px; border-radius: 5px; margin: 20px 0; }}
        .endpoint {{ background: #f0f8ff; padding: 10px; margin: 10px 0; border-radius: 5px; font-family: monospace; }}
        .success {{ color: #28a745; font-weight: bold; }}
        .info {{ color: #17a2b8; }}
        .warning {{ color: #ffc107; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🚀 RAT Engine ACME 演示</h1>
            <h2>Let's Encrypt 沙盒环境证书申请成功！</h2>
        </div>
        
        <div class="status">
            <h3 class="success">✅ HTTPS 连接已建立</h3>
            <p><strong>域名:</strong> {}</p>
            <p><strong>证书类型:</strong> Let's Encrypt Staging (沙盒环境)</p>
            <p><strong>协议:</strong> HTTP/2 over TLS</p>
            <p><strong>时间:</strong> <span id="current-time"></span></p>
        </div>
        
        <h3>📋 可用 API 端点:</h3>
        <div class="endpoint">GET /health - 健康检查</div>
        <div class="endpoint">GET /api/status - 服务状态</div>
        <div class="endpoint">GET /api/cert-info - 证书信息</div>
        
        <h3>🔍 测试命令:</h3>
        <div class="endpoint">curl -k https://{}/health</div>
        <div class="endpoint">curl -k https://{}/api/status</div>
        
        <h3>🔐 证书验证命令:</h3>
        <div class="endpoint">openssl s_client -connect {}:8443 -servername {} -showcerts</div>
        
        <div class="status">
            <p class="info">💡 这是一个演示环境，使用 Let's Encrypt 沙盒证书。</p>
            <p class="warning">⚠️ 沙盒证书不被浏览器信任，这是正常现象。</p>
        </div>
    </div>
    
    <script>
        function updateTime() {{
            document.getElementById('current-time').textContent = new Date().toLocaleString('zh-CN');
        }}
        updateTime();
        setInterval(updateTime, 1000);
    </script>
</body>
</html>
            "#, domain, domain, domain, domain, domain);

            let body = Full::new(Bytes::from(html_content));
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-cache")
                .body(body)
                .unwrap())
        })
    });

    // API 状态端点
    let domain_clone = domain.clone();
    router.add_route(Method::GET, "/api/status", move |_req| {
        let domain = domain_clone.clone();
        Box::pin(async move {
            let response_data = json!({
                "status": "success",
                "message": "RAT Engine ACME 演示服务运行正常",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "server": "rat_engine",
                "version": "0.2.9",
                "domain": domain,
                "tls_enabled": true,
                "http2_enabled": true,
                "acme_mode": "sandbox",
                "cert_authority": "Let's Encrypt Staging",
                "auto_renewal": true,
                "renewal_threshold_days": 30
            });

            let body = Full::new(Bytes::from(response_data.to_string()));
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "no-cache")
                .body(body)
                .unwrap())
        })
    });

    // 证书信息端点
    let domain_clone = domain.clone();
    router.add_route(Method::GET, "/api/cert-info", move |_req| {
        let domain = domain_clone.clone();
        Box::pin(async move {
            let response_data = json!({
                "status": "success",
                "message": "证书信息获取成功",
                "domain": domain,
                "cert_type": "ACME",
                "issuer": "Let's Encrypt Staging",
                "environment": "sandbox",
                "auto_renewal": true,
                "dns_provider": "Cloudflare",
                "challenge_type": "DNS-01",
                "key_type": "ECDSA P-384",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "note": "这是沙盒环境证书，不被浏览器信任"
            });

            let body = Full::new(Bytes::from(response_data.to_string()));
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(body)
                .unwrap())
        })
    });

    // 健康检查端点
    router.add_route(Method::GET, "/health", |_req| {
        Box::pin(async move {
            let response_data = json!({
                "status": "healthy",
                "service": "rat_engine_acme_demo",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "uptime_seconds": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "tls": "enabled",
                "acme": "sandbox"
            });

            let body = Full::new(Bytes::from(response_data.to_string()));
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(body)
                .unwrap())
        })
    });

    Ok(router)
}