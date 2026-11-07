#!/bin/bash
# ai_orz - target/ 精准 GC（编译产物瘦身，一处实现）
#
# 【为什么 target 会涨到几百 G】（2026-11 实测 346G / 46.8 万文件）
#   Cargo 1.99.1 只回收 ~/.cargo 全局缓存，target/ 里的 build artifacts GC 尚未实现
#   （Cargo Book: "Support for tracking build artifacts is not yet implemented", cargo#13136）。
#   deps/ 产物名带 16 位构建指纹 hash，源码 / RUSTFLAGS / feature 每次变化都重编译出
#   整套新 hash 副本，旧副本永不回收：
#   - *.rcgu.o 是 rustc 的 codegen-unit 中间对象，链接完就没用了（实测 26.6 万个 / 273 GiB）
#   - 40+ 个 [[test]] 各自静态链接 duckdb（单个 libduckdb.a 1.46 GiB），
#     每个测试二进制 ~700 MiB；libduckdb-sys 的 build/out 攒了 21 份
#   - incremental/ 每个 crate 一堆旧 session，llvm-cov-target/ 是覆盖率的平行 target
#   2026-10 的旧 clean-slim 用 `*-[0-9a-f]\{16\}.*`，BSD find 的 -name 走 fnmatch
#   不认 {16} 量词 → 一个副本都没匹配上（该 bug 已修，实现改为 Python 精确匹配）。
#
# 【现在怎么做】精准 GC，算法实现在 scripts/clean_target.py：
#   以 .fingerprint/<crate>-<hash>/invoked.timestamp 的 mtime 为「最近被使用」判据，
#   按 crate 保留最近 N 份 + 时间窗口内全部，其余视为死副本。
#   保留：rlib / rmeta / .d / 最新 build out / fingerprint 目录（真正能加快编译的缓存）
#   删除：全部 *.rcgu.o、历史指纹副本、旧 incremental session、闲置工具 target
#   （*.rcgu.o 另有「编译后即时清理」：各脚本 EXIT trap 调 common.sh::
#    clean_generated_rcgu，命令一返回就删；本 GC 只作兜底）
#   可执行产物（集成测试二进制）不是缓存：只留 2 小时内链接出的、每个 target 最新 1 份，
#   更早的整批清掉（否则换 profile 后会留几十个 700M 的孤儿副本）
#   ⚠️ 不能按「最近一次构建批次」一刀切：实测 9336 个 fingerprint 里最近 1 小时
#      只 touch 了 82 个，但 10/16 那批 2113 个依赖 rlib 依然有效，删了就是冷构建。
#
# 【子命令】
#   stale [天数]   常规 GC（默认保留 7 天窗口 / 每 crate 2 份）—— 所有编译入口经
#                  common.sh::clean_stale_artifacts 在编译前自动调用
#   slim           激进 GC（1 天 / 每 crate 1 份 / 强制删闲置工具 target / 清空 incremental）
#   dry-run        只报告不删除（等价 AI_ORZ_GC_DRY_RUN=1 stale）
#   all            cargo clean 全量重置（下次为完整冷构建，慎用）
#
# 【环境变量】
#   AI_ORZ_GC_KEEP_DAYS       stale 的保留窗口天数（默认 7）
#   AI_ORZ_GC_KEEP_PER_CRATE  每 crate 保留的历史指纹份数（默认 2）
#   AI_ORZ_GC_MIN_INTERVAL    两次 GC 的最小间隔秒数（默认 300；短间隔内重复编译跳过，
#                             避免 make lint 里 4 次入口各扫一遍 30 万文件）
#   AI_ORZ_GC_DRY_RUN         1 = 只报告不删除
#   AI_ORZ_STALE_DAYS         旧口径兼容，作为 KEEP_DAYS 的默认值

set -eu

# shellcheck source=./lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

PY="$SCRIPTS_DIR/clean_target.py"
STAMP="$REPO_ROOT/target/.ai_orz_gc_stamp"
MANIFEST="$REPO_ROOT/target/.ai_orz_gc_manifest"
KEEP_DAYS="${AI_ORZ_GC_KEEP_DAYS:-${AI_ORZ_STALE_DAYS:-7}}"
KEEP_PER_CRATE="${AI_ORZ_GC_KEEP_PER_CRATE:-2}"
MIN_INTERVAL="${AI_ORZ_GC_MIN_INTERVAL:-300}"
DRY_RUN="${AI_ORZ_GC_DRY_RUN:-0}"

has_target() {
    [ -d "$REPO_ROOT/target" ] || {
        info "无 target/，无需清理"
        return 1
    }
}

# macOS 用 stat -f，Linux 用 stat -c
file_mtime() {
    stat -f %m "$1" 2>/dev/null || stat -c %Y "$1" 2>/dev/null || echo 0
}

# 短时间内重复编译不必重扫；dry-run 与 slim 不受节流影响
throttled() {
    [ "$DRY_RUN" = "1" ] && return 1
    [ -f "$STAMP" ] || return 1
    local now last
    now=$(date +%s)
    last=$(file_mtime "$STAMP")
    [ "$((now - last))" -lt "$MIN_INTERVAL" ]
}

# python3 缺失时降级：至少把最占体积的 codegen 中间对象删掉
fallback_stale() {
    local days="$1"
    find "$REPO_ROOT/target" -maxdepth 5 -type f -name '*.rcgu.o' -mtime +"$days" \
        -print0 2>/dev/null | xargs -0 rm -f 2>/dev/null || true
    find "$REPO_ROOT/target" -maxdepth 5 -type f -perm -u+x -name '*-????????????????' \
        -mtime +"$days" -print0 2>/dev/null | xargs -0 rm -f 2>/dev/null || true
}

run_gc() {
    local days="$1" per="$2" force="$3" allow_throttle="${4:-0}"
    has_target || return 0

    if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$PY" ]; then
        warn "⚠️ python3 或 clean_target.py 缺失，降级为旧版清理"
        fallback_stale "$days"
        return 0
    fi

    # 只有编译钩子（stale --auto）才节流；手动 make clean-stale 永远立即执行
    if [ "$force" != "1" ] && [ "$allow_throttle" = "1" ] && throttled; then
        return 0
    fi

    local args=(--keep-days "$days" --keep-per-crate "$per")
    [ "$force" = "1" ] && args+=(--force-oneoff)
    [ "$DRY_RUN" = "1" ] && args+=(--dry-run)

    rm -f "$MANIFEST"
    python3 "$PY" "${args[@]}" --manifest "$MANIFEST" || return 1
    # Python 只生成清单；批量删除交给 rm（实测比逐文件 os.unlink 快三个数量级）。
    if [ "$DRY_RUN" != "1" ] && [ -s "$MANIFEST" ]; then
        xargs -0 -n 1000 rm -rf -- < "$MANIFEST" 2>/dev/null || true
    fi
    rm -f "$MANIFEST"
    [ "$DRY_RUN" = "1" ] || : > "$STAMP"
}

cmd_stale() {
    local days="$KEEP_DAYS" auto=0 arg
    for arg in "$@"; do
        case "$arg" in
            --auto) auto=1 ;;
            *) days="$arg" ;;
        esac
    done
    case "$days" in '' | *[!0-9]*) die "天数须为非负整数: ${days}" ;; esac
    run_gc "$days" "$KEEP_PER_CRATE" 0 "$auto"
    if [ "$DRY_RUN" = "1" ]; then
        ok "✓ dry-run 预览完成（未删除任何文件）"
    else
        ok "✓ GC 完成（每 crate 保留 ${KEEP_PER_CRATE} 份热缓存；可执行文件只留最近 2 小时链接的 1 份）"
    fi
}

cmd_slim() {
    has_target || return 0
    echo "== 清空 incremental（可再生缓存，下次编译重建）=="
    rm -rf "$REPO_ROOT/target/debug/incremental" \
        "$REPO_ROOT/target/release/incremental" \
        "$REPO_ROOT/target/wasm32-unknown-unknown/debug/incremental" \
        "$REPO_ROOT/target/wasm32-unknown-unknown/release/incremental" \
        "$REPO_ROOT/target/llvm-cov-target/debug/incremental" 2>/dev/null || true
    run_gc 1 1 1
    ok "✓ 深度瘦身完成（下次编译无需完整冷构建）"
}

cmd_all() {
    (cd "$REPO_ROOT" && cargo clean)
    (cd "$REPO_ROOT/frontend" && cargo clean) 2>/dev/null || true
    ok "✓ 已全量清理 target（下次编译为完整冷构建）"
}

cmd_dry_run() {
    DRY_RUN=1
    cmd_stale "$@"
}

SUB="${1:-}"
[ $# -gt 0 ] && shift || true

case "$SUB" in
    stale) cmd_stale "$@" ;;
    slim) cmd_slim ;;
    dry-run | --dry-run) cmd_dry_run "$@" ;;
    all) cmd_all ;;
    help | --help | -h | "")
        sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
        ;;
    *)
        err "未知子命令: ${SUB:-}（可选 stale [天数] / slim / dry-run / all）"
        exit 2
        ;;
esac
