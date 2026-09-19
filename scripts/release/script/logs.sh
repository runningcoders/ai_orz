#!/bin/bash
# ai_orz 发布包 - 实时跟踪日志（别名）：自动在 run.log 与按日业务日志间选择
#
# ⚠️ 本文件只在发布包内生效（需同目录存在 prod.sh）；仓库内的副本是模板，不要直接执行。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/prod.sh" logs "$@"
