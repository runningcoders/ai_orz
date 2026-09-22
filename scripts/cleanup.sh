#!/bin/bash
# ai_orz - 残留进程清理脚本
# 清理上次未正常退出的后端/前端进程与端口占用，避免：
#   - DuckDB 文件锁冲突（单写者，.ai_orz/stats.duckdb）
#   - dx 构建锁争抢（target/dx）与 8080、3000 端口占用
#
# 停止策略与 make stop 完全一致（同一处实现）：
#   后端二进制 → lib/service.sh::graceful_stop（事件驱动等待关停编排完成，超时才 -9）
#   dx serve   → SIGTERM + 短等待 + SIGKILL（开发服务器无优雅关停契约）
# ⚠️ 后端**不能**用「kill + 固定 sleep 1 + kill -9」：优雅退出链路含 10s HTTP drain 窗口
#    （SSE 长连接必然吃满）+ 渠道停服 + AOP 排空 + DuckDB flush，1s 后 -9 会打断落盘丢统计。
#
# ⚠️ 本脚本同时服务两种数据根（部署根见 lib/service.sh）：
#    生产实例 = <部署根>/bin/ai_orz，数据在 <部署根>/data；开发实例 = target/debug/ai_orz，
#    数据在仓库本地 .ai_orz。所以「按进程名扫描」与「终态标记日志」两处都必须两套都覆盖。
#
# run.sh（开发态）启动前自动调用；也可手动执行：
#   ./scripts/ai_orz.sh clean         直接清理（统一入口，推荐）
#   ./scripts/cleanup.sh              等价直接清理（兼容别名）
#   ./scripts/cleanup.sh --dry-run    仅列出将清理的进程，不实际 kill
#   make clean-proc                   等价直接清理
#
# 环境变量:
#   AI_ORZ_STOP_TIMEOUT   优雅停止超时秒数（默认 30，与 prod.sh 同口径）

DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

# shellcheck source=./lib/service.sh
# 用 service.sh 而非 common.sh：优雅停止 / 进程判活 / 日志路径都在那里（common.sh 会被它自动带上）
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/service.sh"

STOP_TIMEOUT="${AI_ORZ_STOP_TIMEOUT:-30}"
RUN_LOG="$(run_log_path)"
DAY_LOG="$(day_log_path)"

# 关停编排终态标记的日志候选（生产写部署根数据目录、开发写仓库本地 .ai_orz）
# 两份都看：否则会出现「日志里明明有终态标记、却白等到超时」的退化（行为仍正确，只是慢）
MARKER_LOGS=()
while IFS= read -r _marker_log; do
    MARKER_LOGS+=("$_marker_log")
done < <(shutdown_marker_logs)

CLEANED=0
HANDLED=""   # 已处理过的 PID（跨步骤去重）

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

# 后端优雅停止（与 prod.sh::cmd_stop 同一条链路、同一个函数）
# 进度输出一律走 stderr：stdout 只承载「命中列表」，避免调用方命令替换捕到人读文案。
stop_backend() {
    local pid=$1
    # 去重：同一 PID 可能既被「按进程名」扫到、又被「端口占用」扫到，只处理一次
    case " $HANDLED " in *" $pid "*) return 0 ;; esac
    HANDLED="$HANDLED $pid"
    pid_alive "$pid" || return 0 # 已被上一处停掉/本就已退出（幂等）
    if [ "$DRY_RUN" = "1" ]; then
        echo "${YELLOW}  [dry-run] 将优雅停止残留后端进程 PID=${pid}${NC}" >&2
        return 0
    fi
    echo "${YELLOW}  🧹 优雅停止残留后端进程 PID=${pid}（等关停编排完成，上限 ${STOP_TIMEOUT}s）${NC}" >&2
    if graceful_stop "$pid" "$STOP_TIMEOUT" "${MARKER_LOGS[@]}" >&2; then
        echo "${GREEN}  ✓ PID=${pid} 已优雅退出（stats 已落盘）${NC}" >&2
        return 0
    fi
    echo "${YELLOW}  ⏰ ${STOP_TIMEOUT}s 未完成关停，最近日志（定位卡点）:${NC}" >&2
    for _marker_log in "${MARKER_LOGS[@]}"; do
        tail -n 10 "$_marker_log" 2>/dev/null >&2 || true
    done
    echo "${YELLOW}  💥 强杀 PID=${pid}${NC}" >&2
    kill -9 "$pid" 2>/dev/null || true
}

echo "🧹 ai_orz 残留进程清理$( [ "$DRY_RUN" = "1" ] && echo '（dry-run 模式）' )..."

# 1. 残留后端二进制进程（持有 DuckDB 文件锁、3000 端口）—— 走优雅停止
#    仓库模式下有两个落点：部署根安装位（生产实例）与 target/（cargo run --release）
BE_PATTERN="target/(debug|release)/ai_orz( |$)"
if in_repo_checkout; then
    BE_PATTERN="${BE_PATTERN}|$(egrep_literal "$DEPLOY_ROOT/bin/ai_orz")( |$)"
else
    BE_PATTERN="$(egrep_literal "$REPO_ROOT/ai_orz")( |$)"
fi
STALE_BE=$(/bin/ps aux | /usr/bin/grep -E "$BE_PATTERN" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}')
for pid in $STALE_BE; do
    [ -z "$pid" ] && continue
    pid_alive "$pid" || continue
    CLEANED=1
    stop_backend "$pid"
done

# 2. 残留 dx serve 前端进程（持有 8080 端口与构建锁）
STALE_DX=$(/bin/ps aux | /usr/bin/grep -E "dx serve( |$)" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}')
DX_HIT=$(kill_gentle "残留 dx serve 进程（释放 8080 端口与构建锁）" $STALE_DX)

# 3. dx 温和杀不掉的强杀（后端已在步骤 1 自行升级，这里只剩 dx）
if [ -n "$(echo $DX_HIT | /usr/bin/tr -d ' ')" ]; then
    CLEANED=1
fi
if [ -n "$(echo $DX_HIT | /usr/bin/tr -d ' ')" ] && [ "$DRY_RUN" = "0" ]; then
    sleep 1
    for pid in $DX_HIT; do
        if kill -0 "$pid" 2>/dev/null; then
            echo "${YELLOW}  💥 强杀 PID=${pid}（温和信号未生效）${NC}" >&2
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
    sleep 1
fi

# 4. 端口占用复查：本项目监听进程应已释放；若被无关进程占用则仅警告（不误杀）
# ⚠️ 必须限定 -sTCP:LISTEN：不加会把「连到本端口的客户端」也算进来
#    （实测浏览器 WebKit、微信等已建立连接会被误报成「端口被占用」）
for port in 3000 8080; do
    OCCUPANTS=$(/usr/sbin/lsof -ti :"$port" -sTCP:LISTEN 2>/dev/null || true)
    [ -z "$OCCUPANTS" ] && continue
    for pid in $OCCUPANTS; do
        CMD=$(/bin/ps -p "$pid" -o command= 2>/dev/null || true)
        case "$CMD" in
            *ai_orz*)
                # 后端变体（上面按名字没扫到）：同样走优雅停止，避免这里给 -9 破功
                CLEANED=1
                stop_backend "$pid"
                ;;
            *dx* | *dioxus*)
                # dev server 无优雅关停契约：温和信号后仍活则强杀
                CLEANED=1
                if [ "$DRY_RUN" = "1" ]; then
                    echo "${YELLOW}  [dry-run] 将清理端口 ${port} 占用者 PID=${pid}${NC}"
                else
                    echo "${YELLOW}  🧹 清理端口 ${port} 占用者 PID=${pid}${NC}"
                    kill "$pid" 2>/dev/null || true
                    sleep 1
                    kill -9 "$pid" 2>/dev/null || true
                fi
                ;;
            "")
                ;; # 进程已退出（TIME_WAIT 残留连接等）
            *)
                echo "${RED}  ⚠️ 端口 ${port} 被无关进程占用，未清理（请人工确认）: PID=${pid} $CMD${NC}"
                ;;
        esac
    done
done

if [ "$DRY_RUN" = "1" ]; then
    echo "${GREEN}✓ dry-run 完成（未实际清理）${NC}"
elif [ "$CLEANED" = "0" ]; then
    echo "${GREEN}✓ 无残留进程${NC}"
else
    echo "${GREEN}✓ 清理完成${NC}"
fi
