#!/bin/bash
# ai_orz - 全量编译脚本
# 统一委托给 start.sh build 执行

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

exec "$SCRIPT_DIR/start.sh" build "$@"
