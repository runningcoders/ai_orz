#!/bin/bash
# ai_orz - 全量编译脚本
# 编译后端 + 编译前端，输出到 dist 目录

set -e

# 加载 rustup 环境
source "$HOME/.cargo/env"

echo "🔨 Building ai_orz full-stack..."
echo ""

# 编译前端
echo "🎨 Building frontend with dioxus-cli..."
cd "$(dirname "$0")/frontend"
dx build --release

# 复制前端输出到项目根目录 dist
mkdir -p ../dist
cp -r target/dx/frontend/release/web/public/* ../dist/

echo ""
echo "🏗️  Building backend..."
cd ..
cargo build --release

echo ""
echo "✅ Build complete!"
echo "📦 Output: ./target/release/ai_orz"
echo "🌐 Static files: ./dist/"
echo ""
echo "Run it with: ./target/release/ai_orz"
