# 前端架构设计

> 最后更新：2026-07-15

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
├── index.html                # 全局 CSS 设计系统（Mistral 暖色调变量 + 组件类）
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
    │   ├── navbar.rs         # 顶部导航栏（5 个下拉菜单 + Router Link）
    │   └── app_layout.rs     # 应用布局（Navbar + 内容区）
    │
    └── pages/                # 页面模块（按业务域分组）
        ├── mod.rs            # Route 枚举定义（15 条路由）
        ├── reception.rs      # 前台接待（登录/初始化闭环）
        ├── settings.rs       # 系统设置（API 地址配置）
        │
        ├── organization/     # 组织模块
        │   ├── info.rs       # 组织信息管理
        │   └── users.rs      # 用户管理
        │
        ├── hr/               # HR 模块
        │   ├── agents.rs     # Agent 管理列表
        │   ├── agent_detail.rs  # Agent 详情
        │   └── skills.rs     # 技能库管理
        │
        ├── finance/          # Finance 模块
        │   ├── model_providers.rs  # 模型提供商管理
        │   ├── tools.rs      # 工具管理
        │   └── message_channels.rs  # 消息渠道管理
        │
        ├── project/          # Project 模块
        │   ├── projects.rs   # 项目列表
        │   └── project_detail.rs  # 项目详情
        │
        ├── message/          # Message 模块
        │   └── chat.rs       # 消息对话
        │
        ├── system/           # System 模块
        │   ├── triggers.rs   # 定时触发器管理
        │   └── health.rs     # 健康检查
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

**组件类清单：**
- 布局工具：`.app-container`、`.content-area`、`.flex`、`.flex-col`、`.items-center`、`.justify-between`、`.gap-*`、`.w-full`
- Navbar：`.navbar`、`.navbar-brand`、`.navbar-section`、`.navbar-item`、`.navbar-dropdown`、`.navbar-dropdown-item`
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
| `api/hr.rs` | Agent CRUD（支持统计参数）、工具包/技能包管理、技能库 |
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

- **Navbar**：顶部导航栏，5 个下拉菜单（人力资源/财务管理/项目管理/系统管理/用户菜单），使用 `Link<Route>` 导航
- **AppLayout**：应用布局包装器，组合 Navbar + children 内容区

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
| FinanceTools | `/api/v1/finance/tools` | finance |
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
