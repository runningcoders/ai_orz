#!/bin/bash
# ai_orz - 实时查看运行日志（tail -f .ai_orz/run.log）

set -e

cd "$(cd "$(dirname "$0")/.." && pwd)"

LOG=./.ai_orz/run.log
if [ ! -f "$LOG" ]; then
    echo "暂无运行日志: $LOG（先执行 ./script/start.sh）"
    exit 1
fi

exec tail -f "$LOG"
