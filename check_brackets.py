#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
检查Rust文件中的括号匹配情况
"""

def check_brackets(filename):
    """检查文件中的括号是否匹配"""
    try:
        with open(filename, 'r', encoding='utf-8') as f:
            content = f.read()
    except UnicodeDecodeError:
        with open(filename, 'r', encoding='gbk') as f:
            content = f.read()

    lines = content.split('\n')

    # 括号计数器
    curly_braces = 0      # {}
    square_brackets = 0    # []
    parentheses = 0        # ()

    issues = []

    for line_num, line in enumerate(lines, 1):
        # 逐个字符检查
        for col_num, char in enumerate(line, 1):
            if char == '{':
                curly_braces += 1
            elif char == '}':
                curly_braces -= 1
                if curly_braces < 0:
                    issues.append(f"第{line_num}行第{col_num}列: 多余的 '}}'")
                    curly_braces = 0
            elif char == '[':
                square_brackets += 1
            elif char == ']':
                square_brackets -= 1
                if square_brackets < 0:
                    issues.append(f"第{line_num}行第{col_num}列: 多余的 ']'")
                    square_brackets = 0
            elif char == '(':
                parentheses += 1
            elif char == ')':
                parentheses -= 1
                if parentheses < 0:
                    issues.append(f"第{line_num}行第{col_num}列: 多余的 ')'")
                    parentheses = 0

    # 检查未闭合的括号
    if curly_braces > 0:
        issues.append(f"未闭合的 '{{' 数量: {curly_braces}")
    if square_brackets > 0:
        issues.append(f"未闭合的 '[' 数量: {square_brackets}")
    if parentheses > 0:
        issues.append(f"未闭合的 '(' 数量: {parentheses}")

    return issues

def find_method_bounds(filename, method_name):
    """找到方法的开始和结束位置"""
    try:
        with open(filename, 'r', encoding='utf-8') as f:
            lines = f.readlines()
    except UnicodeDecodeError:
        with open(filename, 'r', encoding='gbk') as f:
            lines = f.readlines()

    start_line = None
    for i, line in enumerate(lines):
        # 精确匹配方法定义行
        if f"pub async fn {method_name}" in line:
            start_line = i
            break

    if start_line is None:
        return None, None, f"未找到方法: {method_name}"

    # 找到方法的结束位置
    brace_count = 0
    in_method = False

    for i in range(start_line, len(lines)):
        line = lines[i]
        for char in line:
            if char == '{':
                brace_count += 1
                in_method = True
            elif char == '}':
                brace_count -= 1
                if in_method and brace_count == 0:
                    return start_line + 1, i + 1, None

    return start_line + 1, None, f"方法 {method_name} 未找到结束括号"

if __name__ == "__main__":
    import sys

    if len(sys.argv) < 2:
        print("用法: python check_brackets.py <文件名>")
        sys.exit(1)

    filename = sys.argv[1]

    print(f"检查文件: {filename}")
    print("=" * 50)

    # 检查括号匹配
    issues = check_brackets(filename)

    if issues:
        print("发现括号问题:")
        for issue in issues:
            print(f"  - {issue}")
    else:
        print("OK: 所有括号匹配正确")

    print()

    # 如果指定了方法名，检查方法边界
    if len(sys.argv) > 2:
        method_name = sys.argv[2]
        print(f"查找方法: {method_name}")

        start, end, error = find_method_bounds(filename, method_name)

        if error:
            print(f"ERROR: {error}")
        else:
            print(f"OK: 方法位置: 第{start}行 - 第{end}行 (共{end-start+1}行)")