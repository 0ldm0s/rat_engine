# -*- coding: utf-8 -*-
"""
Web应用演示 - 模仿经典Web框架风格的示例
展示如何使用RAT Engine构建现代Web应用
"""

import json
import random
from rat_engine import RatApp

# 创建应用实例
app = RatApp(name="web_app_demo")

# 模拟数据库
users_db = [
    {"id": 1, "name": "张三", "email": "zhangsan@example.com", "age": 25},
    {"id": 2, "name": "李四", "email": "lisi@example.com", "age": 30},
    {"id": 3, "name": "王五", "email": "wangwu@example.com", "age": 28}
]

products_db = [
    {"id": 1, "name": "笔记本电脑", "price": 5999.99, "category": "电子产品"},
    {"id": 2, "name": "无线鼠标", "price": 99.99, "category": "电子产品"},
    {"id": 3, "name": "机械键盘", "price": 299.99, "category": "电子产品"}
]

# 首页路由
@app.html("/")
def index(request_data):
    """首页 - 显示欢迎信息"""
    return """
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>RAT Engine Web应用演示</title>
        <style>
            body { font-family: Arial, sans-serif; margin: 40px; background-color: #f5f5f5; }
            .container { max-width: 800px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
            h1 { color: #333; text-align: center; }
            .nav { margin: 20px 0; text-align: center; }
            .nav a { margin: 0 15px; padding: 10px 20px; background: #007bff; color: white; text-decoration: none; border-radius: 5px; }
            .nav a:hover { background: #0056b3; }
            .update-btn { margin: 0 15px; padding: 10px 20px; background: #28a745; color: white; border: none; border-radius: 5px; cursor: pointer; }
            .update-btn:hover { background: #218838; }
            .feature { margin: 20px 0; padding: 15px; border-left: 4px solid #007bff; background: #f8f9fa; }
        </style>
        <script>
            function randomUpdate() {
                fetch('/api/random-update', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json'
                    }
                })
                .then(response => response.json())
                .then(data => {
                    if (data.success) {
                        alert(`${data.message}\n更新了 ${data.updated.users_count} 个用户和 ${data.updated.products_count} 个产品`);
                        // 可选：刷新页面查看更新效果
                        // window.location.reload();
                    } else {
                        alert('更新失败: ' + (data.error || '未知错误'));
                    }
                })
                .catch(error => {
                    alert('网络错误: ' + error.message);
                });
            }
        </script>
    </head>
    <body>
        <div class="container">
            <h1>🚀 欢迎使用 RAT Engine</h1>
            <p>这是一个现代化的Web应用演示，展示了RAT Engine的强大功能。</p>
            
            <div class="nav">
                <a href="/users">用户管理</a>
                <a href="/products">产品列表</a>
                <a href="/api/status">API状态</a>
                <a href="/about">关于我们</a>
                <button class="update-btn" onclick="randomUpdate()">🎲 随机更新数据</button>
            </div>
            
            <div class="feature">
                <h3>🎯 核心特性</h3>
                <ul>
                    <li>高性能Rust后端</li>
                    <li>简洁的Python API</li>
                    <li>现代化的Web框架设计</li>
                    <li>内置JSON和HTML支持</li>
                </ul>
            </div>
        </div>
    </body>
    </html>
    """

# 用户列表页面
@app.html("/users")
def users_page(request_data):
    """用户管理页面"""
    users_html = ""
    for user in users_db:
        users_html += f"""
        <tr>
            <td>{user['id']}</td>
            <td>{user['name']}</td>
            <td>{user['email']}</td>
            <td>{user['age']}</td>
            <td>
                <a href="/api/users/{user['id']}" style="color: #007bff;">查看详情</a>
            </td>
        </tr>
        """
    
    return f"""
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <title>用户管理 - RAT Engine</title>
        <style>
            body {{ font-family: Arial, sans-serif; margin: 40px; background-color: #f5f5f5; }}
            .container {{ max-width: 900px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
            table {{ width: 100%; border-collapse: collapse; margin: 20px 0; }}
            th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }}
            th {{ background-color: #f8f9fa; font-weight: bold; }}
            .back-btn {{ display: inline-block; margin-bottom: 20px; padding: 10px 20px; background: #6c757d; color: white; text-decoration: none; border-radius: 5px; }}
            .refresh-btn {{ margin-left: 10px; padding: 10px 20px; background: #28a745; color: white; border: none; border-radius: 5px; cursor: pointer; }}
            .refresh-btn:hover {{ background: #218838; }}
        </style>
    </head>
    <body>
        <div class="container">
            <a href="/" class="back-btn">← 返回首页</a>
            <button class="refresh-btn" onclick="window.location.reload()">🔄 刷新数据</button>
            <h1>👥 用户管理</h1>
            <table>
                <thead>
                    <tr>
                        <th>ID</th>
                        <th>姓名</th>
                        <th>邮箱</th>
                        <th>年龄</th>
                        <th>操作</th>
                    </tr>
                </thead>
                <tbody>
                    {users_html}
                </tbody>
            </table>
        </div>
    </body>
    </html>
    """

# 产品列表页面
@app.html("/products")
def products_page(request_data):
    """产品列表页面"""
    products_html = ""
    for product in products_db:
        products_html += f"""
        <div class="product-card">
            <h3>{product['name']}</h3>
            <p class="price">¥{product['price']}</p>
            <p class="category">{product['category']}</p>
            <button onclick="viewProduct({product['id']})">查看详情</button>
        </div>
        """
    
    return f"""
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <title>产品列表 - RAT Engine</title>
        <style>
            body {{ font-family: Arial, sans-serif; margin: 40px; background-color: #f5f5f5; }}
            .container {{ max-width: 1000px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }}
            .products-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 20px; margin: 20px 0; }}
            .product-card {{ border: 1px solid #ddd; border-radius: 8px; padding: 20px; text-align: center; background: #f8f9fa; }}
            .price {{ font-size: 1.2em; font-weight: bold; color: #e74c3c; }}
            .category {{ color: #6c757d; font-size: 0.9em; }}
            button {{ padding: 8px 16px; background: #007bff; color: white; border: none; border-radius: 4px; cursor: pointer; }}
            .back-btn {{ display: inline-block; margin-bottom: 20px; padding: 10px 20px; background: #6c757d; color: white; text-decoration: none; border-radius: 5px; }}
            .refresh-btn {{ margin-left: 10px; padding: 10px 20px; background: #28a745; color: white; border: none; border-radius: 5px; cursor: pointer; }}
            .refresh-btn:hover {{ background: #218838; }}
        </style>
        <script>
            function viewProduct(id) {{
                fetch(`/api/products/${{id}}`)
                    .then(response => response.json())
                    .then(data => {{
                        if (data.success && data.data) {{
                            alert(`产品详情:\n名称: ${{data.data.name}}\n价格: ¥${{data.data.price}}\n分类: ${{data.data.category}}`);
                        }} else {{
                            alert(`错误: ${{data.message || data.error || '获取产品信息失败'}}`);
                        }}
                    }})
                    .catch(error => {{
                        alert(`网络错误: ${{error.message}}`);
                    }});
            }}
        </script>
    </head>
    <body>
        <div class="container">
            <a href="/" class="back-btn">← 返回首页</a>
            <button class="refresh-btn" onclick="window.location.reload()">🔄 刷新数据</button>
            <h1>🛍️ 产品列表</h1>
            <div class="products-grid">
                {products_html}
            </div>
        </div>
    </body>
    </html>
    """

# 关于页面
@app.html("/about")
def about_page(request_data):
    """关于页面"""
    return """
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <title>关于我们 - RAT Engine</title>
        <style>
            body { font-family: Arial, sans-serif; margin: 40px; background-color: #f5f5f5; }
            .container { max-width: 800px; margin: 0 auto; background: white; padding: 30px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
            .back-btn { display: inline-block; margin-bottom: 20px; padding: 10px 20px; background: #6c757d; color: white; text-decoration: none; border-radius: 5px; }
            .tech-stack { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 15px; margin: 20px 0; }
            .tech-item { padding: 15px; background: #e3f2fd; border-radius: 5px; text-align: center; }
        </style>
    </head>
    <body>
        <div class="container">
            <a href="/" class="back-btn">← 返回首页</a>
            <h1>🚀 关于 RAT Engine</h1>
            <p>RAT Engine 是一个现代化的Web应用框架，结合了Rust的高性能和Python的易用性。</p>
            
            <h2>🛠️ 技术栈</h2>
            <div class="tech-stack">
                <div class="tech-item">
                    <h3>🦀 Rust</h3>
                    <p>高性能后端引擎</p>
                </div>
                <div class="tech-item">
                    <h3>🐍 Python</h3>
                    <p>简洁的API接口</p>
                </div>
                <div class="tech-item">
                    <h3>🌐 Web</h3>
                    <p>现代化前端支持</p>
                </div>
                <div class="tech-item">
                    <h3>⚡ 高性能</h3>
                    <p>毫秒级响应时间</p>
                </div>
            </div>
            
            <h2>✨ 特性</h2>
            <ul>
                <li>🚀 极致性能：基于Rust的高性能Web服务器</li>
                <li>🐍 简单易用：Python风格的API设计</li>
                <li>🔧 灵活配置：支持多种部署方式</li>
                <li>📦 开箱即用：内置常用功能模块</li>
                <li>🛡️ 类型安全：Rust的内存安全保证</li>
            </ul>
        </div>
    </body>
    </html>
    """

# API路由 - 系统状态
@app.json("/api/status")
def api_status(request_data):
    """API状态检查"""
    return {
        "status": "healthy",
        "version": "1.0.0",
        "engine": "RAT Engine",
        "timestamp": "2024-01-01T00:00:00Z",
        "endpoints": {
            "users": "/api/users",
            "products": "/api/products",
            "health": "/api/status"
        },
        "stats": {
            "total_users": len(users_db),
            "total_products": len(products_db),
            "uptime": "24h 30m 15s"
        }
    }

# API路由 - 获取所有用户
@app.json("/api/users")
def api_users_list(request_data):
    """获取用户列表API"""
    return {
        "success": True,
        "data": users_db,
        "total": len(users_db),
        "message": "用户列表获取成功"
    }

# API路由 - 获取单个用户（动态路由）
@app.json("/api/users/<id>")
def api_user_detail(request_data):
    """获取指定用户的详细信息"""
    user_id = request_data.get('path_params', {}).get('id', 'unknown')
    
    # 尝试将用户ID转换为整数
    try:
        user_id_int = int(user_id)
        user = next((u for u in users_db if u["id"] == user_id_int), None)
        if user:
            return {
                "success": True,
                "data": user,
                "message": f"用户 {user_id} 信息获取成功"
            }
    except ValueError:
        pass
    
    return {
        "success": False,
        "error": "用户不存在",
        "code": 404,
        "message": f"用户 {user_id} 不存在"
    }

# API路由 - 获取所有产品
@app.json("/api/products")
def api_products_list(request_data):
    """获取产品列表API"""
    return {
        "success": True,
        "data": products_db,
        "total": len(products_db),
        "message": "产品列表获取成功"
    }

# API路由 - 获取单个产品（动态路由）
@app.json("/api/products/<id>")
def api_product_detail(request_data):
    """获取指定产品的详细信息"""
    product_id = request_data.get('path_params', {}).get('id', 'unknown')
    
    # 尝试将产品ID转换为整数
    try:
        product_id_int = int(product_id)
        product = next((p for p in products_db if p["id"] == product_id_int), None)
        if product:
            return {
                "success": True,
                "data": product,
                "message": f"产品 {product_id} 信息获取成功"
            }
    except ValueError:
        pass
    
    return {
        "success": False,
        "error": "产品不存在",
        "code": 404,
        "message": f"产品 {product_id} 不存在"
    }

# API路由 - 用户资料（动态路由）
@app.json("/api/users/<id>/profile")
def api_user_profile(request_data):
    """获取指定用户的详细资料"""
    user_id = request_data.get('path_params', {}).get('id', 'unknown')
    
    try:
        user_id_int = int(user_id)
        user = next((u for u in users_db if u["id"] == user_id_int), None)
        if user:
            return {
                "success": True,
                "data": {
                    "user_id": user_id_int,
                    "profile": {
                        "name": user["name"],
                        "email": user["email"],
                        "age": user["age"],
                        "created_at": "2024-01-01",
                        "last_login": "2024-01-15",
                        "status": "active"
                    }
                },
                "message": f"用户 {user_id} 资料获取成功"
            }
    except ValueError:
        pass
    
    return {
        "success": False,
        "error": "用户不存在",
        "code": 404,
        "message": f"用户 {user_id} 不存在"
    }

# API路由 - 产品分类（动态路由）
@app.json("/api/products/category/<category>")
def api_products_by_category(request_data):
    """根据分类获取产品列表"""
    category = request_data.get('path_params', {}).get('category', 'unknown')
    
    # 过滤指定分类的产品
    filtered_products = [p for p in products_db if p["category"] == category]
    
    return {
        "success": True,
        "data": filtered_products,
        "total": len(filtered_products),
        "category": category,
        "message": f"分类 '{category}' 下的产品获取成功"
    }

# API路由 - 搜索功能
@app.json("/api/search")
def api_search(request_data):
    """搜索API - 支持用户和产品搜索"""
    # 这里可以解析查询参数，但目前简化处理
    return {
        "success": True,
        "results": {
            "users": users_db[:2],  # 返回前2个用户作为示例
            "products": products_db[:2]  # 返回前2个产品作为示例
        },
        "total_results": 4,
        "message": "搜索完成"
    }

# API路由 - 随机更新数据库（POST）
@app.json("/api/random-update", methods=["POST"])
def api_random_update(request_data):
    """随机更新模拟数据库"""
    # 随机更新用户年龄
    for user in users_db:
        user["age"] = random.randint(20, 50)
    
    # 随机更新产品价格
    for product in products_db:
        # 在原价格基础上随机变动 ±20%
        base_price = product["price"]
        variation = random.uniform(0.8, 1.2)
        product["price"] = round(base_price * variation, 2)
    
    return {
        "success": True,
        "message": "数据库已随机更新！",
        "updated": {
            "users_count": len(users_db),
            "products_count": len(products_db)
        }
    }

def main():
    """主函数 - 启动Web应用"""
    print("🚀 启动 RAT Engine Web应用演示...")
    print("📍 访问地址: http://127.0.0.1:3000")
    print("📋 可用路由:")
    print("   🏠 首页: /")
    print("   👥 用户管理: /users")
    print("   🛍️ 产品列表: /products")
    print("   ℹ️ 关于页面: /about")
    print("   🔌 API状态: /api/status")
    print("   📊 用户API: /api/users")
    print("   🛒 产品API: /api/products")
    print("   🔍 搜索API: /api/search")
    print("   🎲 随机更新API: POST /api/random-update")
    print("\n" + "="*50)
    
    try:
        # 启动服务器（阻塞模式，优雅处理退出信号）
        app.run(host="127.0.0.1", port=3000, debug=True, blocking=True)
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")

if __name__ == "__main__":
    main()