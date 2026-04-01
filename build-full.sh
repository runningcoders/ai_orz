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

# 从环境变量读取后端 API 地址，默认 http://localhost:3000
export BACKEND_API_URL=${BACKEND_API_URL:-http://localhost:3000}

dx build --release

# dx 输出位置可能变化，复制已经编译好的产物
mkdir -p ../dist
mkdir -p ../dist/wasm
cp index.html ../dist/

# dx 编译成功后产物在这里
if [ -d target/dx/frontend/release/web/public/wasm ]; then
    cp -r target/dx/frontend/release/web/public/wasm/* ../dist/wasm/
elif [ -d pkg ]; then
    # 降级：复制 pkg 目录的产物
    cp -r pkg/* ../dist/wasm/
else
    echo "⚠️  No frontend build output found"
    exit 1
fi

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
