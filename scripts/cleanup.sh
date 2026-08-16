#!/bin/bash
# ai_orz - 残留进程清理脚本
# 清理上次未正常退出的后端/前端进程与端口占用，避免：
#   - DuckDB 文件锁冲突（单写者，.ai_orz/stats.duckdb）
#   - dx 构建锁争抢（target/dx）与 8080、3000 端口占用
#
# start.sh 启动前自动调用；也可手动执行：
#   ./scripts/cleanup.sh            直接清理
#   ./scripts/cleanup.sh --dry-run  仅列出将清理的进程，不实际 kill
#   make clean-proc                 等价直接清理

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

YELLOW=$(printf '\033[0;33m')
GREEN=$(printf '\033[0;32m')
RED=$(printf '\033[0;31m')
NC=$(printf '\033[0m')

# 温和清理一批 PID；stdout 返回实际命中的 PID 列表（供强杀阶段复用），日志走 stderr
kill_gentle() {
    local desc=$1
    shift
    local hit=""
    for pid in "$@"; do
        [ -z "$pid" ] && continue
        kill -0 "$pid" 2>/dev/null || continue # 进程已不存在则跳过
        hit="$hit $pid"
        if [ "$DRY_RUN" = "1" ]; then
            echo "${YELLOW}  [dry-run] 将清理 $desc PID=$pid${NC}" >&2
        else
            echo "${YELLOW}  🧹 清理 $desc PID=$pid${NC}" >&2
            kill "$pid" 2>/dev/null || true
        fi
    done
    echo "$hit"
}

echo "🧹 ai_orz 残留进程清理$( [ "$DRY_RUN" = "1" ] && echo '（dry-run 模式）' )..."

# 1. 残留后端二进制进程（持有 DuckDB 文件锁、3000 端口）
STALE_BE=$(/bin/ps aux | /usr/bin/grep -E "target/(debug|release)/ai_orz( |$)" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}')
BE_HIT=$(kill_gentle "残留后端进程（避免 DuckDB 锁冲突）" $STALE_BE)

# 2. 残留 dx serve 前端进程（持有 8080 端口与构建锁）
STALE_DX=$(/bin/ps aux | /usr/bin/grep -E "dx serve( |$)" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}')
DX_HIT=$(kill_gentle "残留 dx serve 进程（释放 8080 端口与构建锁）" $STALE_DX)

# 3. 温和杀不掉的强杀（SKIP_DRY_RUN 阶段整体跳过）
ALL_HIT="$BE_HIT $DX_HIT"
if [ -n "$(echo $ALL_HIT | /usr/bin/tr -d ' ')" ] && [ "$DRY_RUN" = "0" ]; then
    sleep 1
    for pid in $ALL_HIT; do
        if kill -0 "$pid" 2>/dev/null; then
            echo "${YELLOW}  💥 强杀 PID=$pid（温和信号未生效）${NC}"
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
    sleep 1
fi

# 4. 端口占用复查：本项目进程应已释放；若被无关进程占用则仅警告（不误杀）
for port in 3000 8080; do
    OCCUPANTS=$(/usr/sbin/lsof -ti :"$port" 2>/dev/null || true)
    [ -z "$OCCUPANTS" ] && continue
    for pid in $OCCUPANTS; do
        CMD=$(/bin/ps -p "$pid" -o command= 2>/dev/null || true)
        case "$CMD" in
            *ai_orz* | *dx* | *dioxus*)
                # 属于本项目但上面未匹配到的变体：直接清理
                if [ "$DRY_RUN" = "1" ]; then
                    echo "${YELLOW}  [dry-run] 将清理端口 $port 占用者 PID=$pid${NC}"
                else
                    echo "${YELLOW}  🧹 清理端口 $port 占用者 PID=$pid${NC}"
                    kill -9 "$pid" 2>/dev/null || true
                fi
                ;;
            "")
                ;; # 进程已退出（TIME_WAIT 残留连接等）
            *)
                echo "${RED}  ⚠️ 端口 $port 被无关进程占用，未清理（请人工确认）: PID=$pid $CMD${NC}"
                ;;
        esac
    done
done

if [ "$DRY_RUN" = "1" ]; then
    echo "${GREEN}✓ dry-run 完成（未实际清理）${NC}"
elif [ -z "$(echo $ALL_HIT | /usr/bin/tr -d ' ')" ]; then
    echo "${GREEN}✓ 无残留进程${NC}"
else
    echo "${GREEN}✓ 清理完成${NC}"
fi
