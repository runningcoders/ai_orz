---
kind: dependency_management
name: Rust Workspace + NPM 双包管理器依赖管理
category: dependency_management
scope:
    - '**'
source_files:
    - Cargo.toml
    - Cargo.lock
    - rust-toolchain.toml
    - common/Cargo.toml
    - ai-orz-macros/Cargo.toml
    - frontend/Cargo.toml
    - frontend/package.json
    - frontend/package-lock.json
---

## 1. 使用的系统/方法

本项目采用 **双包管理器** 的依赖管理模式：
- **后端与共享库**：使用 Rust 的 Cargo workspace，通过根 `Cargo.toml` 聚合四个成员（`ai_orz`、`frontend`、`common`、`ai-orz-macros`），并启用 `resolver = "2"`。
- **前端样式工具链**：使用 npm（`frontend/package.json` + `package-lock.json`）管理 Tailwind CSS v4 与 DaisyUI v5 等构建期依赖。
- **锁定文件**：Rust 侧使用仓库根目录的 `Cargo.lock`（由 Cargo 自动生成，不可手动编辑）；前端侧使用 `frontend/package-lock.json`（lockfileVersion 3）。
- **工具链锁定**：通过根目录 `rust-toolchain.toml` 固定为 `stable` channel，并预装 `rustfmt`、`clippy` 组件及 `wasm32-unknown-unknown` 目标。

## 2. 关键文件

- `Cargo.toml`（根 workspace 定义，声明 members 与 resolver）
- `Cargo.lock`（全仓库统一的锁定文件，包含所有 crate 的精确版本与 checksum）
- `rust-toolchain.toml`（固定 Rust 工具链与 wasm 目标）
- `common/Cargo.toml`（共享 crate，通过 optional features 按需引入 sqlx/axum/reqwest/tokio/jsonwebtoken 等）
- `ai-orz-macros/Cargo.toml`（proc-macro crate，仅依赖 syn/quote/proc-macro2/schemars/glob/serde_json）
- `frontend/Cargo.toml`（Dioxus WASM 前端，依赖 dioxus 0.7.9、web-sys、wasm-bindgen 等）
- `frontend/package.json` + `frontend/package-lock.json`（Tailwind/DaisyUI 构建依赖）

## 3. 架构与约定

### Workspace 内聚策略
- 所有内部 crate 通过 `path = "./xxx"` 引用，不发布到 crates.io，确保 workspace 内版本一致。
- `common` crate 作为前后端共享层，使用 optional features（`sqlx`、`axum-integration`、`jwt-integration`、`base64-integration`、`reqwest-integration`、`toml-integration`、`bincode-integration`、`tokio-integration`）实现按需编译，避免 WASM 前端引入不必要的后端依赖。
- `ai-orz-macros` 是独立的 proc-macro crate，被 `common` 和主服务共同引用，提供 `#[generate_http_handler]`、日志字段派生、统计事件等能力。

### 版本约束风格
- 外部 crate 普遍使用语义化版本范围（如 `axum = { version = "0.8", ... }`、`tokio = { version = "1", ... }`），而非固定到具体 patch 版本，由 `Cargo.lock` 锁定实际解析结果。
- 对需要严格控制的 crate（如 `uuid = { version = "1.23.0", ... }`、`duckdb = { version = "1.4.0", ... }`）使用精确版本号。
- 通过 `default-features = false` + 显式 `features = [...]` 裁剪依赖树（例如 `fastembed` 禁用 image-models 以避免 x86 const eval bug，`rmcp` 仅启用 client 与 child-process transport）。

### 平台相关依赖
- 使用 `[target.'cfg(unix)'.dependencies]` 仅在 Unix 平台引入 `libc`，体现跨平台依赖隔离。

### 前端依赖
- 仅将 Tailwind CLI、DaisyUI 列为 devDependencies，不参与运行时产物。
- 通过 `build.rs` + Dioxus 构建流程生成 WASM，CSS 通过 `npm run build:css` 独立编译输出到 `public/output.css`。

## 4. 约定与约束

- **Cargo.lock 必须提交**：根级 `Cargo.lock` 已纳入版本控制，保证所有开发者与 CI 获得完全一致的依赖图。
- **workspace 成员统一升级**：新增 crate 应加入根 `Cargo.toml` 的 `members` 列表，而非单独创建顶层 package。
- **共享类型走 common crate**：前后端共享的 DTO/枚举/错误类型集中在 `common`，通过 path 依赖引用，禁止在多个 crate 中重复定义。
- **可选 feature 隔离**：`common` 中的后端特性（sqlx、axum、reqwest、tokio）全部标记为 optional，由使用者按需启用，避免 WASM 环境编译失败。
- **无私有 registry**：未发现 `.cargo/config.toml`、`CRATES_IO_TOKEN`、`CARGO_REGISTRIES_*` 或 npm `.npmrc` 等私有源配置，所有依赖均从 crates.io 与 npmjs.org 拉取。
- **无 vendoring**：未使用 `cargo vendor` 或 `git submodule` 方式内联第三方源码，依赖全部来自远程索引。
- **工具链锁定**：通过 `rust-toolchain.toml` 强制团队使用同一 stable 版本，并预装格式化与检查工具，减少环境差异导致的依赖解析问题。