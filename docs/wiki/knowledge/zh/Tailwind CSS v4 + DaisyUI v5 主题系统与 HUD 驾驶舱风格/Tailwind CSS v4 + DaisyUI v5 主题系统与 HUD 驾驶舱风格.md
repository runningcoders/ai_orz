---
kind: frontend_style
name: Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格
category: frontend_style
scope:
    - '**'
source_files:
    - frontend/styles/input.css
    - frontend/styles/input.css#L1249-L1550
    - frontend/package.json
    - frontend/build.rs
    - frontend/Dioxus.toml
    - frontend/src/hooks/mod.rs
    - frontend/src/pages/settings.rs
    - frontend/src/components/hud.rs#L1-L262
    - frontend/src/pages/hr/agent_detail.rs
    - frontend/src/pages/message/chat.rs
    - docs/design/ui_design_system.md
    - docs/design/frontend_architecture.md
    - docs/wiki/zh/content/前端应用/UI 样式与主题.md
---

## 1. 样式体系概览

前端基于 **Dioxus 0.7 (WASM)**，样式采用 **Tailwind CSS v4.1** + **DaisyUI v5** 组件库，通过 `frontend/styles/input.css` 作为唯一入口，构建时由 `frontend/build.rs` 调用 `@tailwindcss/cli` 编译为 `frontend/public/output.css` 并嵌入到 Dioxus 产物中。构建脚本在 Cargo 编译阶段自动执行 `npm install`（若缺失）并运行 `tailwindcss -i styles/input.css -o public/output.css --minify`，同时监听 `styles/input.css`、`package.json`、`package-lock.json` 变化触发重建。

## 2. 核心文件与包

- `frontend/styles/input.css`：Tailwind 入口，声明 DaisyUI 插件、31 个主题、`@theme` 字体变量、自定义 `orz-light` 品牌主题、HUD/知识图谱动画、Markdown 渲染样式。
- `frontend/package.json`：仅依赖 `@tailwindcss/cli ^4.1.0`、`tailwindcss ^4.1.0`、`daisyui ^5.0.0`，提供 `build:css` / `watch:css` 脚本。
- `frontend/build.rs`：在 Rust 编译期调用 Tailwind CLI 生成 CSS；同时复制 `docs/` 文档至 `public/docs/` 并生成 `index.json` 供文档中心页面使用。
- `frontend/Dioxus.toml`：配置输出目录 `dist`、静态资源目录 `public`、WASM 优化级别、watcher 路径等。
- `frontend/src/hooks/mod.rs`：提供 `use_provide_theme()` / `use_theme()` Hook 与 `ThemeController`，管理全局 `Signal<String>` 主题状态，读写 `localStorage.ai_orz_theme` 并设置 `document.documentElement.dataTheme`；定义 `AVAILABLE_THEMES` 30+ 主题列表（含中文名）。
- `frontend/src/pages/settings.rs`：主题选择 UI，遍历 `AVAILABLE_THEMES` 列表渲染按钮，点击后调用 `theme_ctrl.set(theme_id)`；全站 HUD 收口后新增组织级配置区，统一 HudCard + HudPanel 容器风格。
- `frontend/src/components/hud.rs#L1-L262`：HUD 原子组件集合——`HudPanel`（基础发丝边面板）、`HudCard`（带 tone 切换的卡片，替代旧 hud-tone 变体）、`HudSection`（分组容器）、`HudProgress`（薄轨道发光填充进度条）、`HudCallout`（提示条）、`HudDivider`（分割线）、`HudTable`（斑马纹表格）、`HudTabs`（标签页）、`PageHeader`（页面标题）、`StatReadout`/`StatGrid`（等宽数字指标）。全站统一收口，禁止自定义非 HUD 风格卡片/徽章/标签。
- `frontend/styles/input.css#L1249-L1550`：HUD CSS 皮肤块——`.hud-panel`（渐变发丝边 + backdrop-blur）、`.hud-panel.hud-tone-{primary,accent,success,neutral}` 四色变体、`.hud-signal`（流光条 keyframes）、`.hud-eyebrow`（等宽 eyebrow 文字）、`.hud-stat`（等宽数字指标）、`.hud-progress` 系列、`.hud-divider`、`.hud-table`（斑马纹 + 悬停发光）、`.hud-modal` / `.hud-input`、`.badge.hud-badge`（backdrop-blur + glow 光晕，L1507-L1549）、`.hud-glass` / `.hud-collapse-btn`。
- `frontend/src/pages/hr/agent_detail.rs`：Agent 详情页，T5 HUD 收口后工具与技能全景改用 HudCard + HudPanel，指标卡统一 StatReadout。
- `frontend/src/pages/message/chat.rs`：聊天页，气泡 + 消息侧面板 HUD 统一，进度条改用 HudProgress，提示条改用 HudCallout。
- `docs/design/ui_design_system.md`：设计系统权威文档，记录从早期内联样式迁移到 Tailwind+DaisyUI 的完整规范、色彩语义、组件类名约定与 HUD 驾驶舱风格说明；前半部分保留原始设计语言（暖橙主色、近零圆角、金色多层阴影、82px 超大标题等），后半部分记录已落地实现规范。
- `docs/design/frontend_architecture.md`：前端架构总览。

## 3. 架构与设计决策

### 主题系统
- 通过 `@plugin "daisyui" { themes: orz-light --default, ... }` 启用 31 个主题，其中 `orz-light` 为默认主题。
- `orz-light` 在 `[data-theme="orz-light"]` 块中覆盖 DaisyUI 语义色变量（`--color-primary`、`--color-secondary`、`--color-accent`、`--color-base-*`、`--color-success/warning/error/info`），主色调 `oklch(0.63 0.24 50)` ≈ Mistral 品牌橙 `#fa520f`，背景基色 `oklch(0.98 0.02 85)` ≈ Warm Ivory `#fffaeb`，圆角统一为 `--radius-selector: 0.375rem` / `--radius-field: 0.375rem` / `--radius-box: 0.5rem`。
- 通过 `@theme` 注入全局字体族：`--font-family-sans` 使用系统栈（-apple-system、BlinkMacSystemFont、'Segoe UI'、Roboto 等），`--font-family-mono` 使用 SF Mono/Monaco/Cascadia Code/'Roboto Mono' 等。
- **主题切换机制**：通过在 `<html>` 上设置 `data-theme="xxx"` 属性实现；主题值持久化到浏览器 `localStorage` 的 `ai_orz_theme` 键中；切换流程：`settings.rs` → `ThemeController::set()` → 写入 `localStorage.ai_orz_theme` → 调用 `set_html_theme()` 设置 `document.documentElement.setAttribute("data-theme", theme)` → 触发 Dioxus Signal 更新；默认主题从 `localStorage` 读取，不存在则回退到 `orz-light`。

### 扫描机制
- `@source "../index.html"` 与 `@source "../src/**/*.rs"` 让 Tailwind 在构建时扫描 HTML 和 Dioxus `.rs` 源码中的 class 字符串，确保按需生成 CSS。

### HUD 驾驶舱风格
- 自定义动画与类集中在 `input.css`：`.hud-streak`（未读消息流光竖条，配合 `hud-streak-flow` keyframes 实现从上到下扫过的流光效果）、`.kg-bg`（知识图谱深色径向渐变+淡橙色网格背景）、`.kg-node-pulse`（节点呼吸光晕）/`.kg-scan-ring`（选中节点扫描环）/`.kg-ring-spin`（刻度旋转）/`.kg-edge-flow`（边实线流光）/`.kg-edge-glow`（边发光）/`.kg-corner`（四角刻度装饰）/`.kg-node-appear`（节点出现动画）/`.kg-node-group:hover`（hover 放大）等 Canvas/SVG 节点动画。
- **打字指示器动画**：`typing-bounce` 关键帧用于聊天输入状态。
- 这些样式服务于 `components/canvas_scene.rs`、`components/graph_canvas.rs`、`components/relation_graph.rs` 等可视化组件。

### Markdown 渲染样式
- `.markdown-body` 与 `.markdown-compact` 两套样式用于渲染 pulldown-cmark 输出的 HTML，全部引用 DaisyUI 主题变量（`--color-base-content`、`--color-primary`、`--color-base-200` 等），随主题自动适配。

## 4. 组件层约定

- 所有 UI 组件（Button、Modal、ConfirmDialog、Toast、SearchableSelect、CodeEditor、Chart、Graph、CanvasScene、WorkspaceGraph 等）位于 `frontend/src/components/`，按功能分目录（`charts/`、`chat/` 等），通过 DaisyUI 标准类名（`btn`、`modal`、`alert`、`badge`、`card`、`table`、`tabs`、`loading`、`dropdown`、`steps` 等）组合样式，不再手写通用 CSS 类。
- **组件内不使用内联样式或 CSS-in-Rust**，而是通过 Tailwind 原子类 + DaisyUI 语义类组合；布局组件位于 `frontend/src/layouts/`（`app_layout.rs`、`navbar.rs`），负责整体框架结构；页面级组件位于 `frontend/src/pages/`，按业务域划分（`finance/`、`hr/`、`message/`、`organization/`、`project/`、`system/`、`user/`）。
- 业务特定视觉（HUD 流光、知识图谱节点呼吸/扫描环、边流动）以独立 CSS 类形式集中管理，避免散落在组件内联样式中。
- 响应式策略依赖 Tailwind 内置断点（sm/md/lg）与 DaisyUI 组件自带响应行为，**未在 CSS 中定义额外媒体查询**。

## 5. 约束与规则

1. **单一入口**：所有样式必须通过 `frontend/styles/input.css` 引入，禁止在其他位置新增全局 CSS 或在组件中直接写 `<style>` 标签。
2. **主题优先**：颜色、圆角、阴影等视觉属性应通过 DaisyUI 语义变量或 `@theme` 变量表达，禁止硬编码十六进制颜色值（HUD/KG 专用动画除外）。
3. **组件类名**：优先使用 DaisyUI v5 官方类名（`btn-primary`、`modal-box`、`alert-error` 等），自定义扩展仅限 HUD/KG/Markdown 等无法用 DaisyUI 表达的领域。
4. **构建集成**：CSS 编译是 Rust 编译流程的一部分，修改 `styles/input.css` 会触发 `cargo build` 重新执行 Tailwind 编译；新增 `.rs` 组件 class 需保证能被 `@source "../src/**/*.rs"` 扫描到；构建阶段 `build.rs::compile_tailwind()` 检测 `node_modules/.bin/tailwindcss`，若不存在则先执行 `npm install`；开发模式可通过 `watch:css` 脚本独立监听。
5. **主题切换**：通过 `<html data-theme="...">` 切换，新增主题需在 `@plugin "daisyui" { themes: ... }` 列表中注册，并在 `[data-theme="xxx"]` 块中定义必要变量，**同时必须在 `hooks/mod.rs` 的 `AVAILABLE_THEMES` 中添加对应条目**，否则无法被主题选择器使用；运行时主题切换只能通过 `ThemeController` 修改 `data-theme` 属性，禁止直接操作 DOM 的 style 属性。
6. **文档同步**：`docs/design/ui_design_system.md` 是样式规范的权威来源，新增主题或组件类名约定需同步更新该文档。
7. **构建产物不可提交**：`public/output.css` 由 Tailwind CLI 自动生成，不应手动编辑，不应纳入版本控制（应忽略）。
8. **新增全局动画应放在 `input.css` 中**，并按现有命名约定（如 `hud-*`、`kg-*`）组织，避免在 Rust 组件中嵌入 CSS 片段。
9. **Markdown 内容样式隔离**：用户/后端生成的 Markdown 仅通过 `.markdown-body` 或 `.markdown-compact` 容器应用样式，避免污染全局 DOM。
10. **文档中心构建集成**：`build.rs::copy_docs()` 递归扫描 `../docs/design|plan|archive|wiki/zh/content` 下的 `.md`，复制到 `public/docs/` 并生成 `index.json` 供前端文档页面动态加载。
11. **全站 HUD 原子组件收口红线**：全站所有卡片/徽章/标签/提示条/进度条/分割线/表格/标签页 **必须** 使用 `components/hud.rs` 原语（`HudPanel`/`HudCard`/`HudCallout`/`HudProgress`/`HudDivider`/`HudTable`/`HudTabs`）或 `input.css` 的 `.hud-*` CSS 变体（`.badge.hud-badge`/`.hud-modal`/`.hud-input`）。**禁止** 自定义非 HUD 风格的卡片容器、徽章或标签页。
12. **HudBadge 玻璃光晕约束**：所有徽章类视觉 **必须** 使用 `.badge.hud-badge` 皮肤（带 `backdrop-blur` + `glow` 光晕，见 `frontend/styles/input.css#L1507-L1549`），保持 HUD 驾驶舱视觉一致性。禁用旧版无光晕裸 `badge`。
13. **hud-tone 旧变体移除红线**：前端页面已废弃 hud-tone 独立变体，统一收口为 `HudCard { tone: Some("primary"|"accent"|"success"|"neutral") }`。**禁止** 新增 `.hud-tone-*` 类名直写或自定义 tone 变体。