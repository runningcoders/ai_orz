#!/bin/bash
# ai_orz - 依赖预检脚本
# 检测指定模式所需的工具链依赖，缺失时打印精确安装命令；--fix 自动安装可自动装的项
#
# Usage:
#   ./scripts/check_deps.sh [dev|frontend|backend|build|prod] [--fix]
#   make doctor                # 等价 ./scripts/check_deps.sh dev
#   make doctor FIX=1          # 等价 --fix
#
# 模式 → 依赖矩阵：
#   依赖                  | dev | frontend | backend | build/prod
#   cargo / rustc (rustup) | ✅  | ✅       | ✅      | ✅
#   protoc                | ✅  | -        | ✅      | ✅        (lancedb build script 需要)
#   dx (dioxus-cli)       | ✅  | ✅       | -       | ✅
#   wasm32-unknown-unknown| ✅  | ✅       | -       | ✅        (前端 WASM target)
#   node + npm            | ✅  | ✅       | -       | ✅        (build.rs 自动触发 tailwind 编译)
#   frontend/node_modules | ✅  | ✅       | -       | ✅        (tailwindcss，build.rs 会自动 npm install)
#
# --fix 自动安装能力：
#   ✅ wasm32 target    → rustup target add
#   ✅ tailwindcss      → cd frontend && npm install
#   ✅ dx               → cargo install dioxus-cli --locked（源码编译，耗时数分钟）
#   ✅ protoc (brew)    → brew install protobuf（有 brew 时自动）
#   ⚠️ protoc (apt)     → 打印 sudo 命令，不代跑（需要 root）
#   ⚠️ rustup / node    → 打印官方安装命令，不代跑（涉及系统级变更）
#
# 环境变量：
#   DX_VERSION  覆盖 dioxus-cli 版本（默认 0.7.10，须与 frontend dioxus 大版本匹配）

set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# 加载 rustup 环境（与 start.sh 同口径：非交互 shell 里 cargo 可能不在 PATH）
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

# PATH 探测增强：服务器 / CI / IDE 调起的非交互 shell 常缺用户级与包管理器 bin，
# 先补齐常见位置再检测，避免「已安装却误报缺失」（探测到的不覆盖已有 PATH 顺序）
for _dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin /usr/local/bin "$HOME/bin"; do
    if [ -d "$_dir" ]; then
        case ":$PATH:" in *":$_dir:"*) ;; *) PATH="$_dir:$PATH" ;; esac
    fi
done
# nvm：取最高版本的 node bin（若 node 尚不可用）
if ! command -v node >/dev/null 2>&1 && [ -d "$HOME/.nvm/versions/node" ]; then
    _nvm_bin=$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | sort -V | tail -1)
    if [ -n "$_nvm_bin" ]; then
        PATH="$_nvm_bin:$PATH"
    fi
fi
export PATH

# ===== 参数解析 =====
MODE="dev"
FIX="${FIX:-0}"
for arg in "$@"; do
    case "$arg" in
        dev|frontend|backend|build|prod) MODE="$arg" ;;
        --fix) FIX=1 ;;
        *)
            echo "未知参数: $arg（用法: $0 [dev|frontend|backend|build|prod] [--fix]）" >&2
            exit 2
            ;;
    esac
done
DX_VERSION="${DX_VERSION:-0.7.10}"

# 颜色输出（与 start.sh 同款，实际转义字符避免 echo -e 兼容性问题）
RED=$(printf '\033[0;31m')
GREEN=$(printf '\033[0;32m')
YELLOW=$(printf '\033[0;33m')
BLUE=$(printf '\033[0;34m')
NC=$(printf '\033[0m')

# ===== 模式 → 依赖需求 =====
needs_backend() {
    case "$MODE" in dev|backend|build|prod) return 0 ;; *) return 1 ;; esac
}
needs_frontend() {
    case "$MODE" in dev|frontend|build|prod) return 0 ;; *) return 1 ;; esac
}

# ===== 检测结果收集 =====
# 每个缺失项一个条目：名称|用途|安装命令|能否自动装(rustup/npm/dx/brew/no)
MISSING=()
MISSING_COUNT=0
OK_LINES=()

# 用法: record_ok <名称> <状态详情>
record_ok() {
    OK_LINES+=("✅ $1  $2")
}

# 用法: record_missing <名称> <用途> <安装命令> <autofix类别>
record_missing() {
    MISSING+=("$1|$2|$3|$4")
    MISSING_COUNT=$((MISSING_COUNT + 1))
}

# ===== 逐项检测 =====

# 1. cargo / rustc（全部模式必需）
if command -v cargo >/dev/null 2>&1; then
    record_ok "cargo" "$(cargo --version 2>/dev/null || echo installed)"
else
    record_missing "cargo/rustc" "Rust 工具链（全部模式必需）" \
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" "no"
fi

# 2. protoc（backend 编译 lancedb 需要）
if needs_backend; then
    if command -v protoc >/dev/null 2>&1; then
        record_ok "protoc" "$(protoc --version 2>/dev/null || echo installed)"
    else
        # 按平台给出对应命令；macOS 有 brew 时 --fix 可自动装
        if command -v brew >/dev/null 2>&1; then
            record_missing "protoc" "lancedb / lance-encoding build script 需要" \
                "brew install protobuf" "brew"
        elif command -v apt-get >/dev/null 2>&1; then
            record_missing "protoc" "lancedb / lance-encoding build script 需要（Linux 还需 well-known types 头文件）" \
                "sudo apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev" "no"
        else
            record_missing "protoc" "lancedb / lance-encoding build script 需要" \
                "安装 protobuf-compiler（系统包管理器）" "no"
        fi
    fi
fi

# 3. dx / dioxus-cli（前端开发与构建需要）
if needs_frontend; then
    if command -v dx >/dev/null 2>&1; then
        DX_VER="$(dx --version 2>/dev/null | head -1)"
        record_ok "dx" "$DX_VER"
        # 版本主次号校验：与 frontend dioxus 大版本（0.7）不匹配时警告（不阻断）
        case "$DX_VER" in
            *" 0.7."*) ;;
            *)
                echo "${YELLOW}⚠️  dx 版本与 frontend dioxus 0.7.x 不匹配（$DX_VER），可能出现 WASM 绑定/配置不兼容${NC}" >&2
                echo "   建议: cargo install dioxus-cli --version $DX_VERSION --locked" >&2
                ;;
        esac
    else
        record_missing "dx (dioxus-cli)" "前端开发服务器与 WASM 构建（dev/frontend/build/prod）" \
            "cargo install dioxus-cli --version $DX_VERSION --locked" "dx"
    fi
fi

# 4. wasm32-unknown-unknown target（前端 WASM 编译需要）
if needs_frontend; then
    if rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
        record_ok "wasm32 target" "wasm32-unknown-unknown"
    elif command -v rustup >/dev/null 2>&1; then
        record_missing "wasm32-unknown-unknown" "前端 WASM 编译 target" \
            "rustup target add wasm32-unknown-unknown" "rustup"
    else
        record_missing "wasm32-unknown-unknown" "前端 WASM 编译 target（需先装 rustup）" \
            "rustup target add wasm32-unknown-unknown" "no"
    fi
fi

# 5. node + npm（build.rs 编译 Tailwind 时自动调用）
if needs_frontend; then
    if command -v node >/dev/null 2>&1; then
        record_ok "node" "$(node --version 2>/dev/null || echo installed)"
    else
        record_missing "node" "frontend/build.rs 编译 Tailwind CSS 需要 npm" \
            "brew install node（或 apt install nodejs npm / nvm）" "no"
    fi
    if command -v npm >/dev/null 2>&1; then
        record_ok "npm" "$(npm --version 2>/dev/null || echo installed)"
    else
        record_missing "npm" "frontend/build.rs 编译 Tailwind CSS 需要" \
            "随 node 一起安装（brew install node / apt install npm / nvm）" "no"
    fi
fi

# 6. frontend/node_modules tailwindcss（build.rs 会自动 npm install，这里只预警）
if needs_frontend; then
    if [ -x "$REPO_ROOT/frontend/node_modules/.bin/tailwindcss" ]; then
        record_ok "tailwindcss" "frontend/node_modules（build.rs 自动维护）"
    elif command -v npm >/dev/null 2>&1; then
        # 不算硬缺失：build.rs 首次编译会自动 npm install，列为「将自动处理」
        OK_LINES+=("🔄 tailwindcss  未安装 — 首次构建时 build.rs 自动 npm install（无需处理）")
    fi
fi

# ===== 输出报告 =====
echo ""
echo "🔍 依赖预检（模式: ${BLUE}$MODE${NC}）"
echo "────────────────────────────────────────────"
for line in "${OK_LINES[@]}"; do
    echo "  $line"
done

if [ "$MISSING_COUNT" -eq 0 ]; then
    echo "────────────────────────────────────────────"
    echo "${GREEN}✅ 依赖检查通过，$MODE 模式所需的工具链齐备${NC}"
    exit 0
fi

# 缺失项报告
echo "────────────────────────────────────────────"
echo "${RED}❌ 缺失 $MISSING_COUNT 项:${NC}"
echo ""
i=0
for entry in "${MISSING[@]}"; do
    IFS='|' read -r name purpose install_cmd autofix <<< "$entry"
    i=$((i + 1))
    echo "  $i. ${RED}$name${NC} — $purpose"
    echo "     ${BLUE}安装:${NC} $install_cmd"
    case "$autofix" in
        no) echo "     （需手动执行）" ;;
        *) echo "     （可通过 --fix 自动安装）" ;;
    esac
done
echo ""

# ===== 自动修复 =====
if [ "$FIX" = "1" ]; then
    echo "${YELLOW}🛠  开始自动修复...${NC}"
    FIXED=0
    for entry in "${MISSING[@]}"; do
        IFS='|' read -r name purpose install_cmd autofix <<< "$entry"
        case "$autofix" in
            rustup)
                echo "→ rustup target add wasm32-unknown-unknown"
                rustup target add wasm32-unknown-unknown && FIXED=$((FIXED + 1)) || echo "${RED}   失败，请手动执行: $install_cmd${NC}"
                ;;
            dx)
                echo "→ cargo install dioxus-cli --version $DX_VERSION --locked"
                echo "${YELLOW}   （源码编译，预计 5~15 分钟，取决于机器与网络）${NC}"
                cargo install dioxus-cli --version "$DX_VERSION" --locked && FIXED=$((FIXED + 1)) || echo "${RED}   失败，请手动执行: $install_cmd${NC}"
                ;;
            brew)
                echo "→ brew install protobuf"
                brew install protobuf && FIXED=$((FIXED + 1)) || echo "${RED}   失败，请手动执行: $install_cmd${NC}"
                ;;
            *)
                echo "→ 跳过 $name（无法安全自动安装，请手动执行上方命令）"
                ;;
        esac
    done
    # tailwindcss 交给 build.rs；node_modules 若缺失且 npm 可用则顺手装上
    if needs_frontend && command -v npm >/dev/null 2>&1 \
        && [ ! -x "$REPO_ROOT/frontend/node_modules/.bin/tailwindcss" ]; then
        echo "→ cd frontend && npm install（tailwindcss）"
        (cd "$REPO_ROOT/frontend" && npm install) \
            && echo "✅ tailwindcss 安装完成" \
            || echo "${YELLOW}   npm install 失败 — 不阻断（build.rs 首次构建时会重试）${NC}"
    fi
    echo ""
    if [ "$FIXED" -gt 0 ]; then
        echo "${GREEN}✅ 已自动安装 $FIXED 项${NC}"
    fi
    echo "${YELLOW}请重新运行本脚本确认: $0 $MODE${NC}"
    exit 1
fi

echo "${YELLOW}💡 一键修复（可自动项）: $0 $MODE --fix   或   make doctor FIX=1${NC}"
exit 1
