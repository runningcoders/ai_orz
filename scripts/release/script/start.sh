#!/bin/bash
# ai_orz 发布包 - 启动服务（别名）
#
# 实现在同目录 prod.sh —— 与仓库 scripts/prod.sh 是同一份文件（打包时复制进来），
# 停止策略 / 幂等重启 / 日志落盘规则因此全项目只有一处实现。
#
# 用法: ./script/start.sh        后台启动（已在运行则先优雅停止，幂等重启）
#       ./script/start.sh -f     前台运行（Ctrl+C 停止）
#       make start               等价后台启动
#
# ⚠️ 本文件只在发布包内生效（需同目录存在 prod.sh）；仓库内的副本是模板，不要直接执行。

set -eu

exec "$(cd "$(dirname "$0")" && pwd)/prod.sh" start "$@"
