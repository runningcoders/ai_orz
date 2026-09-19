#!/bin/bash
# ai_orz - 别名：实时跟踪服务日志 → 统一入口 ai_orz.sh logs
# 实现在 scripts/prod.sh（logs），本文件只转发。等价 make logs。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/ai_orz.sh" logs "$@"
