#!/bin/bash
# ai_orz - 统一启动脚本
# 一个脚本覆盖开发、生产、构建、单服务启动所有场景
#
# Usage:
#   ./scripts/start.sh dev      开发模式（后端 cargo run + 前端 dx serve）【默认】
#   ./scripts/start.sh prod     生产模式（编译 + 运行 release 二进制）
#   ./scripts/start.sh build    仅编译（前端 release + 后端 release）
#   ./scripts/start.sh backend  仅启动后端（cargo run）
#   ./scripts/start.sh frontend 仅启动前端开发服务器（dx serve）
#   ./scripts/start.sh help     显示帮助
# 也可通过根目录 Makefile 路由：make dev / make prod / make build / make run / make serve

set -e

# 加载 rustup 环境
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODE="${1:-dev}"

# 颜色输出（使用实际转义字符，避免 echo -e 兼容性问题）
RED=$(printf '\033[0;31m')
GREEN=$(printf '\033[0;32m')
YELLOW=$(printf '\033[0;33m')
BLUE=$(printf '\033[0;34m')
NC=$(printf '\033[0m') # No Color

print_banner() {
    echo ""
    echo "🚀 ai_orz 启动脚本"
    echo "   模式: ${GREEN}$MODE${NC}"
    echo ""
}

print_help() {
    cat << 'EOF'
ai_orz - 统一启动脚本

用法: ./scripts/start.sh [模式]

模式:
  dev       开发模式（默认）：同时启动后端 cargo run + 前端 dx serve
            后端: http://localhost:3000 | 前端: http://localhost:8080

  prod      生产模式：编译 release 版本并运行生产二进制
            服务监听 0.0.0.0:3000，前端静态文件从 dist/ 提供

  build     仅编译：编译前端 release + 后端 release，不启动服务

  backend   仅启动后端：cargo run（开发热重载）

  frontend  仅启动前端：dx serve（开发热重载）

  help      显示此帮助信息

示例:
  ./scripts/start.sh dev      # 开发全栈
  ./scripts/start.sh prod     # 生产部署
  ./scripts/start.sh build    # CI 构建
  ./scripts/start.sh backend  # 只跑后端 API
  或等价 make 命令：make dev / make prod / make build / make run / make serve
EOF
}

# 启动前预检：清理残留的 ai_orz 后端 / dx serve 前端进程与端口占用
# 避免 DuckDB 文件锁冲突（单写者）和 dx 构建锁争抢
preflight_cleanup() {
    local cleaned=0

    # 残留后端二进制进程（上次未正常退出，持有 .ai_orz/stats.duckdb 锁）
    local stale_be
    stale_be=$(/bin/ps aux | /usr/bin/grep -E "target/(debug|release)/ai_orz( |$)" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}')
    for pid in $stale_be; do
        echo "${YELLOW}🧹 清理残留后端进程 PID=$pid（避免 DuckDB 锁冲突）${NC}"
        kill "$pid" 2>/dev/null || true
        cleaned=1
    done

    # 残留 dx serve 进程（持有 8080 端口与构建锁）
    local stale_dx
    stale_dx=$(/bin/ps aux | /usr/bin/grep -E "dx serve( |$)" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}')
    for pid in $stale_dx; do
        echo "${YELLOW}🧹 清理残留 dx serve 进程 PID=$pid（释放 8080 端口）${NC}"
        kill "$pid" 2>/dev/null || true
        cleaned=1
    done

    if [ "$cleaned" = "1" ]; then
        sleep 1
        # 温和杀不掉的强杀
        for pid in $stale_be $stale_dx; do
            kill -9 "$pid" 2>/dev/null || true
        done
        sleep 1
    fi
}

# 等待端口就绪（纯 bash /dev/tcp，无外部依赖）
# 用法: wait_for_port <host> <port> <超时秒> <描述> [监控PID]
wait_for_port() {
    local host=$1 port=$2 timeout=${3:-600} desc=$4 monitor_pid=${5:-}
    local elapsed=0
    while ! (echo > "/dev/tcp/$host/$port") 2>/dev/null; do
        # Ctrl+C 被按下：立即退出等待，让外层 trap cleanup 生效
        if [ "$INT_RECEIVED" = "1" ]; then
            return 1
        fi
        # 被监控进程已退出（编译失败等），提前结束等待
        if [ -n "$monitor_pid" ] && ! kill -0 "$monitor_pid" 2>/dev/null; then
            echo "${RED}❌ $desc 进程已退出（疑似编译失败），请检查上方日志${NC}"
            return 1
        fi
        if [ "$elapsed" -ge "$timeout" ]; then
            echo "${RED}⏰ 等待 $desc 超时（${timeout}s），请检查上方编译日志${NC}"
            return 1
        fi
        # sleep 放子进程跑，INT 信号到来时立即打断 sleep（不阻塞 trap）
        sleep 2 &
        wait $!
        elapsed=$((elapsed + 2))
    done
    return 0
}

# 开发模式：同时启动后端 + 前端
cmd_dev() {
    print_banner

    preflight_cleanup
    cd "$REPO_ROOT"

    # 中断标志：trap 里置 1，轮询循环检测到即退出
    INT_RECEIVED=0
    cleanup() {
        INT_RECEIVED=1
        echo ""
        echo "🛑 正在停止服务..."
        kill $BACKEND_PID 2>/dev/null || true
        kill $FRONTEND_PID 2>/dev/null || true
        # 给进程 2 秒优雅退出时间，杀不掉再强杀
        sleep 2
        kill -9 $BACKEND_PID $FRONTEND_PID 2>/dev/null || true
        wait $BACKEND_PID 2>/dev/null || true
        wait $FRONTEND_PID 2>/dev/null || true
        echo "${GREEN}👋 服务已停止${NC}"
        exit 0
    }
    trap cleanup INT TERM

    echo "📦 启动后端开发服务器（先编译后运行，冷构建可能需要数分钟）..."
    cargo run &
    BACKEND_PID=$!

    echo "🎨 启动前端开发服务器（WASM 编译中）..."
    # exec：让 FRONTEND_PID 直接指向 dx 进程（否则 kill 到的是子 shell，dx 会变孤儿进程）
    (cd frontend && exec dx serve) &
    FRONTEND_PID=$!

    echo ""
    echo "⏳ 等待服务就绪，编译日志会持续输出（属正常现象，勿关闭窗口）..."
    echo ""

    if wait_for_port localhost 3000 600 "后端 localhost:3000" "$BACKEND_PID"; then
        echo "${GREEN}✅ 后端就绪${NC}: ${BLUE}http://localhost:3000${NC}"
    fi
    if [ "$INT_RECEIVED" != "1" ] && wait_for_port localhost 8080 600 "前端 localhost:8080" "$FRONTEND_PID"; then
        echo "${GREEN}✅ 前端就绪${NC}: ${BLUE}http://localhost:8080${NC}"
        echo "   （浏览器若仍显示编译页，等 WASM 编译完成会自动刷新）"
    fi

    echo ""
    echo "按 Ctrl+C 停止所有服务"
    echo ""

    wait $BACKEND_PID $FRONTEND_PID
    cleanup
}

# 仅启动后端
cmd_backend() {
    print_banner
    cd "$REPO_ROOT"
    echo "📦 启动后端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:3000${NC}"
    echo ""
    cargo run
}

# 仅启动前端
cmd_frontend() {
    print_banner
    preflight_cleanup
    cd "$REPO_ROOT/frontend"
    echo "🎨 启动前端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:8080${NC}"
    echo ""
    dx serve
}

# 仅构建
cmd_build() {
    print_banner
    cd "$REPO_ROOT"

    echo "🔨 开始编译..."
    echo ""

    # 编译前端（同目录 build_frontend.sh，与 CI e2e job 共用，保证「产物怎么进 dist/」只有一处逻辑）
    echo "🎨 编译前端 (release)..."
    "$SCRIPT_DIR/build_frontend.sh"

    # 编译后端
    echo ""
    echo "🏗️  编译后端 (release)..."
    cd "$REPO_ROOT"
    cargo build --release

    echo ""
    echo "${GREEN}✅ 编译完成${NC}"
    echo "   后端二进制: ${BLUE}./target/release/ai_orz${NC}"
    echo "   前端静态文件: ${BLUE}./dist/${NC}"
}

# 生产模式：构建 + 运行
cmd_prod() {
    print_banner

    cmd_build

    echo ""
    echo "🚀 启动生产服务..."
    echo "   监听: ${BLUE}0.0.0.0:${SERVER_PORT:-3000}${NC}"
    echo ""

    cd "$REPO_ROOT"
    ./target/release/ai_orz
}

# 主逻辑
case "$MODE" in
    dev)
        cmd_dev
        ;;
    backend)
        cmd_backend
        ;;
    frontend)
        cmd_frontend
        ;;
    build)
        cmd_build
        ;;
    prod)
        cmd_prod
        ;;
    help|--help|-h)
        print_help
        ;;
    *)
        echo "${RED}未知模式: $MODE${NC}"
        echo ""
        print_help
        exit 1
        ;;
esac
