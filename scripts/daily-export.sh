#!/bin/bash
# tokscale 每日自动导出脚本
# 导出三种数据：models、monthly、graph（stats）

export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.npm-global/bin:$HOME/.bun/bin:$PATH"

OUTPUT_DIR="$HOME/code/token-check"
DATE=$(date +%Y%m%d)
DATE_DIR="$OUTPUT_DIR/data/$DATE"
LATEST_DIR="$OUTPUT_DIR/data/latest"

# 今天已经导出过就跳过（检查文件存在且非空）
if [ -s "$DATE_DIR/models.json" ]; then
    exit 0
fi

mkdir -p "$DATE_DIR" "$LATEST_DIR"
TMP=$(mktemp -d)

# 1. Models 视图数据（按 client+model 分组）
npx tokscale@latest --json > "$TMP/models.json" 2>/dev/null

# 2. Monthly 视图数据（按月聚合）
npx tokscale@latest monthly --json > "$TMP/monthly.json" 2>/dev/null

# 3. Graph/Stats 视图数据（每日贡献图 + 统计）
npx tokscale@latest graph --output "$TMP/graph.json" 2>/dev/null

# 只在所有文件非空时才移动到目标目录
if [ -s "$TMP/models.json" ] && [ -s "$TMP/monthly.json" ] && [ -s "$TMP/graph.json" ]; then
    cp "$TMP/models.json"  "$DATE_DIR/models.json"
    cp "$TMP/monthly.json" "$DATE_DIR/monthly.json"
    cp "$TMP/graph.json"   "$DATE_DIR/graph.json"
    cp "$TMP/models.json"  "$LATEST_DIR/models.json"
    cp "$TMP/monthly.json" "$LATEST_DIR/monthly.json"
    cp "$TMP/graph.json"   "$LATEST_DIR/graph.json"
    echo "导出完成: $DATE_DIR"
else
    echo "导出失败: 部分数据为空" >&2
fi

rm -rf "$TMP"
