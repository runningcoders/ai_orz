#!/bin/bash
# ai_orz 服务进程治理库 —— 二进制定位 / 端口等待 / PID 文件 / 优雅停止
#
# 仅供其它脚本 source（本库自身依赖 common.sh，会自动带上）：
#   source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/service.sh"
#
# 关键设计：本库被「仓库 scripts/」与「发布包 script/」共用同一份文件，
# 因此「怎么优雅停服务」在全项目只有一处实现（此前 scripts/start.sh 与
# scripts/release/script/stop.sh 各写了一份，注释里只能互相标注「同源」）。
# 两个环境的差异全部收敛在 in_repo_checkout() 这一个判定上。
#
# 兼容 macOS 自带 bash 3.2（不用 mapfile / 关联数组）。

# shellcheck source=./common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

# ===== 运行环境判定 =====
# 仓库签出：根目录有 Cargo.toml（有源码，prod 前要构建，控制台日志交给后端文件层）
# 发布包  ：只有 ai_orz + dist/ + script/（无需构建，stdout 需落到 run.log）
in_repo_checkout() { [ -f "$REPO_ROOT/Cargo.toml" ]; }

# ===== 部署根（生产实例的落点）=====
# 目的：生产实例的「数据 / 二进制 / PID / run.log」不再长在 git 工作树里——
#   - `git clean -xdf`、删仓库、切分支都不会碰到生产数据
#   - 同一台机器从任意 checkout 调用 `make prod`，都指向同一个生产实例
#
# 默认值按环境分叉（沿用 in_repo_checkout() 这一个判定，不新增第二个环境开关）：
#   仓库   → $HOME/.ai_orz（数据在 <部署根>/data，二进制安装在 <部署根>/bin/ai_orz）
#   发布包 → 解压目录自身（布局与历史完全一致：数据在 <包根>/.ai_orz、二进制在 <包根>/ai_orz）
# 显式覆盖：AI_ORZ_DEPLOY_ROOT > 上述默认。
#
# ⚠️ 开发态（run.sh / cargo run / cargo test）**刻意不使用部署根**：它继续落在仓库内 `.ai_orz`，
#    靠「相对路径 + 进程 CWD」天然隔离，各 checkout / 集成测试互不干扰。
#    若把 BASE_DATA_PATH 默认值改成全局 home，所有实例会共享同一份 SQLite/DuckDB/向量库
#    （WAL 锁冲突、测试污染真实数据、bincode 元数据互相覆盖），且全程静默难以定位 —— 是禁区。
DEPLOY_ROOT="$(
    if [ -n "${AI_ORZ_DEPLOY_ROOT:-}" ]; then
        echo "$AI_ORZ_DEPLOY_ROOT"
    elif in_repo_checkout; then
        echo "$HOME/.ai_orz"
    else
        echo "$REPO_ROOT"
    fi
)"

# ===== 运行态路径（两种环境同口径，均相对部署根/数据目录）=====
# 数据目录：`AI_ORZ_BASE_PATH` 是后端与脚本共用的唯一开关（后端每次调用都会重新读它）
if [ -n "${AI_ORZ_BASE_PATH:-}" ]; then
    DATA_DIR="$AI_ORZ_BASE_PATH"
elif in_repo_checkout; then
    DATA_DIR="$DEPLOY_ROOT/data"
else
    DATA_DIR="$DEPLOY_ROOT/.ai_orz"
fi

# 服务二进制：显式覆盖 > 部署根安装位 > 发布包根的 ./ai_orz > 仓库 target/release/ai_orz
# 部署根安装位（<部署根>/bin/ai_orz）由 `prod.sh install` 从 build 产物搬运，与 target/ 解耦：
#   `cargo clean` / 重建 target 不会让生产实例在下次重启时找不到二进制。
# 末位兜底保留 target/release/ai_orz，使未搬运时 `make restart` 仍可直接用（start 会告警提示 install）。
resolve_bin() {
    if [ -n "${AI_ORZ_BIN:-}" ]; then
        echo "$AI_ORZ_BIN"
    elif [ -x "$DEPLOY_ROOT/bin/ai_orz" ]; then
        echo "$DEPLOY_ROOT/bin/ai_orz"
    elif [ -x "$REPO_ROOT/ai_orz" ]; then
        echo "$REPO_ROOT/ai_orz"
    else
        echo "$REPO_ROOT/target/release/ai_orz"
    fi
}

# PID 文件：统一 .ai_orz/server.pid（与发布包同口径）；
# 仓库历史文件名 prod.pid 作一次性兼容读取，下次写入即回到统一路径。
pid_file_path() {
    if [ -f "$DATA_DIR/server.pid" ]; then
        echo "$DATA_DIR/server.pid"
    elif [ -f "$DATA_DIR/prod.pid" ]; then
        echo "$DATA_DIR/prod.pid"
    else
        echo "$DATA_DIR/server.pid"
    fi
}

# stdout 重定向目标：发布包落 run.log（唯一持久化来源），仓库丢弃（后端自己写按日文件日志）
run_log_path() { echo "$DATA_DIR/run.log"; }

# 后端按日滚动的业务日志（关停编排的终态标记从这里读）
day_log_path() { echo "$DATA_DIR/logs/ai_orz.log.$(date +%F)"; }

# 前端静态产物目录（生产实例的 dist/）：
#   仓库   → <部署根>/dist（由 `prod.sh install` 从仓库 dist/ 搬运过去，与 git 工作树解耦）
#   发布包 → 解压目录/dist（打包时就在包根，与历史布局完全一致）
# 部署根已按环境分叉，故这里无需再判定 —— 两种环境同为 "$DEPLOY_ROOT/dist"。
# ⚠️ 后端的 `frontend.dist_dir` 默认是相对值 "dist"（common/src/config.rs），落点由进程 CWD 决定；
#    要让生产实例读部署根产物，必须由启动器显式注入 FRONTEND_DIST_DIR（见 prod.sh::cmd_start）。
dist_dir_path() { echo "$DEPLOY_ROOT/dist"; }

# 开发态（cargo run，未走部署根）的数据目录 —— 仅供需要「同时兼容两种数据根」的清理逻辑使用
repo_local_data_dir() { echo "$REPO_ROOT/.ai_orz"; }

# 优雅停止时用于确认「关停编排已完成」的日志候选（每行一个路径）
# 生产实例写 DATA_DIR；开发实例（cargo run）写仓库本地 .ai_orz。cleanup.sh 同时服务两种实例，
# 故两份都作为终态标记来源；两份都没有该标记时自动退化为「纯等进程退出」，行为仍正确。
shutdown_marker_logs() {
    echo "$(run_log_path)"
    echo "$(day_log_path)"
    echo "$(repo_local_data_dir)/run.log"
    echo "$(repo_local_data_dir)/logs/ai_orz.log.$(date +%F)"
}

# 把路径转成 egrep 的字面量模式（ps 输出里匹配二进制路径时用，避免路径中的 . 等元字符误匹配）
egrep_literal() { printf '%s' "$1" | sed 's/[][\\.^$*+?(){}|]/\\&/g'; }

# ===== 进程 / 端口 =====

# 进程存活判定（zombie 安全）
# kill -0 对「已退出但未被 wait 收尸的自身子进程」(zombie) 仍返回 0，
# dev 模式下后端/前端是脚本子进程，用 kill -0 判活会永远为真 → 白等满超时。
# 用 ps 状态位排除：stat 以 Z 开头即为 zombie（已死）；进程不存在时 ps 非零退出。
pid_alive() {
    local stat
    stat=$(ps -o stat= -p "$1" 2>/dev/null) || return 1
    [ -n "$stat" ] || return 1
    [ "${stat#Z}" = "$stat" ]
}

# 等待端口就绪（纯 bash /dev/tcp，无外部依赖）
# 用法: wait_for_port <host> <port> <超时秒> <描述> [监控PID]
#   监控PID 可选：进程提前退出（编译失败等）时立刻返回 1，不用干等超时
wait_for_port() {
    local host=$1 port=$2 timeout=${3:-600} desc=$4 monitor_pid=${5:-}
    local elapsed=0
    while ! (echo > "/dev/tcp/$host/$port") 2>/dev/null; do
        # Ctrl+C 已按下：立即退出等待，让外层 trap 生效
        if [ "${INT_RECEIVED:-0}" = "1" ]; then
            return 1
        fi
        if [ -n "$monitor_pid" ] && ! pid_alive "$monitor_pid"; then
            err "❌ $desc 进程已退出（疑似编译失败），请检查上方日志"
            return 1
        fi
        if [ "$elapsed" -ge "$timeout" ]; then
            err "⏰ 等待 $desc 超时（${timeout}s），请检查上方编译日志"
            return 1
        fi
        # sleep 放子进程跑：INT 信号到来时立即打断，不阻塞 trap
        sleep 2 &
        wait $!
        elapsed=$((elapsed + 2))
    done
    return 0
}

# ===== 优雅停止 =====
# 后端优雅退出链路（src/lib.rs）：信号 → HTTP drain（上限 10s，SSE/WS 排空）→
# 渠道停服 → AOP worker 排空 → DuckDB 统计 flush 落盘 → 连接池关闭 → 打印终态日志
# "Shutdown complete, goodbye"。固定 sleep 等待会在 flush 前强杀 → 丢统计数据。
# 因此等「事件」而非「固定时长」：
#   硬信号 —— 进程退出（zombie 安全判定），唯一可靠终态
#   加速器 —— 日志出现终态标记（数据已安全落盘，进程只剩 runtime 收尾）
#   兜底   —— 超时返回 1，由调用方决定强杀并出示日志尾部
SHUTDOWN_DONE_MARKER="Shutdown complete, goodbye"

# 某日志文件「第 base 行之后」是否出现终态标记
log_has_marker() {
    local base=$1 file=$2
    [ -f "$file" ] || return 1
    tail -n +"$((base + 1))" "$file" 2>/dev/null | grep -q "$SHUTDOWN_DONE_MARKER"
}

# 优雅停止：先取各日志基线 → 发 TERM → 事件驱动等待
# 用法: graceful_stop <pid> [超时秒] [日志文件...]
#   基线必须在发信号之前取（只认本次产生的新增行），否则上次停止留下的旧标记会被误判；
#   跨天滚动时新文件无新增行，自动退化为纯进程退出等待，行为仍正确。
# 返回: 0 = 进程已退出；1 = 超时仍存活（不强杀，交由调用方决定并出示日志）
graceful_stop() {
    local pid=$1 timeout=${2:-30}
    shift $(( $# < 2 ? $# : 2 ))
    local logs=("$@")

    # 基线（发信号前）
    local bases=() i
    for ((i = 0; i < ${#logs[@]}; i++)); do
        bases[i]=0
        if [ -f "${logs[i]}" ]; then
            bases[i]=$(wc -l < "${logs[i]}" 2>/dev/null | tr -d ' ')
            [ -n "${bases[i]}" ] || bases[i]=0
        fi
    done

    kill "$pid" 2>/dev/null || true

    local half=0 max_half=$((timeout * 2)) marker_seen=0
    while pid_alive "$pid"; do
        if [ "$half" -ge "$max_half" ]; then
            return 1
        fi
        if [ "$marker_seen" = "0" ]; then
            for ((i = 0; i < ${#logs[@]}; i++)); do
                if log_has_marker "${bases[i]}" "${logs[i]}"; then
                    marker_seen=1
                    ok "🧹 关停编排已完成（日志确认），等待进程退出..."
                    break
                fi
            done
        fi
        sleep 0.5
        half=$((half + 1))
    done
    return 0
}

# 读取 PID 文件中的有效 PID（文件缺失 / PID 已死 均返回空）
read_pid() {
    local pf
    pf=$(pid_file_path)
    [ -f "$pf" ] || return 0
    local pid
    pid=$(cat "$pf" 2>/dev/null | tr -d '[:space:]')
    [ -n "$pid" ] || return 0
    pid_alive "$pid" || return 0
    echo "$pid"
}

# 按进程名兜底找 PID（无 PID 文件时使用）
# 用法: find_pids_by_cmd <egrep 模式>
find_pids_by_cmd() {
    /bin/ps aux 2>/dev/null | /usr/bin/grep -E "$1" | /usr/bin/grep -v grep | /usr/bin/awk '{print $2}'
}
