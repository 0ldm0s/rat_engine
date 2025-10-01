# RAT Engine 黑名单功能使用指南

## 概述

RAT Engine 提供了完整的 IP 黑名单功能，允许您动态管理被阻断的 IP 地址。当 IP 地址被添加到黑名单后，来自该 IP 的所有请求都会被自动拒绝，返回 403 Forbidden 状态码。

## 功能特性

- ✅ **动态管理**: 运行时添加/移除黑名单 IP
- ✅ **实时生效**: 黑名单更改立即生效，无需重启服务器
- ✅ **IPv4/IPv6 支持**: 同时支持 IPv4 和 IPv6 地址
- ✅ **线程安全**: 多线程环境下安全使用
- ✅ **内存高效**: 使用 HashSet 实现快速查找

## API 接口

### 1. 添加 IP 到黑名单

```python
engine = rat_engine.PyRatEngine()

# 添加单个 IP
result = engine.add_to_blacklist("192.168.1.100")
if result:
    print("IP 已成功添加到黑名单")

# 添加多个 IP
ips_to_block = ["10.0.0.1", "172.16.0.1", "203.0.113.1"]
for ip in ips_to_block:
    try:
        engine.add_to_blacklist(ip)
        print(f"已阻断: {ip}")
    except ValueError as e:
        print(f"添加失败: {e}")
```

### 2. 从黑名单移除 IP

```python
# 移除单个 IP
result = engine.remove_from_blacklist("192.168.1.100")
if result:
    print("IP 已从黑名单移除")

# 批量移除
ips_to_unblock = ["10.0.0.1", "172.16.0.1"]
for ip in ips_to_unblock:
    try:
        engine.remove_from_blacklist(ip)
        print(f"已解除阻断: {ip}")
    except ValueError as e:
        print(f"移除失败: {e}")
```

### 3. 检查 IP 是否在黑名单中

```python
# 检查单个 IP
ip = "192.168.1.100"
try:
    is_blocked = engine.is_blacklisted(ip)
    if is_blocked:
        print(f"{ip} 已被阻断")
    else:
        print(f"{ip} 允许访问")
except ValueError as e:
    print(f"IP 格式无效: {e}")

# 批量检查
ips_to_check = ["192.168.1.1", "10.0.0.1", "8.8.8.8"]
for ip in ips_to_check:
    try:
        status = "阻断" if engine.is_blacklisted(ip) else "允许"
        print(f"{ip}: {status}")
    except ValueError:
        print(f"{ip}: 无效IP格式")
```

### 4. 获取完整黑名单

```python
# 获取所有被阻断的 IP
blacklist = engine.get_blacklist()
print(f"当前黑名单包含 {len(blacklist)} 个IP:")
for i, ip in enumerate(blacklist, 1):
    print(f"{i}. {ip}")

# 检查黑名单是否为空
if not blacklist:
    print("黑名单为空，所有IP都允许访问")
```

## 完整示例

```python
import rat_engine
import time
import threading

def main():
    # 创建引擎实例
    engine = rat_engine.PyRatEngine(host="127.0.0.1", port=3000)
    
    # 注册处理函数
    def handler(request):
        return rat_engine.HttpResponse(
            status=200,
            headers={"Content-Type": "application/json"},
            body=f'{{"client_ip": "{request.real_ip}", "message": "访问成功"}}'
        )
    
    engine.register_handler("/", handler)
    
    # 启动服务器（后台线程）
    server_thread = threading.Thread(target=engine.run, daemon=True)
    server_thread.start()
    time.sleep(1)  # 等待服务器启动
    
    # 黑名单管理
    print("=== 黑名单管理演示 ===")
    
    # 添加恶意IP到黑名单
    malicious_ips = ["192.168.1.100", "10.0.0.50"]
    for ip in malicious_ips:
        engine.add_to_blacklist(ip)
        print(f"已阻断恶意IP: {ip}")
    
    # 显示当前黑名单
    blacklist = engine.get_blacklist()
    print(f"\n当前黑名单: {blacklist}")
    
    # 检查特定IP状态
    test_ips = ["192.168.1.100", "192.168.1.1", "8.8.8.8"]
    print("\nIP访问状态:")
    for ip in test_ips:
        status = "🚫 阻断" if engine.is_blacklisted(ip) else "✅ 允许"
        print(f"{ip}: {status}")
    
    # 解除某个IP的阻断
    ip_to_unblock = "192.168.1.100"
    engine.remove_from_blacklist(ip_to_unblock)
    print(f"\n已解除 {ip_to_unblock} 的阻断")
    
    # 最终状态
    final_blacklist = engine.get_blacklist()
    print(f"最终黑名单: {final_blacklist}")
    
    print("\n服务器运行中，可以使用 curl 测试:")
    print("curl http://127.0.0.1:3000/")
    
    # 保持运行
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n服务器已停止")

if __name__ == "__main__":
    main()
```

## 错误处理

### 常见错误类型

1. **无效IP格式**
```python
try:
    engine.add_to_blacklist("invalid-ip")
except ValueError as e:
    print(f"IP格式错误: {e}")
    # 输出: IP格式错误: Invalid IP address: invalid-ip
```

2. **重复添加IP**
```python
# 重复添加同一个IP不会报错，但会返回成功状态
engine.add_to_blacklist("192.168.1.1")
engine.add_to_blacklist("192.168.1.1")  # 不会报错
```

3. **移除不存在的IP**
```python
# 移除不在黑名单中的IP也会返回成功
result = engine.remove_from_blacklist("1.1.1.1")
print(result)  # True
```

## 性能考虑

- **查找复杂度**: O(1) - 使用 HashSet 实现
- **内存使用**: 每个IP地址约占用 16-32 字节
- **并发安全**: 使用 RwLock 保证线程安全
- **建议限制**: 黑名单建议不超过 10,000 个IP以保持最佳性能

## 最佳实践

1. **定期清理**: 定期检查和清理过期的黑名单条目
2. **日志记录**: 记录黑名单操作以便审计
3. **备份恢复**: 实现黑名单的持久化存储
4. **监控告警**: 监控黑名单大小和阻断频率

```python
# 示例：黑名单管理类
class BlacklistManager:
    def __init__(self, engine):
        self.engine = engine
        self.blocked_count = 0
    
    def block_ip(self, ip, reason=""):
        """阻断IP并记录原因"""
        try:
            result = self.engine.add_to_blacklist(ip)
            if result:
                self.blocked_count += 1
                print(f"[{time.time()}] 阻断IP: {ip}, 原因: {reason}")
            return result
        except ValueError as e:
            print(f"阻断失败: {e}")
            return False
    
    def unblock_ip(self, ip):
        """解除IP阻断"""
        result = self.engine.remove_from_blacklist(ip)
        if result:
            print(f"[{time.time()}] 解除阻断: {ip}")
        return result
    
    def get_stats(self):
        """获取黑名单统计"""
        current_blacklist = self.engine.get_blacklist()
        return {
            "total_blocked": self.blocked_count,
            "current_blacklist_size": len(current_blacklist),
            "blacklist": current_blacklist
        }
```

## 注意事项

- 黑名单功能基于客户端IP地址，可能受到代理服务器影响
- 支持 IPv4 和 IPv6 地址格式
- 黑名单在服务器重启后会清空（如需持久化请自行实现）
- 建议结合日志系统使用以便追踪阻断记录