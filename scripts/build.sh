#!/bin/bash
# ai_orz - 兼容别名：build.sh → 统一入口 ai_orz.sh build
# 实现在 scripts/prod.sh（build）+ scripts/build_frontend.sh（前端），本文件只转发。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/ai_orz.sh" build "$@"
