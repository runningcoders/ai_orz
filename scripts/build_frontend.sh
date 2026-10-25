#!/bin/bash
# 前端构建 + 产物复制（dx build --release → dist/）
#
# 【唯一实现】「构建产物怎么进 dist/」只在这里：
# prod.sh build（make build / make build-fe）、Dockerfile frontend-builder 阶段、
# CI release 打包链路全部复用本脚本。
# 依赖：dx（dioxus-cli）、wasm32-unknown-unknown target；
# Tailwind CSS 编译由 frontend/build.rs 在 dx build 时自动触发。

set -e

# shellcheck source=./lib/common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

cd "$REPO_ROOT/frontend"
export BACKEND_API_URL=${BACKEND_API_URL:-http://localhost:3000}

# 先清理再编译：删 1 天前的旧哈希副本（前端 wasm 产物同样按指纹累积）
clean_stale_artifacts

# dx build 在 wasm-opt 失败时仍会生成产物，忽略此错误
dx build --release 2>&1 || true

mkdir -p "$REPO_ROOT/dist"
cp index.html "$REPO_ROOT/dist/"

# 查找编译产物（dx 可能输出到 frontend/target 或 workspace 根 target，优先新构建产物，pkg 仅作最后回退）
DX_OUTPUT_DIR=""
if [ -d target/dx/frontend/release/web/public ]; then
    DX_OUTPUT_DIR="target/dx/frontend/release/web/public"
elif [ -d "$REPO_ROOT/target/dx/frontend/release/web/public" ]; then
    DX_OUTPUT_DIR="$REPO_ROOT/target/dx/frontend/release/web/public"
elif [ -d target/dx/frontend/web/public ]; then
    DX_OUTPUT_DIR="target/dx/frontend/web/public"
elif [ -d "$REPO_ROOT/target/dx/frontend/web/public" ]; then
    DX_OUTPUT_DIR="$REPO_ROOT/target/dx/frontend/web/public"
elif [ -d pkg ]; then
    DX_OUTPUT_DIR="pkg"
fi

if [ -n "$DX_OUTPUT_DIR" ]; then
    # 新版 dx 产出带 hash 的资源（assets/frontend-*.js 等）并在 index.html 中注入引用，
    # 因此除 pkg 回退路径外，直接整体复制 dx 输出目录（含注入后的 index.html 与 assets/）
    if [ "$DX_OUTPUT_DIR" != "pkg" ]; then
        rm -rf "$REPO_ROOT/dist"
        mkdir -p "$REPO_ROOT/dist"
        cp -R "$DX_OUTPUT_DIR/." "$REPO_ROOT/dist/"
    else
        # 旧版 dx / pkg 回退：固定文件名复制到 dist/pkg/
        mkdir -p "$REPO_ROOT/dist/pkg"
        if [ -f "$DX_OUTPUT_DIR/frontend_bg.wasm" ]; then
            cp "$DX_OUTPUT_DIR/frontend_bg.wasm" "$REPO_ROOT/dist/pkg/"
        fi
        if [ -f "$DX_OUTPUT_DIR/frontend.js" ]; then
            cp "$DX_OUTPUT_DIR/frontend.js" "$REPO_ROOT/dist/pkg/"
        fi
        if [ -d "$DX_OUTPUT_DIR/snippets" ]; then
            cp -r "$DX_OUTPUT_DIR/snippets" "$REPO_ROOT/dist/pkg/"
        fi
    fi
    # 复制 public/ 静态资源（output.css / vendor / docs 等，index.html 直接引用）
    # 注：新版 dx 已把 public 资产并入输出目录，此处幂等覆盖兼容两种情况
    if [ -d public ]; then
        cp -r public/. "$REPO_ROOT/dist/"
    fi
    # dx 输出目录会累积历史构建的 hash 资产，清理未被入口引用的旧文件
    # 做法：从 index.html 出发沿文本产物（js/css/html…）做引用闭包——wasm 只被 js 引用，
    # 且旧 js 也会引用旧 wasm，必须传递追溯，否则删掉旧 js 后旧 wasm 仍会被误判为「有引用」
    # 性能：按文件一次 grep，避免「每个资产都对整个 assets/ 扫一遍」的 O(n²)（后者会反复读几十 MB wasm）
    if [ -d "$REPO_ROOT/dist/assets" ]; then
        asset_names=$(mktemp)
        referenced=$(mktemp)
        scanned=$(mktemp)
        ls -1 "$REPO_ROOT/dist/assets" > "$asset_names"
        grep -hoF -f "$asset_names" "$REPO_ROOT/dist/index.html" 2>/dev/null | sort -u > "$referenced" || true
        while true; do
            pending=$(grep -vxF -f "$scanned" "$referenced" 2>/dev/null || true)
            [ -z "$pending" ] && break
            printf '%s\n' "$pending" >> "$scanned"
            while IFS= read -r name; do
                case "$name" in
                    *.js|*.mjs|*.css|*.json|*.html|*.map|*.svg|*.txt)
                        [ -f "$REPO_ROOT/dist/assets/$name" ] || continue
                        grep -hoF -f "$asset_names" "$REPO_ROOT/dist/assets/$name" 2>/dev/null \
                            | sort -u >> "$referenced" || true
                        ;;
                esac
            done <<< "$pending"
            sort -u -o "$referenced" "$referenced"
        done
        while IFS= read -r name; do
            grep -qxF "$name" "$referenced" || rm -f "$REPO_ROOT/dist/assets/$name"
        done < "$asset_names"
        rm -f "$asset_names" "$referenced" "$scanned"
    fi
    echo "✅ 前端编译产物已复制"
    echo "   来源: $DX_OUTPUT_DIR"
else
    echo "⚠️  未找到前端编译产物" >&2
    exit 1
fi
