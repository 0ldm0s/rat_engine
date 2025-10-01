#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
mTLS (双向 TLS 认证) 示例

本示例展示如何使用 RAT Engine 的 mTLS 功能：
1. 自签名模式：服务端和客户端都使用自签名证书
2. ACME 混合模式：服务端使用 ACME 证书，客户端使用自签名证书

测试命令：
# 自签名模式测试（需要客户端证书）
curl -v -k --cert client.crt --key client.key --resolve gs1.sukiyaki.su:8443:127.0.0.1 https://gs1.sukiyaki.su:8443/health

# 查看服务器证书信息
openssl s_client -connect 127.0.0.1:8443 -servername gs1.sukiyaki.su -showcerts
"""

import asyncio
import sys
import os
import time
from pathlib import Path

# 添加 rat_engine 到 Python 路径
sys.path.insert(0, str(Path(__file__).parent.parent))

from rat_engine import (
    RatApp,
    ServerConfig,
    CertManagerConfig,
    HttpResponse
)





async def run_mtls_self_signed_example():
    """运行自签名 mTLS 示例"""
    print("🔐 启动自签名 mTLS 示例...")
    
    # 创建应用实例
    app = RatApp(name="mtls_self_signed_demo")
    
    # 配置证书管理器 - 自签名模式
    cert_config = CertManagerConfig.mtls_self_signed_config(
        auto_generate=True,
        client_cert_subject="RAT Engine mTLS Client",
        client_cert_path="./certs/client.crt",
        client_key_path="./certs/client.key",
        cert_dir="./certs"
    )
    
    # 配置 mTLS
    app.configure_mtls(cert_config)
    
    # 使用装饰器注册路由
    @app.json("/")
    def root_handler(request_data):
        return {
            "message": "Hello from mTLS server!",
            "client_cert": "验证成功" if hasattr(request_data, 'client_cert') else "未提供"
        }
    
    @app.json("/hello")
    def hello_route(request_data):
        return {
            "message": "Hello from mTLS server!",
            "client_cert": "验证成功" if hasattr(request_data, 'client_cert') else "未提供"
        }
    
    @app.json("/health")
    def health_route(request_data):
        return {
            "status": "healthy",
            "mtls": "enabled",
            "timestamp": time.time()
        }
    
    @app.json("/cert-info")
    def cert_info_route(request_data):
        return {
            "server_cert": "mTLS enabled",
            "client_auth": "required",
            "mode": "self_signed or acme_mixed"
        }
    
    print("🚀 mTLS 服务器启动在 https://127.0.0.1:8443")
    print("📋 可用路由:")
    print("   GET /        - Hello 消息")
    print("   GET /hello   - Hello 消息")
    print("   GET /health  - 健康检查")
    print("   GET /cert-info - 证书信息")
    print("")
    print("🔑 客户端证书已生成:")
    print("   证书: ./certs/client.crt")
    print("   私钥: ./certs/client.key")
    print("")
    print("🧪 测试命令:")
    print("   curl -v -k --cert certs/client.crt --key certs/client.key --resolve gs1.sukiyaki.su:8443:127.0.0.1 https://gs1.sukiyaki.su:8443/health")
    print("")
    print("按 Ctrl+C 停止服务器")
    
    try:
        # 启动服务器（阻塞模式，优雅处理退出信号）
        app.run(host="127.0.0.1", port=8443, debug=True, blocking=True)
    except KeyboardInterrupt:
        print("\n👋 服务器已停止")
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")


async def run_mtls_acme_mixed_example():
    """运行 ACME 混合 mTLS 示例"""
    print("🌐 启动 ACME 混合 mTLS 示例...")
    
    # 检查环境变量
    acme_email = os.getenv("ACME_EMAIL")
    cloudflare_token = os.getenv("CLOUDFLARE_API_TOKEN")
    
    if not acme_email or not cloudflare_token:
        print("❌ 错误: 需要设置环境变量:")
        print("   export ACME_EMAIL=your-email@example.com")
        print("   export CLOUDFLARE_API_TOKEN=your-cloudflare-token")
        return
    
    # 创建应用实例
    app = RatApp(name="mtls_acme_mixed_demo")
    
    # 配置证书管理器 - ACME 混合模式
    cert_config = CertManagerConfig.mtls_acme_mixed_config(
        email=acme_email,
        production=False,  # 使用沙盒环境
        cloudflare_token=cloudflare_token,
        auto_generate_client=True,
        client_cert_subject="RAT Engine ACME mTLS Client",
        cert_dir="./acme_certs"
    )
    
    # 配置 ACME 证书
    app.configure_acme_certs(["gs1.sukiyaki.su"], cert_config)
    
    # 配置 mTLS
    app.configure_mtls(cert_config)
    
    # 使用装饰器注册路由
    @app.json("/")
    def root_handler(request_data):
        return {
            "message": "Hello from mTLS server!",
            "client_cert": "验证成功" if hasattr(request_data, 'client_cert') else "未提供"
        }
    
    @app.json("/hello")
    def hello_route(request_data):
        return {
            "message": "Hello from mTLS server!",
            "client_cert": "验证成功" if hasattr(request_data, 'client_cert') else "未提供"
        }
    
    @app.json("/health")
    def health_route(request_data):
        return {
            "status": "healthy",
            "mtls": "enabled",
            "timestamp": time.time()
        }
    
    @app.json("/cert-info")
    def cert_info_route(request_data):
        return {
            "server_cert": "mTLS enabled",
            "client_auth": "required",
            "mode": "self_signed or acme_mixed"
        }
    
    print("🚀 ACME mTLS 服务器启动在 https://127.0.0.1:8443")
    print("📋 可用路由:")
    print("   GET /        - Hello 消息")
    print("   GET /hello   - Hello 消息")
    print("   GET /health  - 健康检查")
    print("   GET /cert-info - 证书信息")
    print("")
    print("🔑 客户端证书已生成:")
    print("   证书: ./acme_client.crt")
    print("   私钥: ./acme_client.key")
    print("")
    print("🧪 测试命令:")
    print("   curl -v -k --cert acme_client.crt --key acme_client.key --resolve gs1.sukiyaki.su:8443:127.0.0.1 https://gs1.sukiyaki.su:8443/health")
    print("")
    print("按 Ctrl+C 停止服务器")
    
    try:
        # 启动服务器（阻塞模式，优雅处理退出信号）
        app.run(host="127.0.0.1", port=8443, debug=True, blocking=True)
    except KeyboardInterrupt:
        print("\n👋 服务器已停止")
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")


if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="mTLS 示例")
    parser.add_argument(
        "--mode",
        choices=["self_signed", "acme_mixed"],
        default="self_signed",
        help="mTLS 模式 (默认: self_signed)"
    )
    
    args = parser.parse_args()
    
    if args.mode == "self_signed":
        asyncio.run(run_mtls_self_signed_example())
    elif args.mode == "acme_mixed":
        asyncio.run(run_mtls_acme_mixed_example())