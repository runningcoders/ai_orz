#!/bin/bash
# ai_orz - 统一启动脚本
# 一个脚本覆盖开发、生产、构建、单服务启动所有场景
#
# Usage:
#   ./start.sh dev      开发模式（后端 cargo run + 前端 dx serve）【默认】
#   ./start.sh prod     生产模式（编译 + 运行 release 二进制）
#   ./start.sh build    仅编译（前端 release + 后端 release）
#   ./start.sh backend  仅启动后端（cargo run）
#   ./start.sh frontend 仅启动前端开发服务器（dx serve）
#   ./start.sh help     显示帮助

set -e

# 加载 rustup 环境
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
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

用法: ./start.sh [模式]

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
  ./start.sh dev      # 开发全栈
  ./start.sh prod     # 生产部署
  ./start.sh build    # CI 构建
  ./start.sh backend  # 只跑后端 API
EOF
}

# 开发模式：同时启动后端 + 前端
cmd_dev() {
    print_banner

    cd "$SCRIPT_DIR"

    echo "📦 启动后端开发服务器..."
    cargo run &
    BACKEND_PID=$!

    # 等待后端基本就绪
    sleep 2

    echo "🎨 启动前端开发服务器..."
    cd frontend
    dx serve &
    FRONTEND_PID=$!

    echo ""
    echo "${GREEN}✅ 双服务已启动${NC}"
    echo "   后端 API: ${BLUE}http://localhost:3000${NC}"
    echo "   前端 UI:  ${BLUE}http://localhost:8080${NC}"
    echo ""
    echo "按 Ctrl+C 停止所有服务"
    echo ""

    # 捕获中断信号，优雅终止子进程
    cleanup() {
        echo ""
        echo "🛑 正在停止服务..."
        kill $BACKEND_PID 2>/dev/null || true
        kill $FRONTEND_PID 2>/dev/null || true
        wait $BACKEND_PID 2>/dev/null || true
        wait $FRONTEND_PID 2>/dev/null || true
        echo "${GREEN}👋 服务已停止${NC}"
        exit 0
    }
    trap cleanup INT TERM

    wait $BACKEND_PID $FRONTEND_PID
    cleanup
}

# 仅启动后端
cmd_backend() {
    print_banner
    cd "$SCRIPT_DIR"
    echo "📦 启动后端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:3000${NC}"
    echo ""
    cargo run
}

# 仅启动前端
cmd_frontend() {
    print_banner
    cd "$SCRIPT_DIR/frontend"
    echo "🎨 启动前端开发服务器..."
    echo "   地址: ${BLUE}http://localhost:8080${NC}"
    echo ""
    dx serve
}

# 仅构建
cmd_build() {
    print_banner
    cd "$SCRIPT_DIR"

    echo "🔨 开始编译..."
    echo ""

    # 编译前端
    echo "🎨 编译前端 (release)..."
    cd frontend
    export BACKEND_API_URL=${BACKEND_API_URL:-http://localhost:3000}
    
    # dx build 在 wasm-opt 失败时仍会生成产物，忽略此错误
    dx build --release 2>&1 || true

    mkdir -p ../dist
    mkdir -p ../dist/wasm
    cp index.html ../dist/

    # 查找编译产物（dx 可能输出到不同位置）
    DX_OUTPUT_DIR=""
    if [ -d target/dx/frontend/release/web/public ]; then
        DX_OUTPUT_DIR="target/dx/frontend/release/web/public"
    elif [ -d target/dx/frontend/web/public ]; then
        DX_OUTPUT_DIR="target/dx/frontend/web/public"
    elif [ -d pkg ]; then
        DX_OUTPUT_DIR="pkg"
    fi

    if [ -n "$DX_OUTPUT_DIR" ]; then
        # 复制 .wasm 和 .js 文件到 dist/wasm/
        if [ -f "$DX_OUTPUT_DIR/frontend_bg.wasm" ]; then
            cp "$DX_OUTPUT_DIR/frontend_bg.wasm" ../dist/wasm/
        fi
        if [ -f "$DX_OUTPUT_DIR/frontend.js" ]; then
            cp "$DX_OUTPUT_DIR/frontend.js" ../dist/wasm/
        fi
        # 复制整个 wasm 子目录（如果存在）
        if [ -d "$DX_OUTPUT_DIR/wasm" ]; then
            cp -r "$DX_OUTPUT_DIR/wasm"/* ../dist/wasm/
        fi
        echo "${GREEN}✅ 前端编译产物已复制${NC}"
        echo "   来源: ${BLUE}$DX_OUTPUT_DIR${NC}"
    else
        echo "${RED}⚠️  未找到前端编译产物${NC}"
        exit 1
    fi

    # 编译后端
    echo ""
    echo "🏗️  编译后端 (release)..."
    cd "$SCRIPT_DIR"
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

    cd "$SCRIPT_DIR"
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
