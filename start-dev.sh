#!/bin/bash
# ai_orz - 全栈启动脚本
# 同时启动后端和前端开发服务器

set -e

# 加载 rustup 环境
source "$HOME/.cargo/env"

echo "🚀 Starting ai_orz full-stack development server..."
echo ""

# 启动后端
echo "📦 Starting backend server on http://localhost:3000"
cd "$(dirname "$0")"
cargo run &
BACKEND_PID=$!

# 等待后端启动
sleep 2

# 启动前端开发服务器（dioxus-cli 已安装，使用官方热重载）
echo "🎨 Starting frontend dev server with dioxus-cli on http://localhost:8080"
cd frontend
dx serve &
FRONTEND_PID=$!

echo ""
echo "✅ Both servers started!"
echo "📍 Backend API: http://localhost:3000"
echo "📍 Frontend UI: http://localhost:8080"
echo ""
echo "Press Ctrl+C to stop both servers"

# 等待任意进程退出
wait $BACKEND_PID $FRONTEND_PID

# 清理
kill $BACKEND_PID 2>/dev/null || true
kill $FRONTEND_PID 2>/dev/null || true

echo ""
echo "👋 Servers stopped"
