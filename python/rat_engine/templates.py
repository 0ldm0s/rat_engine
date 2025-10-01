# -*- coding: utf-8 -*-
"""
RAT Engine 模板引擎

高性能、生产级模板引擎，专为RAT Engine设计。
支持变量替换、条件语句、循环和JavaScript兼容语法。

特性：
- 高性能编译和渲染
- JavaScript模板语法兼容
- 自动HTML转义
- 错误处理和调试支持
- 缓存优化
"""

import re
import html
import threading
from typing import Dict, Any, Optional, Union, List, Callable
from functools import lru_cache
import logging


class TemplateError(Exception):
    """模板引擎异常"""
    pass


class TemplateSyntaxError(TemplateError):
    """模板语法错误"""
    def __init__(self, message: str, line: int = None, column: int = None):
        self.line = line
        self.column = column
        super().__init__(f"语法错误: {message}" + 
                        (f" (行 {line}, 列 {column})" if line and column else ""))


class TemplateRuntimeError(TemplateError):
    """模板运行时错误"""
    pass


class TemplateNode:
    """模板节点基类"""
    def render(self, context: Dict[str, Any]) -> str:
        raise NotImplementedError


class TextNode(TemplateNode):
    """文本节点"""
    def __init__(self, text: str):
        self.text = text
    
    def render(self, context: Dict[str, Any]) -> str:
        return self.text


class VariableNode(TemplateNode):
    """变量节点"""
    def __init__(self, variable: str, escape: bool = True):
        self.variable = variable.strip()
        self.escape = escape
    
    def render(self, context: Dict[str, Any]) -> str:
        try:
            # 支持点号访问和数组索引
            value = self._resolve_variable(self.variable, context)
            if value is None:
                return ""
            
            result = str(value)
            return html.escape(result) if self.escape else result
        except Exception as e:
            logging.warning(f"变量 '{self.variable}' 解析失败: {e}")
            return ""
    
    def _resolve_variable(self, var_path: str, context: Dict[str, Any]) -> Any:
        """解析变量路径，支持 obj.attr 和 obj[key] 语法"""
        parts = re.split(r'[.\[\]]', var_path)
        parts = [p for p in parts if p]  # 移除空字符串
        
        current = context
        for part in parts:
            if isinstance(current, dict):
                current = current.get(part)
            elif hasattr(current, part):
                current = getattr(current, part)
            elif hasattr(current, '__getitem__'):
                try:
                    # 尝试数字索引
                    if part.isdigit():
                        current = current[int(part)]
                    else:
                        current = current[part]
                except (KeyError, IndexError, TypeError):
                    return None
            else:
                return None
        
        return current


class IfNode(TemplateNode):
    """条件节点"""
    def __init__(self, condition: str, if_nodes: List[TemplateNode], 
                 else_nodes: List[TemplateNode] = None):
        self.condition = condition.strip()
        self.if_nodes = if_nodes or []
        self.else_nodes = else_nodes or []
    
    def render(self, context: Dict[str, Any]) -> str:
        try:
            if self._evaluate_condition(self.condition, context):
                return ''.join(node.render(context) for node in self.if_nodes)
            else:
                return ''.join(node.render(context) for node in self.else_nodes)
        except Exception as e:
            raise TemplateRuntimeError(f"条件语句执行失败: {e}")
    
    def _evaluate_condition(self, condition: str, context: Dict[str, Any]) -> bool:
        """安全的条件表达式求值"""
        # 简单的条件求值，支持基本比较
        condition = condition.strip()
        
        # 处理否定
        if condition.startswith('!'):
            return not self._evaluate_condition(condition[1:], context)
        
        # 处理比较操作符
        for op in ['==', '!=', '>=', '<=', '>', '<']:
            if op in condition:
                left, right = condition.split(op, 1)
                left_val = self._resolve_value(left.strip(), context)
                right_val = self._resolve_value(right.strip(), context)
                
                if op == '==':
                    return left_val == right_val
                elif op == '!=':
                    return left_val != right_val
                elif op == '>=':
                    return left_val >= right_val
                elif op == '<=':
                    return left_val <= right_val
                elif op == '>':
                    return left_val > right_val
                elif op == '<':
                    return left_val < right_val
        
        # 简单的真值检查
        value = self._resolve_value(condition, context)
        return bool(value)
    
    def _resolve_value(self, value: str, context: Dict[str, Any]) -> Any:
        """解析值，可能是变量或字面量"""
        value = value.strip()
        
        # 字符串字面量
        if (value.startswith('"') and value.endswith('"')) or \
           (value.startswith("'") and value.endswith("'")):
            return value[1:-1]
        
        # 数字字面量
        if value.isdigit():
            return int(value)
        
        try:
            return float(value)
        except ValueError:
            pass
        
        # 布尔字面量
        if value.lower() == 'true':
            return True
        elif value.lower() == 'false':
            return False
        elif value.lower() == 'null' or value.lower() == 'none':
            return None
        
        # 变量
        var_node = VariableNode(value, escape=False)
        return var_node._resolve_variable(value, context)


class ForNode(TemplateNode):
    """循环节点"""
    def __init__(self, var_name: str, iterable: str, nodes: List[TemplateNode]):
        self.var_name = var_name.strip()
        self.iterable = iterable.strip()
        self.nodes = nodes or []
    
    def render(self, context: Dict[str, Any]) -> str:
        try:
            var_node = VariableNode(self.iterable, escape=False)
            items = var_node._resolve_variable(self.iterable, context)
            
            if not items:
                return ""
            
            result = []
            for i, item in enumerate(items):
                # 创建新的上下文
                loop_context = context.copy()
                loop_context[self.var_name] = item
                loop_context['loop'] = {
                    'index': i,
                    'index0': i,
                    'index1': i + 1,
                    'first': i == 0,
                    'last': i == len(items) - 1,
                    'length': len(items)
                }
                
                for node in self.nodes:
                    result.append(node.render(loop_context))
            
            return ''.join(result)
        except Exception as e:
            raise TemplateRuntimeError(f"循环语句执行失败: {e}")


class Template:
    """模板类"""
    
    # 编译缓存
    _cache = {}
    _cache_lock = threading.RLock()
    
    def __init__(self, template_string: str, name: str = "<template>", 
                 auto_escape: bool = True, cache: bool = True):
        self.template_string = template_string
        self.name = name
        self.auto_escape = auto_escape
        self.cache = cache
        self._nodes = None
        
        # 编译模板
        self._compile()
    
    def _compile(self):
        """编译模板"""
        cache_key = (self.template_string, self.auto_escape) if self.cache else None
        
        if cache_key and cache_key in self._cache:
            self._nodes = self._cache[cache_key]
            return
        
        try:
            self._nodes = self._parse(self.template_string)
            
            if cache_key:
                with self._cache_lock:
                    self._cache[cache_key] = self._nodes
        except Exception as e:
            raise TemplateSyntaxError(f"模板编译失败: {e}")
    
    def _parse(self, template: str) -> List[TemplateNode]:
        """解析模板字符串"""
        nodes = []
        pos = 0
        line = 1
        
        # 模板语法正则表达式
        patterns = {
            'variable': re.compile(r'\{\{\s*([^}]+)\s*\}\}'),
            'raw_variable': re.compile(r'\{\{\{\s*([^}]+)\s*\}\}\}'),
            'if_start': re.compile(r'\{%\s*if\s+([^%]+)\s*%\}'),
            'elif': re.compile(r'\{%\s*elif\s+([^%]+)\s*%\}'),
            'else': re.compile(r'\{%\s*else\s*%\}'),
            'endif': re.compile(r'\{%\s*endif\s*%\}'),
            'for_start': re.compile(r'\{%\s*for\s+(\w+)\s+in\s+([^%]+)\s*%\}'),
            'endfor': re.compile(r'\{%\s*endfor\s*%\}'),
            'comment': re.compile(r'\{#.*?#\}', re.DOTALL)
        }
        
        while pos < len(template):
            # 查找最近的标签
            next_match = None
            next_type = None
            next_pos = len(template)
            
            for tag_type, pattern in patterns.items():
                match = pattern.search(template, pos)
                if match and match.start() < next_pos:
                    next_match = match
                    next_type = tag_type
                    next_pos = match.start()
            
            # 添加文本节点
            if next_pos > pos:
                text = template[pos:next_pos]
                if text:
                    nodes.append(TextNode(text))
                    line += text.count('\n')
            
            if not next_match:
                break
            
            # 处理标签
            if next_type == 'variable':
                var_name = next_match.group(1)
                nodes.append(VariableNode(var_name, escape=self.auto_escape))
            
            elif next_type == 'raw_variable':
                var_name = next_match.group(1)
                nodes.append(VariableNode(var_name, escape=False))
            
            elif next_type == 'comment':
                # 忽略注释
                pass
            
            elif next_type == 'if_start':
                condition = next_match.group(1)
                if_nodes, else_nodes, end_pos = self._parse_if_block(template, next_match.end(), line)
                nodes.append(IfNode(condition, if_nodes, else_nodes))
                pos = end_pos
                continue
            
            elif next_type == 'for_start':
                var_name = next_match.group(1)
                iterable = next_match.group(2)
                for_nodes, end_pos = self._parse_for_block(template, next_match.end(), line)
                nodes.append(ForNode(var_name, iterable, for_nodes))
                pos = end_pos
                continue
            
            pos = next_match.end()
            line += next_match.group(0).count('\n')
        
        return nodes
    
    def _parse_if_block(self, template: str, start_pos: int, start_line: int):
        """解析if块"""
        if_nodes = []
        else_nodes = []
        current_nodes = if_nodes
        pos = start_pos
        
        while pos < len(template):
            # 查找控制标签
            elif_match = re.search(r'\{%\s*elif\s+([^%]+)\s*%\}', template[pos:])
            else_match = re.search(r'\{%\s*else\s*%\}', template[pos:])
            endif_match = re.search(r'\{%\s*endif\s*%\}', template[pos:])
            
            matches = [(m, t) for m, t in [(elif_match, 'elif'), (else_match, 'else'), (endif_match, 'endif')] if m]
            
            if not matches:
                raise TemplateSyntaxError("未找到对应的 endif", start_line)
            
            # 找到最近的匹配
            next_match, match_type = min(matches, key=lambda x: x[0].start())
            
            # 解析到匹配位置的内容
            block_content = template[pos:pos + next_match.start()]
            if block_content:
                block_nodes = self._parse(block_content)
                current_nodes.extend(block_nodes)
            
            if match_type == 'endif':
                return if_nodes, else_nodes, pos + next_match.end()
            elif match_type == 'else':
                current_nodes = else_nodes
            elif match_type == 'elif':
                # 将elif转换为嵌套的if-else
                condition = next_match.group(1)
                elif_if_nodes, elif_else_nodes, end_pos = self._parse_if_block(template, pos + next_match.end(), start_line)
                else_nodes.append(IfNode(condition, elif_if_nodes, elif_else_nodes))
                return if_nodes, else_nodes, end_pos
            
            pos += next_match.end()
        
        raise TemplateSyntaxError("未找到对应的 endif", start_line)
    
    def _parse_for_block(self, template: str, start_pos: int, start_line: int):
        """解析for块"""
        nodes = []
        pos = start_pos
        
        endfor_match = re.search(r'\{%\s*endfor\s*%\}', template[pos:])
        if not endfor_match:
            raise TemplateSyntaxError("未找到对应的 endfor", start_line)
        
        block_content = template[pos:pos + endfor_match.start()]
        if block_content:
            nodes = self._parse(block_content)
        
        return nodes, pos + endfor_match.end()
    
    def render(self, context: Dict[str, Any] = None) -> str:
        """渲染模板"""
        if context is None:
            context = {}
        
        try:
            return ''.join(node.render(context) for node in self._nodes)
        except Exception as e:
            raise TemplateRuntimeError(f"模板渲染失败: {e}")
    
    @classmethod
    def clear_cache(cls):
        """清空模板缓存"""
        with cls._cache_lock:
            cls._cache.clear()


class TemplateEngine:
    """模板引擎"""
    
    def __init__(self, auto_escape: bool = True, cache: bool = True):
        self.auto_escape = auto_escape
        self.cache = cache
        self.globals = {}
        self.filters = {
            'upper': str.upper,
            'lower': str.lower,
            'title': str.title,
            'length': len,
            'default': lambda value, default='': value if value else default,
        }
    
    def add_global(self, name: str, value: Any):
        """添加全局变量"""
        self.globals[name] = value
    
    def add_filter(self, name: str, func: Callable):
        """添加过滤器"""
        self.filters[name] = func
    
    def from_string(self, template_string: str, name: str = "<template>") -> Template:
        """从字符串创建模板"""
        return Template(template_string, name, self.auto_escape, self.cache)
    
    def render_string(self, template_string: str, context: Dict[str, Any] = None) -> str:
        """直接渲染字符串模板"""
        template = self.from_string(template_string)
        
        # 合并全局变量
        if context is None:
            context = {}
        
        render_context = self.globals.copy()
        render_context.update(context)
        
        return template.render(render_context)


# 默认模板引擎实例
default_engine = TemplateEngine()


# 便捷函数
def render_template(template_string: str, context: Dict[str, Any] = None) -> str:
    """渲染模板字符串"""
    return default_engine.render_string(template_string, context)


def render_template_safe(template_string: str, context: Dict[str, Any] = None, 
                        fallback: str = "") -> str:
    """安全渲染模板，出错时返回fallback"""
    try:
        return render_template(template_string, context)
    except Exception as e:
        logging.error(f"模板渲染失败: {e}")
        return fallback


# JavaScript兼容的模板语法转换器
class JSTemplateConverter:
    """JavaScript模板语法转换器"""
    
    @staticmethod
    def convert_js_template(js_template: str) -> str:
        """将JavaScript模板语法转换为RAT模板语法"""
        # 转换 ${variable} 为 {{ variable }}
        result = re.sub(r'\$\{([^}]+)\}', r'{{ \1 }}', js_template)
        
        # 转换 this.property 为 this_property (避免点号问题)
        result = re.sub(r'this\.([a-zA-Z_][a-zA-Z0-9_]*)', r'this_\1', result)
        
        return result
    
    @staticmethod
    def prepare_js_context(context: Dict[str, Any]) -> Dict[str, Any]:
        """准备JavaScript兼容的上下文"""
        js_context = context.copy()
        
        # 如果有this对象，展平其属性
        if 'this' in js_context and isinstance(js_context['this'], dict):
            this_obj = js_context.pop('this')
            for key, value in this_obj.items():
                js_context[f'this_{key}'] = value
        
        return js_context


def render_js_template(js_template: str, context: Dict[str, Any] = None) -> str:
    """渲染JavaScript风格的模板"""
    converter = JSTemplateConverter()
    rat_template = converter.convert_js_template(js_template)
    rat_context = converter.prepare_js_context(context or {})
    
    return render_template(rat_template, rat_context)