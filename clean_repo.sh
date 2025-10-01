#!/bin/bash

# RAT Engine 仓库清理脚本
# 清理所有不应该存在于代码库中的文件

set -e

echo "🧹 开始清理 RAT Engine 仓库..."

# 定义要清理的目录模式
CLEAN_PATTERNS=(
    "**/cache_l2/"
    "**/test_cache/"
    "**/test_l2_cache/"
    "**/debug_cache/"
    "**/.cache/"
    "**/.caches/"
    "**/tmp/"
    "**/temp/"
    "**/temporary/"
    "*-session-*.txt"
    "*-conversation-*.txt"
    "*-continued-*.txt"
    "*-claude-*.txt"
    "*-anthropic-*.txt"
    "[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]-*.txt"
)

cleaned_count=0

# 清理文件系统中的文件
echo "🗑️  清理文件系统中的危险文件..."
for pattern in "${CLEAN_PATTERNS[@]}"; do
    # 使用 find 查找匹配的文件
    while IFS= read -r -d '' file; do
        if [ -e "$file" ]; then
            echo "   删除: $file"
            rm -rf "$file"
            ((cleaned_count++))
        fi
    done < <(find . -name "$pattern" -print0 2>/dev/null || true)
done

# 清理 git 中的文件
echo "🗑️  清理 Git 中的危险文件..."
git_files=$(git ls-files | grep -E "(cache_l2|test_cache|test_l2_cache|debug_cache|session|conversation|continued|claude|anthropic|[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]-)" || true)

if [ -n "$git_files" ]; then
    echo "   从 Git 中移除以下文件："
    echo "$git_files" | while read file; do
        echo "   - $file"
    done

    echo "$git_files" | xargs git rm --cached --quiet
    echo "✅ 已从 Git 中移除这些文件"
else
    echo "✅ 没有发现需要从 Git 中移除的文件"
fi

# 统计清理结果
echo ""
echo "📊 清理统计："
echo "   - 从文件系统删除了 $cleaned_count 个文件/目录"
if [ -n "$git_files" ]; then
    git_count=$(echo "$git_files" | wc -l | tr -d ' ')
    echo "   - 从 Git 中移除了 $git_count 个文件"
fi

# 检查 gitignore
echo ""
echo "📋 检查 .gitignore 规则..."
if [ -f ".gitignore" ]; then
    echo "✅ .gitignore 存在"

    # 检查关键规则
    required_patterns=(
        "**/cache_l2/"
        "**/test_cache/"
        "**/test_l2_cache/"
        "*-session-*.txt"
        "*-conversation-*.txt"
        "*-continued-*.txt"
    )

    for pattern in "${required_patterns[@]}"; do
        if grep -q "$pattern" .gitignore; then
            echo "   ✅ 包含规则: $pattern"
        else
            echo "   ❌ 缺少规则: $pattern"
        fi
    done
else
    echo "❌ .gitignore 不存在"
fi

echo ""
echo "🎉 清理完成！"
echo ""
echo "💡 建议："
echo "   1. 运行 'git status' 检查是否有其他需要清理的文件"
echo "   2. 运行 'git add .gitignore' 如果修改了忽略规则"
echo "   3. 运行 'git commit -m \"清理仓库和更新忽略规则\"' 提交清理结果"