---
kind: knowledge
name: Doctor 依赖预检
category: 质量工程
scope:
    - 'scripts/check_deps.sh'
    - 'scripts/start.sh'
    - 'Makefile'
    - 'Cargo.toml'
source_files:
    - scripts/check_deps.sh
    - scripts/start.sh
    - Makefile
    - Cargo.toml
    - docs/wiki/knowledge/zh/工具输出与安全治理/工具输出与安全治理.md
    - docs/wiki/zh/content/基础设施/Doctor 依赖预检.md
---

## §1 概述

本卡片定义 AI Orz 项目的**工具链依赖预检体系**——以 `scripts/check_deps.sh` 为核心的多模式依赖检测脚本，配合 `Makefile` 的 `doctor` 目标和 `scripts/start.sh` 的启动前预检，形成「检测 → 修复 → 验证」闭环。核心决策为：

1. **多模式依赖矩阵**：支持 dev/frontend/backend/build/prod 五种模式，每种模式对应不同的工具链依赖集合
2. **--fix 自动修复**：对可自动安装的项（wasm32 target、dx、protoc(brew)、tailwindcss）提供一键修复能力
3. **非破坏性预检**：检测失败不阻断，仅输出精确安装命令和自动修复建议
4. **PATH 探测增强**：自动补齐常见包管理器 bin 目录，避免非交互 shell 误报缺失

## §2 关键文件表

| 文件 | 职责 |
|---|---|
| `scripts/check_deps.sh` | 依赖预检核心脚本（检测 + --fix 自动修复） |
| `scripts/start.sh` | 统一启动脚本（集成 `preflight_deps` 预检步骤） |
| `Makefile` | 提供 `make doctor` 入口（路由到 check_deps.sh） |
| `Cargo.toml` | 后端依赖清单（`doctor` 隐式关联） |
| `docs/wiki/knowledge/zh/工具输出与安全治理/工具输出与安全治理.md` | Level 3 兄弟卡：工具运行时安全治理 |

## §3 架构约定

### 3.1 模式 → 依赖矩阵

| 依赖 | dev | frontend | backend | build/prod |
|---|---|---|---|---|
| cargo / rustc (rustup) | ✅ | ✅ | ✅ | ✅ |
| protoc | ✅ | - | ✅ | ✅ (lancedb build 需要) |
| dx (dioxus-cli) | ✅ | ✅ | - | ✅ |
| wasm32-unknown-unknown | ✅ | ✅ | - | ✅ (前端 WASM target) |
| node + npm | ✅ | ✅ | - | ✅ (build.rs 自动触发 tailwind) |
| frontend/node_modules | ✅ | ✅ | - | ✅ (tailwindcss) |

### 3.2 自动修复能力

| 依赖 | 自动修复方式 | 安全性 |
|---|---|---|
| wasm32 target | `rustup target add wasm32-unknown-unknown` | ✅ 可自动 |
| dx | `cargo install dioxus-cli --version $DX_VERSION --locked` | ✅ 可自动（耗时数分钟） |
| protoc (brew) | `brew install protobuf` | ✅ 可自动（macOS 有 brew 时） |
| tailwindcss | `cd frontend && npm install` | ✅ 可自动 |
| protoc (apt) | 打印 `sudo apt-get install` 命令 | ⚠️ 仅打印（需 root） |
| rustup / node | 打印官方安装命令 | ⚠️ 仅打印（系统级变更） |

### 3.3 PATH 探测增强

非交互 shell（CI/服务器/IDE）常缺用户级与包管理器 bin 目录。脚本启动时自动补齐：

```bash
for _dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin /usr/local/bin "$HOME/bin"; do
    if [ -d "$_dir" ]; then
        case ":$PATH:" in *":$_dir:"*) ;; *) PATH="$_dir:$PATH" ;; esac
    fi
done
# nvm：取最高版本的 node bin
if ! command -v node >/dev/null 2>&1 && [ -d "$HOME/.nvm/versions/node" ]; then
    _nvm_bin=$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | sort -V | tail -1)
    PATH="$_nvm_bin:$PATH"
fi
```

### 3.4 与启动流程的集成

`scripts/start.sh` 在所有模式（除 help）的启动流程中集成 `preflight_deps`：

```
start.sh [mode]
  → preflight_deps(mode)
      → check_deps.sh $MODE
      → 失败时打印 --fix 建议并 exit 1
  → 执行对应模式的启动逻辑
```

### 3.5 Makefile 入口

```makefile
doctor: ## 依赖预检：MODE 指定模式（默认 dev），FIX=1 自动安装可自动项
	./scripts/check_deps.sh $(MODE) $(if $(FIX),--fix)
```

### 3.6 版本兼容性校验

dx（dioxus-cli）版本与前端 dioxus 大版本需匹配：

```bash
DX_VER="$(dx --version 2>/dev/null | head -1)"
case "$DX_VER" in
    *" 0.7."*) ;;  # 匹配，放行
    *) echo "⚠️ dx 版本不匹配，建议: cargo install dioxus-cli --version $DX_VERSION --locked" ;;
esac
```

默认 `DX_VERSION=0.7.10`，须与 `frontend/Cargo.toml` 中 dioxus 大版本对齐。

## §4 硬约束

1. **启动前必须通过预检**：`scripts/start.sh` 在 help 之外的所有模式下强制调用 `preflight_deps`，预检失败直接 exit 1。
2. **禁止绕过预检启动**：不得在 `start.sh` 中移除 `preflight_deps` 调用；不得直接 `cargo run` 或 `dx serve` 跳过预检。
3. **自动修复仅限安全操作**：`--fix` 仅能执行 rustup target add、cargo install、brew install、npm install 四类安全操作，涉及 root 权限或系统级变更的操作一律仅打印命令。
4. **版本对齐必须校验**：dx 版本必须与前端 dioxus 大版本匹配（当前 0.7.x），不匹配时给出警告。
5. **PATH 探测必须增强**：脚本启动时必须补齐常见包管理器 bin 目录（~/.cargo/bin、~/.local/bin、/opt/homebrew/bin、/usr/local/bin、~/bin）。
6. **预检脚本必须幂等**：多次执行 `check_deps.sh` 不应产生副作用（已安装的工具不重复安装）。
7. **--fix 必须可预测**：`--fix` 仅安装脚本明确标记为"可自动"的项，不得隐式安装其他工具。
8. ** tailwindcss 不强制 --fix 安装**：`frontend/node_modules` 缺失时列为「将自动处理」（首次构建时 build.rs 会自动 `npm install`），`--fix` 模式下顺手安装但不阻断。
9. **错误信息必须包含安装命令**：所有缺失项的错误信息必须包含精确的安装命令（含 brew/apt/cargo/rustup/npm 对应的命令）。
10. **DX_VERSION 必须可覆盖**：支持通过环境变量 `DX_VERSION` 覆盖默认 dioxus-cli 版本，避免版本锁定。