---
kind: frontend_style
name: Dioxus + Tailwind CSS v4 + DaisyUI v5 前端样式体系
category: frontend_style
scope:
    - '**'
source_files:
    - frontend/styles/input.css
    - frontend/package.json
    - frontend/build.rs
    - frontend/Dioxus.toml
    - frontend/src/hooks/mod.rs
    - frontend/src/pages/settings.rs
---

## 1. 系统概览

前端基于 **Dioxus 0.7（WASM）** 构建，位于 `frontend/` 子目录；样式体系采用 **Tailwind CSS v4** 配合 **DaisyUI v5** 组件库，通过 Cargo build script 在编译期自动调用 tailwindcss CLI 将 `styles/input.css` 编译为 `public/output.css`。该方案与作者指南一致："前端为 Dioxus 0.7 WASM 应用……Tailwind CSS v4 + DaisyUI v5"。

## 2. 关键文件与包

- `frontend/package.json`：声明依赖 `tailwindcss ^4.1.0`、`@tailwindcss/cli ^4.1.0`、`daisyui ^5.0.0`，并提供 `build:css` / `watch:css` 脚本。
- `frontend/styles/input.css`：唯一样式入口，使用 `@import "tailwindcss"`、`@plugin "daisyui" { themes: ... }` 引入 30+ 主题，并通过 `@theme` 定义全局字体变量。
- `frontend/build.rs`：Cargo 构建脚本，在编译时执行 `compile_tailwind()` 调用 node_modules 中的 tailwindcss 二进制生成 `public/output.css`，同时复制 `docs/` 文档到 `public/docs/` 并生成 `index.json`。
- `frontend/Dioxus.toml`：配置输出目录 `dist`、静态资源目录 `public`、WASM 优化级别、watcher 路径等。
- `frontend/src/hooks/mod.rs` 与 `frontend/src/pages/settings.rs`：运行时通过设置根元素 `data-theme` 属性切换 DaisyUI 主题。

## 3. 架构与约定

### 3.1 样式管线
```
styles/input.css → (build.rs 调用 tailwindcss -i -o --minify) → public/output.css → Dioxus 渲染时由 index.html 引用
```
- 构建阶段：`build.rs::compile_tailwind()` 检测 `node_modules/.bin/tailwindcss`，若不存在则先执行 `npm install`，再运行 `tailwindcss -i styles/input.css -o public/output.css --minify`。
- 开发阶段：`package.json` 的 `watch:css` 脚本可独立监听 `input.css` 变化增量编译。
- 文档中心：`build.rs::copy_docs()` 递归扫描 `../docs/design|plan|archive|wiki/zh/content` 下的 `.md`，复制到 `public/docs/` 并生成 `index.json` 供前端文档页面动态加载。

### 3.2 主题系统
- 在 `input.css` 中通过 `@plugin "daisyui" { themes: orz-light --default, light, dark, cupcake, ... }` 启用 30+ 内置主题，其中 `orz-light` 被标记为默认主题。
- 自定义主题色覆盖：`[data-theme="orz-light"]` 块中用 oklch 值重定义 primary/secondary/accent/neutral/base-* 等 DaisyUI 语义变量，形成项目专属品牌色。
- 运行时切换：`hooks/mod.rs` 中通过 `html.set_attribute("data-theme", theme)` 修改根节点属性；`pages/settings.rs` 提供 UI 让用户选择主题 ID。

### 3.3 设计令牌（Design Tokens）
- 字体令牌：`@theme { --font-family-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif; --font-family-mono: 'SF Mono', Monaco, 'Cascadia Code', 'Roboto Mono', monospace; }`，供 Markdown 代码块等引用。
- 圆角令牌：在 `orz-light` 主题中显式设置 `--radius-selector` / `--radius-field` / `--radius-box` 统一圆角。
- 颜色令牌：全部使用 oklch 色彩空间，保证感知均匀性。

### 3.4 业务风格扩展
- HUD 风格：`.hud-streak` 实现未读消息提示的流光竖条动画（`hud-streak-flow` keyframes），用于驾驶舱式通知。
- 知识图谱风格：`.kg-bg` 背景网格 + 径向光晕；`.kg-node-pulse` / `.kg-scan-ring` / `.kg-ring-spin` / `.kg-node-appear` / `.kg-edge-flow` / `.kg-edge-glow` / `.kg-corner` 等类构成完整的 HUD 风格可视化效果。
- Markdown 渲染：`.markdown-body` 和 `.markdown-compact` 两套样式，全部引用 DaisyUI 主题变量（`--color-base-content` / `--color-primary` / `--color-base-200` 等），确保随主题自动适配。

### 3.5 组件层样式组织
- 所有 Dioxus 组件位于 `frontend/src/components/`，按功能分目录（`charts/`、`chat/` 等）。
- 组件内不使用内联样式或 CSS-in-Rust，而是通过 Tailwind 原子类 + DaisyUI 语义类组合样式（如按钮、模态框、图表容器等）。
- 布局组件位于 `frontend/src/layouts/`（`app_layout.rs`、`navbar.rs`），负责整体框架结构。
- 页面级组件位于 `frontend/src/pages/`，按业务域划分（`finance/`、`hr/`、`message/`、`organization/`、`project/`、`system/`、`user/`）。

## 4. 约定与约束

- **单一入口**：所有样式必须写在 `styles/input.css`，禁止在其他位置新增 CSS 文件。
- **主题优先**：颜色、圆角、阴影等视觉属性应优先使用 DaisyUI 语义变量（`--color-*`、`--radius-*`），而非硬编码十六进制值。
- **源码扫描**：`@source "../index.html"` 与 `@source "../src/**/*.rs"` 使 Tailwind 能扫描 Dioxus 模板中的 class 字符串，无需手动维护白名单。
- **构建集成**：CSS 编译是 Cargo 构建的一部分，任何 `styles/input.css` 变更都会触发重建；生产构建自动 minify。
- **主题切换机制**：通过设置根元素 `data-theme` 属性切换，新增主题需在 `input.css` 的 `@plugin "daisyui" { themes: ... }` 列表中注册。
- **Markdown 样式规范**：用户生成的 Markdown 内容统一通过 `.markdown-body` 或 `.markdown-compact` 包裹，禁止直接渲染原始 HTML 样式。
- **HUD/知识图谱风格**：复杂动画（keyframes）集中在 `input.css` 中定义，组件仅通过 class 引用，避免在 Rust 组件中嵌入 CSS 片段。