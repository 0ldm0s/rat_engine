//! SPA (Single Page Application) 支持示例
//! 
//! 本示例展示如何配置和使用 rat_engine 的 SPA 支持功能
//! SPA 支持允许在找不到匹配路由时，自动回退到指定的 HTML 文件
//! 这对于 React、Vue 等前端框架的单页应用非常有用
//! 
//! 新的架构流程：
//! 1. 使用 RatEngineBuilder 配置所有服务器参数
//! 2. 使用 with_router() 方法在构建器中直接配置路由
//! 3. 构建并启动服务器

use rat_engine::RatEngine;
use rat_engine::{Method, Response, StatusCode, Bytes, Full};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 日志通过RatEngineBuilder初始化
    
    println!("🚀 SPA 示例 - 使用新的 RatEngineBuilder 架构");
    println!("=============================================");
    
    // 使用 RatEngineBuilder 创建服务器，启用 SPA 支持
    let engine = RatEngine::builder()
        .worker_threads(4)                      // 设置工作线程数
        .max_connections(1000)                   // 设置最大连接数
        .buffer_size(8192)                      // 设置缓冲区大小
        .timeout(std::time::Duration::from_secs(30)) // 设置超时时间
        .keepalive(true)                        // 启用 Keep-Alive
        .tcp_nodelay(true)                      // 启用 TCP_NODELAY
        .with_log_config(rat_engine::utils::logger::LogConfig::default()) // 启用日志
                .spa_config("/index.html".to_string())  // 配置SPA支持
        .with_router(|mut router| {             // 配置路由
            // 添加根路由，显示SPA页面
            router.add_route(Method::GET, "/", |_req| {
                Box::pin(async {
                    let html_content = r#"<!DOCTYPE html>
<html>
<head>
    <title>SPA 示例</title>
    <meta charset="utf-8">
</head>
<body>
    <div id="app">
        <h1>欢迎使用 SPA 示例</h1>
        <p>这是一个单页应用示例 - 使用新的 RatEngineBuilder 架构</p>
        <nav>
            <a href="/">首页</a> |
            <a href="/about">关于</a> |
            <a href="/contact">联系</a>
        </nav>
        <div id="content">
            <p>当您访问 /about 或 /contact 等路由时，由于没有对应的服务器路由，</p>
            <p>系统会自动回退到这个 index.html 文件。</p>
            <p>前端路由器（如 React Router）可以接管这些路由的处理。</p>
        </div>
    </div>
    
    <script>
        // 简单的前端路由示例
        function updateContent() {
            const path = window.location.pathname;
            const content = document.getElementById('content');
            
            switch(path) {
                case '/':
                    content.innerHTML = '<h2>首页</h2><p>欢迎来到首页！</p>';
                    break;
                case '/about':
                    content.innerHTML = '<h2>关于我们</h2><p>这是关于页面。</p>';
                    break;
                case '/contact':
                    content.innerHTML = '<h2>联系我们</h2><p>这是联系页面。</p>';
                    break;
                default:
                    content.innerHTML = '<h2>页面未找到</h2><p>请检查 URL 是否正确。</p>';
            }
        }
        
        // 页面加载时更新内容
        window.addEventListener('load', updateContent);
        
        // 监听浏览器前进后退
        window.addEventListener('popstate', updateContent);
        
        // 拦截链接点击，使用前端路由
        document.addEventListener('click', function(e) {
            if (e.target.tagName === 'A' && e.target.href.startsWith(window.location.origin)) {
                e.preventDefault();
                history.pushState(null, '', e.target.href);
                updateContent();
            }
        });
    </script>
</body>
</html>"#;
                    
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/html; charset=utf-8")
                        .body(Full::new(Bytes::from(html_content)))
                        .unwrap();
                    Ok(response)
                })
            });
            
            // 添加 API 路由
            router.add_route(Method::GET, "/api/users", |_req| {
                Box::pin(async {
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(r#"{"users": ["Alice", "Bob"]}"#)))
                        .unwrap();
                    Ok(response)
                })
            });
            
            // 添加静态文件路由 - 模拟 index.html
            router.add_route(Method::GET, "/index.html", |_req| {
                Box::pin(async {
                    let html_content = r#"<!DOCTYPE html>
<html>
<head>
    <title>SPA 示例</title>
    <meta charset="utf-8">
</head>
<body>
    <div id="app">
        <h1>欢迎使用 SPA 示例</h1>
        <p>这是一个单页应用示例 - 使用新的 RatEngineBuilder 架构</p>
        <nav>
            <a href="/">首页</a> |
            <a href="/about">关于</a> |
            <a href="/contact">联系</a>
        </nav>
        <div id="content">
            <p>当您访问 /about 或 /contact 等路由时，由于没有对应的服务器路由，</p>
            <p>系统会自动回退到这个 index.html 文件。</p>
            <p>前端路由器（如 React Router）可以接管这些路由的处理。</p>
        </div>
    </div>
    
    <script>
        // 简单的前端路由示例
        function updateContent() {
            const path = window.location.pathname;
            const content = document.getElementById('content');
            
            switch(path) {
                case '/':
                    content.innerHTML = '<h2>首页</h2><p>欢迎来到首页！</p>';
                    break;
                case '/about':
                    content.innerHTML = '<h2>关于我们</h2><p>这是关于页面。</p>';
                    break;
                case '/contact':
                    content.innerHTML = '<h2>联系我们</h2><p>这是联系页面。</p>';
                    break;
                default:
                    content.innerHTML = '<h2>页面未找到</h2><p>请检查 URL 是否正确。</p>';
            }
        }
        
        // 页面加载时更新内容
        window.addEventListener('load', updateContent);
        
        // 监听浏览器前进后退
        window.addEventListener('popstate', updateContent);
        
        // 拦截链接点击，使用前端路由
        document.addEventListener('click', function(e) {
            if (e.target.tagName === 'A' && e.target.href.startsWith(window.location.origin)) {
                e.preventDefault();
                history.pushState(null, '', e.target.href);
                updateContent();
            }
        });
    </script>
</body>
</html>"#;
                
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(Full::new(Bytes::from(html_content)))
                    .unwrap();
                Ok(response)
                })
            });
            
            // 添加一些静态资源路由（模拟真实的文件请求）
            router.add_route(Method::GET, "/favicon.ico", |_req| {
                Box::pin(async {
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Full::new(Bytes::from("Favicon not found")))
                        .unwrap();
                    Ok(response)
                })
            });
            
            router // 返回配置好的router
        })
        .build_and_start("127.0.0.1".to_string(), 3000).await
        .expect("Failed to start server");
    
    println!("✅ 服务器已启动，访问 http://127.0.0.1:3000");
    println!("📝 服务器配置信息：");
    println!("   - 工作线程数: {}", engine.get_workers());
    println!("   - 最大连接数: {}", engine.get_max_connections());
    println!("   - 主机地址: {}", engine.get_host());
    println!("   - 端口: {}", engine.get_port());
    println!("   - SPA 配置: /index.html");
    println!("");
    println!("🔍 测试说明:");
    println!("  • 访问 http://localhost:3000/ - 首页");
    println!("  • 访问 http://localhost:3000/about - 关于页面（SPA 回退）");
    println!("  • 访问 http://localhost:3000/contact - 联系页面（SPA 回退）");
    println!("  • 访问 http://localhost:3000/api/users - API 接口");
    println!("  • 访问 http://localhost:3000/nonexistent - 不存在的路由（SPA 回退）");
    println!("");
    println!("💡 SPA 回退逻辑:");
    println!("  • 当请求路径没有文件扩展名时，会触发 SPA 回退");
    println!("  • 系统会自动将请求重定向到 /index.html");
    println!("  • 前端路由器可以接管这些路由的处理");
    println!("");
    println!("⏳ 服务器正在运行，按 Ctrl+C 停止...");
    
    // 在实际应用中，这里会一直运行直到收到停止信号
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    
    Ok(())
}