#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ACME 证书管理示例

本示例展示如何在 Python 中使用 rat_engine 的 ACME 证书管理功能：
1. 配置 ACME 自动证书申请和续期
2. 配置生产环境证书
3. 启用开发模式（自签名证书）

注意：
- ACME 生产环境需要真实域名和 DNS 配置
- 测试环境使用 Let's Encrypt Staging，证书不被浏览器信任但可用于测试
- 开发模式使用自签名证书，仅适用于本地开发
"""

import asyncio
import os
import sys
import argparse
from rat_engine import RatApp, ServerConfig, CertManagerConfig


def create_basic_routes(app):
    """创建基本的测试路由"""
    
    @app.json("/")
    def hello_handler(request):
        return {
            "message": "Hello from HTTPS server!",
            "path": request.get("path", "/"),
            "method": request.get("method", "GET"),
            "tls_enabled": True
        }
    
    @app.json("/hello")
    def hello_handler_alias(request):
        return {
            "message": "Hello from HTTPS server!",
            "path": request.get("path", "/"),
            "method": request.get("method", "GET"),
            "tls_enabled": True
        }
    
    @app.json("/health")
    def health_handler(request):
        return {
            "status": "healthy",
            "timestamp": request.get("timestamp"),
            "server": "rat_engine with ACME"
        }
    
    print("✅ 基本路由已配置（使用装饰器）")


def example_acme_staging():
    """示例1: ACME 测试环境配置"""
    print("\n=== 示例1: ACME 测试环境配置 ===")
    
    # 从命令行参数获取配置
    domain = _cli_args.domain if _cli_args.domain else "gs1.sukiyaki.su"
    email = _cli_args.email if _cli_args.email else "oldmos@gmail.com"
    cloudflare_token = _cli_args.cloudflare_token if _cli_args.cloudflare_token else "_qNrowN18mIYT0qRZFzxRJzDxh2Qw0_qzxJoGhIg"
    
    print(f"📋 使用配置:")
    print(f"   域名: {domain}")
    print(f"   邮箱: {email}")
    print(f"   Cloudflare Token: {cloudflare_token[:8]}...{cloudflare_token[-8:] if len(cloudflare_token) > 8 else cloudflare_token}")
    
    # 创建应用实例
    app = RatApp(name="acme_staging_app")
    
    # 创建 ACME 测试环境配置
    cert_config = CertManagerConfig.acme_config(
        email=email,                          # 必需：ACME 注册邮箱
        production=False,                    # 使用测试环境
        cloudflare_token=cloudflare_token,  # 可选：Cloudflare API Token
        renewal_days=30,                     # 30天内到期时续期
        cert_dir="./certs_staging"           # 证书存储目录
    )
    
    print(f"🔧 配置类型: {cert_config.get_config_type()}")
    print(f"✅ 配置有效性: {cert_config.is_valid()}")
    
    # 配置 ACME 证书
    domains = [domain]
    
    try:
        app.configure_acme_certs(domains, cert_config)
        print(f"✅ ACME 证书配置成功，域名: {domains}")
    except Exception as e:
        print(f"❌ ACME 证书配置失败: {e}")
        return None
    
    # 添加基本路由
    create_basic_routes(app)
    
    return app


def example_acme_production():
    """示例2: ACME 生产环境配置"""
    print("\n=== 示例2: ACME 生产环境配置 ===")
    
    # 从环境变量读取配置
    email = os.getenv("ACME_EMAIL", "admin@yourdomain.com")
    cloudflare_token = os.getenv("CLOUDFLARE_API_TOKEN")
    domains_str = os.getenv("ACME_DOMAINS", "yourdomain.com,www.yourdomain.com")
    domains = [d.strip() for d in domains_str.split(",")]
    
    print(f"📧 ACME 邮箱: {email}")
    print(f"🌐 域名列表: {domains}")
    print(f"🔑 Cloudflare Token: {'已设置' if cloudflare_token else '未设置'}")
    
    # 创建应用实例
    app = RatApp(name="acme_production_app")
    
    # 创建 ACME 生产环境配置
    cert_config = CertManagerConfig.acme_config(
        email=email,
        production=True,  # 🚨 生产环境！
        cloudflare_token=cloudflare_token,
        renewal_days=30,
        cert_dir="/etc/ssl/acme"
    )
    
    print(f"📋 证书配置: {cert_config}")
    
    try:
        app.configure_acme_certs(domains, cert_config)
        print("✅ ACME 生产环境证书配置成功")
        print("⚠️  请确保域名 DNS 记录指向此服务器")
    except Exception as e:
        print(f"❌ ACME 生产环境配置失败: {e}")
        return None
    
    # 添加基本路由
    create_basic_routes(app)
    
    return app


def example_production_certs():
    """示例3: 使用现有的生产环境证书"""
    print("\n=== 示例3: 生产环境证书配置 ===")
    
    # 证书文件路径
    cert_file = "/path/to/your/cert.pem"
    key_file = "/path/to/your/key.pem"
    
    print(f"📄 证书文件: {cert_file}")
    print(f"🔐 私钥文件: {key_file}")
    
    # 创建应用实例
    app = RatApp(name="production_certs_app")
    
    # 创建生产环境证书配置
    cert_config = CertManagerConfig.production_config(
        cert_file=cert_file,
        key_file=key_file
    )
    
    print(f"📋 证书配置: {cert_config}")
    print(f"✅ 配置有效性: {cert_config.is_valid()}")
    
    try:
        app.configure_production_certs(cert_config)
        print("✅ 生产环境证书配置成功")
    except Exception as e:
        print(f"❌ 生产环境证书配置失败: {e}")
        return None
    
    # 添加基本路由
    create_basic_routes(app)
    
    return app


def example_development_mode():
    """示例4: 开发模式（自签名证书）"""
    print("\n=== 示例4: 开发模式配置 ===")
    
    # 创建应用实例
    app = RatApp(name="development_app")
    
    # 启用开发模式（自动生成自签名证书）
    hostnames = ["localhost", "127.0.0.1", "dev.local"]
    
    try:
        app.enable_development_mode(hostnames)
        print(f"✅ 开发模式启用成功，主机名: {hostnames}")
        print("⚠️  自签名证书不被浏览器信任，仅用于开发测试")
    except Exception as e:
        print(f"❌ 开发模式启用失败: {e}")
        return None
    
    # 添加基本路由
    create_basic_routes(app)
    
    return app


def run_server(app, host="127.0.0.1", port=8443):
    """运行服务器"""
    if app is None:
        print("❌ 应用配置失败，无法启动服务器")
        return
    
    print(f"\n🚀 启动服务器...")
    print(f"📍 应用名称: {app.name}")
    print(f"🌐 监听地址: {host}:{port}")
    
    try:
        print("✅ 服务器启动成功")
        print(f"\n📋 测试命令:")
        if hasattr(_cli_args, 'domain') and _cli_args.domain:
            print(f"curl -v -k --resolve {_cli_args.domain}:{port}:127.0.0.1 https://{_cli_args.domain}:{port}/")
            print(f"curl -v -k --resolve {_cli_args.domain}:{port}:127.0.0.1 https://{_cli_args.domain}:{port}/health")
        
        # 启动应用（阻塞模式，优雅处理退出信号）
        app.run(host="0.0.0.0", port=port, debug=True, blocking=True)
        
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")


def parse_args():
    """解析命令行参数"""
    parser = argparse.ArgumentParser(description="RAT Engine ACME 证书管理示例")
    parser.add_argument("--mode", choices=["development", "acme_staging", "acme_production", "production_certs"],
                       default=os.getenv("CERT_MODE", "development"),
                       help="运行模式 (默认: development)")
    parser.add_argument("--domain", help="域名 (ACME模式必需)")
    parser.add_argument("--email", help="ACME注册邮箱")
    parser.add_argument("--cloudflare-token", help="Cloudflare API Token")
    parser.add_argument("--port", type=int, default=8443, help="服务器端口 (默认: 8443)")
    parser.add_argument("--host", default="127.0.0.1", help="服务器主机 (默认: 127.0.0.1)")
    return parser.parse_args()


def main():
    """主函数"""
    args = parse_args()
    
    print("🔐 rat_engine ACME 证书管理示例")
    print("=" * 50)
    
    print(f"🎯 运行模式: {args.mode}")
    print(f"🌐 监听地址: {args.host}:{args.port}")
    
    # 根据模式设置全局变量，供示例函数使用
    global _cli_args
    _cli_args = args
    
    if args.mode == "acme_staging":
        app = example_acme_staging()
    elif args.mode == "acme_production":
        app = example_acme_production()
    elif args.mode == "production_certs":
        app = example_production_certs()
    elif args.mode == "development":
        app = example_development_mode()
    else:
        print(f"❌ 未知模式: {args.mode}")
        print("支持的模式: development, acme_staging, acme_production, production_certs")
        return
    
    # 运行服务器
    run_server(app, args.host, args.port)


# 全局变量存储命令行参数
_cli_args = None


if __name__ == "__main__":
    main()