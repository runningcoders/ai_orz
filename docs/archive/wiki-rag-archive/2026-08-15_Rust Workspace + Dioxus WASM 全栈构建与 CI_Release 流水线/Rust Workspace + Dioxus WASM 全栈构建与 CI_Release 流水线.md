> 📦 归档标记（2026-08-15）：被 [Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线](docs/wiki/knowledge/zh/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线/Rust Workspace + Dioxus WASM 全栈构建与 CI_CD 流水线.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: build_system
name: Rust Workspace + Dioxus WASM 全栈构建与 CI/Release 流水线
category: build_system
scope:
    - '**'
source_files:
    - Cargo.toml
    - rust-toolchain.toml
    - start.sh
    - build.sh
    - scripts/build_frontend.sh
    - frontend/build.rs
    - .github/workflows/rust.yml
    - .github/workflows/release.yml
    - common/config/ai_orz.toml
---

## 1. 构建系统与工具链

- **Cargo Workspace**：根 `Cargo.toml` 定义 workspace，成员包含后端 `ai_orz`、前端 `frontend`（Dioxus 0.7 WASM）、共享 crate `common` 与过程宏 crate `ai-orz-macros`，resolver = "2"。
- **固定工具链**：`rust-toolchain.toml` 锁定 `stable` channel，并预注册 `wasm32-unknown-unknown` target 以及 `rustfmt`、`clippy` 组件。
- **统一入口脚本**：`start.sh` 是开发/生产/构建的统一 CLI，支持 `dev`（`cargo run` + `dx serve` 双进程）、`build`（前端 release + 后端 release）、`prod`（编译后运行 `target/release/ai_orz`）、`backend`、`frontend`、`help`；`build.sh` 仅转发到 `start.sh build`。
- **前端构建脚本**：`scripts/build_frontend.sh` 封装 `dx build --release`、查找 dx 输出目录（兼容新旧路径）、复制 `index.html` 与 `public/` 静态资源到仓库根 `dist/`，并清理未被引用的旧 hash 资产。该脚本被 `start.sh` 和 CI e2e job 共用，保证「产物如何进入 dist/」只有一处逻辑。
- **Tailwind CSS v4 编译**：`frontend/build.rs` 在 cargo 构建时调用 `node_modules/.bin/tailwindcss -i styles/input.css -o public/output.css --minify`，若 `node_modules` 缺失则自动执行 `npm install`，失败时以 warning 跳过而非中断构建。
- **编译期配置注入**：`frontend/build.rs` 读取 `.ai_orz/ai_orz.toml`（不存在则回退到 `common/config/ai_orz.toml`），解析为 `AppConfig` 并生成 `OUT_DIR/compiled_config.rs`，提供 `COMPILED_CONFIG` 常量与 `get_config()` 函数，确保前后端配置一致。
- **文档中心静态资源**：同 build.rs 递归扫描 `docs/design/`、`docs/plan/`、`docs/archive/`、`docs/wiki/zh/content/`，复制到 `frontend/public/docs/` 并生成 `index.json` 供前端运行时加载。

## 2. CI 流水线（GitHub Actions）

### rust.yml（PR / main push）
- **jobs 顺序**：`fmt` → `lint`（clippy --all-targets -D warnings）→ `backend` + `frontend` + `coverage`（三者并行，依赖 lint）。
- **缓存策略**：全局启用 `sccache`（`mozilla-actions/sccache-action`，本地磁盘模式，`~/.cache/sccache`），key 基于 `**/Cargo.lock`；`ort-sys` 预编译二进制单独缓存到 `~/.cache/ort`。
- **环境约束**：`SQLX_OFFLINE=true`（使用 `.sqlx/` 离线数据库 schema）、`CARGO_INCREMENTAL=0`（让 sccache 完全接管缓存，避免增量编译干扰）。
- **依赖安装**：lancedb 的 build script 需要 `protobuf-compiler` + `libprotobuf-dev`，CI 显式安装。
- **覆盖率门禁**：通过 `cargo-llvm-cov` 收集覆盖率，`push main` 要求 fail-under-lines ≥ 45%，PR 放宽至 38%；报告同时输出日志与 GitHub Summary。
- **E2E 测试**：注释说明当前已移出 CI，仅在 `e2e/` 目录本地运行 Playwright。

### release.yml（tag v* 或手动触发）
- **矩阵构建**：`ubuntu-latest` 目标 `x86_64-unknown-linux-gnu`，`macos-latest` 目标 `aarch64-apple-darwin`；明确不做交叉编译（lancedb/ort-sys 交叉链太重）。
- **构建流程**：安装 dioxus-cli (`curl https://dioxuslabs.com/install.sh | bash`) → 复用 `./start.sh build` → 将 `target/release/ai_orz` 与 `dist/` 打包为 `ai_orz-${TAG}-${TARGET}.tar.gz`。
- **发布**：仅当 tag 形如 `refs/tags/v*` 时触发 `publish` job，下载所有 artifact 并通过 `softprops/action-gh-release@v2` 创建 GitHub Release，自动生成 release notes。
- **安全校验**：对 `${REF_NAME}` 做白名单正则匹配，拒绝含 shell/路径特殊字符的非法值。

## 3. 架构约定与约束

- **单入口原则**：所有构建/启动场景均通过 `start.sh` 子命令进入，禁止绕过脚本直接调用 `cargo` 或 `dx`。
- **产物位置约定**：前端静态资源统一产出到仓库根 `dist/`（由 `scripts/build_frontend.sh` 负责），后端二进制位于 `target/release/ai_orz`；生产模式下后端从同目录 `dist/` 提供前端 SPA。
- **配置来源优先级**：编译期优先 `.ai_orz/ai_orz.toml`，不存在则回退到 `common/config/ai_orz.toml`，两者任一变更都会触发前端重新构建（`cargo:rerun-if-changed`）。
- **跨模块依赖边界**：workspace 中 `common` 与 `ai-orz-macros` 不依赖后端 `service` 代码，保持可独立编译；前端通过 `common` 共享 DTO/枚举/错误类型。
- **无 Dockerfile**：项目未使用容器化镜像，发布产物为自包含 tar.gz（二进制 + dist/ 静态文件 + migrations 内嵌），解压后可直接运行。
- **迁移管理**：SQLx 迁移位于 `migrations/`，CI 通过 `SQLX_OFFLINE=true` 使用 `.sqlx/` 中的离线 schema，无需连接真实数据库即可编译。

## 4. 关键文件清单

- `Cargo.toml`（workspace 定义、成员、features、[[test]] 集成测试声明）
- `rust-toolchain.toml`（稳定版 toolchain + wasm32 target）
- `start.sh`（统一 dev/build/prod/backend/frontend 入口）
- `build.sh`（转发到 start.sh build）
- `scripts/build_frontend.sh`（dx build 产物复制与 dist/ 整理）
- `frontend/build.rs`（Tailwind 编译、docs 复制、编译期配置注入）
- `.github/workflows/rust.yml`（fmt/clippy/test/coverage 主流水线）
- `.github/workflows/release.yml`（tag 触发的多平台 release 构建与发布）
- `common/Cargo.toml`、`ai-orz-macros/Cargo.toml`、`frontend/Cargo.toml`（各 crate 依赖）
- `common/config/ai_orz.toml`（默认配置，作为前端编译期回退源）
- `migrations/*.sql`（SQLx 数据库迁移脚本）
- `.env.example`（环境变量示例）