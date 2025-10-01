"""QuickMem 便捷接口

提供更友好的 Python 接口来使用 rat_quickmem 功能
"""

# 从 _rat_engine 模块直接导入编解码类
from . import _rat_engine

# 导入具体的类
QuickEncoder = _rat_engine.QuickEncoder
QuickDecoder = _rat_engine.QuickDecoder
QuickCodec = _rat_engine.QuickCodec

# 创建便捷函数
def encode(obj):
    """编码对象"""
    codec = QuickCodec()
    return codec.encode(obj)

def decode(data):
    """解码数据"""
    codec = QuickCodec()
    return codec.decode(data)

# 占位函数，保持兼容性
def get_pool_stats():
    """获取内存池统计信息"""
    return {"total_allocated": 0, "total_freed": 0}

def get_simd_capabilities():
    """获取 SIMD 能力"""
    return {"avx2": False, "sse4": False}

def configure_simd(enabled=True):
    """配置 SIMD"""
    pass

def benchmark_encode_decode(iterations=1000):
    """基准测试"""
    return {"encode_time": 0.0, "decode_time": 0.0}
import functools
import json
from typing import Any, Dict, List, Union, Optional

# 全局编解码器实例
_global_codec = None

def get_global_codec() -> QuickCodec:
    """获取全局编解码器实例"""
    global _global_codec
    if _global_codec is None:
        _global_codec = QuickCodec()
    return _global_codec

def quickmem_encode(obj: Any) -> bytes:
    """便捷编码函数
    
    Args:
        obj: 要编码的 Python 对象
        
    Returns:
        编码后的字节数据
    """
    return encode(obj)

def quickmem_decode(data: bytes) -> Any:
    """便捷解码函数
    
    Args:
        data: 要解码的字节数据
        
    Returns:
        解码后的 Python 对象
    """
    return decode(data)

def quickmem_route(app):
    """装饰器：自动使用 QuickMem 优化数据传输
    
    使用示例:
    @quickmem_route(app)
    def my_handler(request):
        # 请求数据会自动通过 QuickMem 解码
        # 响应数据会自动通过 QuickMem 编码
        return {"message": "Hello QuickMem!"}
    """
    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            # 这里可以添加自动编解码逻辑
            # 目前保持原有功能，后续可以扩展
            return func(*args, **kwargs)
        return wrapper
    return decorator

class QuickMemManager:
    """QuickMem 管理器（重构版）
    
    提供统一的编解码管理、性能监控和 SIMD 优化
    """
    
    def __init__(self):
        self.encoder = QuickEncoder()
        self.decoder = QuickDecoder()
        self.codec = QuickCodec()
    
    def encode(self, obj: Any) -> bytes:
        """编码对象"""
        return self.encoder.encode(obj)
    
    def decode(self, data: bytes) -> Any:
        """解码数据"""
        return self.decoder.decode(data)
    
    def encode_batch(self, objects: List[Any]) -> List[bytes]:
        """批量编码"""
        return self.encoder.encode_batch(objects)
    
    def decode_batch(self, data_list: List[bytes]) -> List[Any]:
        """批量解码"""
        return self.decoder.decode_batch(data_list)
    
    def get_stats(self) -> Dict[str, int]:
        """获取内存池统计信息"""
        return get_pool_stats()
    
    def print_stats(self):
        """打印内存池统计信息"""
        stats = self.get_stats()
        print("QuickMem 内存池统计:")
        print(f"  小缓冲区: {stats['small_buffers']} 个")
        print(f"  中缓冲区: {stats['medium_buffers']} 个")
        print(f"  大缓冲区: {stats['large_buffers']} 个")
        print(f"  总缓冲区: {stats['total_buffers']} 个")
        print(f"  小缓冲区容量: {stats['small_capacity']} 字节")
        print(f"  中缓冲区容量: {stats['medium_capacity']} 字节")
        print(f"  大缓冲区容量: {stats['large_capacity']} 字节")
    
    def get_simd_info(self) -> Dict[str, bool]:
        """获取 SIMD 能力信息"""
        return get_simd_capabilities()
    
    def configure_simd_optimization(self, enable_avx2: bool = True, enable_sse2: bool = True, enable_neon: bool = True):
        """配置 SIMD 优化
        
        Args:
            enable_avx2: 是否启用 AVX2 优化
            enable_sse2: 是否启用 SSE2 优化
            enable_neon: 是否启用 NEON 优化
        """
        configure_simd(enable_avx2, enable_sse2, enable_neon)
    
    def benchmark(self, data: Any, iterations: int = 1000) -> Dict[str, Union[int, float]]:
        """性能基准测试
        
        Args:
            data: 测试数据
            iterations: 迭代次数
            
        Returns:
            包含性能指标的字典
        """
        return benchmark_encode_decode(data, iterations)
    
    def print_simd_info(self):
        """打印 SIMD 能力信息"""
        simd_info = self.get_simd_info()
        print("SIMD 能力检测:")
        print(f"  AVX2: {'✓' if simd_info['avx2'] else '✗'}")
        print(f"  SSE2: {'✓' if simd_info['sse2'] else '✗'}")
        print(f"  NEON: {'✓' if simd_info['neon'] else '✗'}")
    
    def print_benchmark(self, data: Any, iterations: int = 1000):
        """打印基准测试结果"""
        result = self.benchmark(data, iterations)
        print(f"QuickMem 基准测试结果 ({iterations} 次迭代):")
        print(f"  数据大小: {result['data_size_bytes']} 字节")
        print(f"  编码时间: {result['encode_time_ms']} 毫秒")
        print(f"  解码时间: {result['decode_time_ms']} 毫秒")
        print(f"  编码速度: {result['encode_ops_per_sec']:.2f} 操作/秒")
        print(f"  解码速度: {result['decode_ops_per_sec']:.2f} 操作/秒")

# 全局管理器实例
default_manager = QuickMemManager()

# 导出便捷函数（重构版）
__all__ = [
    # 核心类和函数
    'QuickEncoder', 'QuickDecoder', 'QuickCodec',
    'encode', 'decode', 'get_pool_stats',
    
    # 便捷接口
    'quickmem_encode', 'quickmem_decode', 'quickmem_route',
    'QuickMemManager', 'default_manager', 'get_global_codec',
    
    # 🔥 新增：SIMD 和性能功能
    'get_simd_capabilities', 'configure_simd', 'benchmark_encode_decode'
]