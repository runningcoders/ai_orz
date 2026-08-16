#!/bin/bash
# ai_orz - 全量编译脚本
# 统一委托给同目录 start.sh build 执行（推荐直接用 make build）

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

exec "$SCRIPT_DIR/start.sh" build "$@"
