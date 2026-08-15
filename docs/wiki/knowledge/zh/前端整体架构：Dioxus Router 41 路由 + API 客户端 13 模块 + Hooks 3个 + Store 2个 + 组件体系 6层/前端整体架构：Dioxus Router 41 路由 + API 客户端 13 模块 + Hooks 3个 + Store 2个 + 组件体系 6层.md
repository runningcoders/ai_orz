---
kind: RAG 原子知识卡
name: 前端整体架构：Dioxus Router 41 路由 + API 客户端 13 模块 + Hooks 3个 + Store 2个 + 组件体系 6层
category: 前端应用 / 架构总览
scope:
  - "frontend/src/api/**"
  - "frontend/src/components/**"
  - "frontend/src/hooks/**"
  - "frontend/src/layouts/**"
  - "frontend/src/pages/**"
  - "frontend/src/store/**"
  - "frontend/src/utils/**"
  - "frontend/src/main.rs"
  - "frontend/src/config.rs"
source_files:
  - frontend/src/pages/mod.rs#L61-L163 (Dioxus Router enum Route：41 条路由 = 登录/对话 2 + 组织 2 + HR 5 + Finance 11 + Project 6 + System 9 + 用户 1 + 工作台 1 + 设置 1；Dioxus 0.7 派生宏 #[route("/path")])
  - frontend/src/api/mod.rs (API 客户端聚合 13 子模块：auth/background_task/finance/github/hr/lark/log_stats/message/organization/project/seed/system；统一封装 Result<ApiResponse<T>> + JWT 自动携带 Cookie)
  - frontend/src/hooks/use_resource.rs (use_resource：通用异步资源 Hook，参数 FnOnce() -> Result<T> + Vec<Dep>；Dioxus use_future + use_signal 包装；use_breakpoint 断点 Hook 用于响应式)
  - frontend/src/store/auth.rs (AuthStore：全局单例 use_shared_state<AuthState>；字段 token/user_id/username/organization_id/role；login() 写 store + 触发全局 rerender；logout() 清空 store + Cookie 删除)
  - frontend/src/store/toast.rs (ToastStore：全局消息队列，push_info/push_success/push_warn/push_error 四 API；顶部 HUD 层自动渲染，5s 自动消失，点击关闭)
  - frontend/src/layouts/app_layout.rs (AppLayout：顶层布局 = Navbar 顶部导航 + Sidebar 左侧菜单(6大域Tab) + Main 内容区 + Toast 容器 + HUD 未读橙光条；use_require_auth 守卫重定向 /login)
  - frontend/src/components/mod.rs (组件体系 6 层：1.基础 Button/Modal/Toast/Confirm/Markdown/CodeEditor 2.状态 State 3.统计 Stats 4.图表 Charts(Line/Donut) 5.业务 Chat(气泡/侧栏/打字)/GraphCanvas/Gauge/RuntimePanel 6.复合 KanbanCanvas/WorkspaceGraph)
  - frontend/src/utils/time.rs (utils 5 子模块：time 格式化时间戳/ message 截断字数+Markdown预览/ status 枚举转中文标签/ file 字节大小转MB格式化；禁止跨 utils 互引)
  - docs/design/frontend_architecture.md（§Dioxus WebAssembly 渲染管线 §41条路由表 §API客户端分层 + 拦截器 §响应式原则 + 状态只读）
  - docs/design/ui_design_system.md（§HUD 驾驶舱风格 §DaisyUI 30+主题枚举 §Tailwind v4 @theme 自定义类 §橙光光晕 .hud-glow CSS）
  - docs/plan/前端工具与进程管理.md（§Finance Tools Tab 三视图 Builtin/HTTP/MCP §进程列表 Modal 复用 §前端 DTO re-export 自 common::api）
  - docs/plan/前端API协议结构重构.md（§frontend/src/api/*.rs 本地 struct 清理 §pub use common::api::* 统一入口 §DTO 漂移检查）
  - docs/plan/聊天MVP.md（§对话页 SSE 订阅 §use_resource 拉 message 列表 §ChatSidePanel 侧栏项目信息）
  - docs/wiki/zh/content/前端应用/前端应用.md（前端入口总览：WASM 编译 + Dioxus 运行时 + build.rs 自动 npm install + Tailwind v4 实时编译）
  - docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/HR 管理页面.md（HR 域 5 页面：Agent 列表/详情 + Skill 列表/详情 + 记忆搜索 + 知识图谱 Canvas）
  - docs/wiki/zh/content/前端应用/组件系统/钩子系统.md（use_resource/use_breakpoint/use_require_auth 三 Hooks 设计模式 + 依赖数组触发条件）
  - docs/wiki/zh/content/前端应用/UI 样式与主题.md（DaisyUI 30+ 主题：dark/light/cyberpunk/synthwave/coffee；theme=localStorage 持久化 + 用户偏好自报同步）
  - 【平行卡 1】docs/wiki/knowledge/zh/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格/Tailwind CSS v4 + DaisyUI v5 主题系统与 HUD 驾驶舱风格.md（样式系统：Tailwind v4 @theme + DaisyUI 组件类 + .hud-glow 橙光 + 30+ 主题切换）
  - 【平行卡 2】docs/wiki/knowledge/zh/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE.md（DTO 协议：前端 API client 通过 common::api re-export，禁止本地定义重复 DTO）
---

## §1 概述

**本卡角色**：Dioxus 0.7 前端整体架构知识卡。覆盖 41 条 Dioxus Router 路由、13 模块 API 客户端分层、3 个核心 Hooks（use_resource/use_breakpoint/use_require_auth）、2 个全局 Store（Auth/Toast）、6 层组件体系（基础→状态→统计→图表→业务→复合）、AppLayout 布局守卫。**定位：新增页面路由、排查页面 404、调试 use_resource 不刷新、理解 API 拦截器携带 JWT 时读。**

- **41 条路由分布（7 大域 + 4 单页）**（pages/mod.rs Route enum）：① 登录 Reception + 对话 2（Chat/Search）= 3 条；② 组织域 Organization 2（Info/Users）；③ HR 域 5（AgentList/Detail + SkillList/Detail + MemorySearch + KnowledgeGraph）；④ Finance 域 11（ModelProviders 2 + Tools 3 + Identity + MessageChannels 2 + McpServers 2 + Attachments 2）；⑤ Project 域 6（List/Detail + Artifacts 2 + TaskList/Detail）；⑥ System 域 9（Triggers/Health/Docs/Logs/Backup/Processes/Aop/Seed/Tasks）；⑦ 用户 Profile + Workspace 工作台 + Settings 设置 = 3 条。前台无 token 访问受保护路由 → use_require_auth Hook 自动 redirect /login。
- **API 客户端 13 模块分层（禁止直接在页面组件里写 reqwest）**（api/ 目录）：每个域一个文件（hr.rs / finance.rs / project.rs 等），内部 use `api_client()` 获取配置了 base_url + JWT 的 Client；调用方式：`hr::list_agents(ctx, pagination).await` 返回 `Result<ApiResponse<PagedResult<AgentDto>>>`；DTO 类型全部 `pub use common::api::AgentDto`，不在前端本地 struct 重复定义。API 失败 401 统一由 Axum 拦截器处理 → 前端 `result.map_err(|_| AuthStore.logout())`。
- **组件体系 6 层单向引用（下层不允许 use 上层）**（components/）：Layer1 基础（Button/Modal/Toast/ConfirmDialog/Markdown/CodeEditor/SearchableSelect/ProcessDetail）；Layer2 状态展示（StateTag/TaskProgress）；Layer3 统计（StatsSummary/StatsOverviewCard）；Layer4 图表（charts/LineChart/DonutChart + ChartScene + Gauge/AopGauge）；Layer5 业务组件（chat/MessageBubble/TypingIndicator/ChatSidePanel/ToolCallsTab + RuntimePanel + GraphCanvas + ArtifactMetaModal + CreateHttpTool）；Layer6 复合 Canvas（CanvasScene + ForceLayout + LayeredLayout + RelationGraph + WorkspaceGraph + KanbanCanvas + Particles + HudPalette）。跨层引用只能上层 import 下层，循环依赖会触发 Rust 编译器错误。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| pages/mod.rs Route enum | 路由表 41 条 | Dioxus 0.7 `#[derive(Routable, Clone, PartialEq)]`；每个变体一个 #[route(path)]；未匹配路径走 "/" 默认（聊天页） | `:L61-L163` |
| api/mod.rs 聚合 | 13 域客户端 | `pub mod auth; pub mod hr; ...` 统一 re-export；`fn api_client()` → reqwest ClientBuilder + default_headers(Cookie: ai_orz_token=JWT) + 超时 30s | 见 mod.rs pub mod |
| hooks/use_resource.rs | 通用异步资源 Hook | `pub fn use_resource<T: Clone + 'static, D>(f: impl Fn() -> Result<T> + 'static, deps: D) -> Resource<T>` 内部：use_signal(None) 存数据 + use_future(deps) 跑 future → ready 写 signal；空值显示 spinner 占位 | 见 use_resource fn |
| store/auth.rs AuthStore | 登录态全局单例 | `use_shared_state::<AuthState>` 跨组件共享；`fn login(claims)` → write + persist to localStorage；`fn logout()` → 清空 state + remove Cookie + redirect /login；401 自动走 logout | 见 AuthState struct |
| store/toast.rs ToastStore | 消息提醒队列 | `fn push_success(msg: &str)` → append to Vec<ToastItem{id, kind, text, created_at}>；顶部 HUD Toast 容器 5s 后 remove；点击 × 立即移除；超过 5 条自动 drop 最旧 | 见 ToastItem struct |
| layouts/app_layout.rs | 顶层布局 + 守卫 | 首行 `let _ = use_require_auth().read().redirect_if_unauthenticated();` → 无 token 302 /login；内部 rsx!(Navbar { }, Sidebar { routes }, Outlet::<Route> {}, ToastContainer { }) 四部分 | 见 AppLayout fn |
| components/mod.rs 6 层导出 | 组件聚合 | pub use button::Button; pub use modal::Modal; pub use stats::StatsSummary; pub use charts::LineChart; pub use chat::{MessageBubble, ChatSidePanel}; pub use graph_canvas::GraphCanvas；顺序严格按依赖层排列 | `见 pub use 顺序` |
| utils/time.rs 等 5 子模块 | 纯函数工具 | `format_timestamp(ts: i64) -> String` 相对时间 "3分钟前" / 绝对 "2026-07-12 09:30"；`truncate_markdown(text, 200)` 中文截断 + 加省略号；`status_label(TaskStatus::Running)` → `<span class="badge badge-info">运行中</span>` | 见 utils/mod.rs pub use |

**章节来源**
- [pages/mod.rs:L61-L163](frontend/src/pages/mod.rs#L61-L163)
- [frontend_architecture.md:L1-L50](docs/design/frontend_architecture.md#L1-L50)
- [store/auth.rs](frontend/src/store/auth.rs)

---

## §3 页面加载到首屏渲染 6 步链路

```
浏览器加载 wasm → dioxus_web::launch(App)
  ↓
1. main.rs App 组件初始化
   → config.rs 读 BASE_URL（dev=http://localhost:8080，prod 同源）
   → AuthStore 从 localStorage 恢复 token，写全局单例

2. use_require_auth 守卫（AppLayout 第一行）
   → 读 AuthStore.token：
      - None → navigator().push(Route::Reception {}) → 渲染登录页
      - Some → 继续渲染主布局

3. Dioxus Router 解析路径 → 匹配 Route 变体
   例：/hr/agents/ag_123 → Route::HrAgentDetail { id: "ag_123" }
   → Outlet::<Route> 渲染对应 page 组件

4. page 组件首次渲染：
   → use_resource(deps: (id,), || async move {
        hr::get_agent_detail(id).await.map(|r| r.data)
     })
   → 返回 Resource::Loading：显示 <div class="loading loading-spinner">

5. use_future 内 reqwest 发 HTTP：
   → POST /api/v1/hr/agents/ag_123
   → Headers: Cookie: ai_orz_token=JWT（api_client() 自动注入）
   → 30s 超时 / 401 → AuthStore.logout() 自动跳转

6. Result Ok → use_signal 写数据 → 组件自动 rerender
   → rsx!(
        div.card { Agent 名字 }
        MessageBubble { msg: last_message }
        RuntimePanel { agent_id: id }  // 思考运行时面板（独立 Canvas 轮询）
     )
```

---

## §4 硬约束与回归红线（7 条）

1. **所有页面首行必须 use_require_auth（/login 除外）**：漏掉守卫会导致未登录用户看到空白页（拿不到数据但也不跳登录）；测试渲染 /hr/agents 不带 token 必须 302 → /login。
2. **API DTO 一律 pub use common::api::**：前端 `frontend/src/api/` 下写 `pub struct AgentDto` = 违反；clippy unused import 允许删除无用 re-export，但不允许本地新 struct；新增字段统一先改 common/src/api/*.rs 再重新编译前端（因为是 workspace 共享 crate，自动触发 frontend rebuild）。
3. **组件引用严格单向 6 层：上层用下层，下层不能 use 上层**：`components/charts/line_chart.rs` 里 `use super::chat::MessageBubble` = 违反；正确做法：公共依赖下沉到 Layer1-3（例：图表要展示 Tooltip 用 Modal，Modal 在 Layer1，两层都可以 use）。
4. **use_resource 的 deps 必须 Clone + PartialEq**：deps 传不可克隆对象（例如 reqwest Client）→ 编译失败；推荐 deps 用 (id_string.clone(), page_num) 这类纯值元组，避免 deps 每次 render 都变化造成 infinite reload loop。
5. **AuthStore 和 ToastStore 必须是全局单例（use_shared_state），不能传 props 穿透 10 层**：代码里 `rsx!(Child { auth: auth.clone() })` 传 auth = 违反；所有子组件直接 `let auth = use_shared_state::<AuthState>().read();` 读取。若需要测试隔离改用 `provide_context` 覆盖。
6. **AppLayout 不能做业务级数据请求（懒加载）**：AppLayout 拿用户权限菜单是 OK 的（一次性），但不能每 3 秒拉系统状态塞全局（会让所有页面 rerender 性能炸）；系统监控类轮询放到 `/system/health` 页面自己的 use_future 里。
7. **41 条路由变体名与目录一一对应（命名一致性）**：`Route::FinanceAttachmentDetail { id }` → 对应文件 `pages/finance/attachment_detail.rs`，struct 名 `AttachmentDetail`；路由枚举值命名必须 "域+页面+变体"，新增页面必须先追加 Route enum → 再创建对应 pages/{domain}/{page_snake}.rs 文件，顺序反了 Router 找不到。
