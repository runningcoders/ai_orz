#!/bin/bash
# ai_orz - 重启脚本（停止后启动，参数透传给 start.sh）

set -e

cd "$(cd "$(dirname "$0")/.." && pwd)"

./script/stop.sh
exec ./script/start.sh "$@"
