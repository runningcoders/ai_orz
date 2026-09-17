#!/bin/bash
# ai_orz - 停止脚本（按 PID 文件优雅停止，事件驱动等待 + 超时强杀）
#
# 等待策略（与仓库内 scripts/start.sh 同源）：
#   硬信号  —— 进程退出（zombie 安全判定），唯一可靠终态
#   加速器  —— 终态日志 "Shutdown complete, goodbye" 出现（数据已安全落盘，
#             进程只剩 runtime 收尾）；基线行数在发信号前取，只认本次新增行，
#             防止上次停止留下的旧标记被误判为本次完成
#   兜底    —— 超时强杀（默认 30s，AI_ORZ_STOP_TIMEOUT 可覆盖），强杀前出示日志尾部
#
# 日志源双查（兼容两种启动方式）：
#   run.log                —— start.sh 后台模式的 stdout/stderr 重定向目标
#   logs/ai_orz.log.<日期> —— 后端按日滚动的文件日志层，前台模式（-f）下唯一落盘源

set -e

cd "$(cd "$(dirname "$0")/.." && pwd)"

PID_FILE=./.ai_orz/server.pid
RUN_LOG=./.ai_orz/run.log
STOP_TIMEOUT="${AI_ORZ_STOP_TIMEOUT:-30}"
SHUTDOWN_DONE_MARKER="Shutdown complete, goodbye"

# 进程存活判定（zombie 安全）：stat 以 Z 开头即为 zombie（已死）；进程不存在时 ps 非零退出
pid_alive() {
    local stat
    stat=$(ps -o stat= -p "$1" 2>/dev/null) || return 1
    [ -n "$stat" ] || return 1
    [ "${stat#Z}" = "$stat" ]
}

# 基线行数之后是否出现终态标记（$1 基线行数，$2 日志文件）
log_hit() {
    [ -f "$2" ] && tail -n +"$(( $1 + 1 ))" "$2" 2>/dev/null | grep -q "$SHUTDOWN_DONE_MARKER"
}

# 优雅停止等待（事件驱动）
# 用法: wait_graceful_stop <pid> <超时秒> <run日志基线> <文件日志基线>
# 返回: 0 = 进程已退出；1 = 超时仍存活
wait_graceful_stop() {
    local pid=$1 timeout=${2:-30} run_base=${3:-0} day_base=${4:-0}
    local half=0
    local max_half=$((timeout * 2)) # 0.5s 一轮，半秒计数
    local marker_seen=0
    while pid_alive "$pid"; do
        if [ "$half" -ge "$max_half" ]; then
            return 1
        fi
        if [ "$marker_seen" = "0" ]; then
            if log_hit "$run_base" "$RUN_LOG" || log_hit "$day_base" "$DAY_LOG"; then
                marker_seen=1
                echo "🧹 关停编排已完成（日志确认），等待进程退出..."
            fi
        fi
        sleep 0.5
        half=$((half + 1))
    done
    return 0
}

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
# 日志基线必须在发信号前取（只认本次停止产生的新增日志）
DAY_LOG=./.ai_orz/logs/ai_orz.log."$(date +%F)"
run_base=0
day_base=0
[ -f "$RUN_LOG" ] && run_base=$(wc -l < "$RUN_LOG" 2>/dev/null || echo 0)
[ -f "$DAY_LOG" ] && day_base=$(wc -l < "$DAY_LOG" 2>/dev/null || echo 0)
kill "$pid" 2>/dev/null || true

if wait_graceful_stop "$pid" "$STOP_TIMEOUT" "$run_base" "$day_base"; then
    echo "✅ 服务已停止"
else
    echo "⏰ 温和停止 ${STOP_TIMEOUT}s 未完成，最近日志（定位卡点）:"
    tail -n 15 "$RUN_LOG" 2>/dev/null || tail -n 15 "$DAY_LOG" 2>/dev/null || true
    echo "💥 强制结束 (PID $pid)..."
    kill -9 "$pid" 2>/dev/null || true
    echo "✅ 服务已强制停止"
fi
