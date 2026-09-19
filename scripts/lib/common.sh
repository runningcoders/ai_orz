#!/bin/bash
# ai_orz 脚本公共库 —— 颜色 / 路径推导 / PATH 补齐
#
# 仅供其它脚本 source（用法见下），禁止直接执行。
# 目的：颜色定义、仓库根推导、PATH 补齐这类「每个脚本都要写一遍」的东西只留一份。
#
#   source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"
#
# 兼容 macOS 自带 bash 3.2（不用 mapfile / 关联数组）。
# 本库被仓库 scripts/ 与发布包 script/ 共用（同一份文件，两处部署），
# 因此不得引用任何仓库专属资源（cargo / dx / frontend/ 等）。

# ===== 颜色输出 =====
# 用实际转义字符而非 echo -e：后者在不同 shell / macOS bash 3.2 下行为不一致
RED=$(printf '\033[0;31m')
GREEN=$(printf '\033[0;32m')
YELLOW=$(printf '\033[0;33m')
BLUE=$(printf '\033[0;34m')
NC=$(printf '\033[0m')

# ===== 路径推导 =====
# 本库固定位于 <根>/scripts/lib/（仓库）或 <根>/script/lib/（发布包），
# 故可由自身位置反推脚本目录与根，调用方无需各自计算 SCRIPT_DIR / REPO_ROOT。
_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="$(cd "$_LIB_DIR/.." && pwd)"
REPO_ROOT="$(cd "$SCRIPTS_DIR/.." && pwd)"

# ===== PATH 补齐 =====
# 服务器 / CI / IDE / Makefile 调起的非交互 shell 常缺用户级与包管理器 bin，
# 不补齐会出现「工具已装却报缺失」（探测到目录只追加，不覆盖已有 PATH 顺序）。
setup_path() {
    # rustup 环境（非交互 shell 里 cargo 常常不在 PATH）
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    fi
    local dir
    for dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin /usr/local/bin "$HOME/bin" \
        /usr/bin /bin /usr/sbin /sbin; do
        if [ -d "$dir" ]; then
            case ":$PATH:" in *":$dir:"*) ;; *) PATH="$dir:$PATH" ;; esac
        fi
    done
    # nvm：node 尚不可用时取最高版本的 node bin
    if ! command -v node >/dev/null 2>&1 && [ -d "$HOME/.nvm/versions/node" ]; then
        local nvm_bin
        nvm_bin=$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | sort -V | tail -1)
        if [ -n "$nvm_bin" ]; then
            PATH="$nvm_bin:$PATH"
        fi
    fi
    export PATH
}

# ===== 输出助手 =====
# 统一前缀，省得每个脚本各写一套 echo 颜色拼接
info() { echo "$1"; }
ok() { echo "${GREEN}$1${NC}"; }
warn() { echo "${YELLOW}$1${NC}"; }
err() { echo "${RED}$1${NC}" >&2; }
die() { err "$1"; exit "${2:-1}"; }

# 用法: usage_die <错误说明> <用法文本>  —— 参数错误统一走这个出口（退出码 2）
usage_die() {
    err "$1"
    echo "$2" >&2
    exit 2
}
