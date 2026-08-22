#!/bin/bash
# ai_orz - 停止脚本（按 PID 文件优雅停止，超时强杀）

set -e

cd "$(cd "$(dirname "$0")/.." && pwd)"

PID_FILE=./.ai_orz/server.pid

if [ ! -f "$PID_FILE" ]; then
    echo "⚪ 服务未在运行（无 PID 文件）"
    exit 0
fi

pid=$(cat "$PID_FILE" 2>/dev/null || true)
rm -f "$PID_FILE"

if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
    echo "⚪ 服务未在运行（PID ${pid:-?} 已不存在）"
    exit 0
fi

echo "🛑 正在停止服务 (PID $pid)..."
kill "$pid" 2>/dev/null || true
for _ in $(seq 1 10); do
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "✅ 服务已停止"
        exit 0
    fi
    sleep 1
done

echo "⏰ 服务未在 10s 内退出，强制结束..."
kill -9 "$pid" 2>/dev/null || true
echo "✅ 服务已强制停止"
