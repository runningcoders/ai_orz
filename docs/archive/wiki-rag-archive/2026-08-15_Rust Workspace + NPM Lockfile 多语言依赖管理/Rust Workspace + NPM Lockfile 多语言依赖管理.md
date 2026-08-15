> 📦 归档标记（2026-08-15）：被 [Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）](docs/wiki/knowledge/zh/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）/Rust Workspace + NPM 多语言依赖管理（Cargo.lock _ package-lock.json _ SQLx 离线构建）.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: dependency_management
name: Rust Workspace + NPM Lockfile 多语言依赖管理
category: dependency_management
scope:
    - '**'
source_files:
    - Cargo.toml
    - Cargo.lock
    - common/Cargo.toml
    - ai-orz-macros/Cargo.toml
    - frontend/Cargo.toml
    - frontend/package.json
    - frontend/package-lock.json
    - e2e/package.json
    - e2e/package-lock.json
    - rust-toolchain.toml
---

## 1. 使用的系统与工具

仓库采用**多语言、多工作区**的依赖管理模式：

- **Rust（后端 + WASM 前端）**：使用 Cargo workspace，根 `Cargo.toml` 通过 `[workspace] members = [".", "frontend", "common", "ai-orz-macros"]` 将四个 crate 纳入统一解析，resolver 版本为 `2`。所有 Rust 依赖声明在各自 crate 的 `Cargo.toml` 中，由根 `Cargo.lock` 锁定全部成员包的精确版本。
- **Node.js（前端构建与 E2E）**：`frontend/package.json` 仅声明 Tailwind CSS v4 + DaisyUI v5 作为 devDependencies；`e2e/package.json` 声明 Playwright 测试依赖。两个目录均附带 `package-lock.json`（lockfileVersion 3）以锁定 npm 包树。
- **Rust toolchain**：`rust-toolchain.toml` 固定 channel 为 `stable`，并预注册 `rustfmt`、`clippy` 组件以及 `wasm32-unknown-unknown` target，确保前后端编译环境一致。

## 2. 关键文件

| 文件 | 作用 |
|---|---|
| `Cargo.toml`（根） | workspace 定义、主后端 `ai_orz` 的依赖声明、`[[test]]` 集成测试入口 |
| `Cargo.lock` | 全 workspace 的精确版本锁定 |
| `common/Cargo.toml` | 前后端共享 crate，通过 optional feature (`sqlx`, `axum-integration`, `reqwest-integration`, `tokio-integration`, `bincode-integration`, `toml-integration`) 按需裁剪依赖 |
| `ai-orz-macros/Cargo.toml` | proc-macro crate，仅依赖 `syn/quote/proc-macro2/schemars/glob/serde_json` |
| `frontend/Cargo.toml` | Dioxus 0.7 WASM 前端依赖，build-dependencies 复用 `common` |
| `frontend/package.json` | Tailwind/DaisyUI 构建脚本 |
| `frontend/package-lock.json` | 锁定前端构建依赖 |
| `e2e/package.json` + `e2e/package-lock.json` | Playwright E2E 测试依赖 |
| `rust-toolchain.toml` | 固定 Rust toolchain 与 wasm target |

## 3. 架构与约定

- **Workspace 内 crate 通过 `path = "..."` 引用**：`ai_orz`、`frontend`、`common`、`ai-orz-macros` 之间不发布到 crates.io，而是本地 path 依赖，保证同一仓库内的 API 变更可原子提交。
- **可选 feature 裁剪依赖**：`common` crate 把 `sqlx`、`axum`、`reqwest`、`tokio`、`toml`、`jsonwebtoken`、`base64` 等重依赖标记为 `optional`，并通过 `features = [...]` 暴露给消费者按需启用。例如根后端启用 `sqlx, axum-integration, bincode-integration, toml-integration, reqwest-integration, tokio-integration`，而 `frontend` 的 build-dependencies 只引入 `common` 的基础类型而不带这些 feature。
- **目标平台条件依赖**：根 `Cargo.toml` 使用 `[target.'cfg(unix)'.dependencies]` 仅在 Unix 下引入 `libc`，避免跨平台编译污染。
- **向量搜索依赖刻意精简**：注释明确说明禁用 `fastembed` 的 `image-models` 默认特性以避免引入 `exr → pulp 0.22.3` 的 x86 const eval bug，体现对第三方依赖副作用的显式控制。
- **Dioxus WASM 前端**：`frontend/Cargo.toml` 启用 `dioxus` 的 `web, router` features，并通过 `web-sys` 白名单方式引入浏览器 API（Window、Storage、EventSource、MessageEvent、Request、Blob、Canvas 等），符合 WASM 最小化原则。
- **NPM 依赖范围极小**：前端仅用 Tailwind CLI 做 CSS 构建，业务逻辑完全由 Rust/WASM 承担；E2E 单独隔离在 `e2e/` 子目录，避免污染主构建。

## 4. 约定与约束

- **版本锁定**：Rust 侧通过根 `Cargo.lock` 锁定全部 workspace 成员的精确版本；npm 侧通过 `package-lock.json` 锁定 `frontend` 和 `e2e` 的依赖树。二者均提交至版本库，保证 CI 可复现。
- **Toolchain 固定**：`rust-toolchain.toml` 强制使用 `stable` 工具链并包含 `rustfmt`、`clippy`、`wasm32-unknown-unknown` target，禁止开发者自行切换 toolchain。
- **无私有 registry / vendor**：未配置 `.cargo/config.toml` 中的 `source.crates-io` 替换或 `CRATES_IO` 镜像；未使用 `cargo vendor` 或 Git submodule 方式 vendoring 第三方 crate。所有 Rust 依赖直接来自 crates.io-index。
- **feature-gated 共享 crate**：新增跨后端/前端的依赖应优先放入 `common` crate 并以 optional feature 暴露，而不是在每个 crate 重复声明，以保持依赖收敛。
- **CI 一致性**：`.github/workflows/rust.yml` 与 `build.sh` 基于 workspace 顶层执行 `cargo build/test`，依赖锁定文件是构建成功的前提。
- **无 Go 依赖**：仓库不存在 `go.mod`/`go.sum`，因此不涉及 Go 生态的依赖管理策略。
