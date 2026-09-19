#!/bin/bash
# ai_orz - 兼容别名：start.sh → 统一入口 ai_orz.sh
#
# 旧调用方式（./scripts/start.sh dev|backend|frontend|build|prod|prod-stop|...）保持可用，
# 实现全部收敛在 scripts/ai_orz.sh 与 scripts/run.sh / scripts/prod.sh，本文件只转发。
# 无参等价 dev（保持历史行为）。

set -eu

DIR="$(cd "$(dirname "$0")" && pwd)"
if [ $# -eq 0 ]; then
    exec "$DIR/ai_orz.sh" dev
fi
exec "$DIR/ai_orz.sh" "$@"
