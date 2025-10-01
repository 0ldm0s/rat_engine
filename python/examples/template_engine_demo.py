# -*- coding: utf-8 -*-
"""
RAT Engine 模板引擎演示

这个示例展示了如何使用RAT Engine的模板引擎功能，
包括基础模板渲染、全局变量设置和高级功能演示。
"""

import sys
import os
import datetime
import random

# 添加rat_engine到路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

from rat_engine import RatApp
import datetime

# 创建应用实例
app = RatApp(name="template_engine_demo")

# 全局模板变量
global_vars = {
    'app_name': 'RAT Engine 模板演示',
    'version': '1.0.0',
    'framework': 'RAT Engine',
    'build_time': datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S'),
    'auto_escape': True,
    'cache_enabled': True,
    'js_compatible': True
}

from rat_engine.templates import TemplateEngine

# 初始化模板引擎
template_engine = TemplateEngine(
    auto_escape=global_vars['auto_escape'],
    cache=global_vars['cache_enabled']
)

# 模拟数据
users_data = [
    {"id": 1, "name": "张三", "email": "zhangsan@example.com", "age": 25, "is_vip": True, "vip_level": "Gold", "active": True},
    {"id": 2, "name": "李四", "email": "lisi@example.com", "age": 30, "is_vip": False, "vip_level": None, "active": True},
    {"id": 3, "name": "王五", "email": "wangwu@example.com", "age": 28, "is_vip": True, "vip_level": "Silver", "active": False}
]

projects_data = [
    {"name": "电商平台", "status": "进行中", "progress": 75, "team_size": 8, "deadline": "2024-03-15"},
    {"name": "移动应用", "status": "已完成", "progress": 100, "team_size": 5, "deadline": "2024-01-20"},
    {"name": "数据分析", "status": "计划中", "progress": 10, "team_size": 3, "deadline": "2024-05-30"}
]
# 首页路由
@app.html("/")
def index(request_data):
    """首页 - 模板引擎功能展示"""
    template = """
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>{{app_name}}</title>
        <style>
            body {
                font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
                margin: 0;
                padding: 20px;
                background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                color: white;
                min-height: 100vh;
            }
            .container {
                max-width: 800px;
                margin: 0 auto;
                background: rgba(255,255,255,0.1);
                padding: 30px;
                border-radius: 15px;
                backdrop-filter: blur(10px);
                box-shadow: 0 8px 32px rgba(0,0,0,0.3);
            }
            h1 {
                text-align: center;
                margin-bottom: 30px;
                font-size: 2.5em;
                text-shadow: 2px 2px 4px rgba(0,0,0,0.3);
            }
            .info {
                background: rgba(255,255,255,0.1);
                padding: 20px;
                border-radius: 10px;
                margin-bottom: 30px;
                border: 1px solid rgba(255,255,255,0.2);
            }
            .demo-links {
                display: grid;
                grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
                gap: 20px;
                margin-top: 30px;
            }
            .demo-card {
                 background: rgba(255,255,255,0.2);
                 padding: 20px;
                 border-radius: 10px;
                 text-decoration: none;
                 color: white;
                 transition: all 0.3s ease;
                 border: 1px solid rgba(255,255,255,0.1);
             }
             .demo-card:hover {
                 transform: translateY(-5px);
                 background: rgba(255,255,255,0.3);
                 box-shadow: 0 10px 25px rgba(0,0,0,0.2);
             }
             .demo-title {
                 font-size: 1.3em;
                 font-weight: bold;
                 margin-bottom: 10px;
             }
             .demo-desc {
                 font-size: 0.9em;
                 opacity: 0.9;
                 line-height: 1.4;
             }
         </style>
     </head>
     <body>
         <div class="container">
             <h1>🎨 {{app_name}}</h1>
             
             <div class="info">
                 <h3>📋 应用信息</h3>
                 <p><strong>版本:</strong> {{version}}</p>
                 <p><strong>框架:</strong> {{framework}}</p>
                 <p><strong>构建时间:</strong> {{build_time}}</p>
                 <p><strong>功能特性:</strong></p>
                 <ul>
                     <li>自动转义: {% if auto_escape %}启用{% else %}禁用{% endif %}</li>
                     <li>模板缓存: {% if cache_enabled %}启用{% else %}禁用{% endif %}</li>
                     <li>JavaScript兼容: {% if js_compatible %}支持{% else %}不支持{% endif %}</li>
                 </ul>
             </div>
             
             <div class="demo-links">
                 <a href="/basic" class="demo-card">
                     <div class="demo-title">📝 基础模板</div>
                     <div class="demo-desc">展示变量替换、条件语句和循环的基本用法</div>
                 </a>
                 
                 <a href="/advanced" class="demo-card">
                     <div class="demo-title">🚀 高级功能</div>
                     <div class="demo-desc">嵌套数据结构和复杂模板逻辑</div>
                 </a>
                 
                 <a href="/api/template" class="demo-card">
                     <div class="demo-title">🔗 API 演示</div>
                     <div class="demo-desc">JSON API 数据返回示例</div>
                 </a>
             </div>
         </div>
     </body>
     </html>
     """
     
    return template_engine.render_string(template, global_vars)

# 基础模板演示路由
@app.html("/basic")
def basic_template(request_data):
    """基础模板演示 - 变量、条件、循环"""
    template = Template("""
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <title>基础模板演示 - {{app_name}}</title>
        <style>
            body {
                font-family: Arial, sans-serif;
                margin: 20px;
                background: #f5f5f5;
                line-height: 1.6;
            }
            .container {
                max-width: 800px;
                margin: 0 auto;
                background: white;
                padding: 30px;
                border-radius: 10px;
                box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            }
            .back-link {
                display: inline-block;
                margin-bottom: 20px;
                color: #007bff;
                text-decoration: none;
                padding: 8px 16px;
                border: 1px solid #007bff;
                border-radius: 5px;
                transition: all 0.3s ease;
            }
            .back-link:hover {
                background: #007bff;
                color: white;
            }
            h1 {
                color: #333;
                border-bottom: 2px solid #007bff;
                padding-bottom: 10px;
            }
            .section {
                margin: 25px 0;
                padding: 20px;
                background: #f8f9fa;
                border-left: 4px solid #007bff;
                border-radius: 5px;
            }
            .user-card {
                background: white;
                padding: 15px;
                margin: 10px 0;
                border-radius: 5px;
                box-shadow: 0 1px 3px rgba(0,0,0,0.1);
                border-left: 3px solid #28a745;
            }
            .status-active {
                color: #28a745;
                font-weight: bold;
            }
            .status-inactive {
                color: #dc3545;
                font-weight: bold;
            }
            code {
                background: #e9ecef;
                padding: 2px 6px;
                border-radius: 3px;
                font-family: 'Courier New', monospace;
                font-size: 0.9em;
            }
        </style>
    </head>
    <body>
        <div class="container">
            <a href="/" class="back-link">← 返回首页</a>
            
            <h1>📝 基础模板功能演示</h1>
            
            <div class="section">
                <h3>1. 变量替换</h3>
                <p><strong>用户名:</strong> {{user.name}}</p>
                <p><strong>邮箱:</strong> {{user.email}}</p>
                <p><strong>年龄:</strong> {{user.age}} 岁</p>
                <p><strong>用户ID:</strong> {{user.id}}</p>
            </div>
            
            <div class="section">
                <h3>2. 条件语句</h3>
                {% if user.is_vip %}
                    <p class="status-active">✅ VIP用户 - 享受专属服务</p>
                    {% if user.vip_level %}
                        <p>VIP等级: {{user.vip_level}}</p>
                    {% endif %}
                {% else %}
                    <p class="status-inactive">❌ 普通用户 - 升级VIP享受更多特权</p>
                {% endif %}
                
                <p>账户状态: 
                {% if user.active %}
                    <span class="status-active">活跃</span>
                {% else %}
                    <span class="status-inactive">非活跃</span>
                {% endif %}
                </p>
            </div>
            
            <div class="section">
                <h3>3. 循环遍历</h3>
                <h4>用户列表 ({{users|length}} 人):</h4>
                {% for u in users %}
                    <div class="user-card">
                        <strong>{{u.name}}</strong> ({{u.age}}岁)
                        <br>邮箱: {{u.email}}
                        <br>状态: 
                        <span class="{% if u.active %}status-active{% else %}status-inactive{% endif %}">
                            {% if u.active %}活跃{% else %}非活跃{% endif %}
                        </span>
                        {% if u.is_vip %}
                            <br>🌟 VIP用户
                        {% endif %}
                    </div>
                {% endfor %}
            </div>
            
            <div class="section">
                <h3>4. 模板语法说明</h3>
                <ul>
                    <li><code>{{variable}}</code> - 变量输出</li>
                    <li><code>{% if condition %}</code> - 条件判断</li>
                    <li><code>{% for item in list %}</code> - 循环遍历</li>
                    <li><code>{{variable|filter}}</code> - 过滤器应用</li>
                    <li><code>{{object.property}}</code> - 对象属性访问</li>
                        </ul>
                    </div>
                </div>
            </body>
            </html>
            """)
            
    # 使用第一个用户作为当前用户
    current_user = users_data[0]
    
    return template_engine.render(template, {
        'user': current_user,
        'users': users_data
    })
        
# 高级功能演示路由
@app.html("/advanced")
def advanced_template(request_data):
    """高级模板演示 - 过滤器、嵌套、复杂逻辑"""
    template = Template("""
    <!DOCTYPE html>
    <html lang="zh-CN">
    <head>
        <meta charset="UTF-8">
        <title>高级模板演示 - {{app_name}}</title>
        <style>
            body {
                font-family: Arial, sans-serif;
                margin: 20px;
                background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                min-height: 100vh;
                color: #333;
            }
            .container {
                max-width: 1000px;
                margin: 0 auto;
                background: rgba(255, 255, 255, 0.95);
                padding: 30px;
                border-radius: 15px;
                box-shadow: 0 10px 30px rgba(0,0,0,0.2);
                backdrop-filter: blur(10px);
            }
            .back-link {
                display: inline-block;
                margin-bottom: 20px;
                color: #667eea;
                text-decoration: none;
                padding: 10px 20px;
                border: 2px solid #667eea;
                border-radius: 25px;
                transition: all 0.3s ease;
                font-weight: bold;
            }
            .back-link:hover {
                background: #667eea;
                color: white;
                transform: translateY(-2px);
            }
            h1 {
                color: #333;
                text-align: center;
                margin-bottom: 30px;
                font-size: 2.5em;
                background: linear-gradient(45deg, #667eea, #764ba2);
                -webkit-background-clip: text;
                -webkit-text-fill-color: transparent;
                background-clip: text;
            }
            .feature-grid {
                display: grid;
                grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
                gap: 20px;
                margin: 30px 0;
            }
            .feature-card {
                background: white;
                padding: 25px;
                border-radius: 10px;
                box-shadow: 0 5px 15px rgba(0,0,0,0.1);
                border-left: 5px solid #667eea;
                transition: transform 0.3s ease;
            }
            .feature-card:hover {
                transform: translateY(-5px);
            }
            .feature-card h3 {
                color: #667eea;
                margin-bottom: 15px;
                font-size: 1.3em;
            }
            .stats-container {
                display: flex;
                justify-content: space-around;
                margin: 30px 0;
                flex-wrap: wrap;
            }
            .stat-item {
                text-align: center;
                padding: 20px;
                background: linear-gradient(45deg, #667eea, #764ba2);
                color: white;
                border-radius: 10px;
                margin: 10px;
                min-width: 150px;
                box-shadow: 0 5px 15px rgba(0,0,0,0.2);
            }
            .stat-number {
                font-size: 2em;
                font-weight: bold;
                display: block;
            }
            .project-list {
                margin: 20px 0;
            }
            .project-item {
                background: #f8f9fa;
                padding: 20px;
                margin: 15px 0;
                border-radius: 8px;
                border-left: 4px solid #28a745;
                transition: all 0.3s ease;
            }
            .project-item:hover {
                background: #e9ecef;
                transform: translateX(5px);
            }
            .project-status {
                display: inline-block;
                padding: 5px 12px;
                border-radius: 20px;
                font-size: 0.8em;
                font-weight: bold;
                text-transform: uppercase;
            }
            .status-active { background: #d4edda; color: #155724; }
            .status-pending { background: #fff3cd; color: #856404; }
            .status-completed { background: #d1ecf1; color: #0c5460; }
            .progress-bar {
                width: 100%;
                height: 8px;
                background: #e9ecef;
                border-radius: 4px;
                margin: 10px 0;
                overflow: hidden;
            }
            .progress-fill {
                height: 100%;
                background: linear-gradient(90deg, #28a745, #20c997);
                transition: width 0.3s ease;
            }
            .tag {
                display: inline-block;
                background: #667eea;
                color: white;
                padding: 3px 8px;
                border-radius: 12px;
                font-size: 0.7em;
                margin: 2px;
            }
        </style>
    </head>
    <body>
        <div class="container">
            <a href="/" class="back-link">← 返回首页</a>
            
            <h1>🚀 高级模板功能演示</h1>
            
            <!-- 统计信息 -->
            <div class="stats-container">
                <div class="stat-item">
                    <span class="stat-number">{{projects|length}}</span>
                    <span>项目总数</span>
                </div>
                <div class="stat-item">
                    <span class="stat-number">{{active_count}}</span>
                    <span>活跃项目</span>
                </div>
                <div class="stat-item">
                    <span class="stat-number">{{avg_progress}}%</span>
                    <span>平均进度</span>
                </div>
                <div class="stat-item">
                    <span class="stat-number">{{projects|length}}</span>
                    <span>团队项目</span>
                </div>
            </div>
            
            <!-- 功能演示网格 -->
            <div class="feature-grid">
                <div class="feature-card">
                    <h3>🔢 数据统计</h3>
                    <p>总预算: ${{total_budget}}</p>
                    <p>平均预算: ${{avg_budget}}</p>
                    <p>最高预算: ${{max_budget}}</p>
                </div>
                
                <div class="feature-card">
                    <h3>📊 项目状态</h3>
                    <p>活跃: {{active_count}} 个</p>
                    <p>已完成: {{completed_count}} 个</p>
                    <p>待处理: {{pending_count}} 个</p>
                </div>
                
                <div class="feature-card">
                    <h3>🎯 进度分析</h3>
                    <p>最高进度: {{max_progress}}%</p>
                    <p>最低进度: {{min_progress}}%</p>
                    <p>平均进度: {{avg_progress}}%</p>
                </div>
                
                <div class="feature-card">
                    <h3>🏷️ 技术标签</h3>
                    {% for tag in all_tags %}
                        <span class="tag">{{tag}}</span>
                    {% endfor %}
                </div>
            </div>
            
            <!-- 项目列表 -->
            <div class="project-list">
                <h2>📋 项目列表</h2>
                {% for project in projects %}
                    <div class="project-item">
                        <div style="display: flex; justify-content: space-between; align-items: center;">
                            <h4>{{project.name}}</h4>
                            <span class="project-status status-{{project.status}}">{{project.status}}</span>
                        </div>
                        
                        <p>{{project.description}}</p>
                        
                        <div class="progress-bar">
                            <div class="progress-fill" style="width: {{project.progress}}%"></div>
                        </div>
                        <small>进度: {{project.progress}}% | 预算: ${{project.budget}}</small>
                        
                        <div style="margin-top: 10px;">
                            {% for tag in project.tags %}
                                <span class="tag">{{tag}}</span>
                            {% endfor %}
                        </div>
                    </div>
                {% endfor %}
            </div>
            
            <!-- 条件过滤演示 -->
            <div class="feature-card">
                <h3>🔍 条件过滤演示</h3>
                <h4>高进度项目 (进度 > 70%):</h4>
                <ul>
                    {% for project in projects %}
                        {% if project.progress > 70 %}
                            <li>{{project.name}} - {{project.progress}}%</li>
                        {% endif %}
                    {% endfor %}
                </ul>
                
                <h4>大预算项目 (预算 > $80000):</h4>
                <ul>
                    {% for project in projects %}
                        {% if project.budget > 80000 %}
                            <li>{{project.name}} - ${{project.budget}}</li>
                        {% endif %}
                    {% endfor %}
                </ul>
            </div>
        </div>
    </body>
    </html>
    """)
    
    # 项目数据
    projects = [
        {
            'name': '电商平台重构',
            'status': 'active',
            'progress': 85,
            'budget': 120000,
            'description': '基于RAT Engine的现代化电商平台',
            'tags': ['Python', 'RAT Engine', 'PostgreSQL', 'Redis']
        },
        {
            'name': '移动端应用',
            'status': 'completed',
            'progress': 100,
            'budget': 75000,
            'description': '跨平台移动应用开发',
            'tags': ['React Native', 'TypeScript', 'MongoDB']
        },
        {
            'name': '数据分析平台',
            'status': 'pending',
            'progress': 25,
            'budget': 95000,
            'description': '企业级数据分析和可视化平台',
            'tags': ['Python', 'Pandas', 'D3.js', 'Elasticsearch']
        },
        {
            'name': 'AI聊天机器人',
            'status': 'active',
            'progress': 60,
            'budget': 85000,
            'description': '智能客服聊天机器人系统',
            'tags': ['Python', 'TensorFlow', 'NLP', 'Docker']
        }
    ]
    
    # 计算统计数据
    active_count = len([p for p in projects if p['status'] == 'active'])
    completed_count = len([p for p in projects if p['status'] == 'completed'])
    pending_count = len([p for p in projects if p['status'] == 'pending'])
    
    total_budget = sum(p['budget'] for p in projects)
    avg_budget = total_budget // len(projects)
    max_budget = max(p['budget'] for p in projects)
    
    avg_progress = sum(p['progress'] for p in projects) // len(projects)
    max_progress = max(p['progress'] for p in projects)
    min_progress = min(p['progress'] for p in projects)
    
    # 收集所有标签
    all_tags = set()
    for project in projects:
        all_tags.update(project['tags'])
    all_tags = sorted(list(all_tags))
    
    data = {
        'projects': projects,
        'active_count': active_count,
        'completed_count': completed_count,
        'pending_count': pending_count,
        'total_budget': total_budget,
        'avg_budget': avg_budget,
        'max_budget': max_budget,
        'avg_progress': avg_progress,
        'max_progress': max_progress,
        'min_progress': min_progress,
        'all_tags': all_tags
    }
    
    return template_engine.render(template, data)
        
# API演示路由
@app.json("/api/template")
def api_template_demo(request_data):
    """API模板演示 - JSON数据展示"""
    return {
        "status": "success",
        "message": "RAT Engine 模板引擎 API 演示",
        "data": {
            "app_info": {
                "name": "RAT Engine 模板演示",
                "version": "1.0.0",
                "framework": "RAT Engine",
                "build_time": datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')
            },
            "features": {
                "template_engine": True,
                "auto_escape": True,
                "cache_enabled": True,
                "js_compatible": True,
                "performance_optimized": True
            },
            "users": users_data,
            "projects": projects_data,
            "statistics": {
                "total_users": len(users_data),
                "total_projects": len(projects_data),
                "active_users": len([u for u in users_data if u.get('active', False)]),
                "vip_users": len([u for u in users_data if u.get('is_vip', False)]),
                "completed_projects": len([p for p in projects_data if p.get('status') == '已完成'])
            }
        },
        "timestamp": datetime.datetime.now().isoformat(),
        "request_info": {
            "method": "GET",
            "endpoint": "/api/template",
            "content_type": "application/json"
        }
    }
    
def main():
    """主函数 - 启动模板引擎演示应用"""
    print("🚀 启动 RAT Engine 模板引擎演示...")
    print("📍 访问地址: http://127.0.0.1:3000")
    print("📋 可用路由:")
    print("   🏠 首页: /")
    print("   📝 基础功能: /basic")
    print("   🚀 高级功能: /advanced")
    print("   🔌 API演示: /api/template")
    print("\n" + "="*50)
    
    try:
        # 启动服务器（阻塞模式，优雅处理退出信号）
        app.run(host="127.0.0.1", port=3000, debug=True, blocking=True)
    except Exception as e:
        print(f"❌ 服务器启动失败: {e}")

if __name__ == "__main__":
    main()