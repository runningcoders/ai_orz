#!/bin/bash
# ai_orz - 别名：停止生产服务 → 统一入口 ai_orz.sh stop
# 实现在 scripts/prod.sh（stop），本文件只转发。等价 make stop。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/ai_orz.sh" stop "$@"
