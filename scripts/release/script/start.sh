#!/bin/bash
# ai_orz - 启动脚本（默认后台守护，可前台运行）
#
# 用法:
#   ./script/start.sh        后台启动（推荐），stdout/stderr 追加到 .ai_orz/run.log
#   ./script/start.sh -f     前台启动（Ctrl+C 停止，等价直接 ./ai_orz）
#   make start               等价后台启动
#
# 首次启动会自动生成 .ai_orz/（配置/数据库/日志）。

set -e

# 自动 cd 到包根目录，保证相对路径正确（发布包可放在任意路径）
cd "$(cd "$(dirname "$0")/.." && pwd)"

BIN=./ai_orz
DATA_DIR=./.ai_orz
RUN_LOG="$DATA_DIR/run.log"
PID_FILE="$DATA_DIR/server.pid"

mkdir -p "$DATA_DIR"

is_running() {
    [ -f "$PID_FILE" ] || return 1
    local pid
    pid=$(cat "$PID_FILE" 2>/dev/null || true)
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

FOREGROUND=0
if [ "${1:-}" = "-f" ] || [ "${1:-}" = "--foreground" ]; then
    FOREGROUND=1
fi

if [ "$FOREGROUND" = "1" ]; then
    echo "🚀 前台启动 AI Orz..."
    echo "   停止: Ctrl+C"
    exec "$BIN"
fi

if is_running; then
    echo "⚠️  服务已在运行 (PID $(cat "$PID_FILE"))，如需重启: ./script/restart.sh"
    exit 0
fi

"$BIN" >> "$RUN_LOG" 2>&1 &
echo $! > "$PID_FILE"

# 短暂等待，启动即失败时给出日志（如端口被占用）
sleep 2
if ! kill -0 "$(cat "$PID_FILE" 2>/dev/null || true)" 2>/dev/null; then
    echo "❌ 服务启动失败，最近日志如下："
    tail -30 "$RUN_LOG" 2>/dev/null || true
    rm -f "$PID_FILE"
    exit 1
fi

echo "✅ 服务已后台启动 (PID $(cat "$PID_FILE"))"
echo "   访问: http://localhost:3000（远程服务器用 IP，需放行端口）"
echo "   日志: $RUN_LOG（./script/logs.sh 或 tail -f 查看）"
echo "   停止: ./script/stop.sh 或 make stop"
