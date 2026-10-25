#!/bin/bash
# ai_orz - target/ 磁盘治理（编译产物瘦身，一处实现）
#
# 【为什么 target 会涨到几百 G】（2026-10 实测 346G，其中 target/debug 320G）
#   deps/ 里的产物文件名都带构建指纹哈希（如 web_search_tool_test-1a2b3c4d5e6f7a8b）。
#   源码每次变更 → 重链出**新哈希副本**，旧副本 cargo 永不回收：
#   - 集成测试二进制单个 300~800M（全量链接 + debuginfo），几十个测试目标
#     × 每次重链一份 → 无上限增长（实测 266 个旧可执行副本共 151G，最大头）
#   - 巨型原生库随依赖图 / feature 变化积累多份（duckdb 1.4G×5、lance 1.9G×7）
#   - incremental/ 增量缓存与 llvm-cov-target/ 的 .profraw 运行时数据
#   旧 clean-slim 的 find pattern 写成 `*-[0-9a-f]\{16\}.*`——BSD find 的 -name
#   走 fnmatch，不认 {16} 量词 → 哈希副本一个都匹配不上，从未真正删过。
#
# 【三个子命令】危险级别递增，按需取用：
#   stale [天数]   删 mtime 超过 N 天（默认 1）的旧哈希**可执行副本**（deps 内
#                  无扩展名可执行文件）与 .profraw。「先清理再编译」钩子用的
#                  就是它（make prod / build / package 编译前自动执行）：
#                  旧哈希副本当前构建永不复用，删了最多触发一次重链接
#                  （分钟级、无重编译）；依赖的 rlib/rmeta 热缓存**不碰**。
#   slim           stale 30 + incremental/ 全清 + 30 天前的带扩展名哈希产物
#                  （rlib/rmeta 陈旧快照；删了可能触发依赖重编译，故窗口更长）。
#   all            cargo clean 前后端全量（下次编译为完整冷构建，慎用）。
#
# 用法：
#   ./scripts/clean_target.sh stale [天数]    （等价 make clean-stale [DAYS=1]）
#   ./scripts/clean_target.sh slim            （等价 make clean-slim）
#   ./scripts/clean_target.sh all             （等价 make clean）
#
# 环境变量：
#   AI_ORZ_STALE_DAYS   stale 子命令的默认天数（默认 1）

set -eu

# shellcheck source=./lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

STALE_DAYS="${AI_ORZ_STALE_DAYS:-1}"

report_size() {
    du -sh "$REPO_ROOT/target" "$REPO_ROOT/frontend/target" 2>/dev/null || true
}

# 删 mtime 超过 $1 天的旧哈希可执行副本（deps 内无扩展名 + 可执行位）。
# 用 16 个 `?` 表达「16 位十六进制指纹」：BSD find 的 -name 走 fnmatch，
# 不支持 {16} 量词（旧 clean-slim 因此一个副本都没删到，见文件头）。
rm_stale_exes() {
    local days=$1
    # llvm-cov-target/debug/deps 是覆盖率链路的独立产物目录（目标目录镜像），
    # 同样按指纹累积，漏掉它会留下第二份「旧副本」（实测 861M/次全覆盖）。
    find "$REPO_ROOT/target/debug/deps" "$REPO_ROOT/target/release/deps" \
        "$REPO_ROOT/target/llvm-cov-target/debug/deps" "$REPO_ROOT/target/llvm-cov-target/release/deps" \
        -maxdepth 1 -type f -perm -u+x -name '*-????????????????' -mtime +"$days" \
        -print0 2>/dev/null | xargs -0 rm -f 2>/dev/null || true
}

# 删 mtime 超过 $1 天的带扩展名哈希产物（rlib / rmeta / .d 等陈旧快照）
rm_stale_hashed() {
    local days=$1
    find "$REPO_ROOT/target/debug/deps" "$REPO_ROOT/target/release/deps" \
        "$REPO_ROOT/target/wasm32-unknown-unknown" \
        -maxdepth 3 -type f -name '*-????????????????.*' -mtime +"$days" \
        -print0 2>/dev/null | xargs -0 rm -f 2>/dev/null || true
}

# 删 mtime 超过 $1 天的覆盖率运行时数据（按 PID 命名，跑一次测试落一份）
rm_stale_profraw() {
    local days=$1
    find "$REPO_ROOT/target/llvm-cov-target" -maxdepth 1 -type f -name '*.profraw' \
        -mtime +"$days" -print0 2>/dev/null | xargs -0 rm -f 2>/dev/null || true
}

has_target() {
    [ -d "$REPO_ROOT/target" ] || {
        warn "无 target/，无需清理"
        return 1
    }
}

cmd_stale() {
    local days="${1:-$STALE_DAYS}"
    case "$days" in '' | *[!0-9]*) die "天数须为正整数: ${days}" ;; esac
    has_target || return 0
    rm_stale_exes "$days"
    rm_stale_profraw "$days"
    ok "✓ 已清理 ${days} 天前的旧构建副本（可执行副本 + profraw；依赖热缓存未动）"
}

cmd_slim() {
    has_target || return 0
    echo "== 清理 incremental 增量缓存（膨胀主因之一，可安全删除）=="
    rm -rf "$REPO_ROOT/target/debug/incremental" "$REPO_ROOT/target/release/incremental" \
        "$REPO_ROOT/target/wasm32-unknown-unknown/debug/incremental" \
        "$REPO_ROOT/target/wasm32-unknown-unknown/release/incremental" 2>/dev/null || true
    echo "== 清理 30 天前的陈旧哈希产物（可执行副本 + rlib/rmeta 快照 + profraw）=="
    rm_stale_exes 30
    rm_stale_hashed 30
    rm_stale_profraw 30
    echo "== 清理覆盖率镜像目录（14G 级的 target 副本，下次 make coverage 重建）=="
    rm -rf "$REPO_ROOT/target/llvm-cov-target" 2>/dev/null || true
    ok "✓ 瘦身完成（依赖缓存按 30 天口径保留，下次编译无需完整冷构建）"
}

cmd_all() {
    (cd "$REPO_ROOT" && cargo clean)
    (cd "$REPO_ROOT/frontend" && cargo clean) 2>/dev/null || true
    ok "✓ 已全量清理 target（下次编译为完整冷构建）"
}

usage_die_no_cmd() {
    err "未知子命令: ${1:-}（可选 stale [天数] / slim / all）"
    exit 2
}

SUB="${1:-}"
[ $# -gt 0 ] && shift || true

case "$SUB" in
    stale) cmd_stale "$@" ;;
    slim) cmd_slim "$@" ;;
    all) cmd_all "$@" ;;
    help | --help | -h | "")
        sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        ;;
    *) usage_die_no_cmd "$SUB" ;;
esac
