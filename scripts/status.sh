#!/bin/bash
# ai_orz - 别名：查看生产服务状态 → 统一入口 ai_orz.sh status
# 实现在 scripts/prod.sh（status），本文件只转发。等价 make status。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/ai_orz.sh" status "$@"
