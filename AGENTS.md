# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：5分钟了解项目是什么、代码怎么组织、开发遵循什么规范
>
> 最后更新：2026-07-15

---

## 一、项目概览

### 1.1 项目是什么

**AI Orz** - 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + rig-core 0.34
- **前端**：Dioxus 0.7 (WebAssembly)
- **技术特色**：严格分层架构、类型安全、697 个测试 100% 通过率

### 1.2 已实现核心功能

| 模块 | 状态 | 说明 |
|------|------|------|
| 👥 组织用户权限 | ✅ | 多级组织、用户角色、JWT Cookie 认证 |
| 🤖 Agent 全生命周期 | ✅ | 创建、配置、工具绑定、唤醒执行 |
| 🧠 四层记忆系统 | ✅ | Core/Working/Short-term/Long-term |
| 💬 消息对话系统 | ✅ | 用户 ↔ Agent 双向对话，支持项目上下文 |
| 📨 消息渠道系统 | ✅ | 多渠道消息接入，支持启用/禁用/测试 |
| 🛠️ 混合模式工具调用 | ✅ | 简单工具走 rig auto，关键工具走自建 manual 可控链路 |
| 📚 技能库系统 | ✅ | 可复用技能和工作流，支持搜索和分类，tag 技能包安装，唤醒时注入 Prompt |
| 📋 任务 + 项目管理 | ✅ | 任务状态机，项目聚合对话上下文，DAL + Domain 层完整实现 |
| 📎 统一附件存储 | ✅ | 消息附件 + 项目产物，FileMeta + 日期分层路径 |
| 🔌 MCP 服务器集成 | ✅ | MCP 服务器管理、工具同步、MCP 工具调用执行 |
| 🚀 异步消费者系统 | ✅ | 通用消费者框架 + Message Topic 三层分发 |
| 📝 结构化日志系统 | ✅ | JSON 格式、自动上下文关联、日志自动清理 |
| 🔍 向量搜索 | ✅ | SQLite VSS 扩展 + 语义索引 + 可平滑升级 |
| 🔎 全文搜索 | ✅ | FTS5 + trigram 分词器，支持中文全文搜索、BM25 相关性排序 |
| 📊 Agent 统计系统 | ✅ | DuckDB 多维统计、Agent/Project/Task/ModelProvider/Tool 五维度覆盖、实体详情页按需动态注入 |
| 🔄 多回合循环控制 | ✅ | 轮次限制检查、任务完成检测、Prompt 上下文差异化、工具失败计数注入 |
| 🎒 工具包机制 | ✅ | tag 分组工具、Agent 入职自动安装、免绑定校验三层逻辑 |
| 📨 任务分配消息 | ✅ | TaskAssignment 消息类型、自动通知 Agent、神经工具封装 |
| ⏰ 定时触发器系统 | ✅ | Cron Trigger 管理、后台扫描、事件投递、系统领域基础设施 |
| 🏛️ 记忆沉淀机制 | ✅ | Agent 休息与沉淀、短期记忆→长期知识图谱、定时触发沉淀 |
| 🎒 技能包机制 | ✅ | tag 分组技能、批量安装、安装即复制、卸载保留副本 |
| 🔎 综合搜索 | ✅ | FTS5 关键词 + 向量语义 + 图谱关系 三位一体混合搜索，Hybrid/Vector/Keyword 三态匹配 |
| 📊 任务进度追踪 | ✅ | Task progress 字段（0-100）、Agent 神经工具更新进度、complete 自动设 100、progress_updated 事件 |
| 🤝 Agent 协作工具 | ✅ | search_agents 搜索、send_message_to_agent Agent 间消息、collaboration tag 分组工具 |
| 🎨 前端架构重构 | ✅ | Dioxus Router 15 路由 + Mistral CSS 设计系统 + 统一 API 客户端 + 13 CRUD 页面 |
| 💬 对话功能 MVP | ✅ | 左右分栏布局（项目列表 + 对话区）、双向分页、3秒短轮询、消息气泡展示 |
| 📎 对话附件上传 | ✅ | 多文件上传、图片内联展示、文件下载、消息时间分组 |
| 🔍 消息搜索 | ✅ | FTS5 + 向量混合搜索、搜索结果展示匹配类型和向量距离 |
| 🧠 记忆搜索 | ✅ | 关键词 + 类型筛选、短期记忆/知识节点/关系搜索 |
| 🗺️ 知识图谱可视化 | ✅ | SVG 图谱组件、圆形布局、节点连接线、搜索初始节点 |
| 🗺️ 知识图谱交互完善 | ✅ | 关系类型差异化颜色/样式、边标签防重叠、节点拖拽、缩放平移、搜索高亮与历史、详情侧边栏增强 |
| 📡 SSE 消息推送 | ✅ | Server-Sent Events 长连接、订阅者模式、DAO 层连接管理、broadcast 广播 |
| 🔔 Toast 通知系统 | ✅ | 全局状态管理、4 种类型（success/error/warning/info）、滑入滑出动画、进度条倒计时、22 页面统一替换旧式提示 |
| 🔐 Cookie 认证统一 | ✅ | 前后端统一 HttpOnly Cookie + JWT、中间件顺序优化、localStorage 标志位 |
| 🔑 双模式认证 | ✅ | Cookie（浏览器）+ Bearer token（API 工具/代码调用），非浏览器请求返回 401 JSON |
| 📊 任务进度可视化 | ✅ | 项目概览卡片、动态进度条、任务状态分布统计 |
| 🤖 Agent 详情页对话 | ✅ | Agent 详情页集成对话功能、SSE 实时消息、历史消息加载 |
| 📋 任务管理核心功能 | ✅ | 任务创建/编辑弹窗、任务详情页、项目详情页集成创建入口 |
| 📋 独立任务管理页面 | ✅ | 全局任务列表、看板视图（按状态分列）、多维度筛选、统计概览 |
| 🧠 Agent 记忆面板 | ✅ | Agent 详情页记忆浏览、Tab 切换（短期记忆/知识节点/关系）、搜索、卡片展示 |
| 💬 对话体验打磨 | ✅ | 消息复制（hover 显示按钮）、快捷指令（/clear、/help）、键盘导航 |

### 1.3 整体完成度与测试统计（2026-07-15 更新）

| 指标 | 数值 | 说明 |
|------|------|------|
| **总测试数** | **697** | DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
| **通过率** | **100%** | ✅ 全部测试通过 |
| DAO 模块数 | 29 个 | 全部实现并被使用，零闲置（21 核心 DAO + 5 渠道 DAO + 1 统计 DAO + 1 触发器 DAO + 1 消息推送 DAO） |
| DAL 模块数 | 18 个 | 全部完整业务承载，零闲置 |
| Domain 领域数 | 7 个 | 全部完整实现（新增 SystemDomain） |
| Handler API 领域数 | 7 个上线 | organization, hr, finance, project, user, health, system |
| **整体架构完成度** | **~98%** | 从下往上扎实推进 |

---

## 二、文档快速索引

> 📌 **按需要读取详细设计文档**

### 架构总览
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [README.md](./README.md) | 项目概览、快速开始、功能列表、文档索引 | ⭐⭐⭐ |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | **最新**完整架构说明、核心概念解释、实体关系、完成状态 | ⭐⭐⭐ |
| [docs/architecture_status_20260701.md](./docs/architecture_status_20260701.md) | 分层架构现状快照、金字塔结构、各层状态统计 | ⭐⭐⭐ |

### 分层架构与最佳实践
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | **开发必读** 6 个完整架构实践、反模式坑、最佳实践总结 | ⭐⭐⭐ |
| [docs/NAMING_CONVENTION.md](./docs/NAMING_CONVENTION.md) | 全项目统一命名约定、DAO/DAL/Domain 命名规则 | ⭐⭐ |
### 各模块详细设计
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、测试隔离 | ⭐⭐⭐ |
| [docs/runtime_design.md](./docs/runtime_design.md) | **Runtime Domain 总纲**：Agent 唤醒、神经 vs 外骨骼工具二分、上下文极薄设计 | ⭐⭐⭐ |
| [docs/memory_design.md](./docs/memory_design.md) | 四层记忆系统设计、检索策略 | ⭐⭐ |
| [docs/tool_design.md](./docs/tool_design.md) | 混合模式工具调用、工具注册表、调用追踪 | ⭐⭐ |
| [docs/message_interaction_design.md](./docs/message_interaction_design.md) | 消息交互架构、用户↔Agent双向对话、工具调用复用消息表 | ⭐⭐ |
| [docs/message_channel_design.md](./docs/message_channel_design.md) | 消息渠道系统设计、多渠道支持、状态管理 | ⭐⭐ |
| [docs/consumer_architecture.md](./docs/consumer_architecture.md) | 异步消费者框架、按 to_role 分层分发 | ⭐⭐ |
| [docs/task_scheduler_design.md](./docs/task_scheduler_design.md) | 任务调度器设计、Cron 表达式、定时任务执行 | ⭐⭐ |
| [docs/event_design.md](./docs/event_design.md) | 泛型 topic 事件队列、类型安全隔离 | ⭐⭐ |
| [docs/skill_design.md](./docs/skill_design.md) | 技能库系统、Agent 自进化沉淀技能 | ⭐⭐ |
| [docs/vector_search_architecture.md](./docs/vector_search_architecture.md) | 混合搜索架构：FTS5 关键词 + 向量语义 + 三态匹配 | ⭐⭐ |

### 基础设施与规范
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/logging_design.md](./docs/logging_design.md) | **日志系统设计**：统一宏使用规范、上下文检测机制、tracing 语法速查 | ⭐⭐⭐ |
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、STRICT 模式、FTS5 全文搜索、测试隔离 | ⭐⭐⭐ |
| [docs/task_design.md](./docs/task_design.md) | 任务系统设计、状态机、分配与进度追踪 | ⭐ |
| [docs/project_design.md](./docs/project_design.md) | 项目系统设计、聚合对话上下文 | ⭐ |
| [docs/organization_design.md](./docs/organization_design.md) | 组织用户权限体系设计 | ⭐ |
| [docs/attachment_storage.md](./docs/attachment_storage.md) | 产物与消息附件统一存储设计 | ⭐ |

### 前端与 UI
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/frontend_architecture.md](./docs/frontend_architecture.md) | **前端架构设计**：Router/CSS 设计系统/API 客户端/状态管理/页面模块 | ⭐⭐⭐ |
| [docs/ui_design_system.md](./docs/ui_design_system.md) | UI 设计系统、配色、排版、组件规范、实现状态 | ⭐⭐ |

---

## 三、核心架构规范（必须遵守）

### 3.1 代码分层架构

**严格单向调用，禁止跨层和同层互调**：

```
Handler (API 层)
    │ 只调用 Domain
    ▼
Domain (领域层)
    │ 组合多个 DAL，实现业务逻辑
    ▼
DAL (业务数据层)
    │ 组合多个 DAO，提供业务级数据操作
    ▼
DAO (数据访问层)
    │ 单一数据源 CRUD，不包含业务逻辑
    ▼
Models (PO 持久化实体)
```

**各层职责边界**：

| 层级 | 可以做 | 禁止做 |
|------|--------|--------|
| **DAO** | 单一数据源 CRUD、SQL 拼接、PO 转换 | ❌ DAO 调 DAO、❌ 业务逻辑、❌ 实体组装 |
| **DAL** | 依赖多个 DAO、PO → Entity 转换 | ❌ DAL 调 DAL |
| **Domain** | 依赖多个 DAL、核心业务逻辑编排、跨领域事务 | ❌ Domain 调 Domain、❌ 直接调用 DAO |
| **Handler** | HTTP 路由、参数校验、DTO ↔ Command/Query 转换、按用户 Action 编排 Domain、响应 DTO 组装 | ❌ 直接调用 DAL/DAO、❌ 承载复杂业务规则、❌ Handler 间互调、❌ 抽象通用 Handler 框架 |

**Handler 层设计补充**：Handler 与用户 Action 直接对应，一个接口按需求完成自己的请求级编排即可；复用优先通过组织 Command/Query 参数和调用 Domain 能力完成，不为了复用提前抽象 `BaseHandler` / `GenericActionHandler`。复杂业务规则、状态流转、权限语义必须下沉到 Domain。

### 3.2 目录结构

```
ai_orz/
├── common/                     # 公共共享 crate（前后端共用）
│   ├── src/api/               # API 请求响应 DTO 按功能分组
│   ├── src/constants/         # 公共常量、基础类型
│   ├── src/enums/            # 公共枚举（UserRole、TaskStatus 等）
│   ├── src/error/            # 统一错误类型
│   └── src/models/           # 跨层共享模型（ToolCallTraceRef、StatsInterval 等）
│
├── ai-orz-macros/             # 自定义宏 crate（日志宏、统计事件宏）
│
├── src/                        # 后端服务
│   ├── handlers/              # HTTP 接口层（按业务域分组，每个方法一个文件）
│   ├── service/
│   │   ├── dao/               # 数据访问层 DAO（含 stats_duckdb 统计 DAO）
│   │   ├── dal/               # 业务数据访问层 DAL
│   │   └── domain/            # 领域层 Domain
│   ├── models/                # PO 持久化实体 + 业务实体
│   ├── middleware/            # Axum 中间件
│   ├── consumer/              # 异步消费者系统
│   └── pkg/                   # 公共工具包
│       ├── stats/            # DuckDB 统计模块（record_event! 宏、查询 API）
│       └── *test_support.rs  # 测试支持文件（request_context、storage）
│
├── frontend/                   # Dioxus 前端
│   ├── src/api/               # API 客户端
│   └── src/components/        # UI 组件
│
└── docs/                       # 详细设计文档
```

#### 3.1.1 基础设施公共工具位置约定（2026-07-12 新增，强制执行）

**核心原则：通用工具函数必须放在基础设施层，禁止散落在业务 DAO 中造成跨 DAO 依赖。**

| 工具类型 | 存放位置 | 示例 |
|----------|----------|------|
| **FTS5 全文搜索工具** | `src/pkg/storage/fts5.rs` | `escape_fts5_keyword` |
| **向量存储抽象** | `src/pkg/storage/vector.rs` | `VectorStore` trait |
| **日志宏** | `src/pkg/logging.rs` + `ai-orz-macros` | `log_info!`, `log_error!` |
| **统计事件宏** | `src/pkg/stats/` + `ai-orz-macros` | `record_event!` |
| **JWT 工具** | `src/pkg/jwt.rs` | `encode_token`, `decode_token` |

**反模式（禁止）：**
- ❌ 在某个业务 DAO 中定义通用工具函数，其他 DAO 直接 import（造成 DAO → DAO 依赖）
- ❌ 为了复用在每个 DAO 中复制粘贴相同代码
- ❌ 把业务逻辑相关的工具放到 pkg 层（pkg 层必须无业务感知）

**正确模式：**
- ✅ 跨模块复用的通用工具 → 放到 `src/pkg/` 对应子模块
- ✅ 模块内部辅助函数 → 模块内部私有，不对外导出
- ✅ 单个文件使用的小工具 → 文件内定义，不上升到模块级

### 3.2 命名规范

| 元素 | 规范 | 示例 |
|------|------|------|
| **变量/函数/方法** | snake_case | `user_id`, `create_agent`, `get_user_by_id` |
| **类型/结构体/枚举/Trait** | PascalCase | `AgentPo`, `RequestContext`, `AgentDao` |
| **常量** | SNAKE_CASE | `MAX_SIZE`, `LOG_ID`, `DEFAULT_TIMEOUT` |
| **文件名/目录名** | snake_case | `agent.rs`, `request_context.rs`, `sqlite_test.rs` |

**函数/方法前缀约定：**

| 操作 | 前缀 | 示例 |
|------|------|------|
| 获取数据（有参数） | `get_` | `get_agent_by_id`, `get_user_name` |
| 获取单例/无参数 | 直接命名 | `agent_dao()`, `uid()` |
| 创建/新增 | `new_`, `create_` | `new_agent()`, `create_user()` |
| 修改/更新 | `update_` | `update_agent()` |
| 删除（软删除） | `delete_` | `delete_agent()` |
| 列表/批量 | `find_all`, `find_by_` | `find_all_agents()`, `find_by_org()` |
| 布尔判断 | `is_`, `has_`, `can_` | `is_deleted()`, `has_permission()` |

**集合变量：** 使用复数形式 `agents`, `user_ids`

**Trait 与实现类命名：**
- Trait 不加 `Trait` 后缀：`trait AgentDao { ... }`
- 实现类加 `Impl` 后缀：`struct AgentDaoSqliteImpl`

**Context 传递规范（强制执行）：**
- **所有 service 层（DAO/DAL/Domain）公共方法的第一个参数必须是 `ctx: RequestContext`**
- 用户相关信息从 `ctx.uid()` / `ctx.uname()` 获取，不再单独传参
- 内部私有方法可省略，只读操作也需要传递（便于日志记录）

**常见错误对照：**

| 错误写法 | 正确写法 |
|---------|---------|
| `userId`, `agentName` | `user_id`, `agent_name` |
| `newAgent()`, `createAgent()` | `new_agent()`, `create_agent()` |
| `getUserById()` | `get_user_by_id()` |
| `agentPO`, `OrgDAO` | `AgentPo`, `org_dao` |
| `maxSize`, `logId` | `max_size`, `log_id` |

### 3.4 数据对象四层清晰定义

| 对象类型 | 定义位置 | 用途 |
|----------|----------|------|
| **API DTO** | `common/src/api/**` | HTTP 请求/响应，前后端复用；通用响应包装使用 `common::api::ApiResponse<T>` |
| **跨层共享模型** | `common/src/models/**` | DAO/DAL/Domain/API 共用的结果结构体（StatsInterval、TimeSeriesPoint、TokenSumResult 等） |
| **Command/Query** | `src/service/domain/*/mod.rs` | Domain 层输入，表达业务意图 |
| **业务实体** | `src/models/*.rs` | 核心业务对象，包含行为和状态 |
| **PO (持久化对象)** | `src/models/*.rs` | 数据库映射，1:1 对应表结构 |

### 3.5 PO 与业务实体分层边界规范（2026-05-11 新增，强制执行）

**核心原则：PO 仅在 DAO/DAL 层内部使用，绝对不对外暴露到 Domain 层及以上**

#### 分层边界定义

| 层级 | 可使用对象 | 数据传递方式 | 说明 |
|------|------------|------------|------|
| **DAO 层** | 仅 PO | PO ↔ 数据库 | 单一数据源 CRUD，SQL 拼接，无业务逻辑 |
| **DAL 层** | 内部：PO，对外：业务实体 | PO ↔ 业务实体 双向转换 | 组合 DAO，完成业务级数据操作 |
| **Domain 层** | 仅业务实体 | 业务实体 ↔ Command | 核心业务逻辑编排，无 PO 依赖 |
| **Handler 层** | 业务实体 + DTO | DTO ↔ 业务实体 | HTTP 接口，参数校验 |

#### 业务实体内部设计

**标准模式：业务实体内部持有 PO 字段**
```rust
// ✅ 正确：业务实体内部持有 PO，便于 DAL 层传递
pub struct Project {
    pub po: ProjectPo,
    // 可选：额外业务方法和字段
}

pub struct Task {
    pub po: TaskPo,
    // 业务方法...
}
```

**设计优势：**
1. **避免重复转换代码**：DAL 层直接通过 `&xxx.po` 传递给 DAO，无需字段逐一映射
2. **减少出错概率**：修改 PO 字段时只需修改一处，业务实体自动兼容
3. **100% 向后兼容**：现有测试和业务逻辑无需修改
4. **性能优化**：写操作使用引用传递 `&`，避免不必要的 clone

#### DAL 层接口签名规范

**所有 DAL 接口统一使用业务实体，不使用 PO：**
```rust
// ✅ 正确：写操作接收 &业务实体 引用
async fn create(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;
async fn update(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;

// ✅ 正确：读操作返回 业务实体
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>, AppError>;
async fn list_by_user(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<Project>, AppError>;

// ❌ 错误：直接使用 PO
// async fn create(&self, ctx: RequestContext, po: &ProjectPo) -> Result<(), AppError>;
```

#### RequestContext 跨层传递规范

**所有跨层 ctx 传递统一使用 `ctx.clone()`：**
```rust
// ✅ 正确：clone 后传递，避免所有权移动问题
self.project_dal.create(ctx.clone(), project).await?;

// ❌ 错误：直接移动，导致后续无法使用
// self.project_dal.create(ctx, project).await?;
```

**理由：**
- RequestContext 内部是 Arc 引用，clone 成本极低（仅指针复制）
- 避免所有权移动导致的编译错误
- 与 message domain 风格保持一致

#### 软删除设计规范

**`status = 0` 视为软删除，常规查询默认过滤：**
```rust
// DAO 层 find_by_id 示例
sqlx::query_as!(
    TaskPo,
    r#"SELECT ... FROM tasks WHERE id = ? AND "status" != 0"#,
    id
)
```

**典型应用：**
- `TaskStatus::Cancelled = 0` - 取消的任务视为已删除
- 需要查询历史/恢复时，使用 `query` 方法绕过过滤
- 测试需适配此行为：cancel 后 get 返回 None 是预期行为

---

## 四、关键约定（强制执行）

### 4.1 RequestContext 参数

**所有 service 层（DAO/DAL/Domain）公共方法的第一个参数必须是 `ctx: RequestContext`**

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String>;
```

### 4.2 枚举类型安全

所有存储在数据库中的枚举状态/角色字段，**必须使用 Rust 枚举类型**，禁止直接使用 `i32` 存储

- 添加 `#[repr(i32)]` + `#[derive(sqlx::Type)]`
- 实现 `From<i64>` 适配 sqlx 类型推断
- 枚举统一定义在 `common/src/enums/`

### 4.3 SQLite + SQLx 规范

- **所有表必须启用 `STRICT` 模式**
- **SQL 关键字必须转义**：`status` → `"status"`
- **枚举字段显式标注**：`status as "status: TaskStatus"`
- **软删除约定**：已删除 `status = 0`，查询默认过滤
- **`.sqlx` 目录必须纳入版本控制**
- **测试使用 `#[sqlx::test]`**，每个测试独立内存数据库

### 4.4 Handler 拆分规范

- 按业务域分组（hr、finance、organization、user 等）
- **每个业务方法一个独立文件**，单个文件只放一个 handler 函数
- `mod.rs` 只保留模块导出，不存放实现
- 所有 DTO 从 `common/src/api/` 导入；通用响应包装统一使用 `common::api::ApiResponse<T>`，禁止在 `src/handlers` 定义本地 `ApiResponse`

### 4.5 测试隔离原则

- 无状态组件可使用单例（OnceLock）
- 有状态内存组件必须每次新建实例
- 测试使用独立数据库，不依赖全局状态
- 所有测试使用 `#[sqlx::test]` 宏

### 4.6 日志系统规范（强制执行，2026-05-15 新增）

**核心原则：项目内所有代码必须使用统一日志宏，禁止直接调用 tracing::*!**

#### 必须使用的宏

| 级别 | 宏名 |
|------|------|
| INFO | `log_info!` |
| WARN | `log_warn!` |
| ERROR | `log_error!` |
| DEBUG | `log_debug!` |

#### 两种调用模式（自动检测）

```rust
// ✅ 模式 1：无上下文（系统级别，第一个参数是字符串）
log_info!("application started");
log_info!("config loaded from {}", path);

// ✅ 模式 2：带上下文（请求级别，&ctx + operation 字符串）
log_info!(&ctx, "create_memory", "created memory id={}", memory_id);
log_error!(&ctx, "update_project", "db error: {:?}", err);
```

#### 禁止的写法

```rust
// ❌ 禁止：直接调用 tracing
tracing::info!("some message");

// ❌ 禁止：旧函数形式（已删除）
logging::info("some message");

// ❌ 禁止：传 ctx 值而非引用
log_info!(ctx, "operation", "message");  // 必须是 &ctx
```

#### 检测机制

宏通过**语法模式匹配顺序**自动区分两种模式：
1. **优先匹配**：第一个参数是**字符串字面量** → 无上下文模式
2. **兜底匹配**：第一个参数是表达式，第二个是字符串字面量 → 带上下文模式

> 💡 **重要**：Operation 必须是字符串字面量，不能是变量。完整规范请参考 [docs/logging_design.md](./docs/logging_design.md)

---

## 五、核心概念与实体关系

### 5.1 实体关系

```
Organization (组织)
├── User (用户)
├── ModelProvider (模型配置)
└── Agent (智能代理)
     └── Brain
          └── Cortex
               └── ModelProvider (LLM 配置)
```

### 5.2 Agent 思考 + 记忆

```
Agent
└── Brain
     ├── Cortex           # 思考执行，绑定 ModelProvider
     └── Memory           # 记忆系统
          ├── Core        # 核心认知：角色设定、能力清单
          ├── Working     # 当前会话工作记忆
          ├── Short-Term  # 最近会话摘要索引
          └── Long-Term   # 长期沉淀知识图谱
```

---

## 六、工作流与开发记录

### 2026-07-12 里程碑
**✅ 前端架构重构**
- **Dioxus Router 引入**：15 条路由替代 use_signal 状态机，支持 URL 路由 + Link 组件导航
- **Mistral CSS 设计系统**：CSS 变量 + 组件类注入 index.html，替代内联样式（暖色调，与 ui_design_system.md 对齐）
- **统一 API 客户端**：OnceLock 全局单例 + JWT bearer 自动注入 + api_get/api_post/api_put/api_delete 类型化 helper
- **全局认证状态管理**：AuthState + use_context_provider + token localStorage 持久化 + 登录闭环
- **7 业务域 API 客户端**：auth/organization/hr/finance/project/message/system，与后端 Handler 域对齐
- **基础 UI 组件库**：Button（5 variant）/ Modal / Loading / EmptyState / ErrorAlert / SuccessAlert
- **布局组件**：Navbar（5 个下拉菜单 + Router Link）+ AppLayout
- **13 个 CRUD 页面**：Reception 登录 / 组织信息 / 用户管理 / Agent 管理 / 技能库 / 模型提供商 / 工具 / 消息渠道 / 项目管理 / 定时触发器 / 健康检查 / 个人信息 / 设置
- **页面模块化**：pages/ 按 organization/hr/finance/project/message/system/user 分组，与后端 Handler 域对齐
- **common crate 扩展**：补充缺失 DTO 类型、Default derive、ProviderType Display 实现
- **测试统计**：693 个后端测试 100% 通过，前端 cargo check 0 错误
- **文档更新**：新增 [docs/frontend_architecture.md](./docs/frontend_architecture.md)，更新 README/AGENTS/ARCHITECTURE/architecture_status/ui_design_system

**✅ Task 进度追踪 + FTS5 公共工具重构**
- **Task progress 字段**：新增 `progress: i32`（0-100），自动 clamp 防越界，`complete()` 时自动设 100
- **update_progress Domain 方法**：TaskManage trait 新增 `update_progress(ctx, task_id, progress)` 方法
- **progress_updated 事件**：进度更新时自动记录 `progress_updated` 类型的 TaskEvent
- **update_task_progress 神经工具**：注册为 project_management 工具包，Agent 可随时更新任务进度
- **API 路由**：`PUT /api/v1/projects/tasks/{id}/progress`，HTTP + 神经工具双模式
- **escape_fts5_keyword 重构**：从 `dao/memory/sqlite.rs` 提升到 `pkg/storage/fts5.rs`，消除 DAO→DAO 依赖
- **测试统计**：693 个测试 100% 通过

**✅ Runtime Domain Phase 4C - 技能系统增强**
- **DAO 层 tag 过滤**：SkillQuery/ToolQuery 新增 tags 字段，使用 `json_each` 在 SQL 层精确匹配（OR 语义），关键词搜索扩展到 tags 字段
- **AgentRuntimeConfig 扩展**：新增 `installed_skill_packs: Vec<String>` 字段，记录 Agent 已安装技能包
- **install_to_agent 幂等性**：安装前检查 parent_skill_id + author_id 是否已有副本，已存在则跳过
- **技能包完整生命周期**：
  - `install_skill_pack`：按 tag 查询 Published 技能 → 批量 install_to_agent → 记录 tag
  - `uninstall_skill_pack`：移除 tag 关联，保留技能副本（不丢失 Agent 经验）
  - `reinstall_skill_pack`：覆盖式重装，用源技能最新内容更新 Agent 副本
  - `list_installed_skill_packs`：返回已安装技能包 tag 列表
- **技能包管理 API**：3 个新 Handler（install/uninstall/list），路由 `/api/v1/hr/agents/{agent_id}/skill-packs`
- **唤醒时技能注入**：`load_agent_skills` 方法，Agent 唤醒时自动加载技能摘要到 Prompt 的"【可用技能】"部分
- **search_skill 神经工具**：Agent 可按关键词/tag 搜索技能库，返回精简摘要
- **Tool tag 过滤优化**：`load_builtin_tools` 和 `call_manual_tool_for_agent` 从内存过滤改为 SQL 层 `json_each` 过滤
- **测试统计**：601 个测试 100% 通过（+25）

**✅ 记忆搜索 FTS5 增强与综合搜索**
- **FTS5 全文索引**：创建 `short_term_memory_fts` 和 `knowledge_node_fts` 虚拟表，使用 `trigram` 分词器（支持中文全文搜索）
- **触发器自动同步**：6 个触发器（INSERT/UPDATE/DELETE × 2 表）自动维护 FTS 索引，应用层无感知
- **DAO 层搜索改造**：LIKE → FTS5 MATCH + BM25 相关性排序，新增 `escape_fts5_keyword` 转义工具函数
- **死代码清理**：移除 `query_short_term` 和 `query_knowledge_nodes` 中的 MATCH 死代码分支，关键词搜索统一走 search 方法
- **MatchType 三态完善**：Hybrid（双命中）/ Vector（仅向量）/ Keyword（仅关键词），每种命中都附加 `SearchMatchInfo`
- **向量距离阈值可配置**：从硬编码 0.8 改为 `MemorySearch.vector_distance_threshold` 可选参数
- **综合搜索三级排序**：Hybrid 优先 → Vector → Keyword，组内分别按 vector_distance / fts_rank 排序
- **关系关键词搜索补全**：`search_relations_internal` 通过 knowledge_node_fts 搜索节点 → 查关联关系 → 返回节点和关系
- **测试统计**：615 个测试 100% 通过（+14）

**✅ 全实体 FTS5 全文搜索改造**
- **统一搜索标准**：为 Skill/Tool/Message/Task/Project/Agent 6 大实体建立统一的混合搜索模式（FTS5 关键词 + 向量语义 + 三态匹配），全面弃用 LIKE
- **FTS5 迁移文件**：`20260712000001_entity_fts5.sql`，6 个 FTS5 虚拟表（trigram 分词器）+ 18 个触发器（INSERT/UPDATE/DELETE × 6 表）+ 6 条存量回填
- **三态匹配模式**：Hybrid（向量+关键词双命中）/ Vector（仅向量）/ Keyword（仅关键词），每种附加 SearchMatchInfo（vector_distance + fts_rank）
- **综合搜索三级排序**：Hybrid 优先 → Vector 次之 → Keyword 最后，组内按 vector_distance / fts_rank 升序
- **向量索引自动维护**：所有实体 create/update 时自动 upsert 向量索引，delete/archive 时自动清理
- **ToolVectorDao 补全**：新增 `delete_vector` 方法，完善 Tool 向量索引生命周期管理
- **Tool DAL 向量索引维护**：create_tool/update_tool/delete_tool 补全向量索引自动维护逻辑
- **测试统计**：693 个测试 100% 通过（+78，含 6 实体搜索三态匹配测试）

### 2026-07-15 里程碑
**✅ SSE 消息推送系统**
- **SSE 长连接推送**：基于 Server-Sent Events 实现服务器到客户端的单向实时推送
- **订阅者模式**：Handler → Domain → DAL → DAO 分层调用，DAL 负责消息加工，DAO 管理 SSE 连接
- **DAO 层连接管理**：`SsePushDao` 使用 `tokio::sync::RwLock` + `broadcast` 通道管理多个 SSE 连接和消息分发
- **MessageDelivery 扩展**：新增 `subscribe`/`unsubscribe` 方法，`deliver_message` 自动通过 SSE 推送
- **SSE 订阅端点**：`GET /api/v1/finance/messages/sse/{user_id}`，与 finance/message 模块路由对齐
- **消费者集成**：消息消费者处理消息时自动调用 `deliver_message`，通过 SSE 推送到前端

**✅ 前后端认证机制统一**
- **Cookie 认证统一**：前端从 `Authorization: Bearer` 改为 HttpOnly Cookie，浏览器自动携带
- **中间件顺序优化**：调换 `jwt_auth_middleware`（外层先执行）和 `request_context_middleware`（内层后执行）顺序
  - JWT 中间件验证 Cookie 中的 token → 将用户信息写入请求头
  - RequestContext 中间件从请求头（已含用户信息）创建 RequestContext
  - 消除了 JWT 中间件中"克隆并更新 ctx"的冗余逻辑
- **前端认证简化**：移除 token 管理，使用 localStorage 标志位判断登录状态
- **SSE 兼容**：EventSource 自动携带 Cookie，无需额外处理认证
- **测试统计**：696 个测试 100% 通过（+3）

**✅ 双模式认证（Cookie + Bearer）**
- **双模式 JWT 提取**：中间件优先从 Cookie 提取 token，Cookie 不存在时 fallback 到 `Authorization: Bearer` 头
- **智能响应区分**：
  - 浏览器请求（有 Cookie 头或 Accept: text/html）→ 302 重定向到登录页
  - API 调用请求（Bearer 模式）→ 401 JSON 错误响应
- **LoginResponse 扩展**：登录响应新增 `token` 字段，API 调用者可直接获取 JWT 用于后续 Bearer 调用
- **使用场景**：curl/Postman/代码调用均可通过 Bearer token 访问所有受保护 API
- **测试统计**：696 个测试 100% 通过

**✅ 对话功能补全（附件上传 + 时间分组）**
- **附件上传 API 客户端**：使用 web_sys 原生 fetch API + FormData 实现 WASM 环境下的文件上传
- **SendMessageToAgentParams 扩展**：新增 `attachment_ids` 字段支持发送带附件的消息
- **后端附件消息创建**：`delivery.rs` 中根据 attachment_ids 批量创建附件消息，`reply_to_id` 指向根文本消息
- **附件上传 UI**：📎 按钮触发文件选择，支持多文件，上传中显示加载状态
- **附件消息展示**：图片消息内联展示，其他类型文件显示文件名+大小+下载链接
- **消息时间分组**：按日期分组显示（今天/昨天/YYYY-MM-DD），日期分隔符样式
- **测试统计**：697 个测试 100% 通过（+1）

**✅ 任务管理可视化**
- **项目概览统计卡片**：项目总数、进行中任务数、已完成任务数、整体进度
- **动态进度条**：进度 0-25% 橙色警告、26-50% 蓝色主色、51-75% 紫色强调、76-100% 绿色成功
- **任务状态分布**：Pending/InProgress/Completed/Cancelled/Archived 五状态统计卡片
- **项目详情页集成**：在项目详情页顶部展示概览面板，任务列表紧随其后

**✅ Agent 详情页对话集成**
- **消息列表渲染**：复用对话页面的消息气泡组件，支持文本+附件消息展示
- **输入框与发送**：Enter 发送、Shift+Enter 换行，发送中显示 typing 指示器
- **SSE 实时消息接收**：监听全局 SSE 通道，自动过滤当前 Agent 相关消息
- **历史消息加载**：页面加载时拉取最近 20 条消息作为初始上下文
- **无侵入式集成**：对话区域作为 Agent 详情页的第六个 section，与其他管理功能共存

**✅ 知识图谱交互完善**
- **关系类型差异化渲染**：6 种关系类型（属于/引用/包含/关联/派生/依赖）对应不同颜色和虚线样式
- **边标签防重叠**：标签添加背景框，通过 transform 变换优化位置
- **节点拖拽功能**：鼠标拖拽节点，实时更新关联边的端点位置
- **图谱缩放与平移**：滚轮缩放（0.5x-2x），右键拖拽平移，坐标系统完整支持
- **搜索结果高亮与历史记录**：匹配节点发光高亮，搜索历史快捷按钮，支持快速回溯
- **详情侧边栏增强**：展示节点关系信息、内容框样式优化、关闭按钮

**✅ Toast 通知系统**
- **状态管理核心**：`ToastType` 4 种类型（success/error/warning/info）、`ToastState` Copy 结构体、全局 `use_toast()` 上下文 API
- **UI 组件**：`ToastContainer` 容器固定右上角，`ToastItemView` 单条通知，滑入滑出动画、自动关闭、手动关闭、进度条倒计时
- **CSS 样式**：复用现有 CSS 变量，进度条 `@keyframes` 动画，4 种类型配色
- **全局替换**：22 个页面文件、194 处旧式 ErrorAlert/SuccessAlert 提示统一替换为 Toast，净减 54 行代码

**✅ 任务管理核心功能**
- **任务创建/编辑弹窗**：`TaskEditModal` 组件支持 Create/Edit 两种模式，表单字段映射后端 `CreateTaskRequest` / `UpdateTaskRequest`
- **任务详情页**：`/tasks/:id` 路由，展示基本信息、标签与依赖、进度管理（进度条 + 更新弹窗）、状态流转（6 种状态切换按钮）
- **项目详情页集成**：任务列表区域头部增加"+ 新建任务"按钮，任务行可点击跳转到详情页
- **后端无改动**：复用已有 API 客户端和 DTO，纯 UI 集成
- **测试统计**：697 个测试 100% 通过

**✅ 独立任务管理页面 + 看板视图**
- **后端全局任务列表 API**：新增 `GET /api/v1/tasks` Handler，支持 `project_id`/`status`/`assignee_id`/`assignee_type`/`limit` 查询参数，复用 `TaskManage::list()` Domain 方法
- **ListTasksRequest DTO**：新增查询参数 DTO，标注 `#[param(source = "query")]` 供宏自动提取
- **前端 API 客户端**：新增 `list_tasks()` 函数，支持多维度筛选
- **任务管理页面**：`/tasks` 路由，包含统计概览卡片、筛选栏（项目/状态/负责人类型）、视图切换（列表/看板）
- **看板视图**：按 TaskStatus 五列分组（待审核/待处理/进行中/已完成/已归档），任务卡片含标题、优先级徽章、标签、进度条，点击跳转详情页
- **列表视图**：表格展示标题、状态、优先级、进度、负责人、项目、更新时间，行可点击跳转
- **CSS 样式**：看板列布局、卡片悬浮动效、筛选栏响应式布局
- **测试统计**：697 个测试 100% 通过

**✅ Agent 记忆面板**
- **记忆面板组件**：`AgentMemoryPanel` 组件，支持 Tab 切换（短期记忆/知识节点/关系）
- **搜索功能**：无关键词时调用 `query_memory` 按类型查询，有关键词时调用 `search_memory` 混合搜索
- **卡片展示**：类型徽章、内容预览（截取前 120 字符）、摘要、相似度分数
- **关系视图**：额外显示源节点 ID、关系类型徽章、目标节点 ID
- **Agent 详情页集成**：作为第七个 detail-section，传入 `agent_id` 自动加载记忆数据
- **CSS 样式**：16 个新 class，Tab 激活态用主色调，卡片悬浮阴影，记忆列表最大高度 400px 可滚动
- **后端零改动**：复用已有 `query_memory` / `search_memory` API 客户端和 DTO
- **测试统计**：697 个测试 100% 通过

**✅ 对话体验打磨**
- **消息复制**：hover 文本消息气泡显示"复制"按钮，点击调用 `web_sys::Navigator::clipboard().write_text()` 复制到剪贴板，toast 提示结果
- **快捷指令**：输入框输入 `/` 开头时显示快捷指令菜单，支持 `/clear`（清空对话）和 `/help`（显示帮助）
- **键盘导航**：↑↓ 选择菜单项、Enter 执行、Esc 关闭、实时过滤匹配指令
- **CSS 样式**：12 个新 class（消息操作按钮、快捷指令菜单、代码块高亮、代码块复制按钮）
- **web-sys 扩展**：`Cargo.toml` 添加 `Clipboard` 和 `Navigator` features
- **测试统计**：697 个测试 100% 通过

**✅ 实体统计数据动态注入**
- **FetchOptions 模式**：为 Project/Task/Tool/ModelProvider 补齐 FetchOptions + get_xxx(ctx, id, options) 方法，Agent 扩展增加 model_call_stats
- **按需注入**：通过 query 参数 `with_stats`/`with_model_call_stats` 控制是否返回统计数据，响应字段 None 时自动省略
- **时间范围与粒度**：支持 `stats_time_start`/`stats_time_end` 时间范围过滤，`stats_interval` 控制时序聚合粒度（hourly/daily）
- **后端 API**：5 个实体 GET 详情接口全部扩展支持统计参数，一次请求拿到实体+统计
- **严格分层**：Handler → Domain → DAL → DAO 单向调用，无跨层，DAL 层内部组合多个 DAO
- **测试统计**：697 个测试 100% 通过

### 2026-07-13 里程碑
**✅ 管理页面补全**
- **消息 API 路径修复**：前端 API 客户端添加 `/finance` 前缀，修复与后端路由不匹配问题
- **Agent/Project 详情页**：新增 Agent 详情页（技能包管理、工具包管理）、项目详情页（任务列表、进度展示）
- **创建弹窗完善**：技能库、定时触发器、消息渠道新增创建弹窗组件
- **枚举值映射统一**：前端状态映射与后端枚举值对齐（ProjectStatus 等）
- **项目任务数统计**：项目列表页显示每个项目的任务数量

**✅ 对话功能 MVP**
- **左右分栏布局**：左侧项目列表 + 右侧对话区，首页即为对话页
- **双向分页机制**：初始加载最新 10 条，上拉通过 `before_timestamp` 加载历史，下拉通过 `after_timestamp` 轮询新消息
- **3 秒短轮询**：MVP 阶段实时推送方案，后续可升级为 SSE/WebSocket
- **消息气泡展示**：区分用户/Agent/System 角色，不同颜色标识

**✅ 消息搜索、记忆搜索及知识图谱**
- **消息搜索 API**：新增 `search_messages` handler，支持 FTS5 关键词 + 向量语义混合搜索
- **记忆搜索 API**：复用 `search_memory`、`query_memory` 神经工具作为 HTTP 路由
- **消息搜索页面**：关键词搜索、结果表格展示、匹配类型（Hybrid/Vector/Keyword）、向量距离显示
- **记忆搜索页面**：关键词 + 类型筛选（短期记忆/知识节点/关系）、摘要和分数展示
- **知识图谱组件**：SVG 图谱渲染、圆形布局算法、节点连接线、标签显示
- **知识图谱页面**：搜索初始节点、图谱可视化展示

**✅ 测试修复**
- **ToolCallLogger 初始化**：修复 3 个 MCP 相关测试因 `ToolCallLogger` 未初始化导致的 panic
- **测试统计**：693 个测试 100% 通过

### 2026-07-11 里程碑
**✅ Runtime Domain Phase 4A - 工具包机制 + 任务执行闭环**
- **工具包 tag 机制**：通过 `tags` 字段分组工具，Agent 入职时自动安装指定 tag 工具包
- **AgentRuntimeConfig 扩展**：新增 `installed_tags: Vec<String>` 字段，记录 Agent 已安装工具包
- **免绑定校验三层逻辑**：绑定工具 → 神经工具（tags 含 "neural"）→ 已安装工具包（tags 与 installed_tags 有交集）
- **唤醒时加载内置工具**：`load_builtin_tools` 重命名扩展，支持神经工具 + 已安装工具包工具
- **Agent 入职自动安装**：状态流转到 Onboarded 时自动安装 "project_management" 工具包
- **工具包管理 API**：3 个新 Handler（install/uninstall/list installed tool packs）
- **TaskAssignment 消息类型**：`MessageType::TaskAssignment = 9` + `TaskAssignmentMessage` payload
- **send_task_assignment_message 神经工具**：Agent 间任务分配通知，封装 Message Domain 投递方法
- **任务创建 Handler 编排**：`create_task` 创建任务后自动发送 TaskAssignment 消息给 Agent
- **PromptBuilder 差异化**：`【任务分配通知】` 标签，Agent 唤醒时明确感知任务分配
- **三种角色定位明确**：神经工具 Handler（注册为工具）/ 普通 HTTP Handler（不注册）/ Consumer（直接调 Domain）
- **架构职责分离**：Project Domain 只管持久化，Message Domain 管通知，Handler 层编排
- **测试统计**：569 个测试 100% 通过（+15）

**✅ Runtime Domain Phase 4B - 记忆模块增强（定时触发器 + 休息与沉淀）**
- **4B-2 定时触发器系统**：
  - CronTriggerPo/Entity 定义、DAO/DAL/Domain 三层完整实现
  - 系统领域 API：创建/查询/更新/删除/启停触发器、列表查询
  - CronScheduler 后台扫描器：每 5 秒扫描 next_run_at <= now 的触发器
  - CronTriggerEvent 事件投递到 event_queue，CronTriggerConsumer 消费处理
  - 触发器 payload 设计：action + extra 通用结构，支持 agent_rest 等动作
- **4B-3 休息与沉淀机制**：
  - MemoryStatus 新增 Settled(2) 状态，标记已沉淀的短期记忆
  - MemoryDal.settle_short_term_to_long_term()：查询活跃短期记忆 → 创建知识节点 → 标记已沉淀
  - RuntimeMemory.settle() 领域层接口，RuntimeDomain.rest_and_settle() 完整休息流程
  - settle_memory 神经工具：Agent 可主动调用触发记忆沉淀
  - 状态流转：Idle → Resting → 沉淀 → Idle
- **4B-4 定时触发记忆沉淀**：
  - AgentRestPayload 结构体定义（agent_id + settle_limit）
  - handle_agent_rest 实现：解析 payload → 调用 RuntimeDomain.rest_and_settle()
  - 完整链路：CronScheduler 扫描 → 投递事件 → 消费者处理 → Agent 休息沉淀
- **测试统计**：576 个测试 100% 通过（+7）

### 2026-07-10 里程碑
**✅ Runtime Domain Phase 2 - 神经工具集完整落地**
- **宏扩展**：`register_handler_tool` 宏新增 `neural` flag 和 `tags` 参数，神经工具自动打 "neural" tag
- **RuntimeMemory 扩展**：新增 search/query/create/update/delete 5 个公开方法，统一记忆操作接口
- **8 个神经工具全部实现**：
  - 记忆类（5个）：search_memory、query_memory、create_memory、update_memory、delete_memory
  - 消息类（1个）：send_message
  - 工具类（2个）：request_tool_call、list_tools（标记为神经工具）
  - 任务类（1个）：mark_done
- **唤醒流程优化**：唤醒时自动筛选带 "neural" tag 的工具注入 Prompt
- **神经工具免绑定**：调用 Manual 工具时，神经工具无需绑定校验，Agent 天生具备
- **移除自动回复**：消息消费者不再自动生成回复，Agent 通过 `send_message` 神经工具主动发送
- **分层架构严格遵守**：所有 Handler 仅调用 Domain 层，无直接 DAL 调用
- **测试统计**：548 个测试 100% 通过

**✅ Runtime Domain Phase 3 - 多回合循环控制**
- **ToolStatsDao**：新增工具统计 DAO，支持工具调用次数/失败次数查询
- **AgentFetchOptions**：附带信息获取选项，通过参数控制按需注入统计数据
- **轮次限制检查**：消费者层检查 `max_thinking_depth`，超限后发送提示并终止循环
- **任务完成检测**：唤醒前检查任务状态，Completed/Cancelled/Archived 状态下跳过唤醒
- **Prompt 上下文差异化**：不同消息类型使用不同标签（【工具执行结果】、【确认请求】等）
- **工具失败计数注入**：PromptBuilder 新增工具失败警告接口，高失败率工具提醒 Agent 谨慎使用
- **唤醒失败事件记录**：大脑思考失败时也记录 AgentAwakeEvent，便于统计和排查
- **测试统计**：554 个测试 100% 通过（+6）

### 2026-07-06 里程碑
**✅ Project/Task 业务事件完整落地**
- **ProjectEvent**：新增项目生命周期事件表 `project_events`，含 `created`/`started`/`completed`/`archived`/`status_changed` 五种事件类型
- **TaskEvent**：新增任务生命周期事件表 `task_events`，含 `created`/`started`/`completed`/`cancelled`/`status_changed` 五种事件类型
- **操作者字段对齐**：统一使用 `operator_type` + `operator_id` 区分操作者类型（user/agent），支持 Agent 自动操作场景
- **归属人字段**：Project 用 `owner_type` + `owner_id`，Task 用 `assignee_type` + `assignee_id`，各自语义明确
- **Domain 层集成**：Project/Task Domain 层的 10 个状态变更方法全部接入事件记录（create/start/complete/archive/cancel/transition_status）
- **record_event! 宏优化**：改用 `stats_opt()` 替代 `stats()`，stats 未初始化时静默跳过而非 panic，测试更友好
- **事件记录原则**：状态变更必记录、创建删除必记录、关键动作记录、只读操作不记录
- **测试统计**：544 个测试 100% 通过

### 2026-07-05 里程碑
**✅ Agent 唤醒统计事件落地 + Stats DAO 数据源切换**
- **AgentAwakeEvent**：新增 `agent_awake_events` 表，记录 Agent 唤醒事件（唤醒次数、耗时、状态、关联消息等）
- **集成位置**：在 `RuntimeDomain.awaken()` 中记录唤醒事件，每次 Agent 唤醒成功后自动上报
- **数据源切换**：AgentStatsDao 从 `model_call_events` 切换到 `agent_awake_events`，统计内容从"模型调用次数"变为"Agent 唤醒次数"
- **演进验证**：验证了"领域先行，实现后续演进"的设计思路 — 接口不变，仅替换 DAO 实现，上层无感知
- **测试统计**：544 个测试 100% 通过

### 2026-07-05 里程碑（早期）
**✅ Stats DAO 领域拆分重构完成**
- **领域划分**：按领域而非实体划分职责，Agent/Project/Task StatsDao 只负责自身维度的 call_summary；ModelProviderStatsDao 升级为模型调用领域 DAO
- **通用结构体**：新增 `ModelCallStats` 通用结构体（call_summary + token_summary + model_call_time_series），所有实体复用
- **多维过滤**：ModelProviderStatsQuery 增加 agent_id/project_id/task_id 可选字段，支持多维度查询
- **DAL 层组装**：Agent/Project/Task DAL 注入 ModelProviderStatsDao，新增 `get_model_call_stats` 方法组装跨领域统计结果
- **接口精简**：DAL 层统计接口统一为 `get_stats(id, options)` + `get_model_call_stats(id, options)`，删除冗余语法糖方法
- **演进路径**：领域先行，未来实体有了专属统计表时只需替换 DAO 实现，上层无感知
- **测试统计**：544 个测试 100% 通过（+6）

### 2026-07-02 里程碑
**✅ 全实体 Stats DAO 层建设完成**
- **Agent Stats DAO 接口定义 + DuckDB 实现**：`AgentStatsDao` trait，含 `query`/`sum_tokens`/`query_time_series`
- **Project Stats DAO**：按 `project_id` 过滤，4 个单元测试
- **Task Stats DAO**：按 `task_id` 过滤，4 个单元测试
- **ModelProvider Stats DAO**：按 `model_provider_id` 过滤，4 个单元测试
- **统计模型迁移**：`StatsInterval`/`TimeSeriesPoint`/`TokenSumResult` 迁移到 `common/src/models/stats.rs`
- **Bug 修复**：
  - 聚合查询 JSON 返回格式不统一（展平 groups/aggregations）
  - `json_extract` 返回字符串带引号（改用 `json_extract_string`）
- **测试支持**：新增 `request_context_test_support.rs`、`storage/test_support.rs`
- **DAO 层扩展**：从 22 个增加到 26 个（+4 个 stats duckdb dao）
- **测试统计**：538 个测试 100% 通过（+12）
- **整体架构完成度**：~94%

### 2026-07-01 里程碑
**✅ 附件存储系统 + MCP 服务器集成完整落地**
- **附件系统上线**：通用 Attachment 上传 API，支持文件上传和文本创建两种模式，统一存储在日期分层目录
- **MCP 服务器完整支持**：MCP 服务器 CRUD、状态管理、工具同步、MCP 工具调用执行全链路打通
- **项目产物扩展**：Artifact 增加 `source_type` 字段，支持引用 attachment_id 创建产物
- **Finance Domain 完善**：新增 Attachment Domain、McpServer Domain、McpTool Domain、ToolProvider Domain
- **Handler API 全面覆盖**：6 大业务域 API 全部上线（organization/hr/finance/project/user/health）
- **DAO 层扩展**：从 20 个增加到 22 个（新增 attachment + mcp_server 核心 DAO）
- **DAL 层扩展**：从 13 个增加到 16 个（新增 attachment + mcp_server + mcp_tool）
- **测试统计**：516 个测试 100% 通过
- **整体架构完成度**：~92%

### 2026-06-23 里程碑
**✅ MCP 服务器与工具集成**
- 新增 MCP 服务器管理：创建、查询、更新、删除、状态切换
- MCP 工具同步：从 MCP 服务器拉取工具列表并持久化
- MCP 工具执行：通过 rmcp 客户端调用 MCP 工具
- 新增数据库迁移：`20260623000000_mcp_servers.sql`

### 2026-06-18 里程碑
**✅ 产物来源类型扩展**
- Artifact 增加 `source_type` 字段，支持区分不同来源的产物
- 支持引用 Finance 模块的 `attachment_id` 创建项目产物
- 新增数据库迁移：`20260618000000_artifact_add_source_type.sql`

### 2026-06-17 里程碑
**✅ 统一附件存储系统上线**
- 通用 Attachment 模块：上传、创建文本、查询、删除、内容更新
- FileMeta + 日期分层路径存储结构
- 支持 multipart 文件上传和纯文本创建两种模式
- 新增数据库迁移：`20260617000000_attachments.sql`

### 2026-05-15 里程碑
**✅ 日志系统完全宏化重构完成**
- **核心改造**：删除所有旧函数实现，8 个宏合并为 4 个，自动上下文检测
- **检测机制**：语法模式匹配，优先匹配字符串字面量为消息
  - 第一个参数是字符串字面量 → 无上下文模式
  - 第一个参数非字符串 + 第二个是字符串 → 带上下文模式
- **统一 API**：`log_info!` / `log_warn!` / `log_error!` / `log_debug!`
  - 无上下文：`log_info!("message {}", var)`
  - 带上下文：`log_info!(&ctx, "operation", "message {}", var)`
- **向后兼容**：保留 `sys_*` 宏系列作为无上下文别名
- **强制规范**：项目内禁止直接调用 `tracing::*!`，必须使用统一宏
- **测试统计**：所有测试 100% 通过
- **文档同步**：新增 `docs/logging_design.md` 完整设计文档
- **代码统计**：4 次提交，共 10 个文件修改

### 2026-05-11 里程碑
**✅ PO 与业务实体分层架构完整落地**
- **核心改造范围**：Project/Task/Artifact 三大业务对象全部完成分层重构
- **DAO 层**：仅操作 PO，单一职责，不包含业务组装
- **DAL 层**：内部完成 PO↔业务实体双向转换，对外接口统一使用业务实体
- **Domain 层**：100% 无 PO 依赖，所有异步方法携带 RequestContext
- **业务实体设计**：内部持有 `po: XxxPo` 字段，DAL 直接通过 `&xxx.po` 传递给 DAO
- **新增规范落地**：
  - ctx 跨层传递统一使用 `ctx.clone()`（内部 Arc，成本极低）
  - 写操作使用引用传递 & 避免 clone 不必要的 clone
  - TaskStatus::Cancelled = 0 设计为软删除，常规查询默认过滤
- **测试统计**：267 个测试 100% 通过（Project Domain 新增 9 个单元测试）
- **代码统计**：20 个文件修改，+1435/-943 行
- **文档同步更新**：`docs/project_management_design.md` 同步更新架构落地细节

### 2026-05-10 里程碑
**✅ Project Domain 骨架搭建完成**
- **架构设计**：`ProjectDomain` trait 包含 `management`（项目管理）和 `execution`（项目执行）两个子能力
- **业务实体优先**：所有方法入参和出参都使用业务实体（`CreateProjectCommand`、`UpdateProjectCommand`、`ProjectPo`）
- **严格分层**：Domain 层组合 DAL 层，不直接访问 DAO，符合单向依赖原则
- **核心功能**：
  - Management：创建、查询、更新、归档项目，统计项目数量，通用查询
  - Execution：启动、完成、重新激活项目，完整生命周期管理
- **测试覆盖**：9 个完整测试用例，所有核心功能验证通过
- **测试统计**：267 个测试 100% 通过（比前一天新增 9 个）

**✅ 全项目测试代码重构优化完成**（同日早期里程碑）
- **新增 DAL 层模块**：`project.rs`, `task.rs`, `artifact.rs` 及配套测试文件
- **重构 25 个测试文件**：DAO/DAL/Domain 三层全量优化
- **抽取公共初始化函数**：`init_test_env()` 统一初始化模式
- **工厂方法模式**：`create_test_agent()`, `create_test_project()` 减少重复代码
- **修复无限递归 bug**：3 个 domain 层测试文件的递归问题
- **测试统计**：258 个测试 100% 通过
- **代码统计**：29 个文件修改，+1910/-1008 行

所有开发过程和经验都归档在 [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)，包括：

- 每轮重构的背景、问题、解决方案
- 遇到的坑和避坑指南
- 架构决策的原因和权衡
- 最佳实践沉淀

> 💡 **开发前建议先看该文档**，避免重蹈覆辙

---

## 七、Known Issues & 解决方案

### 7.1 Rig 包名问题

**错误现象**：
```
error[E0670]: `async fn` is not permitted in Rust 2015
error[E0432]: unresolved import `rig::completion::ToolDefinition`
```

**解决方案**：
1. 确保 `Cargo.toml` 使用 `edition = "2024"`
2. 从正确路径导入：`use rig::tool::{ToolDyn, ToolError};`
3. 避免从 `rig::completion::*` 导入工具相关类型

---

*本文档是 AI 助手的快速入门手册，详细内容请查阅各专项设计文档*
