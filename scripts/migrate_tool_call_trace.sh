#!/bin/bash
# call_trace 存储布局迁移：tools/{tool_id}/call_trace/*.jsonl → tools/call_trace/*.jsonl
#
# 背景（2026-09-19）：`call_id` 是一次工具调用的唯一身份（UUID v7），`tool_id` 只是
# entry 里的字段。按 tool_id 分目录会让「未带 tool_id 的查询」（详情页按 call_id 查、
# 「最近 N 条」列表）退化成全量目录枚举，并诱导出「按 (tool_id, call_id) 收窄」的
# 错误幂等判定。详见 src/pkg/paths.rs::tool_call_trace_dir 的边界决策。
#
# 迁移方式：按日期 concat。行内顺序不影响查询 —— query_calls 统一按 started_at 排序。
# 每一步都用 `awk 1` 归一化换行，避免源文件缺尾换行导致相邻两行被拼成一行。
#
# ⚠️ 会删除旧目录：仅在行数校验通过后执行，且必须显式 --apply；默认 dry-run。
#
# Usage:
#   ./scripts/migrate_tool_call_trace.sh              # dry-run：只打印计划
#   ./scripts/migrate_tool_call_trace.sh --apply      # 真正执行
#   AI_ORZ_BASE_PATH=/path/to/data ./scripts/migrate_tool_call_trace.sh --apply
#   DATA_DIR=/path/to/data ./scripts/migrate_tool_call_trace.sh --apply   （历史写法，仍兼容）
#
# 数据目录默认取部署根（仓库模式 = $HOME/.ai_orz/data），与 prod.sh 同口径，详见 lib/service.sh。
#
# 兼容 macOS 自带 bash 3.2（不用 mapfile / 关联数组）。

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# 数据目录来源与其它脚本同口径（部署根 / AI_ORZ_BASE_PATH），不再自行拼 $REPO_ROOT/.ai_orz
# —— 生产数据已迁到部署根，自拼路径会静默操作错误目录。
_INCOMING_DATA_DIR="${DATA_DIR:-}"
# shellcheck source=./lib/service.sh
source "$SCRIPT_DIR/lib/service.sh"
# 兼容本脚本历史用法（DATA_DIR=...），但不覆盖 AI_ORZ_BASE_PATH（后者是全局唯一开关）
if [ -n "$_INCOMING_DATA_DIR" ] && [ -z "${AI_ORZ_BASE_PATH:-}" ]; then
    DATA_DIR="$_INCOMING_DATA_DIR"
fi
TOOLS_DIR="$DATA_DIR/tools"
TARGET_DIR="$TOOLS_DIR/call_trace"

APPLY=0
if [ "${1:-}" = "--apply" ]; then
  APPLY=1
elif [ "$#" -gt 0 ]; then
  echo "unrecognized argument: $1" >&2
  echo "usage: $0 [--apply]" >&2
  exit 2
fi

if [ ! -d "$TOOLS_DIR" ]; then
  echo "tools dir not found: $TOOLS_DIR" >&2
  exit 1
fi

# 行数统计：awk 的 NR 会把「缺尾换行」的最后一行也算进去，与 wc -l 不同
count_lines() { awk 'END { print NR + 0 }' "$1"; }

# 源文件：tools/<tool_id>/call_trace/<date>.jsonl（mindepth=3）
# 目标文件：tools/call_trace/<date>.jsonl（mindepth=2），天然不会被下面的 find 命中
SRC_FILES=()
while IFS= read -r f; do
  SRC_FILES+=("$f")
done < <(find "$TOOLS_DIR" -mindepth 3 -maxdepth 3 -type f -name '*.jsonl' -path '*/call_trace/*' | sort)

if [ "${#SRC_FILES[@]}" -eq 0 ]; then
  echo "nothing to migrate: no per-tool call_trace files under $TOOLS_DIR"
  exit 0
fi

# 按 basename（日期）归组，去重
DATES=()
while IFS= read -r d; do
  DATES+=("$d")
done < <(for f in "${SRC_FILES[@]}"; do basename "$f"; done | sort -u)

echo "== call_trace 迁移 =="
echo "数据目录: $DATA_DIR"
echo "源:       tools/<tool_id>/call_trace/*.jsonl  (${#SRC_FILES[@]} 个文件)"
echo "目标:     tools/call_trace/*.jsonl"
echo "日期:     ${DATES[*]}"
echo

total_src=0
for f in "${SRC_FILES[@]}"; do
  n=$(count_lines "$f")
  total_src=$((total_src + n))
done
echo "源文件总行数: $total_src"

if [ "$APPLY" -eq 0 ]; then
  echo
  echo "[dry-run] 未做任何改动。确认无误后加 --apply 执行。"
  exit 0
fi

# 迁移前目标目录已有的行数（新代码可能已经往拉平目录写过）
total_before=0
if [ -d "$TARGET_DIR" ]; then
  for t in "$TARGET_DIR"/*.jsonl; do
    [ -f "$t" ] || continue
    total_before=$((total_before + $(count_lines "$t")))
  done
fi

mkdir -p "$TARGET_DIR"
for d in "${DATES[@]}"; do
  t="$TARGET_DIR/$d"
  for f in "${SRC_FILES[@]}"; do
    [ "$(basename "$f")" = "$d" ] || continue
    awk 1 "$f" >>"$t"
  done
done

# 校验：迁移后总行数 == 迁移前目标行数 + 源文件总行数
total_after=0
for t in "$TARGET_DIR"/*.jsonl; do
  [ -f "$t" ] || continue
  total_after=$((total_after + $(count_lines "$t")))
done
expected=$((total_before + total_src))

if [ "$expected" -ne "$total_after" ]; then
  echo "!! 行数校验失败: expected=$expected actual=$total_after" >&2
  echo "!! 旧目录保持原样，请人工检查 $TARGET_DIR" >&2
  exit 1
fi
echo "校验通过: $total_after 行（迁移前 $total_before + 并入 ${total_src}）"

# 校验通过后才删旧目录
for f in "${SRC_FILES[@]}"; do
  rm -f "$f"
done
for f in "${SRC_FILES[@]}"; do
  ct_dir="$(dirname "$f")"
  if [ -d "$ct_dir" ]; then rmdir "$ct_dir" 2>/dev/null || true; fi
  # 工具目录若已空（如只剩 call_trace）则一并移除；还有 logs 的会保留
  tool_dir="$(dirname "$ct_dir")"
  if [ -d "$tool_dir" ]; then rmdir "$tool_dir" 2>/dev/null || true; fi
done

echo
echo "迁移完成：${total_src} 行已并入 ${TARGET_DIR}，旧 per-tool call_trace 目录已移除。"
