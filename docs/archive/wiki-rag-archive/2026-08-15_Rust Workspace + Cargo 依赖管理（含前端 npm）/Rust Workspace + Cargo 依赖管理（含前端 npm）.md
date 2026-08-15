> 📦 归档标记（2026-08-15）：被 [Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）](docs/wiki/knowledge/zh/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: dependency_management
name: Rust Workspace + Cargo 依赖管理（含前端 npm）
category: dependency_management
scope:
    - '**'
source_files:
    - Cargo.toml
    - Cargo.lock
    - common/Cargo.toml
    - frontend/Cargo.toml
    - ai-orz-macros/Cargo.toml
    - frontend/package.json
    - .sqlx/query-02df1eaa9119585516a2987082896138570199d5edb8133fd8d212f462b02808.json
---

## 1. 使用的系统与工具

- **Cargo workspace**：根 `Cargo.toml` 通过 `[workspace] members = [".", "frontend", "common", "ai-orz-macros"]` 将后端主 crate、Dioxus WASM 前端、共享 common crate 与过程宏 crate 聚合为单一工作区，统一解析依赖。
- **Cargo.lock**：仓库根目录提交 `Cargo.lock`（由 Cargo 自动生成），锁定所有 crate 的精确版本与 checksum，保证构建可复现。
- **npm（仅前端）**：`frontend/package.json` 使用 Tailwind CSS / DaisyUI 等样式依赖，通过 `package-lock.json` 锁定；脚本仅用于编译 CSS，不参与 Rust 构建。
- **SQLx 查询快照**：`.sqlx/` 目录下存放按 SQL 哈希命名的 JSON 快照文件，是 SQLx 在编译期校验 SQL 的产物，随源码一起提交。

## 2. 关键文件与位置

| 文件 | 作用 |
|---|---|
| `Cargo.toml`（根） | workspace 定义、后端主 crate 依赖声明、`[[test]]` 集成测试入口 |
| `common/Cargo.toml` | 共享 crate，通过 optional features (`sqlx`, `axum-integration`, `reqwest-integration`, `tokio-integration` 等) 裁剪可选能力 |
| `frontend/Cargo.toml` | Dioxus WASM 前端依赖，build-dependencies 复用 `common` |
| `ai-orz-macros/Cargo.toml` | 过程宏 crate，仅依赖 `proc-macro2`/`quote`/`syn`/`schemars`/`glob` |
| `Cargo.lock` | 全工作区锁文件，锁定 crates.io 源上每个 crate 的版本与 checksum |
| `.sqlx/*.json` | SQLx 编译期生成的查询校验快照 |
| `patches/rig-fastembed/src/` | 本地补丁目录（当前为空），预留对第三方 crate 的 patch |
| `frontend/package.json` | 前端 npm 依赖（Tailwind/DaisyUI） |

## 3. 架构与约定

### 3.1 工作区划分
- **后端主 crate**（根）：Axum HTTP 服务，集中声明运行时依赖（tokio、axum、reqwest、sqlx、fastembed、lancedb、duckdb 等）。
- **common crate**：前后端共享的 DTO、枚举、错误模型与配置；通过 `features` 暴露 `sqlx`、`axum-integration`、`jwt-integration`、`reqwest-integration`、`toml-integration`、`bincode-integration`、`tokio-integration` 等可选能力，避免引入不必要的运行时开销。
- **ai-orz-macros**：纯过程宏 crate，不引入业务运行时依赖。
- **frontend**：Dioxus + wasm-bindgen 前端，通过 `path = "../common"` 引用共享 crate，并使用独立的 `web-sys`/`js-sys`/`gloo-*` 系列依赖适配浏览器环境。

### 3.2 版本策略
- 大多数依赖使用语义化版本范围（如 `tokio = "1"`、`axum = "0.8"`、`serde = "1"`），允许小版本升级。
- 部分关键库采用较精确的版本号（如 `sqlx = "0.8.6"`、`uuid = "1.23.0"`、`arrow-array = "57.3.0"`），以控制 ABI 稳定性。
- 向量栈（`fastembed`、`lancedb`、`instant-distance`、`arrow-*`）显式禁用默认 feature 并只启用所需子功能，注释说明是为了规避系统依赖或已知 bug。

### 3.3 依赖来源与私有源
- 所有 Rust 依赖均来自 crates.io 官方索引（`source = registry+https://github.com/rust-lang/crates.io-index`），未配置自定义 registry 或 `CARGO_HOME` 镜像。
- 未检出 `.cargo/config.toml` 或 `CARGO_NET_*` 环境变量配置，因此不存在私有 crate registry 或代理设置。

### 3.4 补丁机制
- 根目录存在 `patches/rig-fastembed/src/` 空目录，表明项目曾计划或曾经使用过 `cargo:patch` 机制对 `rig-fastembed` 进行本地修改；当前该目录为空，实际依赖仍走 crates.io。

### 3.5 前端依赖
- 仅包含样式构建工具（`@tailwindcss/cli`、`tailwindcss`、`daisyui`），无 JS 运行时依赖。
- 通过 `scripts.build:css` 和 `scripts.watch:css` 调用 Tailwind CLI 生成 `public/output.css`。

## 4. 约定与约束

- **workspace 内 crate 一律通过 `path = "..."` 引用**：`common`、`ai-orz-macros` 在多个 crate 中以 path dependency 形式引用，确保工作区内版本一致。
- **可选能力通过 feature flags 暴露**：`common` crate 用 optional dependencies + `[features]` 组合出不同集成面，调用方按需启用（如 `features = ["sqlx", "axum-integration", ...]`）。
- **Cargo.lock 纳入版本控制**：锁文件随仓库提交，CI 与本地构建必须基于同一份锁定结果。
- **SQLx 查询快照随源码提交**：`.sqlx/` 下的 JSON 快照文件与迁移脚本配合，确保 SQL 变更在编译期被校验。
- **无 vendoring**：未使用 `cargo vendor` 或 Git submodule 方式拉取第三方源码；所有外部 crate 从 crates.io 下载。
- **无全局 .cargo 配置**：未发现 `.cargo/config.toml`、`config.toml` 或 `CARGO_REGISTRIES_*` 环境变量，依赖来源固定为 crates.io。