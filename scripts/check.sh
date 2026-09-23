#!/bin/bash
# ai_orz - 代码门禁统一实现（格式化 / clippy / 测试 / 覆盖率 / 文档链接 / E2E）
#
# 【唯一实现】所有门禁命令只在这里写一遍，下列调用方全部转发到本脚本，口径不会漂移：
#   - 根目录 Makefile（make fmt / make clippy / make lint / make ci / make coverage ...）
#   - .githooks/pre-commit（= check.sh fmt-check）
#   - .githooks/pre-push （= check.sh pre-push：fmt + clippy + clippy-fe + dx check）
# 注：GitHub Actions（.github/workflows/rust.yml）按 job 粒度拆分，保持显式内联不复用本脚本，
#     但命令与本脚本严格同口径（见各命令注释）。
#
# Usage:
#   ./scripts/ai_orz.sh check <命令>       # 统一入口
#   ./scripts/check.sh <命令>              # 等价别名
#
# 命令:
#   fmt / fmt-check      格式化 / 格式检查（CI fmt job 口径）
#   clippy               clippy -D warnings（CI lint job 口径，需 protoc）
#   clippy-fe            前端 wasm32 clippy（CI frontend job 口径）
#   docs-lint            文档链接规范门禁
#   docs-migrate         文档链接批量迁移（默认 dry-run，APPLY=1 写盘）
#   seed-sync            预置技能同步到运行期数据目录（默认 dry-run，APPLY=1 写盘）
#   test-be / test-fe / test   后端 / 前端 / 全量测试
#   lint                 全部静态检查 = fmt-check + clippy + clippy-fe + docs-lint
#   ci                   lint + 全量测试（与 pre-push 钩子同口径）
#   coverage             覆盖率门禁（FAIL_UNDER 默认 45，PR 口径 38）
#   dx-check             前端 dioxus 构建检查（dx 缺失则跳过，不阻塞）
#   e2e                  Playwright E2E（仅本地，已移出 CI）
#   pre-commit / pre-push  钩子组合命令

set -eu

# shellcheck source=./lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"
setup_path

CMD="${1:-help}"
[ $# -gt 0 ] && shift || true

cd "$REPO_ROOT"

step() { echo ""; echo "== $1 =="; }

# ===== 格式化 =====
cmd_fmt() { step "fmt（cargo fmt --all）"; cargo fmt --all; }

cmd_fmt_check() {
    if ! cargo fmt --all -- --check; then
        err "✗ 格式检查未通过，运行 make fmt 修复后重试。"
        exit 1
    fi
    ok "✓ fmt-check 通过"
}

# ===== 静态检查 =====
# --workspace 不可省：根 Cargo.toml 带 [package]，cargo 默认只选 default member（= 根包 ai_orz），
# common / tools / ai-orz-macros 的 lint 会被静默跳过。
# frontend 由 clippy-fe 以 wasm32 口径全量覆盖（实际运行目标），此处排除以免 native 重复编译。
cmd_clippy() {
    if ! cargo clippy --workspace --exclude frontend --all-targets -- -D warnings; then
        err "✗ clippy 检查未通过。"
        exit 1
    fi
    ok "✓ clippy 通过"
}

cmd_clippy_fe() {
    if ! (cd frontend && cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings); then
        err "✗ 前端 clippy 未通过。"
        exit 1
    fi
    ok "✓ clippy-fe 通过"
}

cmd_docs_lint() { cargo run -p ai-orz-tools --bin docs_lint; }

cmd_docs_migrate() {
    if [ "${APPLY:-0}" = "1" ]; then
        echo "== APPLY 模式：写盘 =="
        cargo run -p ai-orz-tools --bin docs_migrate -- --apply
    else
        echo "== dry-run 模式（预览不写盘；确认后 APPLY=1 make docs-migrate）=="
        cargo run -p ai-orz-tools --bin docs_migrate
    fi
}

# 预置技能同步：把仓库 seed（default.json + skills/<ID>/skill.md）刷到运行期数据目录的
# 「预置技能行本体 + 各 Agent 装机副本」。技能没有内置工具那样的启动同步机制，改 seed 对存量
# Agent 零效果（副本照旧文说）—— 详见 tools/src/bin/sync_seed_skill.rs 头部说明。
# SKILL=<ID> 只同步一个技能；不传即 --all。
cmd_seed_sync() {
    local args=()
    if [ -n "${SKILL:-}" ]; then
        args+=("$SKILL")
    else
        args+=("--all")
    fi
    if [ "${APPLY:-0}" = "1" ]; then
        echo "== APPLY 模式：写盘 =="
        args+=("--apply")
    else
        echo "== dry-run 模式（预览不写盘；确认后 APPLY=1 make seed-sync）=="
    fi
    cargo run -p ai-orz-tools --bin sync_seed_skill -- "${args[@]}"
}

# dx check：dioxus 路由级 + 打包构建校验，作 clippy-fe 之外的**补充**门禁 ——
# 不顶替 clippy-fe：dx check 不编译完整业务代码，曾出现「14 个编译错误仍全绿」的假绿灯。
# 本机未装 dx 时跳过（不阻塞）。
cmd_dx_check() {
    if ! command -v dx >/dev/null 2>&1; then
        warn "⚠ dx 未安装，跳过 dx check（clippy-fe 已覆盖完整类型检查）。安装：cargo install dioxus-cli"
        return 0
    fi
    if ! (cd frontend && dx check); then
        err "✗ dx check 未通过。"
        exit 1
    fi
    ok "✓ dx check 通过"
}

cmd_lint() {
    step "lint（fmt-check + clippy + clippy-fe + docs-lint）"
    cmd_fmt_check
    cmd_clippy
    cmd_clippy_fe
    cmd_docs_lint
}

# ===== 测试 =====
cmd_test_be() {
    cargo test --workspace --exclude frontend --lib
    cargo test --workspace --exclude frontend --test '*'
}

cmd_test_fe() { (cd frontend && cargo test); }

cmd_test() { cmd_test_be; cmd_test_fe; }

cmd_ci() { cmd_lint; step "test（后端 + 前端）"; cmd_test; }

# ===== 覆盖率 =====
cmd_coverage() {
    local under="${FAIL_UNDER:-45}"
    local ignore="(tests/common/|/cargo/registry/|/rustc/|build.rs|target/)"
    cargo llvm-cov --workspace --tests --no-clean --no-fail-fast --ignore-filename-regex "$ignore"
    cargo llvm-cov report --ignore-filename-regex "$ignore" --fail-under-lines "$under"
}

cmd_e2e() { (cd tests/e2e && npx playwright test); }

# ===== 钩子组合 =====
# pre-commit：只跑秒级 fmt（与 CI fmt job 同口径），重门禁留给 pre-push
cmd_pre_commit() {
    step "pre-commit: fmt-check"
    cmd_fmt_check
}

# pre-push：fmt + clippy + clippy-fe + dx check（dx 缺失自动跳过）
cmd_pre_push() {
    step "pre-push: fmt + clippy + clippy-fe + dx check"
    cmd_fmt_check
    cmd_clippy
    cmd_clippy_fe
    cmd_dx_check
}

cmd_help() {
    cat << 'EOF'
ai_orz - 代码门禁（make 与各 git 钩子的共同实现）

用法: ./scripts/check.sh <命令>

  fmt / fmt-check   格式化 / 格式检查
  clippy            后端与共享 crate 的 clippy（-D warnings）
  clippy-fe         前端 wasm32 clippy
  docs-lint         文档链接规范门禁
  docs-migrate      文档链接迁移（APPLY=1 写盘）
  seed-sync         预置技能同步到数据目录（SKILL=<ID> 限定单个；APPLY=1 写盘）
  test-be/test-fe/test   后端 / 前端 / 全量测试
  lint              全部静态检查
  ci                lint + 全量测试
  coverage          覆盖率门禁（FAIL_UNDER 默认 45）
  dx-check          dioxus 构建检查（dx 缺失则跳过）
  e2e               Playwright E2E（仅本地）
  pre-commit        提交钩子口径（fmt-check）
  pre-push          推送钩子口径（fmt + clippy + clippy-fe + dx check）
EOF
}

case "$CMD" in
    fmt) cmd_fmt ;;
    fmt-check) cmd_fmt_check ;;
    clippy) cmd_clippy ;;
    clippy-fe) cmd_clippy_fe ;;
    docs-lint) cmd_docs_lint ;;
    docs-migrate) cmd_docs_migrate ;;
    seed-sync) cmd_seed_sync ;;
    test-be) cmd_test_be ;;
    test-fe) cmd_test_fe ;;
    test) cmd_test ;;
    lint) cmd_lint ;;
    ci) cmd_ci ;;
    coverage) cmd_coverage ;;
    dx-check) cmd_dx_check ;;
    e2e) cmd_e2e ;;
    pre-commit) cmd_pre_commit ;;
    pre-push) cmd_pre_push ;;
    help|--help|-h) cmd_help ;;
    *)
        err "未知命令: $CMD"
        cmd_help
        exit 2
        ;;
esac
