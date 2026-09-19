#!/bin/bash
# ai_orz - 别名：重启生产服务 → 统一入口 ai_orz.sh restart
# 实现在 scripts/prod.sh（restart = stop + start，不重新构建），本文件只转发。等价 make restart。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/ai_orz.sh" restart "$@"
