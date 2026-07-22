# Tailwind CSS + DaisyUI 集成实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将前端从手写 CSS 迁移到 Tailwind CSS v4 + DaisyUI v5 组件库，建立可换肤的主题系统，同时保持现有功能和视觉风格。

**Architecture:** 
1. 采用渐进式迁移策略：先搭建构建工具链（Tailwind + DaisyUI），与现有手写 CSS 共存；再逐模块迁移组件和页面；最后清理旧 CSS。
2. 使用 Trunk 的 pre-build hook 调用 Tailwind CLI 编译 CSS，无需修改 Rust 构建链。
3. 自定义 DaisyUI 主题以保留现有 Mistral 暖橙品牌色。
4. 保留 Rust 组件包装层（Button、Modal 等），内部改用 DaisyUI 类名，对外 API 不变，减少页面改动量。

**Tech Stack:** Tailwind CSS v4、DaisyUI v5、npm（仅前端构建时依赖）、Trunk hooks

---

## 文件结构变更概览

**新增文件：**
- `frontend/package.json` - npm 依赖声明（tailwindcss、daisyui、@tailwindcss/cli）
- `frontend/tailwind.config.js` - Tailwind 配置（内容路径、DaisyUI 插件、自定义主题）
- `frontend/styles/input.css` - Tailwind 入口文件（@import "tailwindcss"; @plugin "daisyui"; 自定义主题）
- `frontend/styles/output.css` - Tailwind 编译输出（gitignore，由构建时生成）
- `Trunk.toml` - Trunk 构建配置（pre-build hook 运行 tailwindcss）

**修改文件：**
- `frontend/index.html` - 移除手写 `<style>` 标签，改为引入 output.css；保留少量不可替代的自定义 CSS（chat 复杂布局、typing 动画等）
- `frontend/.gitignore` - 添加 node_modules/、styles/output.css
- `frontend/src/components/*.rs` - Button/Modal/Loading/EmptyState/Toast/Stats/Graph 组件改用 DaisyUI 类名
- `frontend/src/layouts/*.rs` - Navbar/AppLayout 改用 DaisyUI navbar 类名
- `frontend/src/pages/**/*.rs` - 各页面 CSS 类名从自定义类迁移到 Tailwind+DaisyUI 类
- `frontend/src/hooks/mod.rs` - 新增 use_theme hook
- `frontend/src/pages/settings.rs` - 添加主题切换功能
- `docs/frontend_architecture.md` - 更新 CSS 方案文档

**不修改文件：**
- 后端所有代码
- 前端业务逻辑（api/、store/、utils.rs、config.rs、main.rs）

---

### Task 1: 搭建构建工具链

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/tailwind.config.js`
- Create: `frontend/styles/input.css`
- Create: `Trunk.toml` (项目根目录)
- Create: `frontend/.gitignore`
- Modify: `frontend/index.html` (引入 output.css，保留 base reset 和必要自定义样式)

- [ ] **Step 1: 创建 package.json**

```json
{
  "name": "ai-orz-frontend",
  "private": true,
  "scripts": {
    "build:css": "tailwindcss -i ./styles/input.css -o ./styles/output.css --minify",
    "watch:css": "tailwindcss -i ./styles/input.css -o ./styles/output.css --watch"
  },
  "devDependencies": {
    "@tailwindcss/cli": "^4.1.0",
    "daisyui": "^5.0.0",
    "tailwindcss": "^4.1.0"
  }
}
```

- [ ] **Step 2: 创建 tailwind.config.js**

```js
/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./index.html",
    "./src/**/*.{rs,html}",
  ],
  theme: {
    extend: {},
  },
  plugins: [
    require('daisyui'),
  ],
  daisyui: {
    themes: [
      {
        "orz-light": {
          "primary": "#fa520f",
          "primary-content": "#ffffff",
          "secondary": "#fb6424",
          "secondary-content": "#ffffff",
          "accent": "#ffb83e",
          "accent-content": "#1f1f1f",
          "neutral": "#1f1f1f",
          "neutral-content": "#ffffff",
          "base-100": "#fffaeb",
          "base-200": "#fff0c2",
          "base-300": "#f0e8d4",
          "base-content": "#1f1f1f",
          "info": "#3498db",
          "info-content": "#ffffff",
          "success": "#2ecc71",
          "success-content": "#ffffff",
          "warning": "#f39c12",
          "warning-content": "#1f1f1f",
          "error": "#e74c3c",
          "error-content": "#ffffff",
          "--rounded-btn": "0.375rem",
          "--tab-border": "1px",
        },
      },
      "light",
      "dark",
      "cupcake",
      "bumblebee",
      "emerald",
      "corporate",
      "synthwave",
      "retro",
      "cyberpunk",
      "valentine",
      "halloween",
      "garden",
      "forest",
      "aqua",
      "lofi",
      "pastel",
      "fantasy",
      "wireframe",
      "black",
      "luxury",
      "dracula",
      "cmyk",
      "autumn",
      "business",
      "acid",
      "lemonade",
      "night",
      "coffee",
      "winter",
      "dim",
      "nord",
      "sunset",
    ],
  },
}
```

- [ ] **Step 3: 创建 styles/input.css**

```css
@import "tailwindcss";
@plugin "daisyui";

/* ===== 自定义工具类 / 覆盖 ===== */

/* Chat 布局 - 复杂布局暂保留自定义类 */
.chat-container {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.chat-sidebar {
  width: 320px;
  display: flex;
  flex-direction: column;
  @apply bg-base-100 border-r border-base-300;
}

/* ... 其他 chat 样式在迁移阶段逐步替换为 Tailwind 类 */

/* 打字动画 */
@keyframes typing-bounce {
  0%, 80%, 100% { transform: scale(0); }
  40% { transform: scale(1); }
}

.typing-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  @apply bg-base-content/50;
  animation: typing-bounce 1.4s infinite ease-in-out both;
}
.typing-dot:nth-child(1) { animation-delay: -0.32s; }
.typing-dot:nth-child(2) { animation-delay: -0.16s; }

/* Reception 品牌区渐变背景 */
.reception-brand {
  @apply flex-1 flex flex-col justify-center relative overflow-hidden;
  background: linear-gradient(135deg, #1f1f1f 0%, #2d1a0a 40%, #3d200a 100%);
  padding: 3rem 4rem;
}
```

- [ ] **Step 4: 创建 Trunk.toml（项目根目录）**

```toml
[build]
target = "frontend/index.html"
dist = "dist"

[[hooks]]
stage = "pre_build"
command = "sh"
command_arguments = ["-c", "cd frontend && npm run build:css"]
```

- [ ] **Step 5: 创建 frontend/.gitignore**

```
node_modules/
styles/output.css
```

- [ ] **Step 6: 安装 npm 依赖**

Run: `cd frontend && npm install`
Expected: node_modules/ 创建，tailwindcss 和 daisyui 安装成功

- [ ] **Step 7: 修改 index.html，引入 Tailwind 输出 CSS**

将 `<style>` 标签中的内容替换为：
1. 保留最基本的 reset（box-sizing 等）
2. 添加 `<link rel="stylesheet" href="styles/output.css">`
3. 保留少量暂时无法用 Tailwind 替换的复杂样式（chat 布局、typing 动画、reception 渐变、toast 动画等）
4. 注释标记保留区域为 "Custom overrides - will be migrated incrementally"

- [ ] **Step 8: 首次编译 Tailwind CSS 验证**

Run: `cd frontend && npm run build:css`
Expected: `frontend/styles/output.css` 生成，包含 Tailwind 和 DaisyUI 样式

- [ ] **Step 9: 验证 trunk serve 能正常构建**

Run: `cd /Users/aman/Technology/rust/ai_orz && trunk serve` (或 trunk build)
Expected: 构建成功，页面正常加载，现有样式暂时仍由保留的自定义 CSS 提供

- [ ] **Step 10: Commit**

```bash
git add frontend/package.json frontend/package-lock.json frontend/tailwind.config.js frontend/styles/input.css frontend/.gitignore Trunk.toml frontend/index.html
git commit -m "feat(frontend): 搭建 Tailwind CSS v4 + DaisyUI v5 构建工具链"
```

---

### Task 2: 迁移基础 UI 组件（components/）

**Files:**
- Modify: `frontend/src/components/button.rs`
- Modify: `frontend/src/components/modal.rs`
- Modify: `frontend/src/components/state.rs` (Loading/EmptyState)
- Modify: `frontend/src/components/toast.rs`
- Modify: `frontend/src/components/stats.rs`
- Modify: `frontend/src/components/graph.rs`
- Verify: `trunk build` 编译通过

DaisyUI 组件对应关系：

| 现有组件 | DaisyUI 替代 | 说明 |
|---------|-------------|------|
| Button (Primary) | `<button class="btn btn-primary">` | 映射 btn-primary |
| Button (Accent) | `<button class="btn btn-secondary">` | accent→secondary |
| Button (Secondary) | `<button class="btn btn-ghost">` | 自定义背景→ghost |
| Button (Danger) | `<button class="btn btn-error">` | danger→error |
| Button (Ghost) | `<button class="btn btn-ghost">` | 直接使用 |
| Button (small) | `<button class="btn btn-sm">` | btn-sm |
| Modal overlay/content | `<dialog class="modal modal-open"><div class="modal-box">` | DaisyUI modal |
| Modal close button | `<form method="dialog"><button class="btn btn-sm btn-circle btn-ghost absolute right-2 top-2">✕</button></form>` | DaisyUI modal close |
| Modal footer | `<div class="modal-action">` | modal-action |
| Loading spinner | `<span class="loading loading-spinner loading-lg">` | DaisyUI loading |
| EmptyState | DaisyUI 无直接对应，用 Tailwind 类重写 | 保持现有结构 |
| Alert/Toast | `<div class="alert alert-error/success/warning/info">` | DaisyUI alert |
| Badge | `<span class="badge badge-success/...">` | DaisyUI badge（语义色有差异需调整） |
| Card | `<div class="card bg-base-100 shadow-xl">` | DaisyUI card |

- [ ] **Step 1: 迁移 button.rs**

修改 ButtonVariant 的 class 映射：

```rust
impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn btn-primary",
            ButtonVariant::Accent => "btn btn-secondary",
            ButtonVariant::Secondary => "btn btn-outline",
            ButtonVariant::Danger => "btn btn-error",
            ButtonVariant::Ghost => "btn btn-ghost",
        }
    }
}
```

- [ ] **Step 2: 迁移 modal.rs 到 DaisyUI modal**

使用 DaisyUI 的 `<dialog class="modal">` 模式：

```rust
#[component]
pub fn Modal(props: ModalProps) -> Element {
    if !props.show {
        return rsx! {};
    }
    rsx! {
        dialog {
            class: "modal modal-open",
            open: true,
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-box",
                onclick: |e| e.stop_propagation(),
                form {
                    method: "dialog",
                    button {
                        class: "btn btn-sm btn-circle btn-ghost absolute right-2 top-2",
                        onclick: move |_| props.on_close.call(()),
                        "✕"
                    }
                }
                h3 { class: "font-bold text-lg mb-4", "{props.title}" }
                {props.children}
                if let Some(footer) = &props.footer {
                    div { class: "modal-action", {footer.clone()} }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 迁移 state.rs（Loading/EmptyState）**

- Loading: 使用 `<span class="loading loading-spinner loading-lg"></span>` 替代手写 spinner
- EmptyState: 使用 Tailwind 工具类重写布局（text-center py-12 等）

- [ ] **Step 4: 迁移 toast.rs**

- Toast 容器使用 DaisyUI `<div class="toast toast-end">` 定位
- 每条 toast 使用 `<div class="alert alert-error/success/...">` 
- 保留进度条动画和进入/离开动画

- [ ] **Step 5: 迁移 stats.rs**

- Stats 卡片使用 DaisyUI `<div class="stats shadow">` + `<div class="stat">` 组件

- [ ] **Step 6: 验证编译**

Run: `trunk build`
Expected: 编译通过，组件类名已替换为 DaisyUI 类

- [ ] **Step 7: trunk serve 视觉验证**

Run: `trunk serve`，浏览器打开页面检查：
- 按钮样式正确（颜色、悬停、禁用状态）
- 模态框弹出/关闭正常
- Loading 转圈动画正常
- Toast 通知正常弹出

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/
git commit -m "refactor(frontend/components): 迁移基础 UI 组件到 DaisyUI"
```

---

### Task 3: 迁移布局组件（layouts/）

**Files:**
- Modify: `frontend/src/layouts/navbar.rs`
- Modify: `frontend/src/layouts/app_layout.rs`
- Verify: `trunk build` 编译通过

DaisyUI 导航栏组件：
- Navbar: `<div class="navbar bg-neutral text-neutral-content">`
- Dropdown: `<div class="dropdown dropdown-end">`
- Avatar: `<div class="avatar"><div class="w-7 rounded-full bg-primary">...</div></div>`
- Menu: `<ul class="menu dropdown-content bg-base-100 rounded-box z-[1] w-52 p-2 shadow">`

- [ ] **Step 1: 迁移 navbar.rs**

将 navbar 从自定义类迁移到 DaisyUI navbar + dropdown + menu 组件。
保留移动端汉堡菜单和抽屉逻辑，使用 DaisyUI `dropdown` 实现移动端菜单。
品牌色保持黑底橙黄文字。

- [ ] **Step 2: 迁移 app_layout.rs**

- `app-container` → DaisyUI/Tailwind 类
- `content-area` → `container mx-auto px-4 py-8` 等

- [ ] **Step 3: 验证编译和视觉**

Run: `trunk serve`，检查：
- 桌面端导航栏样式、下拉菜单正常
- 移动端汉堡菜单/抽屉正常
- 布局间距正确

- [ ] **Step 4: Commit**

```bash
git add frontend/src/layouts/
git commit -m "refactor(frontend/layouts): 迁移 Navbar 和 AppLayout 到 DaisyUI"
```

---

### Task 4: 迁移 Hooks 和新增主题切换

**Files:**
- Modify: `frontend/src/hooks/mod.rs` - 添加 use_theme hook
- Modify: `frontend/src/pages/settings.rs` - 添加主题切换 UI
- Modify: `frontend/src/main.rs` - 在根元素设置 data-theme 属性
- Modify: `frontend/src/store/mod.rs` - 可选：添加主题 store

- [ ] **Step 1: 在 hooks/mod.rs 添加 use_theme hook**

```rust
pub fn use_theme() -> (Signal<String>, impl Fn(String)) {
    let theme = use_context::<Signal<ThemeState>>().unwrap_or_else(|| use_signal(|| ThemeState { current: "orz-light".to_string() }));
    // 读取 localStorage 保存的主题，默认 orz-light
    // 切换时设置 data-theme 属性到 document.documentElement
    // 保存到 localStorage
}
```

- [ ] **Step 2: 在 settings.rs 添加主题选择器**

使用 DaisyUI 的主题切换方式，在设置页面添加一个主题选择区域，展示所有可用主题的预览色板。

DaisyUI 支持通过 `data-theme` 属性切换主题：
```rust
div {
    class: "join",
    button { class: "btn btn-sm join-item", "data-theme": "orz-light", "Orz" }
    button { class: "btn btn-sm join-item", "data-theme": "light", "Light" }
    button { class: "btn btn-sm join-item", "data-theme": "dark", "Dark" }
    // ... 更多主题
}
```

- [ ] **Step 3: 在 App 根组件应用主题**

在 main.rs 或 app_layout.rs 中，从 use_theme 获取当前主题，设置到最外层 div 的 `data-theme` 属性。

- [ ] **Step 4: 验证编译和主题切换**

Run: `trunk serve`
- 设置页面出现主题选择器
- 点击不同主题，整个页面颜色方案即时切换
- 刷新页面后主题保持（localStorage 持久化）

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/mod.rs frontend/src/pages/settings.rs frontend/src/main.rs frontend/src/layouts/app_layout.rs
git commit -m "feat(frontend): 添加 DaisyUI 多主题切换支持"
```

---

### Task 5: 逐页面迁移 CSS 类名（第一批 - 简单 CRUD 页面）

**Files:** 按模块分批迁移，每个模块一个 commit

**迁移优先级：**
1. `pages/reception.rs` - 登录页（使用 DaisyUI hero + card）
2. `pages/organization/*.rs` - 组织信息、用户管理
3. `pages/system/health.rs`、`pages/system/backup.rs`、`pages/system/logs.rs` - 系统管理简单页面
4. `pages/finance/model_providers.rs`、`pages/finance/tools.rs`、`pages/finance/message_channels.rs` - 资源列表页
5. `pages/hr/agents.rs`、`pages/hr/skills.rs` - HR 页面
6. `pages/project/projects.rs`、`pages/project/tasks.rs` - 项目列表
7. `pages/user/profile.rs` - 个人信息

**通用迁移映射：**

| 旧类名 | Tailwind/DaisyUI 替代 |
|--------|----------------------|
| `.card` | `card bg-base-100 shadow-md` |
| `.card-header` | `card-title` 或 `div class="flex justify-between items-center pb-4 border-b border-base-200 mb-6"` |
| `.card-title` | `h2 class="card-title text-xl"` |
| `.card-hover` | `hover:shadow-lg hover:-translate-y-0.5 transition-all` |
| `.table` | `table table-zebra` 或 `table table-pin-rows` |
| `.table th` | DaisyUI table 自动处理 |
| `.table-row-clickable` | `hover:bg-base-200 cursor-pointer transition-colors` |
| `.form-group` | `form-control w-full mb-4` |
| `.form-label` | `label class="label" <span class="label-text">` |
| `.form-input` | `input input-bordered w-full` |
| `.form-textarea` | `textarea textarea-bordered w-full` |
| `.form-select` | `select select-bordered w-full` |
| `.form-hint` | `label class="label" <span class="label-text-alt">` |
| `.badge-*` | `badge badge-*`（DaisyUI badge 变体） |
| `.alert-*` | `alert alert-*`（DaisyUI alert 变体） |
| `.btn-sm` | `btn btn-sm` |
| `.text-center` | `text-center` |
| `.text-right` | `text-right` |
| `.w-full` | `w-full` |
| `.mb-4` | `mb-4` |
| `.mt-4` | `mt-4` |
| `.gap-*` | `gap-*` |
| `.flex` | `flex` |
| `.flex-col` | `flex-col` |
| `.items-center` | `items-center` |
| `.justify-between` | `justify-between` |

- [ ] **Step 1: 迁移 reception.rs**
  - 品牌区：保留自定义渐变类（在 input.css 的 custom overrides 中）
  - 登录表单：使用 DaisyUI card + form-control + input input-bordered + btn btn-primary
  - 组织选择：使用 card + hover 样式

- [ ] **Step 2: 迁移 organization 模块**
  - info.rs: card 组件、表单
  - users.rs: table + badge + button + modal

- [ ] **Step 3: 迁移 system 简单页面**
  - health.rs: 状态卡片、stats
  - backup.rs: table + button
  - logs.rs: textarea/log 展示

- [ ] **Step 4: 编译验证**
Run: `trunk build`
Expected: 编译通过

- [ ] **Step 5: 视觉验证**
Run: `trunk serve`，逐页面检查布局、颜色、交互是否正常

- [ ] **Step 6: Commit（分批）**

```bash
git add frontend/src/pages/reception.rs frontend/src/pages/organization/
git commit -m "refactor(frontend/pages): 迁移登录页和组织管理页面到 Tailwind/DaisyUI"

git add frontend/src/pages/system/
git commit -m "refactor(frontend/pages): 迁移系统管理页面到 Tailwind/DaisyUI"
```

---

### Task 6: 逐页面迁移 CSS 类名（第二批 - 复杂页面）

**Files:**
- Modify: `pages/hr/agents.rs`、`pages/hr/agent_detail.rs`、`pages/hr/agent_memory_panel.rs`、`pages/hr/knowledge_graph.rs`、`pages/hr/memory_search.rs`、`pages/hr/skills.rs`
- Modify: `pages/finance/model_provider_detail.rs`、`pages/finance/tool_detail.rs`、`pages/finance/mcp_servers.rs`、`pages/finance/attachments.rs`
- Modify: `pages/project/project_detail.rs`、`pages/project/task_detail.rs`、`pages/project/task_edit_modal.rs`、`pages/project/artifacts.rs`
- Modify: `pages/system/triggers.rs`、`pages/system/aop.rs`
- Modify: `pages/message/search.rs`

这些页面包含更多自定义样式（详情页、进度条、看板视图、图表等）。

- [ ] **Step 1: 迁移 finance 详情页（model_provider_detail、tool_detail）**
  - 详情页通用布局：使用 DaisyUI card + grid
  - 统计卡片：使用 DaisyUI stats
  - 进度条：使用 Tailwind 或 DaisyUI 进度条组件

- [ ] **Step 2: 迁移 hr 模块页面**
  - agent_detail.rs: 详情页 + agent-chat 区域（chat 暂用保留的自定义类）
  - knowledge_graph.rs: 图谱容器 + 详情面板
  - skills.rs: 技能包管理（tag-card 等）

- [ ] **Step 3: 迁移 project 模块**
  - project_detail.rs: 概览统计 + 进度 + 看板（使用 Tailwind grid/flex）
  - tasks.rs: 看板视图（kanban 用 Tailwind 类重写）
  - task_detail.rs / task_edit_modal.rs: 任务详情和编辑
  - artifacts.rs: 附件展示

- [ ] **Step 4: 迁移 system/triggers.rs 和 system/aop.rs**
  - triggers.rs: 表单 + cron 预设按钮 + JSON 错误提示
  - aop.rs: 队列统计表格 + 事件详情模态框

- [ ] **Step 5: 迁移 message/search.rs**

- [ ] **Step 6: 编译验证**
Run: `trunk build`
Expected: 编译通过

- [ ] **Step 7: 视觉验证**
Run: `trunk serve`，逐页面检查

- [ ] **Step 8: Commit（分批）**

```bash
git add frontend/src/pages/finance/ frontend/src/pages/hr/
git commit -m "refactor(frontend/pages): 迁移财务和 HR 模块页面到 Tailwind/DaisyUI"

git add frontend/src/pages/project/ frontend/src/pages/system/ frontend/src/pages/message/search.rs
git commit -m "refactor(frontend/pages): 迁移项目、系统监控和消息搜索页面到 Tailwind/DaisyUI"
```

---

### Task 7: 迁移 Chat 页面（最复杂）

**Files:**
- Modify: `frontend/src/pages/message/chat.rs`
- Modify: `frontend/styles/input.css` - 清理已迁移的 chat 自定义样式，仅保留必要部分

Chat 页面是最复杂的，包含：
- 双栏布局（sidebar + main）
- 消息列表（气泡、头像、工具调用卡片、任务分配卡片）
- 附件展示（图片、视频、音频、文件）
- 输入区（textarea + 发送按钮 + 附件按钮）
- 打字指示器
- SSE 实时消息
- 移动端适配（单栏覆盖式 sidebar）

DaisyUI 有 chat 组件：
- `<div class="chat chat-start/chat-end">` - 消息气泡
- `<div class="chat-image avatar">` - 头像
- `<div class="chat-bubble">` - 气泡
- `<div class="chat-footer">` - 时间戳
- `<div class="chat-bubble-primary">` - 用户消息

- [ ] **Step 1: 迁移 sidebar 区域**
  - 项目列表：使用 DaisyUI menu
  - 选中状态：`active` 类
  - 新建项目弹窗：使用 Modal 组件（已迁移）

- [ ] **Step 2: 迁移消息列表**
  - 使用 DaisyUI chat 组件替代自定义 .message-item/.message-bubble
  - 系统消息使用 `chat-bubble-neutral`
  - 工具调用卡片：保留自定义样式（tool-card 复杂结构），逐步用 Tailwind 类重写
  - 任务分配卡片：保留自定义样式
  - 附件：图片/视频/音频/文件用 Tailwind 类重写

- [ ] **Step 3: 迁移输入区**
  - textarea: 使用 DaisyUI textarea 或保留自定义 chat-input
  - 发送按钮: btn btn-primary
  - 附件按钮: btn btn-ghost btn-circle
  - 待上传附件: badge 组件

- [ ] **Step 4: 迁移移动端适配**
  - 使用 Tailwind 响应式类（md:、sm: 前缀）替代现有 @media 查询
  - sidebar 显示/隐藏逻辑不变

- [ ] **Step 5: 编译验证和功能测试**
Run: `trunk serve`
- 桌面端双栏布局正常
- 发送/接收消息正常（包括 SSE 实时推送）
- 工具调用卡片正常展示和展开/折叠
- 附件上传/预览/下载正常
- 移动端单栏布局正常
- 打字指示器动画正常

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/message/chat.rs frontend/styles/input.css
git commit -m "refactor(frontend/chat): 迁移消息对话页面到 Tailwind/DaisyUI chat 组件"
```

---

### Task 8: 清理旧 CSS 和最终调整

**Files:**
- Modify: `frontend/index.html` - 删除所有已迁移的手写 CSS 类
- Modify: `frontend/styles/input.css` - 只保留真正必要的自定义样式
- Verify: 全页面视觉检查

- [ ] **Step 1: 审计 index.html 中哪些 CSS 类仍被使用**

使用 grep 检查所有 .rs 文件中引用的 CSS 类名，与 index.html 中定义的类对比，找出不再被使用的类。

- [ ] **Step 2: 删除 index.html 中已迁移的样式**

逐块检查并删除：
- ~~Button 样式~~ → 已用 DaisyUI btn
- ~~Card 样式~~ → 已用 DaisyUI card
- ~~Table 样式~~ → 已用 DaisyUI table
- ~~Form 样式~~ → 已用 DaisyUI form-control/input
- ~~Badge 样式~~ → 已用 DaisyUI badge
- ~~Modal 样式~~ → 已用 DaisyUI modal
- ~~Alert 样式~~ → 已用 DaisyUI alert
- ~~State indicators~~ → 已用 DaisyUI loading
- Layout utilities（flex、gap、w-full 等）→ Tailwind 内置
- Navbar 样式 → 已用 DaisyUI navbar
- 保留：reception-page 渐变（暂保留）、chat 复杂组件（逐步清理）、toast 动画、typing 动画、进度条、知识图谱等暂未完全迁移的样式

- [ ] **Step 3: 整理 styles/input.css**

确保 @import 和 @plugin 正确，自定义样式按区域组织，添加注释。

- [ ] **Step 4: trunk build 验证无警告**

Run: `trunk build`
Expected: 编译通过，CSS 输出正常

- [ ] **Step 5: 全页面视觉回归检查**

逐页面检查所有功能，确保没有样式丢失。

- [ ] **Step 6: 更新前端架构文档**

更新 `docs/frontend_architecture.md`：
- CSS 方案从"手写 CSS"改为"Tailwind CSS v4 + DaisyUI v5"
- 更新组件类映射表
- 新增主题切换说明
- 更新构建流程说明（npm install + trunk build）
- 记录迁移完成的更新记录

- [ ] **Step 7: 更新 AGENTS.md**

在更新日志中添加 Tailwind+DaisyUI 迁移记录。

- [ ] **Step 8: Commit 最终清理**

```bash
git add frontend/index.html frontend/styles/input.css docs/frontend_architecture.md AGENTS.md
git commit -m "refactor(frontend): 清理旧 CSS，完成 Tailwind CSS + DaisyUI 迁移"
```

---

### Task 9: 运行全量测试并推送

- [ ] **Step 1: 后端测试**
Run: `cargo test`
Expected: 全部通过（后端未改动）

- [ ] **Step 2: 前端编译**
Run: `trunk build`
Expected: 编译通过，无错误

- [ ] **Step 3: trunk serve 最终验证**
- 所有页面加载正常
- 所有交互正常（按钮、模态框、表单、导航）
- 主题切换正常
- 移动端适配正常
- Chat 功能正常

- [ ] **Step 4: 推送到远程**
```bash
git push
```

---

## 风险与注意事项

1. **DaisyUI 主题颜色映射**：现有 `--color-accent` 是紫色(#6366f1)，在自定义 orz-light 主题中需要确认 accent 色值。我们将 primary 设为 Mistral 橙(#fa520f)，accent 设为金色(#ffb83e)以保持品牌一致性。
2. **Chat 工具卡片/任务卡片**：这些是业务组件，DaisyUI 没有直接对应，保留自定义样式但用 Tailwind 类重写。
3. **Trunk 构建**：确保 CI/CD 环境中也需要运行 `npm install`，或者考虑将 tailwindcss 二进制嵌入 build.rs（更复杂，初期先 npm 方案）。
4. **渐进迁移过程中**：Tailwind CSS 和手写 CSS 会共存一段时间，需要注意类名冲突（Tailwind 的 preflight 会重置一些默认样式）。
5. **DaisyUI v5 vs v4**：v5 是最新版，使用 Tailwind v4 格式。如果 v5 有兼容性问题，可以降级到 v4（配合 Tailwind v3）。

## 自定义 orz-light 主题色对照

| 角色 | 旧 CSS 变量 | DaisyUI 主题变量 | 色值 |
|------|-----------|-----------------|------|
| 主色（品牌橙） | --color-mistral-orange | primary | #fa520f |
| 辅助橙 | --color-mistral-flame | secondary | #fb6424 |
| 强调色（金色） | --color-sunshine-500 | accent | #ffb83e |
| 深色（黑） | --color-mistral-black | neutral | #1f1f1f |
| 背景（暖象牙） | --color-warm-ivory | base-100 | #fffaeb |
| 次级背景（奶油） | --color-cream | base-200 | #fff0c2 |
| 边框色 | --color-border | base-300 | #e0d6c0 |
| 主文字 | --color-text-primary | base-content | #1f1f1f |
| 成功 | --color-success | success | #2ecc71 |
| 警告 | --color-warning | warning | #f39c12 |
| 错误 | --color-error | error | #e74c3c |
| 信息 | --color-info | info | #3498db |
