# 前端架构设计

> 🎯 **本文档定位**：Dioxus 前端架构与技术栈选型、业务域页面组织、DaisyUI 组件体系、HUD 可视化方案的整体设计大纲与演进记录；设计思路快照，组件层级与状态管理逻辑以实际代码为准。
> 状态：v2.1（2026-07-25 DaisyUI 迁移落地，2026-08-15 整理，2026-08-29 HUD 词汇全量收口）
> 查阅场景：需要理解前端技术选型、业务域与后端 Handler 对齐策略、DaisyUI 迁移路径、HUD Canvas 可视化边界时打开；组件 props 签名、API 客户端方法直接读代码。
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构与开发规范
> - [ui_design_system.md](./ui_design_system.md) — UI 设计系统（主题/色彩/组件规范）
> - [hud_design_prototype.html](./hud_design_prototype.html) — HUD 设计原型（未来感驾驶舱可视化基准：设计令牌 orz-hud/orz-light、切角发丝边面板、眉标/信号条等，对应 `components/hud.rs` + `styles/input.css` 的 `.hud-*` 皮肤）
> - 【② Plan 落地】[前端工具与进程管理.md](../plan/前端工具与进程管理.md) — Finance Tools 三 Tab 三视图实现
> - 【② Plan 落地】[前端API协议结构重构.md](../plan/前端API协议结构重构.md) — API 客户端 DTO re-export 自 common::api 规范
> - 【② Plan 落地】[聊天MVP.md](../plan/聊天MVP.md) — 对话页 SSE 订阅 + use_resource 资源加载模式
> - 【③ Wiki 百科】[前端应用.md](docs/wiki/zh/content/前端应用/前端应用.md) — 前端入口总览：WASM 编译 + Dioxus 运行时
> - 【③ Wiki 百科】[钩子系统.md](docs/wiki/zh/content/前端应用/组件系统/钩子系统.md) — use_resource/use_breakpoint/use_require_auth 三 Hooks 规范
> - 【④ RAG 知识卡】[前端整体架构](docs/wiki/knowledge/zh/前端整体架构：Dioxus%20Router%2041%20%E8%B7%AF%E7%94%B1%20+%20API%20%E5%AE%A2%E6%88%B7%E7%AB%AF%2013%20%E6%A8%A1%E5%9D%97%20+%20Hooks%203%E4%B8%AA%20+%20Store%202%E4%B8%AA%20+%20%E7%BB%84%E4%BB%B6%E4%BD%93%E7%B3%BB%206%E5%B1%82/前端整体架构：Dioxus%20Router%2041%20%E8%B7%AF%E7%94%B1%20+%20API%20%E5%AE%A2%E6%88%B7%E7%AB%AF%2013%20%E6%A8%A1%E5%9D%97%20+%20Hooks%203%E4%B8%AA%20+%20Store%202%E4%B8%AA%20+%20%E7%BB%84%E4%BB%B6%E4%BD%93%E7%B3%BB%206%E5%B1%82.md) — §41 路由表 + §组件 6 层单向引用 + §7 条红线

## 概述

AI Orz 前端基于 **Dioxus 0.7 (Rust WebAssembly)** 构建，采用 **Tailwind CSS 4 + DaisyUI 5** 组件库（自定义 HUD 主题 `orz-hud` / `orz-light`），支持 9 种主题切换，使用 Dioxus Router 路由、统一 API 客户端（HttpOnly Cookie 认证）和全局状态管理。前端按业务域组织页面模块，与后端 Handler 域对齐。

---

## 技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| 框架 | Dioxus 0.7 | Rust WebAssembly 前端框架 |
| 路由 | Dioxus Router | URL 路由 + Link 组件导航 |
| HTTP 客户端 | reqwest 0.13 | 全局 OnceLock 单例，复用连接池 |
| 样式框架 | Tailwind CSS 4.3 + DaisyUI 5.7 | 功能类优先 CSS + 组件库 |
| 自定义主题 | orz-hud / orz-light | 基于 HUD 设计系统（品牌橙 #fa520f + 信号黄 #ffd900）的暖色驾驶舱主题；默认 orz-hud，设置页可选 9 主题 |
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
├── index.html                # HTML 入口（无内联样式，所有样式由 styles/input.css 编译产出）
├── styles/
│   └── input.css             # Tailwind CSS 入口（@import tailwindcss + @plugin daisyui + @theme）
├── public/
│   └── output.css            # Tailwind 编译产物（由 build.rs 自动生成）
└── src/
    ├── main.rs               # 入口：Router + 全局状态注入 + 主题初始化
    ├── config.rs             # 前端运行时配置管理（localStorage 读写）
    ├── utils/                # 通用工具函数（按功能分子模块组织）
    │   ├── mod.rs            # 模块入口 + 重新导出所有公共 API（向后兼容 use crate::utils::xxx）
    │   ├── time.rs           # 时间格式化（format_time_hm、now_ms）
    │   ├── file.rs           # 文件大小格式化（format_file_size）
    │   ├── message.rs        # 消息辅助（类型常量、role_avatar/role_class、is_attachment_message、build_optimistic_user_msg、replace_tmp_with_real、tmp_msg_id）
    │   └── status.rs         # 任务/项目状态映射（project_status_text、task_status_text）
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
    │   ├── mod.rs            # use_breakpoint、use_require_auth、use_theme（8主题切换）、ThemeController
    │   └── use_resource.rs   # use_resource hook：三态资源加载（Loading/Ready/Failed）
    │
    ├── store/                # 全局状态管理
    │   ├── auth.rs           # 认证状态（AuthState + localStorage + HttpOnly Cookie）
    │   └── toast.rs          # Toast 状态（ToastState + show/dismiss/success/error/warning/info）
    │
    ├── components/           # 基础 UI 组件库
    │   ├── button.rs         # Button 组件（Primary/Accent/Secondary/Danger/Ghost + sm 尺寸，强制 hud-btn 皮肤）
    │   ├── modal.rs          # Modal 对话框组件（DaisyUI dialog/modal-box）
    │   ├── confirm_dialog.rs # 确认对话框组件（用于二次确认场景）
    │   ├── state.rs          # 状态展示组件（Loading/EmptyState/ErrorAlert/SuccessAlert）
    │   ├── stats.rs          # 统计面板组件（StatsCard/AgentStatsPanel/ProjectStatsPanel/TaskStatsPanel）
    │   ├── input.rs          # Input/Textarea/Select 表单组件
    │   ├── code_editor.rs    # 代码编辑器组件（支持 JSON 高亮和编辑）
    │   ├── toast.rs          # Toast 通知容器（DaisyUI toast + alert）
    │   ├── hud.rs            # HUD 设计系统原语（PageHeader/HudPanel/HudSection/HudCallout/HudTabs/HudTable/HudProgress/StatReadout/StatGrid）
    │   ├── chat/             # 聊天共享组件（跨页面复用）
    │   │   ├── mod.rs        # 模块入口，导出 MessageBubble + TypingIndicator
    │   │   ├── message_bubble.rs  # MessageBubble：单条消息气泡（文本/图片/文件，简版渲染）
    │   │   └── typing_indicator.rs # TypingIndicator：Agent 输入指示器（三点动画）
    │   ├── graph.rs          # 知识图谱 SVG 可视化组件（圆形布局、节点连接线、拖拽缩放）
    │   ├── graph_canvas.rs   # 知识图谱 Canvas HUD 驾驶舱风格渲染（深色径向渐变 + 节点呼吸光晕 + 边流光发光，KnowledgeGraphRenderer 实现 CanvasRenderer trait）
    │   ├── canvas_scene.rs   # Canvas 渲染基础设施（CanvasScene + CanvasRenderer trait + CanvasNode/CanvasEdge 数据结构）
    │   ├── force_layout.rs   # 力导向布局算法（用于 CanvasScene 默认布局）
    │   ├── layered_layout.rs # 分层布局算法（用于关系图分层渲染）
    │   ├── particles.rs      # Canvas 粒子效果（数据流光、呼吸光晕、背景粒子）
    │   ├── relation_graph.rs # 关系图组件（实体关系可视化）
    │   └── workspace_graph.rs # 工作台图谱组件（项目/Agent/任务关系可视化）
    │
    ├── layouts/              # 布局组件
    │   ├── navbar.rs         # 顶部导航栏（扁平 .navbar-link + .online-badge / 移动端抽屉）
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

### 1. 样式系统：Tailwind CSS 4 + DaisyUI 5 + HUD 皮肤层

**构建流程**：
- npm 安装 Tailwind CSS 4 和 DaisyUI 5
- `build.rs` 在编译时自动调用 `node_modules/.bin/tailwindcss` 编译 CSS
- `styles/input.css` 作为入口，引入 Tailwind 和 DaisyUI 插件，并定义 HUD 皮肤覆盖层（`.hud-*`、`.orz-tag`、`.navbar-link`、`.online-badge` 等）
- 编译产物输出到 `public/output.css`，由 Dioxus 自动打包

**自定义主题**：
- 默认主题 `orz-hud`（深色驾驶舱），浅色变体 `orz-light`
- 设计令牌见 [ui_design_system.md](./ui_design_system.md) §2
- 权威可视化基准：`docs/design/hud_design_prototype.html`

**主题切换**：
- `use_theme()` Hook 返回 `ThemeController`（Clone+Copy）
- 主题持久化到 localStorage（`ai_orz_theme` key）
- 通过 `document.documentElement.setAttribute("data-theme", theme)` 切换
- 设置页提供 8 种主题选择按钮
- 主题切换即时生效，无需刷新页面

**组件类规范**：DaisyUI 提供骨架类，HUD 皮肤层提供一致视觉。

| 组件 | 推荐类 / 组件 | 禁止 |
|------|---------------|------|
| 按钮 | `Button` 组件（自动带 `hud-btn`）或手动 `btn hud-btn btn-primary/secondary/error/ghost` | 裸 `btn btn-primary` / `btn btn-outline`（白边） |
| 卡片 / 面板 | `HudPanel` / `.hud-panel` | DaisyUI 原生 `card` 无切角 |
| 表格 | `.hud-table` / `.hud-row` | 裸 `table table-zebra` |
| 表单 | `.input.hud-input` / `.select.hud-input` / `.textarea.hud-input` | `input-bordered` 等 DaisyUI 默认边框 |
| 状态徽章 | `status.rs` helper：`agent_lifecycle_badge`、`task_status_badge`、`auth_state_badge` 等 | 裸 `badge badge-success` |
| 属性标签 | `tag_chip()` → `badge orz-tag badge-sm` | 散写 `badge badge-outline` / `badge badge-ghost` |
| 模态框 | `Modal` 组件 / `.hud-modal` / `.hud-modal-box` | 裸 `modal-box` |
| 提示 | `toast.success/error/warning/info(...)` + `HudCallout` | inline `alert-success` |
| 聊天气泡 | `chat chat-start/end` + `chat-bubble` | — |
| 头像 | `avatar placeholder` + `w-10 rounded-full bg-primary` | — |
| 加载 | `Loading { size: "lg" }` / `loading loading-spinner` | — |
| 分隔线 | `.hud-divider` | 裸 `divider` |
| 统计卡片 | `StatReadout` / `StatsCard` / `StatGrid` | 裸 DaisyUI `stat` |
| 导航栏 | `.navbar-link` 扁平链接 + `.online-badge` | `btn hud-btn btn-ghost btn-sm` 圆角按钮导航 |
| 下拉菜单 | `dropdown dropdown-end` + `menu` | — |
| Toast 容器 | `toast toast-top toast-end z-[9999]` | — |
| Tab 切换器 | `HudTabs` / 手动 `btn hud-btn btn-sm`（选中 `btn-primary`，未选 `btn-ghost`） | `tabs tabs-boxed` |

**自定义 CSS 范围**（`frontend/styles/input.css`）：
- HUD 主题变量与双主题色彩令牌
- `.hud-panel` / `.hud-row` / `.hud-table` / `.hud-divider` / `.hud-modal*` / `.hud-input`
- `.hud-btn` / `.hud-badge` / `.orz-tag` / `.navbar-link` / `.online-badge`
- 知识图谱 Canvas HUD 背景（`.kg-bg`）
- 工作台未读消息 HUD 流光条（`.hud-streak`）
- 滚动条美化
- 接待页品牌视觉、工具调用卡片、任务分配卡片、看板布局等 DaisyUI 未覆盖的局部样式

> 注：`frontend/index.html` 中不再保留 `<style>` 块；所有自定义样式统一收敛到 `styles/input.css`。

### 2. 路由系统（Dioxus Router）

使用 Dioxus 0.7 的 `Routable` derive 宏定义路由枚举，支持 URL 路由和 `Link` 组件导航。

### 3. 统一 API 客户端

- **全局 HTTP 客户端单例**：`OnceLock<Client>` 复用连接池
- **Cookie 认证**：基于 HttpOnly Cookie（JWT），浏览器自动携带
- **类型化 helper**：`api_get`/`api_post`/`api_put`/`api_delete`/`api_post_multipart` 等
- **错误处理**：`parse_api_error_from_body()` 和 `parse_error_response()` 辅助函数
- **URL 拼接 helper**：`build_pagination_url`（分页 query）、`build_query_string`（键值对 query），统一收敛在 `api/mod.rs`

#### API 方法签名约定

- **拆参数方法**（path + query + body 混合）：统一接受 `common::api::*Request` 协议结构体作为入参，URL 拼接逻辑由方法内部手工处理（用 `build_pagination_url` / `build_query_string` helper），调用方无需关心 path/query/body 分配
- **body-only 方法**：直接接受协议结构体作为 body（如 `send_message_to_agent(req)`）
- **单字段方法**（如 `delete_xxx(id)`、`pause_cron_trigger(id)`）：保持原始类型，不包一层 `DeleteXxxRequest`

改造原则：前后端 API 签名对称，协议结构体为 single source of truth。统计参数（`with_stats`/`with_model_call_stats`/`stats_interval` 等）已纳入对应 `GetXxxRequest`，废弃了原 `StatsOptions`。

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
| ConfirmDialog | `components/confirm_dialog.rs` | 二次确认对话框（用于删除、切换等危险操作） |
| Loading | `components/state.rs` | DaisyUI loading loading-spinner |
| EmptyState | `components/state.rs` | 空数据状态 |
| ErrorAlert | `components/state.rs` | DaisyUI alert alert-error |
| Input/Textarea/Select | `components/input.rs` | DaisyUI input/textarea/select input-bordered |
| CodeEditor | `components/code_editor.rs` | 代码编辑器（支持 JSON 高亮和编辑，用于 Action payload 编辑） |
| ToastContainer | `components/toast.rs` | DaisyUI toast + alert，自动消失动画 |
| MessageBubble | `components/chat/message_bubble.rs` | 单条消息气泡（文本/图片/文件），简版渲染，跨页面复用 |
| TypingIndicator | `components/chat/typing_indicator.rs` | Agent 输入指示器（三点动画） |
| Graph (SVG) | `components/graph.rs` | 知识图谱 SVG 可视化（圆形布局、节点连接线、拖拽缩放、搜索高亮） |
| KnowledgeGraphCanvas | `components/graph_canvas.rs` | 知识图谱 Canvas HUD 驾驶舱风格渲染（深色径向渐变背景 + 节点呼吸光晕 + 边流光发光，KnowledgeGraphRenderer 实现 CanvasRenderer trait） |
| CanvasScene | `components/canvas_scene.rs` | Canvas 渲染基础设施（CanvasRenderer trait + CanvasNode/CanvasEdge 数据结构 + 力导向布局 + 粒子效果） |
| RelationGraph | `components/relation_graph.rs` | 关系图组件（实体关系可视化，分层布局） |
| WorkspaceGraph | `components/workspace_graph.rs` | 工作台图谱组件（项目/Agent/任务关系可视化，用于 Workspace Dashboard） |

**聊天共享组件使用约定**（2026-07-25 新增）：
- 三处聊天实现：`pages/message/chat.rs`（主对话页，富渲染含工具卡片/任务卡片/视频/音频）、`pages/hr/agent_detail.rs`（Agent 详情页对话）、`pages/workspace.rs`（工作台底部对话框）
- 极简场景（Agent 详情页、Workspace 底部对话框）统一使用 `MessageBubble` + `TypingIndicator` 组件渲染消息
- 富渲染场景（主对话页）保留独立实现，因其含工具调用卡片、任务卡片、视频/音频附件等复杂内容，且使用 DaisyUI `chat chat-start/chat-end` 样式与 `MessageBubble` 的 `message-item` 样式不一致
- 共享 utils 中的 `build_optimistic_user_msg`、`replace_tmp_with_real`、`role_avatar`、`role_class`、`is_attachment_message`、消息类型常量等已在所有三处聊天实现中复用

### 6. 自定义 Hooks

| Hook | 说明 |
|------|------|
| `use_breakpoint()` | 返回 `Signal<bool>`（true = 移动端），基于 matchMedia 监听 |
| `use_require_auth()` | 权限守卫，未登录自动跳转 Reception 页 |
| `use_theme()` | 返回 ThemeController，支持 8 主题切换 + localStorage 持久化 |
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

> 相关实现细节见：[frontend 前端目录](frontend/src/)

### 开发命令

> 相关实现细节见：[frontend 前端目录](frontend/src/)

### 样式使用规范

1. **按钮一律经由 `Button` 组件或显式带 `hud-btn`**：禁止裸用 `btn btn-primary` / `btn btn-outline` / `btn btn-ghost`，否则会出现 DaisyUI 默认白边。
2. **徽章一律走单一事实源**：状态用 `status.rs` helper（`task_status_badge`、`agent_lifecycle_badge`、`auth_state_badge` 等）；属性/标签用 `tag_chip()`。
3. **面板 / 表格 / Tab 优先使用 HUD 原语**：`HudPanel` / `HudTable` / `HudTabs`，或手动使用 `.hud-panel` / `.hud-table` / `.hud-btn` 切换器。
4. **颜色使用主题变量**：`bg-primary`、`text-primary-content`、`bg-base-100`、`text-base-content`、`bg-base-200`、`border-base-300` 等；特殊 HUD 色值参考 `ui_design_system.md`。
5. **避免内联样式**：除动态计算值外，统一使用类名；新增自定义样式一律写入 `frontend/styles/input.css`。
6. **主题兼容**：使用语义化颜色类（`bg-primary`/`text-error`/`bg-success/20`），确保在 9 个主题下显示正常。
7. **字体规范**：标题/展示文字用 Chakra Petch（`--font-display`），数据/标签/数值用 JetBrains Mono（`--font-family-mono`），正文用 Sora 或默认 sans-serif。

### 新增页面流程

1. 在 `pages/` 对应业务域创建页面文件
2. 在 `pages/mod.rs` Route 枚举添加路由变体
3. 在导航栏 `layouts/navbar.rs` 添加导航链接
4. 在 `api/` 对应域添加 API 函数
5. 使用 DaisyUI 组件类 + Tailwind 工具类构建 UI

---

## 更新记录

### 2026-08-30 设计系统文档同步（以 hud_design_prototype.html 为基准）

| 变更项 | 实现细节 |
|--------|----------|
| **ui_design_system.md 重写** | 以 `docs/design/hud_design_prototype.html` 为唯一视觉基准，替换原 Mistral AI 风格描述；明确 orz-hud / orz-light 双主题令牌、字体系统、面板/导航/按钮/徽章/表格规范；落地位置与常见错误红线 |
| **frontend_architecture.md 更新** | 技术栈/概述改为 HUD 主题；样式系统章节明确 DaisyUI + Tailwind + HUD 皮肤层三层关系；组件类规范表改为「推荐类 / 禁止」形式；样式使用规范强调 `hud-btn` / `status.rs` helper / `tag_chip()` 强制规则；目录结构补充 `hud.rs`；新增本节更新记录 |
| **已知问题已修复** | 默认主题 `orz-hud` 已加入 `AVAILABLE_THEMES` 首项（共 9 主题），设置页可切回 HUD 深色；属性标签 `.orz-tag` 重设计为中性 HUD pill，状态徽章 `.hud-badge` 保留玻璃质感并统一 pill 圆角 |

### 2026-08-29 HUD 词汇全量收口（原语库 + 缺失原语 + 弱层统一 + 设计原型挂接）

| 变更项 | 实现细节 |
|--------|----------|
| **HUD 原语库定型** | `components/hud.rs` 沉淀 `PageHeader`/`HudPanel`/`HudSection`/`StatReadout`/`StatGrid`/`HudCallout`/`HudProgress`/`HudTable`/`HudTabs`/`HudDivider` 十个组件；`styles/input.css` 配套 `.hud-*` 皮肤（渐变发丝边 + 玻璃质感 + 等宽眉标，锚定品牌橙 `#fa520f`/信号黄 `#ffd900`）。`state.rs` 的 `ErrorAlert/SuccessAlert` 委托 `HudCallout`，`stats.rs` 的 `StatsCard` 委托 `StatReadout`、`StatsPanel` 用 `HudPanel+StatGrid` 包裹 |
| **补齐缺失原语** | 新增 `HudDivider`/`HudTable`/`HudTabs` 组件 + `.hud-divider`/`.hud-table`/`.hud-tabs`/`.hud-modal`/`.hud-modal-box`/`.hud-input` 皮肤；散落用法扫描收口：28 处 `table table-zebra`→`hud-table`、7 处 `tabs tabs-boxed`→`hud-tabs`、14 处裸 `divider`→`hud-divider`、4 处 `modal-box`→`hud-modal(-box)`、`input/select/textarea`→`hud-input`；`toast.rs` 单条通知由裸 `alert` 改为 `HudCallout`，闭合提示层矛盾 |
| **进度条 / 消息气泡统一** | 新增 `HudProgress` 原语 + `.hud-progress*` 皮肤，收口三套进度条实现（DaisyUI `progress` / `overview-progress` / `progress-cell` 共 9 处）；消息气泡改为 HUD 玻璃质感（布局机制不动） |
| **Badge / Button 弱层统一** | `input.css` 新增 `.hud-badge`/`.hud-btn` 皮肤；脚本扫描：99 处裸 `badge badge-*`→`hud-badge`、303 处裸 `btn`→`hud-btn`；2 处 `badge badge-neutral tag-item`→`tag_chip()`，`status.rs` 徽章单一事实源与 `button.rs` 薄壳未被改动 |
| **残留子卡收口** | 4 处 `card bg-base-200` 子卡（agent_detail×2 / knowledge_graph / task_detail）改为 `hud-panel` |
| **设计原型挂接** | `frontend/prototype-futuristic-hud.html` 移入 `docs/design/hud_design_prototype.html` 作为 HUD 词汇可视化基准，并在本文「关联文档」列明其对应 `components/hud.rs` + `styles/input.css` 的 `.hud-*` 皮肤 |
| **验证** | 前端门禁全绿：`cargo check`(wasm32) / `fmt` / `clippy -D warnings` / `test-fe`(122 passed) / Tailwind 编译（6 类新皮肤均进入产物）；提交链 `249d49c8`→`b19c50c4`→`001d716d`→`525e7e02`→`64e9764b` |

### 2026-07-25 知识图谱 Canvas HUD + Workspace 对话机制 + 聊天共享组件抽取 + utils 模块化

| 变更项 | 实现细节 |
|--------|----------|
| **知识图谱 Canvas HUD 驾驶舱风格** | 新增 `components/graph_canvas.rs`：`KnowledgeGraphRenderer` 实现 `CanvasRenderer` trait，HUD 风格渲染（深色径向渐变背景 + 淡橙色网格 + 四角 HUD 装饰；节点选中态扫描环 + 旋转刻度环，未选中态呼吸光晕；边实线流光 + drop-shadow 发光）；`KnowledgeGraphCanvas` 组件基于 `CanvasScene` 基础设施，关闭力导向布局和自带粒子避免视觉过载；通过 `sync_state` 同步外部状态（高亮、选中、边 label、节点元数据）；知识图谱页右上角 Canvas/SVG 风格切换按钮（join 按钮组），默认 Canvas，SVG 作为兜底 |
| **web-sys features 扩展** | `frontend/Cargo.toml` 添加 `CanvasGradient` feature，支持 Canvas 渲染中的渐变效果 |
| **Workspace 对话机制** | 底部对话框跟随当前视图（默认/Project/Agent），SSE 实时消息，自动未读消息源追踪（`project_unread`/`agent_unread` Signal<HashSet<String>>）；点击侧边栏红点切换视图并清除 |
| **Workspace HUD 流光提示** | 未读消息提示由静态红点升级为 2px 橙色竖条贴在侧边栏项左侧边缘，带 `box-shadow` 形成 glow 光晕，高亮段从上往下流动（1.8s 周期，cubic-bezier(0.4, 0, 0.6, 1) 缓动），像 HUD 扫描线；`styles/input.css` 新增 `@keyframes hud-streak-flow` 动画和 `.hud-streak` 类 |
| **聊天共享组件抽取** | 新增 `components/chat/` 模块：`MessageBubble`（单条消息气泡，文本/图片/文件简版渲染）、`TypingIndicator`（Agent 输入指示器，三点动画）。Agent 详情页、Workspace 底部对话框改用共享组件，删除本地 `render_message_content`/`render_chat_messages` 重复实现 |
| **主对话页保留独立实现** | `pages/message/chat.rs` 含工具调用卡片、任务卡片、视频/音频附件等复杂内容，且使用 DaisyUI `chat chat-start/chat-end` 样式，与 `MessageBubble` 的 `message-item` 样式不一致，简单 `MessageBubble` 无法覆盖，保留独立富渲染实现 |
| **utils 文件夹化** | 原 `src/utils.rs` 拆分为 `src/utils/` 文件夹，按功能分子模块组织：`time.rs`（时间格式化）、`file.rs`（文件大小格式化）、`message.rs`（消息类型常量、角色映射、乐观消息辅助）、`status.rs`（任务/项目状态映射）。`mod.rs` 通过 `pub use` 重新导出所有公共 API，保持 `use crate::utils::xxx` 向后兼容，无需改动调用方 |
| **DTO PartialEq 派生** | `common::api::MessageListItem` 和 `FileMetaInfo` 添加 `PartialEq` 派生（Dioxus 0.7 组件 prop 要求实现 `PartialEq`，否则 `#[component]` 宏报错 `E0369`） |
| **Canvas 基础设施复用** | 知识图谱 Canvas 版本复用 `CanvasScene` + `CanvasRenderer` trait + `CanvasNode`/`CanvasEdge` 数据结构 + 力导向布局 + 粒子效果基础设施，避免重复造轮子 |
| **测试统计** | 前端 34 测试 + 后端 746 测试 + common 50 测试 100% 通过 |

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

---

## 五、扩展模式

### 5.1 新增业务域页面模块
前端页面按业务域与后端 Handler 域对齐。新增一个业务域时：
1. 在 `frontend/src/pages/` 下新建目录，复制现有业务域模板，参考：[pages 目录](frontend/src/pages)
2. 在 `frontend/src/api/` 下新增对应域的 API client 模块，复用 `common::api::*` DTO 类型，参考：[api 目录](frontend/src/api)
3. 在 Dioxus Router 配置中注册路由，入口参考：[main.rs 路由注册](frontend/src/main.rs)

### 5.2 新增 HUD Canvas 可视化图表类型
现有 GraphCanvas 支持知识图谱等可视化。如果未来新增图表类型：
1. 优先复用 HUD Canvas 渲染基础设施，不单独开独立 canvas 实例，参考：[components/graph_canvas.rs](frontend/src/components/graph_canvas.rs)
2. 数据模型复用通用 `GraphNode` / `GraphEdge` 结构；若新增字段保持向后兼容（Option 字段），参考：[components/graph.rs](frontend/src/components/graph.rs)
