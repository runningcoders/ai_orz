#!/bin/bash
# ai_orz - 生产服务生命周期（构建 / 启动 / 停止 / 状态 / 日志 / 重启）
#
# 【唯一实现】仓库与发布包共用本文件，环境差异由 lib/service.sh::in_repo_checkout() 判定，
# 不再维护两份「同源」的启动/停止脚本：
#   - 仓库  ：scripts/prod.sh（入口 ./scripts/ai_orz.sh，等价 make prod / make stop / ...）
#   - 发布包：script/prod.sh（package.sh 复制进去，script/start.sh 等别名全部指向它）
#
# Usage:
#   ./scripts/prod.sh build      构建 release：前端 dist/ + 后端二进制（仅仓库可用）
#   ./scripts/prod.sh start      后台启动（已运行则先优雅停止，幂等重启）
#   ./scripts/prod.sh start -f   前台运行（Ctrl+C 停止）
#   ./scripts/prod.sh stop       优雅停止（超时强杀前出示日志尾部定位卡点）
#   ./scripts/prod.sh status     查看 PID / 运行时长 / 资源占用 / 监听端口
#   ./scripts/prod.sh logs       实时跟踪日志（tail -F，自动跟随按日滚动）
#   ./scripts/prod.sh restart    停止后启动（不重新构建）
#
# 环境变量：
#   AI_ORZ_BIN            覆盖服务二进制路径
#   AI_ORZ_BASE_PATH      覆盖数据目录（默认 <根>/.ai_orz）
#   AI_ORZ_LISTEN_ADDR    覆盖监听地址（默认 0.0.0.0:3000）
#   AI_ORZ_STOP_TIMEOUT   覆盖优雅停止超时秒数（默认 30）

set -eu

# shellcheck source=./lib/service.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/service.sh"

CMD="${1:-start}"
[ $# -gt 0 ] && shift || true

STOP_TIMEOUT="${AI_ORZ_STOP_TIMEOUT:-30}"
BIN="$(resolve_bin)"
PID_FILE="$(pid_file_path)"
RUN_LOG="$(run_log_path)"
DAY_LOG="$(day_log_path)"
# 仓库：后端自身写按日文件日志，控制台层关闭，stderr 只接 panic 等绕过 tracing 的崩溃输出
BOOT_LOG="$DATA_DIR/logs/prod-boot.log"

listen_port() {
    local spec="${AI_ORZ_LISTEN_ADDR:-0.0.0.0:3000}"
    echo "${spec##*:}"
}

# ===== 构建（仅仓库：发布包里只有二进制，不含源码）=====
cmd_build() {
    if ! in_repo_checkout; then
        die "发布包不含源码，无需构建（直接 ./script/start.sh 启动）"
    fi
    setup_path
    cd "$REPO_ROOT"
    echo "🔨 构建 release（前端 dist/ + 后端二进制）..."
    "$SCRIPTS_DIR/build_frontend.sh"
    echo ""
    echo "🏗️  编译后端 (release)..."
    cargo build --release
    echo ""
    ok "✅ 构建完成"
    echo "   后端二进制: ${BLUE}$BIN${NC}"
    echo "   前端静态文件: ${BLUE}$REPO_ROOT/dist/${NC}"
}

# ===== 启动 =====
cmd_start() {
    local foreground=0
    if [ "${1:-}" = "-f" ] || [ "${1:-}" = "--foreground" ]; then
        foreground=1
    fi

    # 统一以「根」为工作目录：前端静态目录（dist）、数据目录等默认按相对路径解析，
    # 从任意路径调用本脚本行为一致（发布包可放在任意目录，同样成立）
    cd "$REPO_ROOT"

    if [ ! -x "$BIN" ]; then
        die "未找到服务二进制: ${BIN}（仓库先执行 make build 或 make prod）"
    fi

    if [ "$foreground" = "1" ]; then
        echo "🚀 前台启动 ai_orz（停止: Ctrl+C）..."
        exec "$BIN"
    fi

    mkdir -p "$DATA_DIR" "$(dirname "$BOOT_LOG")"

    # 幂等重启：已有实例先优雅停止（make prod 连跑两次 = 重启，不会双实例抢端口/文件锁）
    local old
    old=$(read_pid)
    if [ -n "$old" ]; then
        warn "⚠️  检测到运行中的实例 PID=${old}，先优雅停止..."
        cmd_stop
    fi

    local port
    port=$(listen_port)
    echo "🚀 后台启动 ai_orz..."
    echo "   二进制: ${BLUE}$BIN${NC}"
    echo "   监听:   ${BLUE}${AI_ORZ_LISTEN_ADDR:-0.0.0.0:3000}${NC}（AI_ORZ_LISTEN_ADDR 可覆盖）"

    if in_repo_checkout; then
        # 控制台层交给后端文件日志，stdout 静默、stderr 接崩溃输出（每次启动截断）
        : > "$BOOT_LOG"
        nohup env AI_ORZ_LOG_CONSOLE=0 "$BIN" </dev/null >/dev/null 2>"$BOOT_LOG" &
    else
        # 发布包：stdout/stderr 即唯一持久化来源，追加到 run.log
        mkdir -p "$DATA_DIR"
        "$BIN" >> "$RUN_LOG" 2>&1 &
    fi
    local pid=$!
    echo "$pid" > "$PID_FILE"

    echo "⏳ 等待服务就绪（业务日志: ${BLUE}$DAY_LOG${NC}）..."
    if wait_for_port localhost "$port" 60 "ai_orz 服务" "$pid"; then
        ok "✅ 服务已就绪"
        # ${pid} 必须带花括号：紧随其后的全角逗号会被 bash 当成变量名的一部分（UTF-8 下）
        echo "   地址: ${BLUE}http://localhost:${port}${NC}（PID ${pid}，PID 文件 ${PID_FILE}）"
        echo "   日志: ${BLUE}make logs${NC}   状态: ${BLUE}make status${NC}   停止: ${BLUE}make stop${NC}"
        return 0
    fi

    rm -f "$PID_FILE"
    err "❌ 服务未能就绪，最近输出:"
    in_repo_checkout && tail -n 20 "$BOOT_LOG" 2>/dev/null || true
    tail -n 20 "$RUN_LOG" 2>/dev/null || true
    exit 1
}

# ===== 停止 =====
# 停止目标：PID 文件优先；无 PID 文件（手动启动 / 文件丢失）按进程名兜底
cmd_stop() {
    local pid
    pid=$(read_pid)

    if [ -n "$pid" ]; then
        rm -f "$PID_FILE"
        echo "🛑 停止服务 PID=$pid ..."
        if ! graceful_stop "$pid" "$STOP_TIMEOUT" "$RUN_LOG" "$DAY_LOG"; then
            warn "⏰ 温和停止 ${STOP_TIMEOUT}s 未完成，最近日志（定位卡点）:"
            tail -n 15 "$RUN_LOG" 2>/dev/null || true
            tail -n 15 "$DAY_LOG" 2>/dev/null || true
            echo "💥 强杀 PID=$pid"
            kill -9 "$pid" 2>/dev/null || true
        fi
        ok "👋 服务已停止"
        return 0
    fi

    # 兜底：按进程名扫描（只匹配本环境的二进制，避免误伤同机其它实例）
    local pids pattern
    if in_repo_checkout; then
        pattern="target/(debug|release)/ai_orz( |$)"
    else
        pattern="$REPO_ROOT/ai_orz( |$)"
    fi
    pids=$(find_pids_by_cmd "$pattern")
    if [ -z "$(printf '%s' "$pids" | tr -d '[:space:]')" ]; then
        echo "✓ 服务未在运行"
        return 0
    fi

    echo "🛑 按进程名停止服务: $(printf '%s' "$pids" | tr '\n' ' ')"
    local p
    for p in $pids; do
        if ! graceful_stop "$p" "$STOP_TIMEOUT" "$RUN_LOG" "$DAY_LOG"; then
            echo "💥 强杀 PID=$p"
            kill -9 "$p" 2>/dev/null || true
        fi
    done
    ok "👋 服务已停止"
}

# ===== 状态 =====
cmd_status() {
    local pid
    pid=$(read_pid)
    if [ -z "$pid" ]; then
        echo "○ 未运行（启动: $(in_repo_checkout && echo 'make prod' || echo 'make start')）"
        return 0
    fi
    ok "● 运行中（PID ${pid}）"
    /bin/ps -p "$pid" -o pid,etime,%cpu,%mem,command
    local port
    port=$(listen_port)
    local listeners
    listeners=$(/usr/sbin/lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -i LISTEN | /usr/bin/head -n 3 || true)
    if [ -n "$listeners" ]; then
        echo "   端口 $port 监听:"
        printf '%s\n' "$listeners" | sed 's/^/     /'
    fi
}

# ===== 日志 =====
# 日志源双查：run.log（发布包 stdout 落盘）/ 按日业务日志（仓库与包共用）
cmd_logs() {
    local target=""
    if [ -f "$RUN_LOG" ]; then
        target="$RUN_LOG"
    elif [ -f "$DAY_LOG" ]; then
        target="$DAY_LOG"
    else
        warn "暂无日志文件（run.log / $(basename "$DAY_LOG") 均不存在）"
        echo "已有的日志文件:"
        ls -t "$DATA_DIR"/logs/ai_orz.log.* 2>/dev/null | head -n 5 || echo "  （无）"
        exit 1
    fi
    echo "📜 实时跟踪 ${target}（Ctrl+C 退出跟踪，不影响服务）"
    exec tail -F "$target"
}

cmd_help() {
    cat << 'EOF'
ai_orz - 生产服务生命周期（仓库与发布包共用同一实现）

用法: prod.sh <命令> [选项]

命令:
  build      构建 release（前端 dist/ + 后端二进制）        [仅仓库]
  start      后台启动（已运行则先优雅停止，幂等重启）
  start -f   前台运行（Ctrl+C 停止）
  stop       优雅停止（超时强杀前出示日志尾部）
  restart    停止后启动（不重新构建）
  status     查看 PID / 运行时长 / 资源占用 / 监听端口
  logs       实时跟踪日志（tail -F，自动跟随按日滚动）
  help       显示本帮助

环境变量:
  AI_ORZ_BIN           覆盖服务二进制路径
  AI_ORZ_BASE_PATH     覆盖数据目录（默认 <根>/.ai_orz）
  AI_ORZ_LISTEN_ADDR   覆盖监听地址（默认 0.0.0.0:3000）
  AI_ORZ_STOP_TIMEOUT  覆盖优雅停止超时秒数（默认 30）
EOF
}

case "$CMD" in
    build) cmd_build ;;
    start) cmd_start "$@" ;;
    stop) cmd_stop ;;
    restart) cmd_stop; cmd_start ;;
    status) cmd_status ;;
    logs|log) cmd_logs ;;
    help|--help|-h) cmd_help ;;
    *)
        err "未知命令: $CMD"
        cmd_help
        exit 2
        ;;
esac
