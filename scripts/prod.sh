#!/bin/bash
# ai_orz - 生产服务生命周期（编译 / 搬运 / 启动 / 停止 / 状态 / 日志 / 重启）
#
# 【唯一实现】仓库与发布包共用本文件，环境差异由 lib/service.sh::in_repo_checkout() 判定，
# 不再维护两份「同源」的启动/停止脚本：
#   - 仓库  ：scripts/prod.sh（入口 ./scripts/ai_orz.sh，等价 make prod / make stop / ...）
#   - 发布包：script/prod.sh（package.sh 复制进去，script/start.sh 等别名全部指向它）
#
# Usage:
#   ./scripts/prod.sh build      构建 release：前端 dist/ + 后端二进制（仅仓库，产物留在仓库内）
#   ./scripts/prod.sh install    把仓库构建产物搬运到部署根（build 之后、start 之前；仅仓库）
#   ./scripts/prod.sh start      后台启动（已运行则先优雅停止，幂等重启）
#   ./scripts/prod.sh start -f   前台运行（Ctrl+C 停止）
#   ./scripts/prod.sh stop       优雅停止（超时强杀前出示日志尾部定位卡点）
#   ./scripts/prod.sh status     查看 PID / 运行时长 / 资源占用 / 监听端口
#   ./scripts/prod.sh logs       实时跟踪日志（tail -F，自动跟随按日滚动）
#   ./scripts/prod.sh restart    停止后启动（不重新构建、不搬运）
#
# 【编译 / 搬运 / 启动 —— 三段分离】
#   build   只管「在仓库里把产物编译出来」（dist/ + target/release/ai_orz），不碰部署根。
#   install 只管「把编译产物搬运到部署根」（<部署根>/bin/ai_orz、<部署根>/dist）。
#   start   只管「启动」（仓库模式用部署根产物；产物缺失或落后于仓库构建时明确告警，不静默搬运）。
#   于是同一次 build 的产物有两条互不干扰的消费路径：
#     - 部署：build → install → start（make prod = 这三步 + 残留清理）
#     - 打包：build → scripts/package.sh 组装 tar.gz（CI 与本地共用，不碰本机部署根）
#   为什么 install 不塞进 build：打包不该有「往 $HOME 写约 280MB」的副作用（CI 上白写、干净机器被污染），
#   本机 `make package` 更不该覆盖正在运行实例的 bin/ 与 dist/。
#
# 部署根（生产实例的落点，定义与默认值见 lib/service.sh「部署根」段）：
#   仓库   → $HOME/.ai_orz（数据 <部署根>/data、二进制 <部署根>/bin/ai_orz、前端产物 <部署根>/dist，
#                           PID / run.log 也在部署根下）
#   发布包 → 解压目录自身（数据 <包根>/.ai_orz、前端产物 <包根>/dist，与历史布局完全一致）
#   开发态不走部署根，仍在仓库内 .ai_orz（相对路径天然隔离，各 checkout 互不干扰）
#
# 环境变量：
#   AI_ORZ_DEPLOY_ROOT    覆盖部署根（默认见上）
#   AI_ORZ_BIN            覆盖服务二进制路径
#   AI_ORZ_BASE_PATH      覆盖数据目录（默认 <部署根>/data）
#   FRONTEND_DIST_DIR     覆盖前端静态目录（默认 <部署根>/dist；后端也读它）
#   AI_ORZ_LISTEN_ADDR    覆盖监听地址（默认 0.0.0.0:3000）
#   AI_ORZ_STOP_TIMEOUT   覆盖优雅停止超时秒数（默认 30）

set -eu

# shellcheck source=./lib/service.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/service.sh"

CMD="${1:-start}"
[ $# -gt 0 ] && shift || true

STOP_TIMEOUT="${AI_ORZ_STOP_TIMEOUT:-30}"
BIN="$(resolve_bin)"
PID_FILE="$(pid_file_path)"
RUN_LOG="$(run_log_path)"
DAY_LOG="$(day_log_path)"
# 仓库：后端自身写按日文件日志，控制台层关闭，stderr 只接 panic 等绕过 tracing 的崩溃输出
BOOT_LOG="$DATA_DIR/logs/prod-boot.log"

listen_port() {
    local spec="${AI_ORZ_LISTEN_ADDR:-0.0.0.0:3000}"
    echo "${spec##*:}"
}

# ===== 构建（仅仓库：发布包里只有二进制，不含源码）=====
# 只管「在仓库里把产物编译出来」：前端 dist/ + 后端 target/release/ai_orz。
# 刻意不写部署根 —— 产物有两条消费路径（见文件头「三段分离」），由调用方显式选择：
#   部署 → prod.sh install；打包 → scripts/package.sh。
cmd_build() {
    [ $# -eq 0 ] || usage_die "未知参数: ${1}" "用法: prod.sh build（无参数）"
    if ! in_repo_checkout; then
        die "发布包不含源码，无需构建（直接 ./script/start.sh 启动）"
    fi
    setup_path
    cd "$REPO_ROOT"
    echo "🔨 构建 release（前端 dist/ + 后端二进制）..."
    "$SCRIPTS_DIR/build_frontend.sh"
    echo ""
    echo "🏗️  编译后端 (release)..."
    cargo build --release
    echo ""
    ok "✅ 构建完成（产物在仓库内，尚未部署）"
    echo "   后端二进制: ${BLUE}${REPO_ROOT}/target/release/ai_orz${NC}"
    echo "   前端静态文件: ${BLUE}${REPO_ROOT}/dist/${NC}"
    echo "   部署到部署根: ${BLUE}make install${NC}（或 ${BLUE}make prod${NC} = 构建 + 搬运 + 启动）"
    echo "   打包分发:     ${BLUE}make package${NC}（不触碰本机部署根）"
}

# ===== 搬运（build 产物 → 部署根；仅仓库）=====
# 与 build 分开是为了让「编译」保持无仓库外副作用：同一份 build 产物既要能部署，也要能打包。
# 幂等：可重复执行；覆盖正在运行的二进制走 tmp+mv 原子替换（见 install_bin）。
cmd_install() {
    [ $# -eq 0 ] || usage_die "未知参数: ${1}" "用法: prod.sh install（无参数）"
    in_repo_checkout || die "发布包无需搬运（二进制与 dist/ 已在包根，直接 ./script/start.sh 启动）"
    install_bin
    install_dist
    echo ""
    ok "✅ 已搬运到部署根"
    echo "   部署根:   ${BLUE}$DEPLOY_ROOT${NC}"
    echo "   数据目录: ${BLUE}$DATA_DIR${NC}"
    echo "   静态目录: ${BLUE}$(dist_dir_path)${NC}"
}

# 把 release 二进制安装到部署根（<部署根>/bin/ai_orz），与 target/ 解耦：
#   `cargo clean` / 重建 target / 切分支都不会让生产实例下次重启时找不到二进制。
# 仅仓库需要；发布包的二进制本来就在包根，无需安装。
# ⚠️ 覆盖「正在运行」的二进制必须「写临时文件 + mv 原子替换」：
#   直接 cp 到目标会被内核拒（ETXTBSY），老实例还会读到半截文件。
install_bin() {
    in_repo_checkout || return 0
    local src="$REPO_ROOT/target/release/ai_orz"
    local dst="$DEPLOY_ROOT/bin/ai_orz"
    [ -x "$src" ] || die "未找到 release 二进制: ${src}（先执行 make build 编译）"
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst.tmp"
    chmod +x "$dst.tmp"
    mv "$dst.tmp" "$dst"
    echo "📦 已安装二进制到部署根: ${BLUE}${dst}${NC}"
}

# 把前端产物安装到部署根（<部署根>/dist），同样为了与 git 工作树解耦：
#   dist/ 在 .gitignore 里，`git clean -xdf` 会连它一起删掉 —— 生产实例的前端不该被工作树清理带走。
#   实测若 dist 缺失，后端不会 404：router.rs 的 SPA 回退把读不到的 index.html 当空串，
#   对所有无扩展名路径返回 **200 + 空 body**，症状是「白屏但状态码正常」，极难定位。
# ⚠️ 必须整体换名而不是就地覆盖：dx 产物名带 hash（assets/frontend-*.js），就地覆盖会残留上一版
#   文件，且新 index.html 可能引用到半新半旧的 assets。故先备齐 dist.tmp 再 `mv` 原子换名
#   （与 install_bin 同思路）；旧目录先挪走再删，保证换名瞬间不会有半空目录被 ServeDir 读到。
# 仅仓库需要；发布包在打包时已把 dist/ 放进包根。
install_dist() {
    in_repo_checkout || return 0
    local src="$REPO_ROOT/dist"
    [ -f "$src/index.html" ] || die "未找到前端产物: ${src}/index.html（先执行 make build）"
    local dst
    dst="$(dist_dir_path)"
    mkdir -p "$(dirname "$dst")"
    rm -rf "$dst.tmp" "$dst.old"
    cp -R "$src" "$dst.tmp"
    # 用 if 而非 `[ -e ] && mv`：后者在首次安装（无旧目录）时整句返回非零，
    # 会踩到脚本顶部的 `set -e` 直接退出。
    if [ -e "$dst" ]; then
        mv "$dst" "$dst.old"
    fi
    mv "$dst.tmp" "$dst"
    rm -rf "$dst.old"
    echo "📦 已搬运前端产物到部署根: ${BLUE}${dst}${NC}（$(find "$dst" -type f | wc -l | tr -d ' ') 个文件）"
}

# 部署根产物 vs 仓库构建产物：落后时告警（只提示，不搬运）
#   start 不自动搬运是刻意的：同机多 checkout 共享同一个部署根，若 start 拿当前工作树的产物去覆盖，
#   会把「恰好在这个 checkout 编译过」的东西推上生产（可能反而是降级）。
#   但也不能沉默 —— resolve_bin 优先部署根安装位，`make build && make restart` 会静默跑旧二进制。
warn_if_artifacts_behind() {
    in_repo_checkout || return 0
    local src_bin="$REPO_ROOT/target/release/ai_orz"
    local dst_bin="$DEPLOY_ROOT/bin/ai_orz"
    if [ -x "$src_bin" ] && [ -x "$dst_bin" ] && [ "$src_bin" -nt "$dst_bin" ]; then
        warn "⚠️  部署根二进制落后于仓库构建产物，本次将运行【旧版本】：先执行 make install"
    fi
    local src_index="$REPO_ROOT/dist/index.html"
    local dst_index
    dst_index="$(dist_dir_path)/index.html"
    if [ -f "$src_index" ] && [ -f "$dst_index" ] && [ "$src_index" -nt "$dst_index" ]; then
        warn "⚠️  部署根前端产物落后于仓库构建产物，本次将使用【旧版本】：先执行 make install"
    fi
}

# ===== 启动 =====
cmd_start() {
    local foreground=0
    if [ "${1:-}" = "-f" ] || [ "${1:-}" = "--foreground" ]; then
        foreground=1
    fi

    # 统一以「根」为工作目录：`frontend.dist_dir` 默认是相对值 "dist"（common/src/config.rs），
    # 从任意路径调用本脚本行为一致（发布包可放在任意目录，同样成立）；
    # 它同时是「产物尚未安装到部署根」时的回退来源（见下）。
    cd "$REPO_ROOT"

    # 部署根 / 数据目录 / 日志目录，并把两个**运行期路径**显式传给后端：
    #   AI_ORZ_BASE_PATH  —— 不传则按相对常量 `.ai_orz` 解析，落回进程 CWD（= 仓库根），
    #                        生产数据又长回 git 工作树里。
    #   FRONTEND_DIST_DIR —— 不传则按相对 "dist" 解析，同样落回仓库；而 dist/ 在 .gitignore 里，
    #                        `git clean -xdf` 会连它一起删 → 前端白屏（且不是 404，见 install_dist 注释）。
    # 必须在下面的前台分支之前完成 —— `start -f` 走 exec，之后再 export 就来不及了。
    mkdir -p "$DEPLOY_ROOT" "$DATA_DIR" "$(dirname "$BOOT_LOG")"
    export AI_ORZ_BASE_PATH="$DATA_DIR"

    # 前端产物：只在**仓库**模式注入（发布包靠 CWD 命中包根 dist/，与历史行为完全一致）。
    # 发布包刻意不注入 —— 那会把绝对路径固化进包内 ai_orz.toml（首次初始化时写盘），
    # 包被整体挪到别的路径后就失效了。产物未搬运时不注入，回退 CWD 相对 dist（= 仓库产物）。
    if in_repo_checkout; then
        local dist_dir
        dist_dir="$(dist_dir_path)"
        if [ -f "$dist_dir/index.html" ]; then
            export FRONTEND_DIST_DIR="$dist_dir"
        else
            warn "⚠️  部署根下无前端产物（${dist_dir}/），本次回退用仓库 dist/（执行 make install 搬运）"
        fi
    fi

    # 产物落后只告警不搬运（原因见函数注释），避免「build 了却忘了 install」被静默吞掉
    warn_if_artifacts_behind

    if [ ! -x "$BIN" ]; then
        die "未找到服务二进制: ${BIN}（仓库先 make build，再 make install 或 make prod）"
    fi

    if [ "$foreground" = "1" ]; then
        echo "🚀 前台启动 ai_orz（停止: Ctrl+C）..."
        echo "   数据:   ${BLUE}$DATA_DIR${NC}（AI_ORZ_BASE_PATH 可覆盖）"
        exec "$BIN"
    fi

    # 幂等重启：已有实例先优雅停止（make prod 连跑两次 = 重启，不会双实例抢端口/文件锁）
    local old
    old=$(read_pid)
    if [ -n "$old" ]; then
        warn "⚠️  检测到运行中的实例 PID=${old}，先优雅停止..."
        cmd_stop
    fi

    local port
    port=$(listen_port)
    echo "🚀 后台启动 ai_orz..."
    echo "   二进制: ${BLUE}$BIN${NC}"
    echo "   数据:   ${BLUE}$DATA_DIR${NC}（AI_ORZ_BASE_PATH 可覆盖）"
    echo "   静态:   ${BLUE}${FRONTEND_DIST_DIR:-$REPO_ROOT/dist}${NC}（FRONTEND_DIST_DIR 可覆盖）"
    echo "   监听:   ${BLUE}${AI_ORZ_LISTEN_ADDR:-0.0.0.0:3000}${NC}（AI_ORZ_LISTEN_ADDR 可覆盖）"

    if in_repo_checkout; then
        # 控制台层交给后端文件日志，stdout 静默、stderr 接崩溃输出（每次启动截断）
        : > "$BOOT_LOG"
        nohup env AI_ORZ_LOG_CONSOLE=0 "$BIN" </dev/null >/dev/null 2>"$BOOT_LOG" &
    else
        # 发布包：stdout/stderr 即唯一持久化来源，追加到 run.log
        mkdir -p "$DATA_DIR"
        "$BIN" >> "$RUN_LOG" 2>&1 &
    fi
    local pid=$!
    echo "$pid" > "$PID_FILE"

    echo "⏳ 等待服务就绪（业务日志: ${BLUE}$DAY_LOG${NC}）..."
    if wait_for_port localhost "$port" 60 "ai_orz 服务" "$pid"; then
        ok "✅ 服务已就绪"
        # ${pid} 必须带花括号：紧随其后的全角逗号会被 bash 当成变量名的一部分（UTF-8 下）
        echo "   地址: ${BLUE}http://localhost:${port}${NC}（PID ${pid}，PID 文件 ${PID_FILE}）"
        echo "   日志: ${BLUE}make logs${NC}   状态: ${BLUE}make status${NC}   停止: ${BLUE}make stop${NC}"
        return 0
    fi

    rm -f "$PID_FILE"
    err "❌ 服务未能就绪，最近输出:"
    in_repo_checkout && tail -n 20 "$BOOT_LOG" 2>/dev/null || true
    tail -n 20 "$RUN_LOG" 2>/dev/null || true
    exit 1
}

# ===== 停止 =====
# 停止目标：PID 文件优先；无 PID 文件（手动启动 / 文件丢失）按进程名兜底
cmd_stop() {
    local pid
    pid=$(read_pid)

    if [ -n "$pid" ]; then
        rm -f "$PID_FILE"
        echo "🛑 停止服务 PID=$pid ..."
        if ! graceful_stop "$pid" "$STOP_TIMEOUT" "$RUN_LOG" "$DAY_LOG"; then
            warn "⏰ 温和停止 ${STOP_TIMEOUT}s 未完成，最近日志（定位卡点）:"
            tail -n 15 "$RUN_LOG" 2>/dev/null || true
            tail -n 15 "$DAY_LOG" 2>/dev/null || true
            echo "💥 强杀 PID=$pid"
            kill -9 "$pid" 2>/dev/null || true
        fi
        ok "👋 服务已停止"
        return 0
    fi

    # 兜底：按进程名扫描（只匹配本环境的二进制，避免误伤同机其它实例）
    # 仓库模式要扫两处：部署根安装位（生产实例的主路径）与 target/（手动 cargo run --release / 未安装）
    local pids=""
    if in_repo_checkout; then
        pids=$(find_pids_by_cmd "target/(debug|release)/ai_orz( |$)")
        local deployed
        deployed=$(find_pids_by_cmd "$(egrep_literal "$DEPLOY_ROOT/bin/ai_orz")( |$)")
        if [ -n "$deployed" ]; then
            pids="$pids
$deployed"
        fi
    else
        pids=$(find_pids_by_cmd "$(egrep_literal "$REPO_ROOT/ai_orz")( |$)")
    fi
    if [ -z "$(printf '%s' "$pids" | tr -d '[:space:]')" ]; then
        echo "✓ 服务未在运行"
        return 0
    fi

    echo "🛑 按进程名停止服务: $(printf '%s' "$pids" | tr '\n' ' ')"
    local p
    for p in $pids; do
        if ! graceful_stop "$p" "$STOP_TIMEOUT" "$RUN_LOG" "$DAY_LOG"; then
            echo "💥 强杀 PID=$p"
            kill -9 "$p" 2>/dev/null || true
        fi
    done
    ok "👋 服务已停止"
}

# ===== 状态 =====
cmd_status() {
    local pid
    pid=$(read_pid)
    if [ -z "$pid" ]; then
        echo "○ 未运行（启动: $(in_repo_checkout && echo 'make prod' || echo 'make start')）"
        echo "   数据目录: ${BLUE}$DATA_DIR${NC}"
        echo "   静态目录: ${BLUE}$(dist_dir_path)${NC}"
        return 0
    fi
    ok "● 运行中（PID ${pid}）"
    /bin/ps -p "$pid" -o pid,etime,%cpu,%mem,command
    echo "   数据目录: ${BLUE}$DATA_DIR${NC}"
    echo "   静态目录: ${BLUE}$(dist_dir_path)${NC}"
    echo "   部署根:   ${BLUE}$DEPLOY_ROOT${NC}"
    local port
    port=$(listen_port)
    local listeners
    listeners=$(/usr/sbin/lsof -nP -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -i LISTEN | /usr/bin/head -n 3 || true)
    if [ -n "$listeners" ]; then
        echo "   端口 $port 监听:"
        printf '%s\n' "$listeners" | sed 's/^/     /'
    fi
}

# ===== 日志 =====
# 日志源双查：run.log（发布包 stdout 落盘）/ 按日业务日志（仓库与包共用）
cmd_logs() {
    local target=""
    if [ -f "$RUN_LOG" ]; then
        target="$RUN_LOG"
    elif [ -f "$DAY_LOG" ]; then
        target="$DAY_LOG"
    else
        warn "暂无日志文件（run.log / $(basename "$DAY_LOG") 均不存在）"
        echo "已有的日志文件:"
        ls -t "$DATA_DIR"/logs/ai_orz.log.* 2>/dev/null | head -n 5 || echo "  （无）"
        exit 1
    fi
    echo "📜 实时跟踪 ${target}（Ctrl+C 退出跟踪，不影响服务）"
    exec tail -F "$target"
}

cmd_help() {
    cat << 'EOF'
ai_orz - 生产服务生命周期（仓库与发布包共用同一实现）

用法: prod.sh <命令> [选项]

命令:
  build      构建 release（前端 dist/ + 后端二进制），产物留在仓库内        [仅仓库]
  install    把仓库构建产物搬运到部署根（build 之后、start 之前；别名 deploy）[仅仓库]
  start      后台启动（已运行则先优雅停止，幂等重启）
  start -f   前台运行（Ctrl+C 停止）
  stop       优雅停止（超时强杀前出示日志尾部）
  restart    停止后启动（不重新构建、不搬运）
  status     查看 PID / 运行时长 / 资源占用 / 监听端口
  logs       实时跟踪日志（tail -F，自动跟随按日滚动）
  help       显示本帮助

编译 / 搬运 / 启动 三段分离（build 不写部署根；install 只搬运；start 只启动）:
  部署: build → install → start      （make prod = 这三步 + 残留清理）
  打包: build → scripts/package.sh   （组装 tar.gz，不触碰本机部署根）
  ⚠️ start 只在产物落后时告警、不自动搬运 —— 避免把当前 checkout 的构建覆盖到共享部署根上

环境变量:
  AI_ORZ_DEPLOY_ROOT   覆盖部署根（仓库默认 $HOME/.ai_orz；发布包默认解压目录）
  AI_ORZ_BIN           覆盖服务二进制路径
  AI_ORZ_BASE_PATH     覆盖数据目录（默认 <部署根>/data）
  FRONTEND_DIST_DIR    覆盖前端静态目录（默认 <部署根>/dist；后端也读它）
  AI_ORZ_LISTEN_ADDR   覆盖监听地址（默认 0.0.0.0:3000）
  AI_ORZ_STOP_TIMEOUT  覆盖优雅停止超时秒数（默认 30）
EOF
}

case "$CMD" in
    build) cmd_build "$@" ;;
    install|deploy) cmd_install "$@" ;;
    start) cmd_start "$@" ;;
    stop) cmd_stop ;;
    restart) cmd_stop; cmd_start ;;
    status) cmd_status ;;
    logs|log) cmd_logs ;;
    help|--help|-h) cmd_help ;;
    *)
        err "未知命令: $CMD"
        cmd_help
        exit 2
        ;;
esac
