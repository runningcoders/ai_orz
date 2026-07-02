# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：5分钟了解项目是什么、代码怎么组织、开发遵循什么规范
>
> 最后更新：2026-07-02

---

## 一、项目概览

### 1.1 项目是什么

**AI Orz** - 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + rig-core 0.34
- **前端**：Dioxus 0.7 (WebAssembly)
- **技术特色**：严格分层架构、类型安全、538 个测试 100% 通过率

### 1.2 已实现核心功能

| 模块 | 状态 | 说明 |
|------|------|------|
| 👥 组织用户权限 | ✅ | 多级组织、用户角色、JWT 认证 |
| 🤖 Agent 全生命周期 | ✅ | 创建、配置、工具绑定、唤醒执行 |
| 🧠 四层记忆系统 | ✅ | Core/Working/Short-term/Long-term |
| 💬 消息对话系统 | ✅ | 用户 ↔ Agent 双向对话，支持项目上下文 |
| 📨 消息渠道系统 | ✅ | 多渠道消息接入，支持启用/禁用/测试 |
| 🛠️ 混合模式工具调用 | ✅ | 简单工具走 rig auto，关键工具走自建 manual 可控链路 |
| 📚 技能库系统 | ✅ | 可复用技能和工作流，支持搜索和分类 |
| 📋 任务 + 项目管理 | ✅ | 任务状态机，项目聚合对话上下文，DAL + Domain 层完整实现 |
| 📎 统一附件存储 | ✅ | 消息附件 + 项目产物，FileMeta + 日期分层路径 |
| 🔌 MCP 服务器集成 | ✅ | MCP 服务器管理、工具同步、MCP 工具调用执行 |
| 🚀 异步消费者系统 | ✅ | 通用消费者框架 + Message Topic 三层分发 |
| 📝 结构化日志系统 | ✅ | JSON 格式、自动上下文关联、日志自动清理 |
| 🔍 向量搜索 | ✅ | SQLite VSS 扩展 + 语义索引 + 可平滑升级 |
| 📊 Agent 统计系统 | ✅ | DuckDB 多维统计、Agent/Project/Task/ModelProvider 四维度覆盖 |

### 1.3 整体完成度与测试统计（2026-07-02 更新）

| 指标 | 数值 | 说明 |
|------|------|------|
| **总测试数** | **538** | DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
| **通过率** | **100%** | ✅ 全部测试通过 |
| DAO 模块数 | 26 个 | 全部实现并被使用，零闲置（21 核心 DAO + 5 渠道 DAO） |
| DAL 模块数 | 16 个 | 全部完整业务承载，零闲置 |
| Domain 领域数 | 6 个 | 全部完整实现 |
| Handler API 领域数 | 6 个上线 | organization, hr, finance, project, user, health |
| **整体架构完成度** | **~94%** | 从下往上扎实推进 |

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
| [docs/vector_search_architecture.md](./docs/vector_search_architecture.md) | 向量搜索架构、SQLite VSS 扩展集成 | ⭐⭐ |

### 基础设施与规范
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/logging_design.md](./docs/logging_design.md) | **日志系统设计**：统一宏使用规范、上下文检测机制、tracing 语法速查 | ⭐⭐⭐ |
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、STRICT 模式、测试隔离 | ⭐⭐⭐ |
| [docs/task_design.md](./docs/task_design.md) | 任务系统设计、状态机、分配与进度追踪 | ⭐ |
| [docs/project_design.md](./docs/project_design.md) | 项目系统设计、聚合对话上下文 | ⭐ |
| [docs/organization_design.md](./docs/organization_design.md) | 组织用户权限体系设计 | ⭐ |
| [docs/attachment_storage.md](./docs/attachment_storage.md) | 产物与消息附件统一存储设计 | ⭐ |

### 前端与 UI
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/ui_design_system.md](./docs/ui_design_system.md) | UI 设计系统、配色、排版、组件规范 | ⭐ |

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
