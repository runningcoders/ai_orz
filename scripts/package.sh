#!/bin/bash
# ai_orz - 发布物打包脚本
# 一键编译 release（前端 dist/ + 后端二进制）并组装为可分发目录：
#   发布目录 + tar.gz（内含 ai_orz 二进制 + dist/ + start.sh 启动脚本 + README.txt）
#
# 本地与 CI 共用（release.yml 直接调用本脚本，保证打包逻辑只有一处）。
#
# Usage:
#   ./scripts/package.sh [版本号]
#   make package [VERSION=v1.0.0]
#
# 版本号缺省时自动从 git 推导（git describe --tags --always），无法推导则用 dev。
# 目标平台三元组由 rustc host 推导（本地与 CI 均直接可用）。
# 产物: ./ai_orz-{版本}-{平台}.tar.gz（解压后 ./start.sh 即可运行）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# 颜色输出
GREEN=$(printf '\033[0;32m')
BLUE=$(printf '\033[0;34m')
YELLOW=$(printf '\033[0;33m')
NC=$(printf '\033[0m')

# 版本号：优先显式传入，其次 git tag/describe，兜底 dev
VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    VERSION="$(git -C "$REPO_ROOT" describe --tags --always 2>/dev/null || echo dev)"
fi
# 校验版本号，拒绝含 shell/路径特殊字符的非法值
if ! printf '%s' "$VERSION" | grep -Eq '^[A-Za-z0-9._-]+$'; then
    echo "invalid version: $VERSION" >&2
    exit 1
fi

# 目标平台三元组（与 rustc host 一致）
TARGET="$(rustc -vV | sed -n 's/^host: //p')"

echo "🚀 ai_orz 发布物打包"
echo "   版本: ${BLUE}$VERSION${NC}"
echo "   平台: ${BLUE}$TARGET${NC}"
echo ""

# 1. 编译 release（复用本地构建链路：前端 dx build --release → dist/，后端 cargo build --release）
"$SCRIPT_DIR/start.sh" build

# 2. 组装发布目录
PKG_NAME="ai_orz-${VERSION}-${TARGET}"
PKG_DIR="$REPO_ROOT/$PKG_NAME"
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR"

cp "$REPO_ROOT/target/release/ai_orz" "$PKG_DIR/"
cp -r "$REPO_ROOT/dist" "$PKG_DIR/dist"

# 运维脚本 + Makefile 统一入口（scripts/release/ 是唯一来源，打包即整体复制）
cp -R "$SCRIPT_DIR/release/script" "$PKG_DIR/script"
cp "$SCRIPT_DIR/release/Makefile" "$PKG_DIR/Makefile"
chmod +x "$PKG_DIR/script/"*.sh

# README.md（Markdown 模板在 scripts/release/README.md，替换版本/平台占位符）
sed -e "s/__VERSION__/$VERSION/g" -e "s/__TARGET__/$TARGET/g" \
    "$SCRIPT_DIR/release/README.md" > "$PKG_DIR/README.md"

# 3. 打包 tar.gz
tar czf "${PKG_NAME}.tar.gz" "$PKG_NAME"

echo ""
echo "${GREEN}✅ 打包完成${NC}"
echo "   发布包: ${BLUE}$REPO_ROOT/${PKG_NAME}.tar.gz${NC}"
echo "   目录:   ${BLUE}$REPO_ROOT/$PKG_NAME/${NC}（解压后 make start 即可运行，详见 README.md）"
echo "   体积:   $(du -sh "$PKG_NAME.tar.gz" | cut -f1)"
