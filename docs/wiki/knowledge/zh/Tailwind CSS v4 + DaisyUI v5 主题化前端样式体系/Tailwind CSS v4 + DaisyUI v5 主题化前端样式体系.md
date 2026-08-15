---
kind: frontend_style
name: Tailwind CSS v4 + DaisyUI v5 主题化前端样式体系
category: frontend_style
scope:
    - '**'
source_files:
    - frontend/styles/input.css
    - frontend/build.rs
    - frontend/package.json
    - frontend/src/hooks/mod.rs
    - frontend/src/pages/settings.rs
---

## 1. 系统与方法

前端基于 **Dioxus 0.7（WASM）** 构建，样式层采用 **Tailwind CSS v4** 配合 **DaisyUI v5** 组件库。所有样式通过 `frontend/styles/input.css` 作为唯一入口，由 Cargo build script (`frontend/build.rs`) 在编译期调用 `tailwindcss -i ./styles/input.css -o ./public/output.css --minify` 生成压缩后的 `public/output.css`，再由 Dioxus 应用加载。

- Tailwind 配置完全以 CSS 原生方式声明：使用 `@import "tailwindcss"`、`@plugin "daisyui" { themes: ... }`、`@theme { --font-family-sans, --font-family-mono }` 以及 `[data-theme="orz-light"]` 自定义主题变量，无需 `tailwind.config.js`。
- DaisyUI 启用 30+ 内置主题（light/dark/cupcake/emerald/.../sunset），并新增项目专属主题 `orz-light` 作为默认主题。
- 运行时主题切换通过给 `<html>` 元素设置 `data-theme` 属性实现，主题值持久化到浏览器 `localStorage` 的 `ai_orz_theme` 键中。

## 2. 关键文件

| 文件 | 作用 |
|---|---|
| `frontend/styles/input.css` | 唯一样式入口：引入 Tailwind/DaisyUI、声明主题、字体 token、HUD/知识图谱动画、Markdown 渲染样式 |
| `frontend/build.rs` | 编译期执行 `tailwindcss` CLI，将 `input.css` 编译为 `public/output.css`；同时复制文档资源 |
| `frontend/package.json` | 定义 `build:css` / `watch:css` 脚本及依赖 `tailwindcss ^4.1.0`、`daisyui ^5.0.0`、`@tailwindcss/cli ^4.1.0` |
| `frontend/src/hooks/mod.rs` | 提供 `use_provide_theme()` / `use_theme()` Hook，管理全局 `Signal<String>` 主题状态，读写 `localStorage` 并设置 `document.documentElement.dataTheme` |
| `frontend/src/pages/settings.rs` | 主题选择 UI，遍历 `AVAILABLE_THEMES` 列表渲染按钮，点击后调用 `theme_ctrl.set(theme_id)` |
| `frontend/public/output.css` | 编译产物（由 build.rs 生成） |

## 3. 架构与约定

### 主题系统
- 主题常量集中定义在 `hooks/mod.rs` 的 `AVAILABLE_THEMES` 数组中，包含 30+ DaisyUI 内置主题名与中文显示名。
- 默认主题为 `orz-light`，通过 `get_saved_theme()` 从 `localStorage` 读取，不存在则回退到该值。
- 主题切换流程：`settings.rs` → `ThemeController::set()` → 写入 `localStorage.ai_orz_theme` → 调用 `set_html_theme()` 设置 `document.documentElement.setAttribute("data-theme", theme)` → 触发 Dioxus Signal 更新。
- 自定义主题 `orz-light` 在 `input.css` 中以 `[data-theme="orz-light"]` 块覆盖 DaisyUI 的 `--color-primary/secondary/accent/neutral/base-*` 等 CSS 变量，使用 oklch 色值统一色调。

### 设计 Token 与字体
- 通过 Tailwind v4 的 `@theme` 块定义全局字体 token：`--font-family-sans`（系统字体栈）和 `--font-family-mono`（SF Mono/Cascadia Code 等）。
- 圆角 token 被覆盖为 `--radius-selector: 0.375rem`、`--radius-field: 0.375rem`、`--radius-box: 0.5rem`，统一卡片/输入框/选择器的圆角风格。

### 业务专用样式模块
- **HUD 流光条** (`.hud-streak`)：用于未读消息提示，左侧 2px 竖条 + 流动高光动画，颜色引用 `--color-primary`。
- **知识图谱 HUD 风格** (`.kg-bg` / `.kg-node-pulse` / `.kg-edge-flow` / `.kg-corner` 等)：为 Canvas/SVG 知识图谱提供网格背景、节点呼吸光晕、边流动动画、四角刻度装饰等视觉增强。
- **Markdown 渲染样式** (`.markdown-body` / `.markdown-compact`)：为 pulldown-cmark 输出的 HTML 提供排版，所有颜色引用 DaisyUI 主题变量，随主题自动适配；紧凑变体用于聊天气泡/卡片内嵌场景。

### 构建集成
- `build.rs` 在每次 Rust 编译时检查 `styles/input.css`、`package.json`、`package-lock.json` 变更，若 `node_modules/.bin/tailwindcss` 不存在则自动执行 `npm install`，再调用 tailwindcss 编译并输出到 `public/output.css`。
- 开发模式可通过 `pnpm run watch:css` 或 `pnpm run build:css` 手动触发；生产构建由 Cargo 驱动。

### 组件样式约定
- 组件 JSX 中直接使用 Tailwind 原子类 + DaisyUI 语义类组合（如 `card bg-base-100 shadow-md`、`btn btn-sm btn-primary`、`input input-bordered w-full`、`form-control`、`label label-text`、`divider` 等），不编写额外 CSS 类。
- 业务复杂交互（图表、画布、粒子、HUD）通过独立 CSS 模块（集中在 `input.css`）提供，避免散落的样式片段。

## 4. 约束与规则

- **单一入口原则**：所有样式必须通过 `frontend/styles/input.css` 引入，禁止在组件文件中直接 `<style>` 标签注入 CSS。
- **主题变量优先**：颜色、圆角、边框宽度等视觉属性必须使用 DaisyUI 主题变量（如 `var(--color-primary)`、`bg-base-100`），禁止硬编码十六进制颜色值（HUD/知识图谱等少数动效除外）。
- **主题切换机制固定**：运行时主题切换只能通过 `ThemeController` 修改 `data-theme` 属性，禁止直接操作 DOM 的 style 属性。
- **构建产物不可提交**：`public/output.css` 由 `build.rs` 自动生成，不应纳入版本控制（应忽略）。
- **DaisyUI 主题白名单**：新增主题必须在 `input.css` 的 `@plugin "daisyui" { themes: ... }` 列表中注册，并在 `hooks/mod.rs` 的 `AVAILABLE_THEMES` 中添加对应条目，否则无法被主题选择器使用。
- **响应式策略**：依赖 Tailwind v4 的响应式断点与 DaisyUI 组件内置响应式行为，不使用自定义媒体查询。
- **Markdown 内容样式隔离**：用户/后端生成的 Markdown 仅通过 `.markdown-body` 容器应用样式，避免污染全局 DOM。

## 5. 相关设计文档

- `docs/design/ui_design_system.md`：UI 设计系统设计文档。
- `docs/superpowers/plans/2026-07-22-tailwind-daisyui-migration.md`：Tailwind + DaisyUI 迁移计划。
- `docs/design/frontend_architecture.md`：前端架构总览。
- `docs/wiki/zh/content/前端应用/UI 样式与主题.md`：前端 Wiki 中的样式与主题说明。