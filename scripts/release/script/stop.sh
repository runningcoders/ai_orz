#!/bin/bash
# ai_orz 发布包 - 停止服务（别名）
#
# 优雅停止实现在同目录 prod.sh（其事件驱动等待逻辑来自 script/lib/service.sh），
# 本文件只转发，不再单独维护一份停止实现。
#
# ⚠️ 本文件只在发布包内生效（需同目录存在 prod.sh）；仓库内的副本是模板，不要直接执行。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/prod.sh" stop "$@"
