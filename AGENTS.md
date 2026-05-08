# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：5分钟了解项目是什么、代码怎么组织、开发遵循什么规范
>
> 最后更新：2026-05-08

---

## 一、项目概览

### 1.1 项目是什么

**AI Orz** - 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + rig-core 0.34
- **前端**：Dioxus 0.7 (WebAssembly)
- **技术特色**：严格分层架构、类型安全、450 个测试 100% 通过率

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
| 📋 任务 + 项目管理 | ✅ | 任务状态机，项目聚合对话上下文 |
| 📎 统一附件存储 | ✅ | 消息附件 + 项目产物，FileMeta + 日期分层路径 |
| 🚀 异步消费者系统 | ✅ | 通用消费者框架 + Message Topic 三层分发 |
| 🔍 向量搜索 | ✅ | SQLite VSS 扩展 + 语义索引 + 可平滑升级 |

### 1.3 当前测试统计

- **总测试数**: 450 个
- **通过率**: 100% ✅
- **覆盖范围**: 数据层 + 领域层 100% 覆盖

---

## 二、文档快速索引

> 📌 **按需要读取详细设计文档**

### 架构总览
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [README.md](./README.md) | 项目概览、快速开始、功能列表、文档索引 | ⭐⭐⭐ |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | 完整架构说明、核心概念解释、实体关系 | ⭐⭐⭐ |

### 分层架构与最佳实践
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | **开发记录**：分层架构完整落地过程、反模式、避坑指南、经验总结 | ⭐⭐⭐ |
| [docs/NAMING_CONVENTION.md](./docs/NAMING_CONVENTION.md) | 全项目统一命名约定、DAO/DAL/Domain 命名规则 | ⭐⭐ |
### 各模块详细设计
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、测试隔离 | ⭐⭐⭐ |
| [docs/memory_design.md](./docs/memory_design.md) | 四层记忆系统设计、检索策略 | ⭐⭐ |
| [docs/tool_design.md](./docs/tool_design.md) | 混合模式工具调用、工具注册表、调用追踪 | ⭐⭐ |
| [docs/message_interaction_design.md](./docs/message_interaction_design.md) | 消息交互架构、用户↔Agent双向对话、工具调用复用消息表 | ⭐⭐ |
| [docs/message_channel_design.md](./docs/message_channel_design.md) | 消息渠道系统设计、多渠道支持、状态管理 | ⭐⭐ |
| [docs/consumer_architecture.md](./docs/consumer_architecture.md) | 异步消费者框架、按 to_role 分层分发 | ⭐⭐ |
| [docs/task_scheduler_design.md](./docs/task_scheduler_design.md) | 任务调度器设计、Cron 表达式、定时任务执行 | ⭐⭐ |
| [docs/event_design.md](./docs/event_design.md) | 泛型 topic 事件队列、类型安全隔离 | ⭐⭐ |
| [docs/skill_design.md](./docs/skill_design.md) | 技能库系统、Agent 自进化沉淀技能 | ⭐⭐ |
| [docs/vector_search_architecture.md](./docs/vector_search_architecture.md) | 向量搜索架构、SQLite VSS 扩展集成 | ⭐⭐ |

### 数据层与规范
| 文档 | 内容 | 优先级 |
|------|------|--------|
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
| **Handler** | HTTP 路由、参数校验、调用 Domain | ❌ 直接调用 DAL/DAO |

### 3.2 目录结构

```
ai_orz/
├── common/                     # 公共共享 crate（前后端共用）
│   ├── src/api/               # API 请求响应 DTO 按功能分组
│   ├── src/constants/         # 公共常量、基础类型
│   └── src/enums/            # 公共枚举（UserRole、TaskStatus 等）
│
├── src/                        # 后端服务
│   ├── handlers/              # HTTP 接口层（按业务域分组，每个方法一个文件）
│   ├── service/
│   │   ├── dao/               # 数据访问层 DAO
│   │   ├── dal/               # 业务数据访问层 DAL
│   │   └── domain/            # 领域层 Domain
│   ├── models/                # PO 持久化实体
│   ├── middleware/            # Axum 中间件
│   └── pkg/                   # 公共工具包
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
| **API DTO** | `common/src/api/**` | HTTP 请求/响应，前后端复用 |
| **Command/Query** | `src/service/domain/*/mod.rs` | Domain 层输入，表达业务意图 |
| **业务实体** | `src/models/*.rs` | 核心业务对象，包含行为和状态 |
| **PO (持久化对象)** | `src/models/*.rs` | 数据库映射，1:1 对应表结构 |

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
- 所有 DTO 从 `common/src/api/` 导入

### 4.5 测试隔离原则

- 无状态组件可使用单例（OnceLock）
- 有状态内存组件必须每次新建实例
- 测试使用独立数据库，不依赖全局状态
- 所有测试使用 `#[sqlx::test]` 宏

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
