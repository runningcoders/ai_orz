---
kind: build_system
name: Rust Workspace + Dioxus WASM 全栈构建与 CI/Release 流水线
category: build_system
scope:
    - '**'
source_files:
    - Cargo.toml
    - frontend/Cargo.toml
    - frontend/build.rs
    - start.sh
    - build.sh
    - rust-toolchain.toml
    - .github/workflows/rust.yml
    - .github/workflows/release.yml
    - common/config/ai_orz.toml
---

## 1. 构建系统总览

本项目采用 **Cargo workspace** 组织多 crate 后端（`ai_orz`、`common`、`ai-orz-macros`）与独立前端 crate（`frontend`，Dioxus 0.7 WASM），通过统一的 `start.sh` 脚本覆盖开发、生产、仅编译、仅后端/前端等场景；CI 使用 GitHub Actions 执行格式检查、Clippy、单元测试、集成测试、覆盖率门禁与跨平台 release 打包。

- 工作区定义：根 `Cargo.toml` 的 `[workspace] members = [".", "frontend", "common", "ai-orz-macros"]`，resolver = "2"。
- Rust 工具链锁定：`rust-toolchain.toml` 固定 channel = stable，并预注册 `wasm32-unknown-unknown` target。
- 统一入口：`build.sh` 仅转发到 `start.sh build`；`start.sh` 是唯一的本地构建/运行入口，支持 `dev` / `prod` / `build` / `backend` / `frontend` / `help` 子命令。

## 2. 关键文件与职责

| 文件 | 作用 |
|---|---|
| `Cargo.toml`（根） | workspace 成员声明、后端依赖、`[[test]]` 集成测试目标显式注册 |
| `frontend/Cargo.toml` | 前端 crate 声明，依赖 `dioxus 0.7.9`、`web-sys`、`common` |
| `frontend/build.rs` | 编译期读取 `.ai_orz/ai_orz.toml`（或 `common/config/ai_orz.toml` 默认值）生成 `compiled_config.rs`，复制 `docs/` 为静态文档中心，调用 Tailwind CSS v4 编译 `styles/input.css` → `public/output.css` |
| `start.sh` | 统一脚本：dev 并行启动 `cargo run` + `dx serve`；build 先 `dx build --release` 再 `cargo build --release`；prod 直接运行 `target/release/ai_orz` |
| `.github/workflows/rust.yml` | PR/push main 触发：fmt → clippy → backend(test) + frontend(wasm clippy+test) + coverage(并行)，启用 sccache、SQLX_OFFLINE=true |
| `.github/workflows/release.yml` | 打 `v*` tag 触发：在 ubuntu-latest 与 macos-latest 上分别以 `x86_64-unknown-linux-gnu` / `aarch64-apple-darwin` 原生构建，复用 `./start.sh build`，打包 `ai_orz-{tag}-{target}.tar.gz` 上传 artifact，仅 tag 推送时发布 GitHub Release |
| `migrations/*.sql` | SQLx migrate 迁移文件，由后端二进制内嵌（配合 `sqlx::migrate!`） |
| `common/config/ai_orz.toml` | 默认应用配置，被前端 `build.rs` 作为 fallback 嵌入 |

## 3. 架构与约定

### 3.1 后端构建
- 使用 Cargo workspace 管理四个 crate，`ai_orz` 通过 `path = ./common` 与 `path = ./ai-orz-macros` 引用共享 crate。
- 集成测试通过根 `Cargo.toml` 的多个 `[[test]]` 条目显式声明路径（如 `tests/integration/auth_sysinit_test.rs`），而非仅靠目录约定。
- 数据库迁移通过 SQLx 的 `migrate` feature 与 `migrations/` 目录内按时间戳命名的 `.sql` 文件管理，首次启动自动创建 `.ai_orz/ai_orz.toml`。
- 向量搜索依赖 `fastembed` 禁用 default features 以避免 image-models 引入的 x86 const eval bug，并使用 `duckdb` bundled 版本。

### 3.2 前端构建（Dioxus WASM）
- 前端是独立 crate，通过 `dx build --release` 编译为 WASM + JS，产物可能输出到 `target/dx/frontend/release/web/public` 或 `pkg/`，`start.sh` 会探测多种路径并将 `frontend_bg.wasm`、`frontend.js`、`snippets/` 复制到根 `dist/pkg/`。
- `frontend/build.rs` 在每次构建时：
  - 监听 `../.ai_orz/ai_orz.toml`、`../common/config/ai_orz.toml`、`styles/input.css`、`package.json`、`package-lock.json` 变化；
  - 将后端配置序列化为 JSON 字符串常量 `COMPILED_CONFIG` 并生成 `get_config()` 函数；
  - 递归复制 `docs/design/`、`docs/plan/`、`docs/archive/`、`docs/wiki/zh/content/` 下的所有 `.md` 到 `public/docs/`，同时生成 `index.json` 目录清单；
  - 调用 `node_modules/.bin/tailwindcss -i styles/input.css -o public/output.css --minify`，若 `npm install` 失败则跳过 CSS 构建并警告。
- 前端通过 `web-sys` 白名单暴露浏览器 API（Window、Performance、Storage、EventSource、MessageEvent、Request 等）。

### 3.3 CI 流水线（`.github/workflows/rust.yml`）
- 阶段顺序：`fmt`（最快，独立 job）→ `lint`（clippy --all-targets -D warnings，需安装 `protobuf-compiler`、`libprotobuf-dev`）→ `backend`、`frontend`、`coverage` 三 job 并行。
- 缓存策略：全局启用 sccache（`mozilla-actions/sccache-action`），按 `sccache-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}` 键缓存；另缓存 `~/.cache/ort`（ORT 预编译二进制）。
- 覆盖率：使用 `cargo-llvm-cov`，对 push main 设置 fail-under-lines=45，PR 设置为 38；报告写入 GitHub Step Summary。
- 环境变量：`SQLX_OFFLINE=true`、`CARGO_INCREMENTAL=0`（与 sccache 互斥）。

### 3.4 Release 流水线（`.github/workflows/release.yml`）
- 触发条件：push tag `v*` 或 workflow_dispatch。
- 矩阵：`ubuntu-latest` (x86_64-unknown-linux-gnu) 与 `macos-latest` (aarch64-apple-darwin)，注释明确不做交叉编译（lancedb/ort-sys 太重）。
- 构建流程：安装 dioxus-cli (`dx`) → 执行 `./start.sh build` → 将 `target/release/ai_orz` 与 `dist/` 目录打包为 `ai_orz-{tag}-{target}.tar.gz`。
- 发布：仅当 `refs/tags/v*` 时执行 `publish` job，下载所有 artifact 并通过 `softprops/action-gh-release@v2` 创建 GitHub Release。
- 产物自包含：二进制内嵌 migrations 与默认配置，解压后直接 `./ai_orz` 运行，监听 `0.0.0.0:3000`，从同目录 `dist/` 提供前端静态资源。

## 4. 约定与约束

- **统一入口**：所有本地构建必须通过 `start.sh`，禁止直接调用 `cargo build` 或 `dx build` 绕过前端产物复制逻辑。
- **配置来源优先级**：前端编译期优先读取项目级 `.ai_orz/ai_orz.toml`，不存在则回退到 `common/config/ai_orz.toml` 默认配置，两者任一变更都会触发前端重新编译。
- **CI 质量门禁**：`clippy --all-targets -- -D warnings` 为强制门禁；覆盖率在 main 分支 ≥45%、PR ≥38%，低于阈值即失败。
- **增量编译禁用**：CI 中显式设置 `CARGO_INCREMENTAL=0`，完全交由 sccache 基于内容寻址缓存，避免增量状态污染跨 job 缓存。
- **数据库离线模式**：CI 始终设置 `SQLX_OFFLINE=true`，依赖 `.sqlx/` 缓存的查询类型信息，不连接真实数据库。
- **前端依赖自动安装**：`frontend/build.rs` 会在 Tailwind CSS 缺失时尝试 `npm install`，但失败不会中断构建（仅 warning 并跳过 CSS 编译），属于容错而非强制约束。
- **无 Dockerfile**：项目未使用容器化，部署产物为单二进制 + `dist/` 静态资源的 tar.gz 压缩包。
- **版本策略**：workspace 内各 crate 版本号独立（当前均为 `0.1.0`），release 包名由 git tag 决定，非 cargo metadata 中的 version 字段。