#!/bin/bash
# ai_orz - 开发态启动（dev / backend / frontend）
#
# 【唯一实现】「怎么把开发服务跑起来」只在这里；生产态见 scripts/prod.sh。
# 入口统一走 ./scripts/ai_orz.sh dev|backend|frontend（等价 make dev / make run / make serve）。
#
# Usage:
#   ./scripts/run.sh dev       后端 cargo run + 前端 dx serve（默认）
#   ./scripts/run.sh backend   仅后端 cargo run（http://localhost:3000）
#   ./scripts/run.sh frontend  仅前端 dx serve（http://localhost:8080）
#
# 环境变量:
#   DX_BACKEND_URL   前后端分机部署时覆盖前端 dev server 的 API 代理目标
#                    （默认 http://localhost:3000/api；设置后临时改写 frontend/Dioxus.toml，退出自动恢复）

set -eu

# shellcheck source=./lib/service.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/service.sh"
# IDE / CI / Makefile 调起的非交互 shell 可能缺 cargo / dx，先补齐 PATH 再启动
setup_path

MODE="${1:-dev}"

INT_RECEIVED=0
DX_BAK="$REPO_ROOT/frontend/Dioxus.toml.dxbak"

print_backend_hint() {
    if [ -n "${DX_BACKEND_URL:-}" ]; then
        echo "🔀 后端 API 代理: ${BLUE}$DX_BACKEND_URL${NC} ${YELLOW}（已覆盖默认 localhost:3000）${NC}"
    else
        echo "🔗 后端 API 代理: ${BLUE}http://localhost:3000/api${NC}（与后端同机，默认即可）"
        echo "   前后端分机部署？用环境变量指向远端后端："
        echo "   ${YELLOW}DX_BACKEND_URL=http://<后端地址>:3000/api ./scripts/ai_orz.sh $MODE${NC}"
    fi
}

# DX_BACKEND_URL：后端不在本机时的 proxy 逃生舱
# 默认不动 Dioxus.toml —— 其 backend=http://localhost:3000 是「dx 进程视角」的 loopback，
# dx serve 与后端由本脚本同机启动，localhost 恒正确（远程沙箱同理）。
apply_dx_backend_override() {
    [ -n "${DX_BACKEND_URL:-}" ] || return 0
    if ! grep -q '^backend = ' "$REPO_ROOT/frontend/Dioxus.toml"; then
        die "DX_BACKEND_URL 已设置但 Dioxus.toml 中未找到 backend 配置行"
    fi
    cp "$REPO_ROOT/frontend/Dioxus.toml" "$DX_BAK"
    sed "s|^backend = \".*\"|backend = \"$DX_BACKEND_URL\"|" \
        "$REPO_ROOT/frontend/Dioxus.toml" > "$REPO_ROOT/frontend/Dioxus.toml.tmp" \
        && mv "$REPO_ROOT/frontend/Dioxus.toml.tmp" "$REPO_ROOT/frontend/Dioxus.toml"
    warn "🔀 DX proxy backend 已临时覆盖为: ${DX_BACKEND_URL}（退出时自动恢复）"
}

restore_dx_backend_override() {
    if [ -f "$DX_BAK" ]; then
        mv "$DX_BAK" "$REPO_ROOT/frontend/Dioxus.toml"
    fi
}

# 启动前预检：残留进程清理 + 依赖检查（两者都是各自的唯一实现）
preflight() {
    "$SCRIPTS_DIR/cleanup.sh"
    if ! "$SCRIPTS_DIR/check_deps.sh" "$MODE"; then
        echo ""
        warn "💡 一键修复可自动项: ./scripts/check_deps.sh $MODE --fix（或 make doctor FIX=1）后重试"
        exit 1
    fi
}

# 开发态：先后端、再前端
cmd_dev() {
    cd "$REPO_ROOT"

    # 后端必须先启动：前端 dev server 通过 dx 反代 /api/* 到后端 3000，
    # 且前端启动后的身份回填 / Directory 预载等「默认依赖拉取」依赖后端已就绪，
    # 后端未起好会导致拉取失败、页面回落默认值显示异常。
    CLEANUP_DONE=0
    cleanup() {
        [ "$CLEANUP_DONE" = "1" ] && return 0
        CLEANUP_DONE=1
        INT_RECEIVED=1
        echo ""
        echo "🛑 正在停止服务..."
        kill "${BACKEND_PID:-0}" 2>/dev/null || true
        kill "${FRONTEND_PID:-0}" 2>/dev/null || true
        # 事件驱动等待：后端优雅退出含 10s drain 窗口 + DuckDB flush 编排，
        # 固定 sleep 会在落盘前强杀；全部退出立即继续，15s 仅兜底
        local waited=0
        while [ "$waited" -lt 30 ]; do
            pid_alive "${BACKEND_PID:-0}" || pid_alive "${FRONTEND_PID:-0}" || break
            sleep 0.5
            waited=$((waited + 1))
        done
        kill -9 "${BACKEND_PID:-0}" "${FRONTEND_PID:-0}" 2>/dev/null || true
        wait "${BACKEND_PID:-0}" 2>/dev/null || true
        wait "${FRONTEND_PID:-0}" 2>/dev/null || true
        restore_dx_backend_override
        ok "👋 服务已停止"
        exit 0
    }
    # EXIT：Ctrl+C / kill / set -e 异常退出时兜底恢复 Dioxus.toml（若被覆盖）
    trap cleanup INT TERM EXIT

    apply_dx_backend_override
    print_backend_hint

    echo "📦 启动后端开发服务器（冷构建可能需要数分钟）..."
    # > >(awk)：输出加 📦 前缀，与前端 🎨 日志区分（两者编译日志会交替输出）；
    # process substitution 保持 exec 语义，awk 随进程退出自动结束
    cargo run > >(awk '{ printf "📦 %s\n", $0; fflush() }') 2>&1 &
    BACKEND_PID=$!

    echo "⏳ 等待后端就绪（前端依赖后端 API，先确保后端可连接）..."
    if [ "$INT_RECEIVED" != "1" ] && wait_for_port localhost 3000 600 "后端 localhost:3000" "$BACKEND_PID"; then
        ok "✅ 后端就绪: ${BLUE}http://localhost:3000${NC}"
    fi

    echo "🎨 启动前端开发服务器（WASM 编译中）..."
    # --interactive=false：禁用 dx TUI。TUI 开启终端 raw mode（关闭 ISIG），
    # Ctrl+C 不再产生 SIGINT，整组进程收不到信号 → 脚本 trap 永不触发而卡死。
    # 热重载不受影响（文件监听驱动，与 TUI 无关）。
    # exec：让 FRONTEND_PID 直接指向 dx 进程（否则 kill 到的是子 shell，dx 变孤儿）
    (cd frontend && exec dx serve --interactive=false) > >(awk '{ printf "🎨 %s\n", $0; fflush() }') 2>&1 &
    FRONTEND_PID=$!

    echo ""
    echo "⏳ 等待前端就绪，WASM 编译日志会持续输出（属正常现象，勿关闭窗口）..."
    echo ""

    if wait_for_port localhost 8080 600 "前端 localhost:8080" "$FRONTEND_PID"; then
        ok "✅ 前端就绪: ${BLUE}http://localhost:8080${NC}"
        echo "   （浏览器若仍显示编译页，等 WASM 编译完成会自动刷新）"
    fi

    echo ""
    echo "按 Ctrl+C 停止所有服务"
    echo ""

    wait $BACKEND_PID $FRONTEND_PID
    cleanup
}

cmd_backend() {
    cd "$REPO_ROOT"
    echo "📦 启动后端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:3000${NC}"
    echo ""
    cargo run
}

cmd_frontend() {
    apply_dx_backend_override
    trap restore_dx_backend_override EXIT
    print_backend_hint
    cd "$REPO_ROOT/frontend"
    echo "🎨 启动前端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:8080${NC}"
    echo ""
    dx serve --interactive=false
}

echo ""
echo "🚀 ai_orz 开发态启动（模式: ${GREEN}$MODE${NC}）"

case "$MODE" in
    dev)
        preflight
        cmd_dev
        ;;
    backend)
        preflight
        cmd_backend
        ;;
    frontend)
        preflight
        cmd_frontend
        ;;
    *)
        die "未知模式: ${MODE}（可选: dev / backend / frontend）"
        ;;
esac
