#!/bin/bash
# ai_orz - 全量编译脚本
# 编译后端 + 编译前端，输出到 dist 目录

set -e

echo "🔨 Building ai_orz full-stack..."
echo ""

# 加载 rustup 环境
source "$HOME/.cargo/env"

# 编译前端
echo "🎨 Building frontend..."
cd "$(dirname "$0")/frontend"
cargo build --target wasm32-unknown-unknown --release
mkdir -p pkg
wasm-bindgen target/wasm32-unknown-unknown/release/frontend.wasm --out-dir pkg --target web

# 复制前端输出到项目根目录 dist
mkdir -p ../dist
cp index.html ../dist/
cp -r ./pkg/* ../dist/

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
