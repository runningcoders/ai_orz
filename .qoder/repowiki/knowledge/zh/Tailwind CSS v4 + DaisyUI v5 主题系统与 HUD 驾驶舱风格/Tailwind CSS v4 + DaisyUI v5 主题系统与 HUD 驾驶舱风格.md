---
kind: frontend_style
name: Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格
category: frontend_style
scope:
    - '**'
source_files:
    - frontend/styles/input.css
    - frontend/package.json
    - frontend/build.rs
    - frontend/Dioxus.toml
    - docs/design/ui_design_system.md
---

## 1. 样式体系概览

前端基于 **Dioxus 0.7 (WASM)**，样式采用 **Tailwind CSS v4.1** + **DaisyUI v5** 组件库，通过 `frontend/styles/input.css` 作为唯一入口，构建时由 `frontend/build.rs` 调用 `@tailwindcss/cli` 编译为 `frontend/public/output.css` 并嵌入到 Dioxus 产物中。构建脚本在 Cargo 编译阶段自动执行 `npm install`（若缺失）并运行 `tailwindcss -i styles/input.css -o public/output.css --minify`，同时监听 `styles/input.css`、`package.json`、`package-lock.json` 变化触发重建。

## 2. 核心文件与包

- `frontend/styles/input.css`：Tailwind 入口，声明 DaisyUI 插件、31 个主题、`@theme` 字体变量、自定义 `orz-light` 品牌主题、HUD/知识图谱动画、Markdown 渲染样式。
- `frontend/package.json`：仅依赖 `@tailwindcss/cli ^4.1.0`、`tailwindcss ^4.1.0`、`daisyui ^5.0.0`，提供 `build:css` / `watch:css` 脚本。
- `frontend/build.rs`：在 Rust 编译期调用 Tailwind CLI 生成 CSS；同时复制 `docs/` 文档至 `public/docs/` 并生成 `index.json` 供文档中心页面使用。
- `frontend/Dioxus.toml`：配置输出目录 `dist`、静态资源目录 `public`、WASM 优化级别、watcher 路径等。
- `docs/design/ui_design_system.md`：设计系统权威文档，记录从早期内联样式迁移到 Tailwind+DaisyUI 的完整规范、色彩语义、组件类名约定与 HUD 驾驶舱风格说明。

## 3. 架构与设计决策

### 主题系统
- 通过 `@plugin "daisyui" { themes: orz-light --default, ... }` 启用 31 个主题，其中 `orz-light` 为默认主题。
- `orz-light` 在 `[data-theme="orz-light"]` 块中覆盖 DaisyUI 语义色变量（`--color-primary`、`--color-secondary`、`--color-accent`、`--color-base-*`、`--color-success/warning/error/info`），主色调 `oklch(0.63 0.24 50)` ≈ Mistral 品牌橙 `#fa520f`，背景基色 `oklch(0.98 0.02 85)` ≈ Warm Ivory `#fffaeb`，圆角统一为 `0.375rem`/`0.5rem`。
- 通过 `@theme` 注入全局字体族：`--font-family-sans` 使用系统栈，`--font-family-mono` 使用 SF Mono/Monaco/Cascadia Code 等。
- 主题切换通过在 `<html>` 上设置 `data-theme="xxx"` 实现，无需 JS 逻辑。

### 扫描机制
- `@source "../index.html"` 与 `@source "../src/**/*.rs"` 让 Tailwind 在构建时扫描 HTML 和 Dioxus `.rs` 源码中的 class 字符串，确保按需生成 CSS。

### HUD 驾驶舱风格
- 自定义动画与类集中在 `input.css`：`.hud-streak`（未读消息流光竖条）、`.kg-bg`（知识图谱深色网格背景）、`.kg-node-pulse`/`.kg-scan-ring`/`.kg-ring-spin`/`.kg-edge-flow` 等 Canvas/SVG 节点动画。
- 这些样式服务于 `components/canvas_scene.rs`、`components/graph_canvas.rs`、`components/relation_graph.rs` 等可视化组件。

### Markdown 渲染样式
- `.markdown-body` 与 `.markdown-compact` 两套样式用于渲染 pulldown-cmark 输出的 HTML，全部引用 DaisyUI 主题变量（`--color-base-content`、`--color-primary`、`--color-base-200` 等），随主题自动适配。

## 4. 组件层约定

- 所有 UI 组件（Button、Modal、ConfirmDialog、Toast、SearchableSelect、CodeEditor、Chart、Graph、CanvasScene、WorkspaceGraph 等）位于 `frontend/src/components/`，通过 DaisyUI 标准类名（`btn`、`modal`、`alert`、`badge`、`card`、`table`、`tabs`、`loading`、`dropdown`、`steps` 等）组合样式，不再手写通用 CSS 类。
- 业务特定视觉（HUD 流光、知识图谱节点呼吸/扫描环、边流动）以独立 CSS 类形式集中管理，避免散落在组件内联样式中。
- 响应式策略依赖 Tailwind 内置断点（sm/md/lg）与 DaisyUI 组件自带响应行为，未在 CSS 中定义额外媒体查询。

## 5. 约束与规则

- **单一入口**：所有样式必须通过 `frontend/styles/input.css` 引入，禁止在其他位置新增全局 CSS。
- **主题优先**：颜色、圆角、阴影等视觉属性应通过 DaisyUI 语义变量或 `@theme` 变量表达，禁止硬编码十六进制颜色值（HUD/KG 专用动画除外）。
- **组件类名**：优先使用 DaisyUI v5 官方类名（`btn-primary`、`modal-box`、`alert-error` 等），自定义扩展仅限 HUD/KG/Markdown 等无法用 DaisyUI 表达的领域。
- **构建集成**：CSS 编译是 Rust 编译流程的一部分，修改 `styles/input.css` 会触发 `cargo build` 重新执行 Tailwind 编译；新增 `.rs` 组件 class 需保证能被 `@source "../src/**/*.rs"` 扫描到。
- **主题切换**：通过 `<html data-theme="...">` 切换，新增主题需在 `@plugin "daisyui" { themes: ... }` 列表中注册，并在 `[data-theme="xxx"]` 块中定义必要变量。
- **文档同步**：`docs/design/ui_design_system.md` 是样式规范的权威来源，新增主题或组件类名约定需同步更新该文档。