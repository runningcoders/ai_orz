# 前端架构设计

> 最后更新：2026-07-22

## 概述

AI Orz 前端基于 **Dioxus 0.7 (Rust WebAssembly)** 构建，采用 **Tailwind CSS 4 + DaisyUI 5** 组件库（暖色调自定义主题 orz-light），支持 28 种主题切换，使用 Dioxus Router 路由、统一 API 客户端（HttpOnly Cookie 认证）和全局状态管理。前端按业务域组织页面模块，与后端 Handler 域对齐。

---

## 技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| 框架 | Dioxus 0.7 | Rust WebAssembly 前端框架 |
| 路由 | Dioxus Router | URL 路由 + Link 组件导航 |
| HTTP 客户端 | reqwest 0.13 | 全局 OnceLock 单例，复用连接池 |
| 样式框架 | Tailwind CSS 4.3 + DaisyUI 5.7 | 功能类优先 CSS + 组件库，28 主题支持 |
| 自定义主题 | orz-light | 基于品牌色（#fa520f 橙色）的暖色调主题 |
| 状态管理 | Dioxus Signal + use_context_provider | 全局 AuthState/ToastState 共享 |
| 共享类型 | common crate | 与后端共享 DTO、枚举、常量 |
| 构建工具 | npm + dioxus-cli (dx) | npm 管理 Tailwind/DaisyUI，build.rs 自动编译 CSS |
| 配置嵌入 | build.rs | 编译时读取后端 ai_orz.toml 嵌入前端，自动编译 Tailwind CSS |

---

## 目录结构

```
frontend/
├── Cargo.toml                # 依赖配置（dioxus 0.7 + router feature）
├── package.json              # npm 依赖（tailwindcss + @tailwindcss/cli + daisyui）
├── tailwind.config.js        # Tailwind 配置（内容路径 + DaisyUI 插件 + 主题）
├── Dioxus.toml               # Dioxus CLI 配置（输出目录、资源目录）
├── build.rs                  # 编译时配置嵌入 + Tailwind CSS 自动编译
├── index.html                # HTML 入口 + 少量自定义 CSS（动画/特殊组件）
├── styles/
│   └── input.css             # Tailwind CSS 入口（@import tailwindcss + @plugin daisyui + @theme）
├── public/
│   └── output.css            # Tailwind 编译产物（由 build.rs 自动生成）
└── src/
    ├── main.rs               # 入口：Router + 全局状态注入 + 主题初始化
    ├── config.rs             # 前端运行时配置管理（localStorage 读写）
    ├── utils.rs              # 通用工具函数（localStorage 访问）
    │
    ├── api/                  # API 客户端层
    │   ├── mod.rs            # 统一 HTTP 客户端、Cookie 认证、helper 函数、错误解析
    │   ├── auth.rs           # 认证 API（check_initialized/initialize_system/login/logout）
    │   ├── organization.rs   # 组织管理 API
    │   ├── hr.rs             # HR 域 API（agent/skill/tool-pack/skill-pack）
    │   ├── finance.rs        # Finance 域 API（model_provider/tool/message_channel/mcp_server/attachment）
    │   ├── project.rs        # Project 域 API（project/task/artifact）
    │   ├── message.rs        # Message 域 API（消息加载/发送/SSE）
    │   └── system.rs         # System 域 API（health/cron_trigger/aop_stats/backup/logs）
    │
    ├── hooks/                # 自定义 Hooks
    │   ├── mod.rs            # use_breakpoint、use_require_auth、use_theme（28主题切换）、ThemeController
    │   └── use_resource.rs   # use_resource hook：三态资源加载（Loading/Ready/Failed）
    │
    ├── store/                # 全局状态管理
    │   ├── auth.rs           # 认证状态（AuthState + localStorage + HttpOnly Cookie）
    │   └── toast.rs          # Toast 状态（ToastState + show/dismiss/success/error/warning/info）
    │
    ├── components/           # 基础 UI 组件库
    │   ├── button.rs         # Button 组件（Primary/Secondary/Outline/Error/Ghost + sm 尺寸）
    │   ├── modal.rs          # Modal 对话框组件（DaisyUI dialog/modal-box）
    │   ├── state.rs          # 状态展示组件（Loading/EmptyState/ErrorAlert/SuccessAlert）
    │   ├── stats.rs          # 统计面板组件（StatsCard/AgentStatsPanel/ProjectStatsPanel/TaskStatsPanel）
    │   ├── input.rs          # Input/Textarea/Select 表单组件
    │   ├── toast.rs          # Toast 通知容器（DaisyUI toast + alert）
    │   └── graph.rs          # 知识图谱 SVG 可视化组件
    │
    ├── layouts/              # 布局组件
    │   ├── navbar.rs         # 顶部导航栏（桌面下拉菜单 / 移动端抽屉）
    │   └── app_layout.rs     # 应用布局（Navbar + 内容区 + 权限守卫）
    │
    └── pages/                # 页面模块（按业务域分组）
        ├── mod.rs            # Route 枚举定义
        ├── reception.rs      # 前台接待/登录页
        ├── settings.rs       # 系统设置（API 地址 + 主题切换）
        │
        ├── organization/     # 组织模块（info, users）
        ├── hr/               # HR 模块（agents, agent_detail, skills, memory_search, knowledge_graph, agent_memory_panel）
        ├── finance/          # Finance 模块（model_providers, model_provider_detail, tools, tool_detail, message_channels, mcp_servers, attachments）
        ├── project/          # Project 模块（projects, project_detail, tasks, task_detail, artifacts, task_edit_modal）
        ├── message/          # Message 模块（chat, search）
        ├── system/           # System 模块（triggers, health, logs, backup, aop）
        └── user/             # 用户模块（profile）
```

---

## 核心架构设计

### 1. 样式系统：Tailwind CSS 4 + DaisyUI 5

**构建流程**：
- npm 安装 Tailwind CSS 4 和 DaisyUI 5
- `build.rs` 在编译时自动调用 `node_modules/.bin/tailwindcss` 编译 CSS
- `styles/input.css` 作为入口，引入 Tailwind 和 DaisyUI 插件
- 编译产物输出到 `public/output.css`，由 Dioxus 自动打包

**自定义主题 orz-light**：
```css
@plugin "daisyui" {
  themes: orz-light --default, light, dark, cupcake, ...;
}

[data-theme="orz-light"] {
  --color-primary: oklch(0.63 0.24 50);   /* #fa520f 品牌橙色 */
  --color-neutral: oklch(0.25 0 0);       /* #1f1f1f 深色 */
  --color-base-100: oklch(0.98 0.02 95);  /* #fffaeb 暖白 */
  --color-base-200: oklch(0.96 0.04 90);  /* #fff0c2 奶油色 */
  /* ... */
}
```

**主题切换**：
- `use_theme()` Hook 返回 `ThemeController`（Clone+Copy）
- 主题持久化到 localStorage（`ai_orz_theme` key）
- 通过 `document.documentElement.setAttribute("data-theme", theme)` 切换
- 设置页提供 28 种主题选择按钮
- 主题切换即时生效，无需刷新页面

**DaisyUI 组件使用规范**：

| DaisyUI 类 | 用途 |
|-----------|------|
| `btn btn-primary/secondary/error/ghost/outline btn-sm` | 按钮 |
| `card card-body card-title shadow-md bg-base-100` | 卡片 |
| `table table-zebra` + `overflow-x-auto` 包裹 | 表格 |
| `form-control w-full` + `label/label-text` + `input/textarea/select input-bordered` | 表单 |
| `badge badge-success/error/warning/info/neutral` | 徽章 |
| `modal modal-open` + `modal-box` + `modal-action` | 模态框（公共 Modal 组件封装） |
| `alert alert-success/error/warning/info` | 提示框 |
| `chat chat-start/end` + `chat-bubble` + `chat-image avatar` | 聊天气泡 |
| `avatar placeholder` + `w-10 rounded-full bg-primary` | 头像 |
| `loading loading-spinner loading-sm/md/lg` | 加载动画 |
| `divider` | 分隔线 |
| `stat` | 统计卡片 |
| `navbar bg-neutral text-neutral-content` | 导航栏 |
| `dropdown dropdown-end` + `menu` | 下拉菜单 |
| `toast toast-top toast-end z-[9999]` | Toast 容器 |
| `flex/gap-*/p-*/m-*/w-full/grid/grid-cols-*` 等 | Tailwind 工具类 |

**自定义 CSS 范围**（`index.html` 的 `<style>` 标签）：
- 打字指示器动画（typing-bounce）
- 工具调用卡片（tool-card）
- 任务分配卡片（task-card）
- 消息附件样式
- 接待页品牌视觉（渐变背景、logo、feature 列表）
- 知识图谱布局
- 看板视图布局
- Agent 对话容器
- Cron 预设按钮、JSON 错误提示
- Toast 进度条动画
- 滚动条美化

### 2. 路由系统（Dioxus Router）

使用 Dioxus 0.7 的 `Routable` derive 宏定义路由枚举，支持 URL 路由和 `Link` 组件导航。

### 3. 统一 API 客户端

- **全局 HTTP 客户端单例**：`OnceLock<Client>` 复用连接池
- **Cookie 认证**：基于 HttpOnly Cookie（JWT），浏览器自动携带
- **类型化 helper**：`api_get`/`api_post`/`api_put`/`api_delete`/`api_post_multipart` 等
- **错误处理**：`parse_api_error_from_body()` 和 `parse_error_response()` 辅助函数

### 4. 全局状态管理

- **AuthState**：登录状态、用户名、角色、组织信息（localStorage 标志位 + HttpOnly Cookie）
- **ToastState**：全局 Toast 通知（show/dismiss/success/error/warning/info，自动消失）
- **主题状态**：ThemeController 管理主题切换和持久化
- **断点状态**：is_mobile Signal 通过 use_breakpoint Hook 全局共享

### 5. 基础 UI 组件库

| 组件 | 文件 | 说明 |
|------|------|------|
| Button | `components/button.rs` | 5 种 variant + sm 尺寸，基于 DaisyUI btn |
| Modal | `components/modal.rs` | 对话框（DaisyUI dialog/modal-box，footer prop） |
| Loading | `components/state.rs` | DaisyUI loading loading-spinner |
| EmptyState | `components/state.rs` | 空数据状态 |
| ErrorAlert | `components/state.rs` | DaisyUI alert alert-error |
| Input/Textarea/Select | `components/input.rs` | DaisyUI input/textarea/select input-bordered |
| ToastContainer | `components/toast.rs` | DaisyUI toast + alert，自动消失动画 |

### 6. 自定义 Hooks

| Hook | 说明 |
|------|------|
| `use_breakpoint()` | 返回 `Signal<bool>`（true = 移动端），基于 matchMedia 监听 |
| `use_require_auth()` | 权限守卫，未登录自动跳转 Reception 页 |
| `use_theme()` | 返回 ThemeController，支持 28 主题切换 + localStorage 持久化 |
| `use_resource(fetcher)` | 三态资源加载（Loading/Ready/Failed），自动首次加载 + reload |

---

## 配置机制

### 编译时配置嵌入
- `build.rs` 读取后端 `ai_orz.toml`（优先 `.ai_orz/`，回退 `common/config/`）
- **CSS 编译**：`build.rs` 自动调用 Tailwind CSS CLI 编译 `styles/input.css` → `public/output.css`
- npm 依赖：首次构建需在 `frontend/` 目录执行 `npm install`

### 运行时配置管理
- `config.rs` 管理前端配置（API 地址）
- 设置页支持修改 API 地址和主题，保存到 localStorage

---

## 开发指南

### 环境准备

```bash
cd frontend
npm install          # 安装 Tailwind CSS 和 DaisyUI
```

### 开发命令

```bash
dx serve             # 开发服务器（自动热重载，含 CSS 编译）
dx build             # 构建（build.rs 自动编译 CSS）
npm run watch:css    # 独立监听 CSS 变更
npm run build:css    # 独立构建 CSS
```

### 样式使用规范

1. **优先使用 DaisyUI 组件类**：`btn`、`card`、`table`、`input-bordered`、`badge`、`alert`、`chat-bubble` 等
2. **使用 Tailwind 工具类**：`flex`、`gap-3`、`p-4`、`w-full`、`text-center`、`font-bold`、`rounded-lg`、`shadow-md` 等
3. **颜色使用主题变量**：`bg-primary`、`text-primary-content`、`bg-base-100`、`text-base-content`、`bg-base-200`、`border-base-300` 等，避免硬编码颜色值
4. **避免内联样式**：除动态计算值外，统一使用类名
5. **新增自定义样式**：仅在 DaisyUI/Tailwind 无法满足时添加到 `index.html`，保持精简
6. **主题兼容**：使用语义化颜色类（`bg-primary`/`text-error`/`bg-success/20`），确保在所有主题下显示正常

### 新增页面流程

1. 在 `pages/` 对应业务域创建页面文件
2. 在 `pages/mod.rs` Route 枚举添加路由变体
3. 在导航栏 `layouts/navbar.rs` 添加导航链接
4. 在 `api/` 对应域添加 API 函数
5. 使用 DaisyUI 组件类 + Tailwind 工具类构建 UI

---

## 更新记录

### 2026-07-22 引入 Tailwind CSS + DaisyUI 样式系统

| 变更项 | 实现细节 |
|--------|----------|
| **构建工具链** | 新增 package.json（tailwindcss@4.3.3 + @tailwindcss/cli + daisyui@5.7.0），styles/input.css 入口，tailwind.config.js 配置，build.rs 集成 Tailwind 编译，Dioxus.toml 配置 public 资源目录 |
| **自定义主题** | 定义 orz-light 暖色调主题（primary=#fa520f，base-100=#fffaeb），支持 28 种 DaisyUI 主题切换 |
| **主题切换** | 新增 `use_theme()` Hook + ThemeController 结构体，localStorage 持久化，根元素 data-theme 切换，Settings 页主题选择器 |
| **组件迁移** | Button/Modal/Loading/Toast/Input/Textarea/Select 组件全部改为 DaisyUI 类名，API 保持不变 |
| **布局迁移** | Navbar 改为 DaisyUI navbar + dropdown 组件，AppLayout 改为 flex 布局，移动端使用 DaisyUI drawer 模式 |
| **Chat 迁移** | 消息气泡改为 DaisyUI chat 组件（chat-start/chat-end/chat-bubble/chat-image avatar），输入区使用 textarea-bordered |
| **页面全量迁移** | 全部 30+ 页面从自定义 CSS 类迁移到 DaisyUI + Tailwind 类名，表格/表单/卡片/按钮/徽章统一使用 DaisyUI 组件 |
| **CSS 精简** | index.html 从 1960+ 行精简到 ~420 行，仅保留动画、特殊组件（tool-card/task-card/reception 品牌页/kanban/graph）等 DaisyUI 未覆盖的样式 |

### 2026-07-21 前端代码质量优化
- 修复 AOP 页面模态框和无效 CSS 类
- 统一公共 Modal 组件使用
- 提取 localStorage 工具函数、API 错误解析辅助函数
- 封装 use_resource hook

### 2026-07-17 移动端适配（响应式双端兼容）
- Navbar 汉堡菜单 + 抽屉
- Chat 单栏覆盖式 sidebar
- 表格卡片化（data-label 属性）
- Modal/Toast/网格响应式适配

### 2026-07-16 定时触发器前端体验优化
- 列表信息增强、Action 模板化、Cron 预设按钮、编辑功能
