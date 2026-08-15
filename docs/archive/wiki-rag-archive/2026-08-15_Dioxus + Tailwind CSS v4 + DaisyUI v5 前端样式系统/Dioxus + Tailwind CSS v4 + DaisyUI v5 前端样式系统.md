> 📦 归档标记（2026-08-15）：被 [Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格](docs/wiki/knowledge/zh/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: frontend_style
name: Dioxus + Tailwind CSS v4 + DaisyUI v5 前端样式系统
category: frontend_style
scope:
    - '**'
source_files:
    - frontend/styles/input.css
    - frontend/package.json
    - frontend/Dioxus.toml
    - frontend/build.rs
    - docs/ui_design_system.md
---

## 1. 技术栈与构建方式

- **前端框架**: Dioxus 0.7（Rust → WebAssembly），组件以 `.rs` 文件组织在 `frontend/src/`。
- **CSS 框架**: Tailwind CSS v4.1，通过 `@import "tailwindcss"` 引入；样式入口为 `frontend/styles/input.css`，构建产物输出到 `frontend/public/output.css`。
- **组件库**: DaisyUI v5，通过 `@plugin "daisyui"` 引入，提供按钮、模态框、表格、标签页、加载指示器等现成类名。
- **构建链**: `frontend/build.rs` 在 Rust 编译时自动执行 `npm install` + Tailwind CLI 编译；`package.json` 暴露 `build:css` / `watch:css` 脚本供开发时使用。
- **源码扫描**: `@source "../index.html"` 与 `@source "../src/**/*.rs"` 让 Tailwind 能扫描 Dioxus 组件中的类名。

## 2. 主题与品牌色体系

- 自定义主题 `orz-light` 作为 DaisyUI 默认主题（`--default`），定义在 `[data-theme="orz-light"]` 块中，覆盖 Primary、Secondary、Accent、Base、Success/Warning/Error/Info 等语义变量，全部使用 oklch 色彩空间。
- 同时启用 31 个内置主题（light、dark、cyberpunk、dracula、sunset 等），通过 `<html data-theme="xxx">` 切换。
- 字体族通过 Tailwind v4 的 `@theme` 块声明：sans 使用系统字体栈，mono 使用 SF Mono / Cascadia Code 等。
- 圆角统一通过 `--radius-selector` / `--radius-field` / `--radius-box` 控制，值为 `0.375rem` / `0.375rem` / `0.5rem`。

## 3. 自定义视觉风格（HUD 驾驶舱）

除 DaisyUI 通用组件外，`input.css` 还定义了项目专属的 HUD 风格动画与效果：

- **`.hud-streak`**：左侧 2px 橙色竖条 + 流动高光伪元素，用于 Workspace 未读消息提示，配合 `hud-streak-flow` 关键帧实现从上到下扫过的流光效果。
- **知识图谱 HUD 渲染**：`.kg-bg`（深色径向渐变 + 淡橙色网格背景）、`.kg-node-pulse`（节点呼吸光晕）、`.kg-scan-ring`（选中节点扫描环）、`.kg-ring-spin`（刻度旋转）、`.kg-edge-flow`（边实线流光）、`.kg-corner`（四角刻度装饰）、`.kg-node-group:hover`（hover 放大）等。
- **打字指示器动画**：`typing-bounce` 关键帧用于聊天输入状态。

## 4. 组件样式约定

- 业务页面与通用 UI 优先使用 DaisyUI v5 提供的类名（`btn`、`modal`、`card`、`table`、`badge`、`tabs`、`loading`、`dropdown`、`steps` 等），不再手写 CSS 类。
- 仅在需要领域特定视觉效果（HUD 流光、知识图谱 Canvas 渲染）时才新增自定义 CSS。
- 颜色一律走 DaisyUI 语义变量（`bg-primary`、`text-base-content` 等），避免硬编码色值。
- 布局间距、字号、阴影等基础样式由 Tailwind 工具类组合完成。

## 5. 文档与设计规范

- `docs/ui_design_system.md` 是权威的设计系统文档：前半部分记录受 Mistral AI 启发的原始设计语言（暖橙主色、近零圆角、金色多层阴影、82px 超大标题等），后半部分记录已落地的 Tailwind + DaisyUI 实现规范，包括主题变量映射、31 个主题切换机制、组件类名对照表以及 HUD 驾驶舱风格的详细说明。
- 该文档明确说明“从 `frontend/index.html` 内联样式迁移到 Tailwind CSS v4 + DaisyUI v5”，并保留历史参考章节。

## 6. 约束与规则

- 所有样式必须经 `frontend/styles/input.css` 入口引入，禁止在组件中直接写 `<style>` 标签。
- 新增主题需通过 `[data-theme="xxx"]` 块定义 DaisyUI 变量，并在 `@plugin "daisyui"` 中注册。
- 新增全局动画或 HUD 效果应放在 `input.css` 中，并按现有命名约定（如 `hud-*`、`kg-*`）组织。
- 构建产物 `public/output.css` 由 Tailwind CLI 自动生成，不应手动编辑。