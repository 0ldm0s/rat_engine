#!/usr/bin/env python3
import random
import string

def generate_random_line(length=50):
    """生成指定长度的随机字符串行"""
    return ''.join(random.choice(string.ascii_letters + string.digits) for _ in range(length))

def generate_txt_file(filename, line_count=1000):
    """生成包含随机数据的TXT文件"""
    with open(filename, 'w') as f:
        for _ in range(line_count):
            f.write(generate_random_line() + '\n')
    print(f'已生成文件 {filename}，包含 {line_count} 行随机数据')

if __name__ == '__main__':
    # 硬编码行数变量
    LINES_TO_GENERATE = 5000  # 可以修改这个值来改变生成的行数
    generate_txt_file('random_data.txt', LINES_TO_GENERATE)