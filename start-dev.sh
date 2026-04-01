#!/bin/bash
# ai_orz - 全栈启动脚本
# 同时启动后端和前端开发服务器

set -e

echo "🚀 Starting ai_orz full-stack development server..."
echo ""

# 检查 dioxus-cli 是否安装
if ! command -v dx &> /dev/null; then
    echo "⚠️  dioxus-cli not found. Installing..."
    cargo install dioxus-cli
fi

# 启动后端
echo "📦 Starting backend server on http://localhost:3000"
cd "$(dirname "$0")"
cargo run &
BACKEND_PID=$!

# 等待后端启动
sleep 2

# 启动前端
echo "🎨 Starting frontend dev server on http://localhost:8080"
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
