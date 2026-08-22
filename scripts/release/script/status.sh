#!/bin/bash
# ai_orz - 状态检查脚本

cd "$(cd "$(dirname "$0")/.." && pwd)"

PID_FILE=./.ai_orz/server.pid

if [ -f "$PID_FILE" ]; then
    pid=$(cat "$PID_FILE" 2>/dev/null || true)
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        echo "🟢 服务运行中 (PID $pid)"
        echo "   监听端口:"
        lsof -nP -iTCP -sTCP:LISTEN 2>/dev/null | awk '$1=="ai_orz"{print "     " $9}' || true
        exit 0
    fi
fi

echo "⚪ 服务未运行"
