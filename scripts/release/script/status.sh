#!/bin/bash
# ai_orz 发布包 - 查看服务状态（别名）：PID / 运行时长 / 资源占用 / 监听端口
#
# ⚠️ 本文件只在发布包内生效（需同目录存在 prod.sh）；仓库内的副本是模板，不要直接执行。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/prod.sh" status "$@"
