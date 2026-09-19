#!/bin/bash
# ai_orz - 脚本统一入口（路由层）
#
# 全项目运维脚本的唯一入口：所有命令在这里汇总，实现分散在各专职脚本里
# （run.sh / prod.sh / build_frontend.sh / cleanup.sh / check_deps.sh / package.sh / check.sh）。
# 规则：本文件只做参数解析与转发，不实现任何功能，避免「同一功能多份实现」。
#
# 旧名 scripts/start.sh、scripts/build.sh 等保留为兼容别名，均转发到本入口；
# 日常推荐用根目录 Makefile（make dev / make prod / make stop / ...），它同样只转发到这里。
#
# Usage: ./scripts/ai_orz.sh <命令> [参数]
#        ./scripts/ai_orz.sh help     查看全部命令

set -eu

# shellcheck source=./lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

CMD="${1:-help}"
[ $# -gt 0 ] && shift || true

cmd_help() {
    cat << 'EOF'
ai_orz - 脚本统一入口

用法: ./scripts/ai_orz.sh <命令> [参数]      （等价 make <命令>，推荐用 make）

开发
  dev                开发态全栈：后端 cargo run + 前端 dx serve
  backend            仅后端（http://localhost:3000）
  frontend           仅前端 dev server（http://localhost:8080）

构建
  build              全量 release 构建：前端 dist/ + 后端二进制
  build-fe           仅前端 release 构建并复制到 dist/

生产（仓库自部署）
  prod               构建 + 后台启动 release 服务（连跑两次 = 幂等重启）
  stop               优雅停止服务（超时强杀前出示日志尾部）
  restart            停止后启动（不重新构建）
  status             查看 PID / 运行时长 / 资源占用 / 监听端口
  logs               实时跟踪日志（tail -F，自动跟随按日滚动）

治理
  clean              清理残留进程与端口占用（--dry-run 只列不杀）
  doctor             依赖预检（[dev|frontend|backend|build|prod] [--fix]）
  check              代码门禁：fmt / clippy / clippy-fe / test / lint / ci ...
  migrate            call_trace 存储布局迁移（默认 dry-run，--apply 落盘）

发布
  package [版本号]    构建并打包 tar.gz（版本号缺省取 git describe）

兼容别名（等价命令，可直接调用旧脚本名）
  scripts/start.sh   → dev（无参）/ 原样转发（有参）
  scripts/build.sh   → build
  scripts/stop.sh    → stop
  scripts/status.sh  → status
  scripts/logs.sh    → logs
  scripts/restart.sh → restart
  scripts/cleanup.sh → clean
  scripts/check_deps.sh → doctor

示例
  ./scripts/ai_orz.sh dev
  ./scripts/ai_orz.sh doctor backend --fix
  ./scripts/ai_orz.sh clean --dry-run
  ./scripts/ai_orz.sh package v1.2.0
EOF
}

# 生产模式全景：构建 → 清理残留（DuckDB 文件锁 / 端口占用）→ 后台启动
cmd_prod() {
    "$SCRIPTS_DIR/prod.sh" build
    "$SCRIPTS_DIR/cleanup.sh"
    "$SCRIPTS_DIR/prod.sh" start
}

case "$CMD" in
    # ===== 开发态 =====
    dev|backend|frontend)
        "$SCRIPTS_DIR/run.sh" "$CMD"
        ;;

    # ===== 构建 =====
    build)
        "$SCRIPTS_DIR/prod.sh" build
        ;;
    build-fe|build-frontend)
        "$SCRIPTS_DIR/build_frontend.sh"
        ;;

    # ===== 生产生命周期 =====
    prod|prod-start)
        cmd_prod
        ;;
    stop|prod-stop)
        "$SCRIPTS_DIR/prod.sh" stop
        ;;
    restart|prod-restart)
        "$SCRIPTS_DIR/prod.sh" restart
        ;;
    status|prod-status)
        "$SCRIPTS_DIR/prod.sh" status
        ;;
    logs|log|prod-log)
        "$SCRIPTS_DIR/prod.sh" logs
        ;;

    # ===== 治理 =====
    clean|clean-proc|cleanup)
        "$SCRIPTS_DIR/cleanup.sh" "$@"
        ;;
    doctor|deps|check-deps)
        if [ $# -eq 0 ]; then
            "$SCRIPTS_DIR/check_deps.sh"
        else
            "$SCRIPTS_DIR/check_deps.sh" "$@"
        fi
        ;;
    check)
        "$SCRIPTS_DIR/check.sh" "$@"
        ;;
    migrate)
        "$SCRIPTS_DIR/migrate_tool_call_trace.sh" "$@"
        ;;

    # ===== 发布 =====
    package)
        "$SCRIPTS_DIR/package.sh" "$@"
        ;;

    help|--help|-h|"")
        cmd_help
        ;;
    *)
        err "未知命令: $CMD"
        echo ""
        cmd_help
        exit 2
        ;;
esac
