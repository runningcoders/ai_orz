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

# PATH 探测增强：服务器 / CI / IDE 调起的非交互 shell 常缺用户级与包管理器 bin
# （与 check_deps.sh 同款逻辑，保证 check 通过后 dx/cargo/node 实际可用）
for _dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin /usr/local/bin "$HOME/bin"; do
    if [ -d "$_dir" ]; then
        case ":$PATH:" in *":$_dir:"*) ;; *) PATH="$_dir:$PATH" ;; esac
    fi
done
if ! command -v node >/dev/null 2>&1 && [ -d "$HOME/.nvm/versions/node" ]; then
    _nvm_bin=$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | sort -V | tail -1)
    if [ -n "$_nvm_bin" ]; then
        PATH="$_nvm_bin:$PATH"
    fi
fi
export PATH

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

环境变量:
  DX_BACKEND_URL  覆盖前端 dev server 的 API 代理目标（dx 视角的 backend 地址）
                  默认 http://localhost:3000/api —— dx 与后端由本脚本同机启动，
                  本地 / 远程沙箱 / 服务器均无需设置（localhost 是 dx 进程视角）。
                  仅前后端分机部署（如前端 dev 在本机、后端在远端）时指定：
                    DX_BACKEND_URL=http://192.168.1.5:3000/api ./scripts/start.sh dev
                  设置后运行期间临时替换 frontend/Dioxus.toml，退出自动恢复。
EOF
}

# 打印当前生效的后端代理地址 + 未覆盖时给出设置引导（dev/frontend 模式共用）
print_backend_hint() {
    if [ -n "${DX_BACKEND_URL:-}" ]; then
        echo "🔀 后端 API 代理: ${BLUE}$DX_BACKEND_URL${NC} ${YELLOW}（已覆盖默认 localhost:3000）${NC}"
    else
        echo "🔗 后端 API 代理: ${BLUE}http://localhost:3000/api${NC}（与后端同机，默认即可）"
        echo "   前后端分机部署？用环境变量指向远端后端："
        echo "   ${YELLOW}DX_BACKEND_URL=http://<后端地址>:3000/api ./scripts/start.sh $MODE${NC}"
    fi
}

# 启动前预检：清理残留的 ai_orz 后端 / dx serve 前端进程与端口占用
# 逻辑收敛在 scripts/cleanup.sh（可独立执行：./scripts/cleanup.sh [--dry-run] 或 make clean-proc）
preflight_cleanup() {
    "$SCRIPT_DIR/cleanup.sh"
}

# 启动前预检：依赖检查（工具链缺失时给出精确安装命令，--fix 可自动装可自动项）
# 逻辑收敛在 scripts/check_deps.sh（可独立执行：./scripts/check_deps.sh [模式] [--fix] 或 make doctor）
preflight_deps() {
    if ! "$SCRIPT_DIR/check_deps.sh" "$MODE"; then
        echo ""
        echo "${YELLOW}💡 一键修复可自动项: ./scripts/check_deps.sh $MODE --fix（或 make doctor FIX=1）后重试${NC}"
        exit 1
    fi
}

# ===== DX_BACKEND_URL：后端不在本机时的 proxy 逃生舱 =====
# 默认不动：Dioxus.toml 的 proxy backend=http://localhost:3000 是「dx 进程视角」的
# loopback（dx serve 与后端由本脚本同机启动，localhost 恒正确，远程沙箱同理）。
# 仅当前后端分机部署（如前端 dev 在本机、后端在远端）时，设置：
#   DX_BACKEND_URL=http://192.168.1.5:3000/api ./scripts/start.sh dev
# 实现：启动 dx 前临时替换 Dioxus.toml 的 backend 行（备份 .dxbak），退出时恢复。
DX_BAK="$REPO_ROOT/frontend/Dioxus.toml.dxbak"

apply_dx_backend_override() {
    [ -n "${DX_BACKEND_URL:-}" ] || return 0
    if ! grep -q '^backend = ' "$REPO_ROOT/frontend/Dioxus.toml"; then
        echo "${RED}DX_BACKEND_URL 已设置但 Dioxus.toml 中未找到 backend 配置行${NC}" >&2
        exit 1
    fi
    cp "$REPO_ROOT/frontend/Dioxus.toml" "$DX_BAK"
    sed "s|^backend = \".*\"|backend = \"$DX_BACKEND_URL\"|" \
        "$REPO_ROOT/frontend/Dioxus.toml" > "$REPO_ROOT/frontend/Dioxus.toml.tmp" \
        && mv "$REPO_ROOT/frontend/Dioxus.toml.tmp" "$REPO_ROOT/frontend/Dioxus.toml"
    echo "${YELLOW}🔀 DX proxy backend 已临时覆盖为: $DX_BACKEND_URL（退出时自动恢复）${NC}"
}

restore_dx_backend_override() {
    if [ -f "$DX_BAK" ]; then
        mv "$DX_BAK" "$REPO_ROOT/frontend/Dioxus.toml"
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
    CLEANUP_DONE=0
    cleanup() {
        # 幂等保护：EXIT trap 与显式调用可能先后触发，只执行一次
        [ "$CLEANUP_DONE" = "1" ] && return 0
        CLEANUP_DONE=1
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
        restore_dx_backend_override
        echo "${GREEN}👋 服务已停止${NC}"
        exit 0
    }
    # EXIT：Ctrl+C / kill / set -e 异常退出时兜底恢复 Dioxus.toml（若被覆盖）
    trap cleanup INT TERM EXIT

    apply_dx_backend_override
    print_backend_hint

    echo "📦 启动后端开发服务器（冷构建可能需要数分钟）..."
    # 后端必须先启动：前端 dev server 通过 dx 反代 /api/* 到后端 3000，
    # 且前端启动后的身份回填 / Directory 预载等「默认依赖拉取」依赖后端已就绪，
    # 若后端尚未起好就会拉取失败、页面回落到默认值而显示异常。
    # 因此严格「先后端、再前端」：先 spawn 后端并等其端口就绪，再 spawn 前端。
    # --interactive=false / exec / process substitution 的说明见下方前端段。
    cargo run > >(awk '{ printf "📦 %s\n", $0; fflush() }') 2>&1 &
    BACKEND_PID=$!

    echo "⏳ 等待后端就绪（前端依赖后端 API，先确保后端可连接）..."
    if [ "$INT_RECEIVED" != "1" ] && wait_for_port localhost 3000 600 "后端 localhost:3000" "$BACKEND_PID"; then
        echo "${GREEN}✅ 后端就绪${NC}: ${BLUE}http://localhost:3000${NC}"
    fi

    echo "🎨 启动前端开发服务器（WASM 编译中）..."
    # --interactive=false：禁用 dx 的 TUI。TUI 会开启终端 raw mode（关闭 ISIG），
    # 导致 Ctrl+C 不再产生 SIGINT、整组进程都收不到信号，脚本 trap 永远不触发而卡死。
    # 关闭后 Ctrl+C 正常发信号给全组；热重载不受影响（文件监听驱动，与 TUI 无关）。
    # exec：让 FRONTEND_PID 直接指向 dx 进程（否则 kill 到的是子 shell，dx 会变孤儿进程）
    # > >(awk)：输出加 🎨 前缀，与后端 📦 日志区分（两者编译日志会交替输出）；
    # process substitution 保持 exec 语义，FRONTEND_PID 仍是 dx 本体，awk 随 dx 退出自动结束
    (cd frontend && exec dx serve --interactive=false) > >(awk '{ printf "🎨 %s\n", $0; fflush() }') 2>&1 &
    FRONTEND_PID=$!

    echo ""
    echo "⏳ 等待前端就绪，WASM 编译日志会持续输出（属正常现象，勿关闭窗口）..."
    echo ""

    if wait_for_port localhost 8080 600 "前端 localhost:8080" "$FRONTEND_PID"; then
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
    preflight_cleanup
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
    apply_dx_backend_override
    trap restore_dx_backend_override EXIT
    print_backend_hint
    cd "$REPO_ROOT/frontend"
    echo "🎨 启动前端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:8080${NC}"
    echo ""
    dx serve --interactive=false
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

    preflight_cleanup

    echo ""
    echo "🚀 启动生产服务..."
    echo "   监听: ${BLUE}${AI_ORZ_LISTEN_ADDR:-0.0.0.0:3000}${NC}（AI_ORZ_LISTEN_ADDR 可覆盖）"
    echo ""

    cd "$REPO_ROOT"
    ./target/release/ai_orz
}

# 主逻辑
case "$MODE" in
    help|--help|-h)
        print_help
        ;;
    *)
        # 依赖预检（help 之外的所有模式）：新机器上一次性提示缺什么、怎么装
        preflight_deps
        case "$MODE" in
            dev) cmd_dev ;;
            backend) cmd_backend ;;
            frontend) cmd_frontend ;;
            build) cmd_build ;;
            prod) cmd_prod ;;
            *)
                echo "${RED}未知模式: $MODE${NC}"
                echo ""
                print_help
                exit 1
                ;;
        esac
        ;;
esac
