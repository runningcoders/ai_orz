# 前端架构设计

> 最后更新：2026-07-19

## 概述

AI Orz 前端基于 **Dioxus 0.7 (Rust WebAssembly)** 构建，采用全局 CSS 设计系统（Mistral 暖色调）、Dioxus Router 路由、统一 API 客户端（JWT 注入）和全局认证状态管理。前端按业务域组织页面模块，与后端 Handler 域对齐。

---

## 技术栈

| 组件 | 技术 | 说明 |
|------|------|------|
| 框架 | Dioxus 0.7 | Rust WebAssembly 前端框架 |
| 路由 | Dioxus Router | URL 路由 + Link 组件导航 |
| HTTP 客户端 | reqwest 0.13 | 全局 OnceLock 单例，复用连接池 |
| 设计系统 | 纯 CSS 变量 + 组件类 | Mistral 暖色调，注入 index.html |
| 状态管理 | Dioxus Signal + use_context_provider | 全局 AuthState 共享 |
| 共享类型 | common crate | 与后端共享 DTO、枚举、常量 |
| 配置嵌入 | build.rs | 编译时读取后端 ai_orz.toml 嵌入前端 |

---

## 目录结构

```
frontend/
├── Cargo.toml                # 依赖配置（dioxus 0.7 + router feature）
├── index.html                # 全局 CSS 设计系统（Mistral 暖色调变量 + 组件类 + 移动端适配）
├── build.rs                  # 编译时配置嵌入（保持不变）
└── src/
    ├── main.rs               # 入口：Router 配置 + 路由组件渲染入口
    ├── config.rs             # 前端运行时配置管理（localStorage 读写）
    │
    ├── api/                  # API 客户端层
    │   ├── mod.rs            # 统一 HTTP 客户端 client()、JWT 注入、api_get/api_post/api_put/api_delete helper
    │   ├── auth.rs           # 认证 API（check_initialized/initialize_system/login/logout）
    │   ├── organization.rs   # 组织管理 API（组织 CRUD、用户管理）
    │   ├── hr.rs             # HR 域 API（agent/skill/tool-pack/skill-pack）
    │   ├── finance.rs        # Finance 域 API（model_provider/tool/message_channel）
    │   ├── project.rs        # Project 域 API（project/task）
    │   ├── message.rs        # Message 域 API（消息发送）
    │   └── system.rs         # System 域 API（health/cron_trigger）
    │
    ├── hooks/                # 自定义 Hooks
    │   └── mod.rs            # use_breakpoint：基于 matchMedia 监听移动端（≤768px）
    │
    ├── store/                # 全局状态管理
    │   ├── mod.rs
    │   └── auth.rs           # 认证状态（AuthState + token localStorage 持久化）
    │
    ├── components/           # 基础 UI 组件库
    │   ├── mod.rs
    │   ├── button.rs         # Button 组件（5 种 variant: Primary/Accent/Secondary/Danger/Ghost）
    │   ├── modal.rs          # Modal 对话框组件
    │   ├── state.rs          # 状态展示组件（Loading/EmptyState/ErrorAlert/SuccessAlert）
    │   ├── stats.rs          # 统计面板组件（StatsCard/AgentStatsPanel/ProjectStatsPanel/TaskStatsPanel）
    │   ├── toast.rs          # Toast 通知组件
    │   └── graph.rs          # 知识图谱 SVG 可视化组件
    │
    ├── layouts/              # 布局组件
    │   ├── mod.rs
    │   ├── navbar.rs         # 顶部导航栏（桌面 5 下拉菜单 / 移动端汉堡抽屉）
    │   └── app_layout.rs     # 应用布局（Navbar + 内容区）
    │
    └── pages/                # 页面模块（按业务域分组）
        ├── mod.rs            # Route 枚举定义（21 条路由）
        ├── reception.rs      # 前台接待（登录/初始化闭环，375px 极小屏适配）
        ├── settings.rs       # 系统设置（API 地址配置）
        │
        ├── organization/     # 组织模块
        │   ├── info.rs       # 组织信息管理
        │   └── users.rs      # 用户管理
        │
        ├── hr/               # HR 模块
        │   ├── agents.rs     # Agent 管理列表（类型徽章 + 本地/外部 Agent 创建入口）
        │   ├── agent_detail.rs  # Agent 详情（类型标签 + 外部 Agent 运行时配置展示）
        │   ├── skills.rs     # 技能库管理
        │   ├── memory_search.rs  # 记忆搜索
        │   └── knowledge_graph.rs # 知识图谱
        │
        ├── finance/          # Finance 模块
        │   ├── model_providers.rs  # 模型提供商管理
        │   ├── tools.rs      # 工具管理
        │   ├── message_channels.rs  # 消息渠道管理
        │   ├── attachments.rs # 附件管理
        │   └── mcp_servers.rs # MCP 服务器管理
        │
        ├── project/          # Project 模块
        │   ├── projects.rs   # 项目列表
        │   ├── project_detail.rs  # 项目详情
        │   ├── tasks.rs      # 任务管理（看板/列表双视图）
        │   └── artifacts.rs  # 项目产物
        │
        ├── message/          # Message 模块
        │   ├── chat.rs       # 消息对话（桌面双栏 / 移动端单栏覆盖式 sidebar）
        │   └── search.rs     # 消息搜索
        │
        ├── system/           # System 模块
        │   ├── triggers.rs   # 定时触发器管理（列表+创建/编辑弹窗+Action模板+Cron预设）
        │   ├── health.rs     # 健康检查
        │   ├── logs.rs       # 日志查询
        │   └── backup.rs     # 备份管理
        │
        └── user/             # 用户模块
            └── profile.rs    # 个人信息
```

---

## 核心架构设计

### 1. 路由系统（Dioxus Router）

使用 Dioxus 0.7 的 `Routable` derive 宏定义路由枚举，支持 URL 路由和 `Link` 组件导航。

```rust
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[route("/")]
    Reception {},

    #[route("/organization")]
    OrganizationInfo {},
    #[route("/organization/users")]
    OrganizationUsers {},

    #[route("/hr/agents")]
    HrAgents {},
    #[route("/hr/agents/:id")]
    HrAgentDetail { id: String },
    // ... 15 条路由
}
```

**入口配置**（main.rs）：
```rust
fn App() -> Element {
    use_context_provider(|| Signal::new(AuthState::restore()));
    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        Router::<Route> {}
    }
}
```

### 2. 全局 CSS 设计系统（Mistral 暖色调）

CSS 变量和组件类注入 `index.html` 的 `<style>` 标签，全局可用。

**CSS 变量分类：**
- **品牌色**：mistral-orange (#fa520f)、mistral-flame (#fb6424)、sunshine 系列
- **表面色**：warm-ivory (#fffaeb)、cream (#fff0c2)、pure-white、mistral-black
- **语义色**：success/warning/error/info（含 bg 变体）
- **文字色**：primary/secondary/muted/on-dark
- **边框色**：border/border-light/border-focus
- **排版**：font-family、font-mono
- **间距**：space-1 到 space-12（4px 基准）
- **圆角**：radius-sm/md/lg/xl
- **阴影**：shadow-sm/md/lg/xl（暖金色阴影系统）
- **布局**：navbar-height、max-width
- **响应式断点**：`--breakpoint-sm` (640px)、`--breakpoint-md` (768px，移动/桌面分界点)、`--breakpoint-lg` (1024px)

**组件类清单：**
- 布局工具：`.app-container`、`.content-area`、`.flex`、`.flex-col`、`.items-center`、`.justify-between`、`.gap-*`、`.w-full`
- Navbar：`.navbar`、`.navbar-brand`、`.navbar-section`、`.navbar-item`、`.navbar-dropdown`、`.navbar-dropdown-item`
  - 移动端新增：`.navbar-mobile-toggle`（汉堡按钮）、`.navbar-desktop-only`（桌面端专属容器）、`.navbar-drawer` / `.navbar-drawer.open`（左侧抽屉）、`.navbar-overlay`（遮罩）、`.navbar-drawer-item`、`.navbar-drawer-section`、`.navbar-drawer-divider`
  - Chat 移动端：`.chat-sidebar.open`、`.chat-mobile-back`（chat-header 左侧返回按钮，桌面端隐藏）
- Button：`.btn` + 5 种 variant（`.btn-primary`/`.btn-accent`/`.btn-secondary`/`.btn-danger`/`.btn-ghost`）+ 尺寸（`.btn-sm`/`.btn-lg`）
- Card：`.card`、`.card-header`、`.card-title`
- Table：`.table`、`.table th`、`.table td`
- Form：`.form-group`、`.form-label`、`.form-input`/`.form-textarea`/`.form-select`、`.form-hint`
- Badge：`.badge` + 5 种语义（`.badge-success`/`.badge-warning`/`.badge-error`/`.badge-info`/`.badge-neutral`）
- Modal：`.modal-overlay`、`.modal-content`、`.modal-header`、`.modal-title`、`.modal-close`、`.modal-footer`
- Alert：`.alert` + 4 种语义（`.alert-error`/`.alert-success`/`.alert-warning`/`.alert-info`）
- 状态指示：`.state-loading`、`.state-empty`、`.state-empty-icon`

### 3. 统一 API 客户端

**全局 HTTP 客户端单例**（复用连接池）：
```rust
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

pub fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| Client::new())
}
```

**JWT 自动注入**：从 localStorage 读取 token，通过 `bearer_auth` 自动注入到所有请求。

**类型化 helper 函数**：
- `api_get<T>(path)` - GET 请求，返回 `ApiResponse<T>`
- `api_get_or_default<T: Default>(path)` - GET 请求，空数据返回默认值
- `api_post<T, B>(path, body)` - POST 请求，返回 `ApiResponse<T>`
- `api_post_empty<B>(path, body)` - POST 请求，无响应体
- `api_put<T, B>(path, body)` - PUT 请求
- `api_put_empty<B>(path, body)` - PUT 请求，无响应体
- `api_delete(path)` - DELETE 请求
- `api_get_text(path)` - GET 请求，返回纯文本（用于 /health）

**业务域 API 客户端**（7 个域）：
| 模块 | 覆盖功能 |
|------|---------|
| `api/auth.rs` | check_initialized、initialize_system、login、logout |
| `api/organization.rs` | 组织 CRUD、用户管理、组织信息查询 |
| `api/hr.rs` | Agent CRUD（支持统计参数）、外部 Agent 创建（CLI/Remote）、工具包/技能包管理、技能库 |
| `api/finance.rs` | 模型提供商（支持统计参数）、工具（支持统计参数）、消息渠道 |
| `api/project.rs` | 项目（支持统计参数）、任务管理（支持统计参数） |
| `api/message.rs` | 消息发送 |
| `api/system.rs` | 健康检查、定时触发器 |

**统计参数支持**：`StatsOptions` 结构体统一封装统计查询参数（`with_stats`/`with_model_call_stats`/`stats_interval`），通过 `build_url_with_stats` 函数拼接 URL，各实体详情 API 函数通过 `stats_options: Option<&StatsOptions>` 参数按需传入。

### 4. 全局认证状态管理

```rust
#[derive(Clone, Debug, Default)]
pub struct AuthState {
    pub token: Option<String>,
    pub username: String,
    pub role: i32,
    pub org_id: String,
    pub org_name: String,
}
```

- **初始化**：在 App 根组件通过 `use_context_provider` 注入
- **Token 持久化**：保存到 localStorage（key: `ai_orz_token`）
- **状态恢复**：页面刷新时从 localStorage 恢复 token
- **登录闭环**：Reception 页面登录成功 → `save_token()` → 更新 AuthState → 跳转首页

### 5. 基础 UI 组件库

| 组件 | 文件 | 说明 |
|------|------|------|
| Button | `components/button.rs` | 5 种 variant + sm/lg 尺寸 |
| Modal | `components/modal.rs` | 对话框（遮罩点击关闭、内容区阻止冒泡） |
| Loading | `components/state.rs` | 加载中状态 |
| EmptyState | `components/state.rs` | 空数据状态 |
| ErrorAlert | `components/state.rs` | 错误提示 |
| SuccessAlert | `components/state.rs` | 成功提示 |
| StatsCard | `components/stats.rs` | 统计卡片（图标 + 标题 + 数值 + 副标题） |
| AgentStatsPanel | `components/stats.rs` | Agent 统计面板（唤醒次数、QPS、模型调用、Token） |
| ProjectStatsPanel | `components/stats.rs` | 项目统计面板（事件次数、QPS、模型调用、Token） |
| TaskStatsPanel | `components/stats.rs` | 任务统计面板（事件次数、QPS、模型调用、Token） |

### 6. 布局组件

- **Navbar**：顶部导航栏
  - **桌面端（≥769px）**：5 个下拉菜单（人力资源/财务管理/项目管理/系统管理/用户菜单），使用 `Link<Route>` 导航
  - **移动端（≤768px）**：隐藏桌面菜单，显示汉堡按钮；点击展开左侧抽屉（`.navbar-drawer`）+ 半透明遮罩（`.navbar-overlay`），按"导航/人力资源/财务管理/项目管理/系统/账户"分组垂直排列所有路由项；点击任意导航项后自动关闭抽屉，点击遮罩同样关闭
  - 响应式切换由 `use_breakpoint()` Hook（基于 `window.matchMedia("(max-width: 768px)")`）驱动
- **AppLayout**：应用布局包装器，组合 Navbar + children 内容区

### 7. 响应式与移动端适配

**断点系统**：所有 `@media` 查询统一使用 `:root` 的 `--breakpoint-*` 变量，避免硬编码：
- `--breakpoint-sm: 640px`：表格卡片化分界点
- `--breakpoint-md: 768px`：移动端 / 桌面端主分界点
- `--breakpoint-lg: 1024px`：大屏分界点

**`use_breakpoint` Hook**（`hooks/mod.rs`）：
- 基于 `window.matchMedia("(max-width: 768px)")` 监听，窗口尺寸变化时自动更新
- 通过 `use_context_provider` 在根组件注入，全局共享同一信号与监听器
- 仅在需要切换组件结构时使用（Navbar 抽屉、Chat 单栏），其余适配全部由 CSS 接管

**移动端适配策略**（CSS 优先，零 JS）：

| 适配项 | 实现方式 | 触发断点 |
|--------|----------|----------|
| Navbar 汉堡菜单 + 抽屉 | `use_breakpoint` + 抽屉组件 | ≤768px |
| Chat 单栏覆盖式 sidebar | `use_breakpoint` + `sidebar_open` 信号 + CSS transform | ≤768px |
| 表格 → 卡片列表 | CSS `@media`：thead 隐藏、tr 转卡片、td 转 flex 行、`::before` 显示 `data-label` | ≤640px |
| Modal 全屏化 | CSS：`.modal-content` 100vw/100vh、圆角 0、底部按钮纵向 | ≤640px |
| Toast 横向占满 | CSS：`.toast-container` left/right 12px | ≤640px |
| 网格降列 | CSS：`.overview-stats` 4→2→1 列、`.detail-grid`/`.stats-grid` 1 列 | ≤768px / ≤480px |
| 看板纵向堆叠 | CSS：`.kanban-board` flex-direction column | ≤768px |
| 筛选行/卡片头部纵向 | CSS：`.filter-row`、`.card-header` column | ≤768px |
| 触摸优化 | CSS：按钮最小 40px、navbar 44px、`-webkit-tap-highlight-color` | ≤768px |
| 输入框防 iOS 放大 | CSS：`.form-input`/`.chat-input` font-size 16px | ≤768px |
| hover 降级 | CSS：`.message-item .message-actions` opacity 1 | ≤768px |

**data-label 属性**：所有表格 `<td>` 元素添加 `data-label="字段名"` 属性（与 `<th>` 文本一致），桌面端无视觉影响，移动端通过 CSS `::before` 显示为卡片字段名标签。涉及 13 处表格共 75 个 td。

**双端兼容红线**：
- 桌面端（≥769px）所有样式与交互保持原状，新增 CSS 类与 `data-label` 属性不影响桌面渲染
- 移动端专属元素（汉堡按钮、返回按钮、抽屉）通过 `is_mobile()` 条件渲染，桌面端不渲染
- `use_breakpoint` 使用 `use_context_provider` 全局共享，避免多个组件重复监听

---

## 配置机制

### 编译时配置嵌入

`build.rs` 在编译时读取后端 `ai_orz.toml` 配置文件，序列化为 JSON 嵌入前端 WASM 二进制：
1. 优先读取 `.ai_orz/ai_orz.toml`（用户自定义配置）
2. 回退到 `common/config/ai_orz.toml`（默认配置）

### 运行时配置管理

`config.rs` 提供前端运行时配置管理：
- 首次使用编译时嵌入的默认配置
- 用户可在设置页修改 API 地址，保存到 localStorage
- `current_config()` 全局获取当前配置
- `api_url(path)` 拼接完整 API URL

---

## 页面与后端 API 对应关系

| 前端页面 | 后端 API 路径 | Handler 域 |
|---------|--------------|-----------|
| Reception（登录） | `/api/v1/auth/*` | organization |
| OrganizationInfo | `/api/v1/organization/*` | organization |
| OrganizationUsers | `/api/v1/organization/users` | organization |
| HrAgents | `/api/v1/hr/agents` | hr |
| HrAgentDetail | `/api/v1/hr/agents/:id` | hr |
| HrSkills | `/api/v1/hr/skills` | hr |
| FinanceModelProviders | `/api/v1/finance/model-providers` | finance |
| FinanceModelProviderDetail | `/api/v1/finance/model-providers/:id` | finance |
| FinanceTools | `/api/v1/finance/tools` | finance |
| FinanceToolDetail | `/api/v1/finance/tools/:id` | finance |
| FinanceMessageChannels | `/api/v1/finance/message-channels` | finance |
| ProjectList | `/api/v1/projects` | project |
| ProjectDetail | `/api/v1/projects/:id` | project |
| SystemTriggers | `/api/v1/system/cron-triggers` | system |
| SystemHealth | `/api/v1/health` | health |
| MessageChat | `/api/v1/finance/messages`, `/api/v1/finance/messages/agents` | message |
| UserProfile | `/api/v1/user/profile` | user |
| Settings | （纯前端，无 API） | - |

---

## 设计决策

### 为什么选择纯 CSS 变量而非 Tailwind？

- **Dioxus WASM 环境限制**：Tailwind 构建链在 WASM 环境下复杂，需要额外的 PostCSS/JS 工具链
- **轻量可控**：纯 CSS 变量零运行时开销，编译时确定
- **设计系统一致性**：CSS 变量集中管理，修改一处全局生效
- **与 Mistral 设计系统对齐**：直接映射 ui_design_system.md 中定义的色彩、间距、阴影

### 为什么使用 Dioxus Router 而非 signal 状态机？

- **URL 路由支持**：浏览器前进/后退、书签、分享链接
- **声明式导航**：`Link<Route>` 组件类型安全，编译时检查路由
- **代码分割潜力**：为未来按需加载页面预留空间
- **开发者体验**：URL 与页面状态一致，便于调试

### 为什么 API 客户端使用全局单例？

- **连接池复用**：避免每次请求新建 Client，减少 TCP 连接开销
- **统一 JWT 注入**：所有请求自动携带 token，业务代码无需关心
- **统一错误处理**：`ApiResponse<T>` 统一解析，错误信息一致

---

## 开发指南

### 新增页面流程

1. 在 `pages/` 对应业务域目录下创建页面文件（如 `pages/hr/new_page.rs`）
2. 在 `pages/mod.rs` 的 Route 枚举添加路由变体
3. 在 `main.rs` 添加路由组件渲染入口函数
4. 在 `layouts/navbar.rs` 添加导航链接（如需要）
5. 在 `api/` 对应域 API 客户端添加数据获取函数

### 新增 API 调用流程

1. 在 `common/src/api/` 检查/添加 DTO 类型定义
2. 在 `frontend/src/api/` 对应域模块添加 API 函数，使用 `api_get`/`api_post` 等 helper
3. 在页面组件中调用 API 函数，处理 `Result<T, String>`

### 样式使用规范

- **优先使用组件类**：`.card`、`.btn`、`.table`、`.form-input` 等
- **避免内联样式**：除动态计算的样式外，统一使用 CSS 类
- **新增样式**：在 `index.html` 的 `<style>` 标签中添加，遵循命名规范（`.component-name`、`.component-variant`）
- **CSS 变量**：颜色、间距、圆角、阴影统一使用 CSS 变量（`var(--color-*)`、`var(--space-*)`）

---

## 已知限制与未来规划

- **消息实时推送**：当前使用 3 秒短轮询，后续可引入 SSE/WebSocket 实现实时推送
- **表单验证**：当前表单验证较简单，后续可引入更完善的校验机制
- **错误处理**：API 错误以字符串形式返回，后续可考虑结构化错误类型
- **国际化**：当前文案为硬编码中文，后续可引入 i18n 支持
- **数据导出**：管理页面暂不支持数据导出功能，后续可添加 CSV/JSON 导出
- **WASM 包体优化**：移动端首屏 WASM 加载较慢，后续可考虑代码分割或骨架屏

---

## 更新记录

### 2026-07-17 移动端适配（响应式双端兼容）

在不破坏桌面端（≥769px）现有功能的前提下，完成全前端 375px~768px 移动端可用性适配。

| 适配项 | 实现细节 |
|--------|----------|
| **响应式基础设施** | `:root` 新增 `--breakpoint-sm/md/lg` 三个断点变量；新增 Mobile Adaptation CSS 区块（全局触摸优化、字号 padding、iOS 输入框 16px、hover 降级） |
| **`use_breakpoint` Hook** | 新增 `hooks/mod.rs`，基于 `window.matchMedia("(max-width: 768px)")` 监听，`use_context_provider` 全局共享；web-sys features 补充 `MediaQueryList`/`MediaQueryListEvent` |
| **Navbar 汉堡菜单** | 重写 `layouts/navbar.rs`：桌面端 `navbar-desktop-only` 容器封装原菜单；移动端汉堡按钮 + 左侧抽屉（`.navbar-drawer`）+ 半透明遮罩；点击导航项或遮罩自动关闭；admin 角色项条件渲染 |
| **Chat 单栏** | `pages/message/chat.rs` 新增 `sidebar_open` 信号与 `is_mobile` 切换；移动端 sidebar CSS transform 滑入滑出；chat-header 左侧"←"返回按钮；未选项目时仅显示 sidebar，已选时仅显示 main |
| **表格卡片化** | CSS `@media (max-width: 640px)` thead 隐藏、tr 转卡片、td 转 flex 行、`::before` 显示 `data-label`；13 处表格共 75 个 td 添加 `data-label` 属性 |
| **Modal 全屏化** | CSS `@media (max-width: 640px)` `.modal-content` 100vw/100vh、圆角 0、`.modal-footer` 纵向、按钮 100% 宽 |
| **Toast 适配** | CSS `.toast-container` left/right 12px、`.toast` 100% 宽 |
| **网格响应式** | CSS `.overview-stats` 4→2→1 列、`.overview-grid`/`.detail-grid`/`.stats-grid` 1 列 |
| **看板/筛选/卡片头部** | CSS `.kanban-board` 纵向、`.filter-row` 纵向、`.card-header` 纵向、`.page-header`/`.action-group` 允许换行 |
| **触摸优化** | CSS 按钮最小 40px、`.navbar-dropdown-item`/`.navbar-drawer-item` 44px、`-webkit-tap-highlight-color` 全局透明 |
| **Reception 375px** | CSS `@media (max-width: 375px)` headline 1.5rem、form-side padding 1rem、form-card max-width 100% |

**双端兼容保证**：所有新增 CSS 通过 `@media` 限定作用域；移动端专属组件通过 `is_mobile()` 条件渲染；`data-label` 是 HTML data 属性不影响桌面渲染。编译验证：前端 `cargo check` 通过、WASM release 构建成功、后端 732 测试全过。

### 2026-07-16 定时触发器前端体验优化

对 `pages/system/triggers.rs` 进行完整重写，6 项体验优化：

| 优化项 | 实现细节 |
|--------|----------|
| **列表信息增强** | 7 列展示（名称/类型徽章/调度信息/状态/下次执行/上次执行/操作），使用 `chrono` 格式化时间戳 |
| **Action 模板化** | `agent_rest` 模板（Agent ID + 沉淀数量字段）+ 自定义 JSON 选项 + `validate_json` 实时校验 |
| **Cron 表达式类型** | 支持 `TriggerType::Cron` + 6 个常用预设按钮（每分钟/每小时/每天 0 点/每天 9 点/每周一/每月 1 号） |
| **编辑功能** | 复用创建弹窗（`TriggerEditMode` 枚举区分 Create/Edit），`parse_payload` 自动回填模板参数 |
| **刷新优化** | 提取 `load_triggers` 闭包 + 手动刷新按钮 + 所有操作 toast 提示 |
| **CSS 样式** | 新增 4 个类：`cron-presets`/`cron-preset-btn`/`json-error`/`trigger-type-badge` |

关键设计：
- **模板/JSON 双模式**：选择 Action 模板时显示字段表单，选择自定义时显示 JSON textarea + 实时校验
- **编辑回填**：`parse_payload` 解析 JSON payload，识别 `action` 字段自动选择对应模板，回填 `extra` 参数
- **后端零改动**：全部复用已有 API（list/get/create/update/delete/pause/resume）
