#!/bin/bash
# ai_orz 发布包 - 重启服务（别名）：优雅停止后启动，不重新构建
#
# ⚠️ 本文件只在发布包内生效（需同目录存在 prod.sh）；仓库内的副本是模板，不要直接执行。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/prod.sh" restart "$@"
