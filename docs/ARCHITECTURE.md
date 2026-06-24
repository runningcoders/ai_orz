//! # 架构说明

## 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务。

---

## 项目整体架构：三级 cargo workspace

```
ai_orz/
├── **common** 🎯 独立公共 crate
│   ├── src/api/              # 所有前后端共用 API DTO（按功能分组）
│   ├── src/constants/        # 公共常量、基础类型
│   └── src/enums/            # 公共枚举（UserRole 等）
│
├── **src** 后端服务
│   ├── models/               # 持久化实体 PO
│   ├── handlers/             # HTTP 接口层（按业务域/功能分组，每个方法对应一个用户 Action）
│   ├── service/
│   │   ├── dao/              # 数据访问层 DAO（单一数据源操作）
│   │   ├── dal/              # 业务数据访问层 DAL（组合 DAO 提供业务级数据操作）
│   │   └── domain/           # 领域层（核心业务逻辑）
│   ├── middleware/           # Axum 中间件（JWT认证、RequestContext注入）
│   └── pkg/                  # 公共工具包
│
└── **frontend** 前端 Dioxus 应用
    ├── src/
    │   ├── api/              # API 客户端（调用后端接口，所有 DTO 从 common 导入）
    │   └── components/        # UI 组件（每个页面一个组件）
    └── ...
```

**common crate 设计原则：**
- ✅ 所有前后端共用的 request/response DTO 都放在 `common/src/api/`，消除重复定义
- ✅ 通用响应包装 `ApiResponse<T>` 也只保留在 `common::api`，Handler 不再定义本地响应包装
- ✅ 所有公共枚举都放在 common，保证前后端类型一致
- ✅ PO 实体保持在后端 `models/`，不移动到 common（只需要前端看到 DTO）
- ✅ 后端数据库枚举字段直接使用 common 中的枚举类型，实现编译期类型安全

**Handler 层设计原则：**
- ✅ Handler 与用户 Action / HTTP API 直接对应，每个接口按自身需求完成请求级编排
- ✅ Handler 负责 DTO 解析、`RequestContext` 参数补全、DTO ↔ Command/Query 转换、响应 DTO 组装
- ✅ 复用优先通过组织 Command/Query 参数和调用 Domain 能力完成
- ✅ 管理面 API 补齐先按 Domain 能力盘点，再对照 Handler/Router 覆盖，不按 DAO/DAL 表结构生成接口
- ✅ 状态变更统一为 status update action，由请求参数传目标状态，避免 `/start`、`/complete`、`/enable`、`/disable` 等路由膨胀
- ❌ 不抽象 `BaseHandler` / `GenericActionHandler`，不通过 Handler 间互调复用逻辑
- ❌ 复杂业务规则、状态流转、权限语义放到 Domain，不写在 Handler 中

> 管理面 API 补齐方案见：`docs/handler_management_api_plan.md`

---

## 核心概念

### 1. Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信
- **关系**：直接持有装配好的 Brain，每个 Agent 属于一个组织，有角色字段

### 2. Brain（大脑）
- **定义**：聚合根，包含思考 + 记忆
- **结构**：
```rust
pub struct Brain {
    pub cortex: Cortex,           // 思考推理
    pub memory: Memory,         // 记忆系统 🧠
}
```

### 3. Memory（记忆系统）
- **定义**：分层记忆系统，按照人类认知设计
- **结构**：
```rust
pub struct Memory {
    pub core: CoreMemory,       // 核心认知 → soul + capabilities
    pub working: Vec<MemoryTrace>, // 当前会话工作记忆
}

pub struct CoreMemory {
    pub soul: String,           // 灵魂/性格/角色设定
    pub capabilities: String,   // 能力列表 JSON
}
```

### 4. Cortex（大脑皮层）
- **定义**：具体的思考推理执行，包含模型配置 + 推理实例
- **关系**：一个 ModelProvider 对应一个 Cortex

### 5. ModelProvider（模型提供商）
- **定义**：保存 LLM 模型配置信息，可以被多个 Agent 复用，属于一个组织

### 6. Organization（组织）
- **定义**：顶级租户，包含多个用户、多个 Agent、多个 ModelProvider
- **角色体系**：SuperAdmin → Admin → Member，支持权限控制

### 7. User（用户）
- **定义**：登录用户，属于一个组织，有角色和状态

### 8. EventQueue（事件总线）
- **定义**：轻量级内存事件队列，支持优先级排序和顺序保证
- **设计文档**：详见 [docs/event_design.md](./event_design.md)

---

## 组织用户权限体系

```
Organization (组织)
  └─► User (用户，通过 organization_id 关联)
       ├─► SuperAdmin (超级管理员 - 系统初始化时创建)
       ├─► Admin (管理员)
       └─► Member (普通成员)
```

**认证方案：** JWT + HttpOnly Cookie，适配单实例部署场景
- 公共路由：健康检查、初始化、登录、登出、获取组织列表 → 无需认证
- 保护路由：所有业务接口 → 需要 JWT 认证
- RequestContext 自动注入当前登录用户信息和组织 ID

---

## 🧠 记忆系统最终架构

记忆系统按照人类认知分为四层，设计原则：
- ✅ 核心认知在 Brain 内存，每次调用全部拼入 prompt
- ✅ 当前会话工作记忆在 Brain 内存，每次调用全部拼入 prompt
- ✅ 短期记忆索引存在 SQLite，需要时检索相关片段拼入
- ✅ 长期记忆知识图谱存在 SQLite，需要时检索相关片段拼入
- ✅ 原始细节按天存储为 markdown 文件，人类可读

| 层级 | 位置 | 存储 | 访问方式 | 内容 |
|------|------|------|----------|------|
| **Core Memory** 🎨 | Brain 内存 | 内存 + AgentPo 数据库 | 每次调用 **全部拼入 prompt** | 我是谁，我会做什么，我的性格 → 基础认知底色 |
| **Working Memory** ⚡ | Brain 内存 | 只在内存 | 每次调用 **全部拼入 prompt** | 当前会话正在进行的对话 |
| **Short-Term Memory** 📝 | SQLite 索引 + 按天文件存储原始细节 | 需要时检索相关摘要拼入 | 最近一段时间对话的归纳摘要 |
| **Long-Term Knowledge** 📚 | SQLite 知识图谱 + 按天文件存储原始细节 | 需要时检索相关知识拼入 | 归纳总结后的知识图谱节点，包含关系 |

### 文件存储结构（原始细节）

```
data/
  ├── ai_orz.db              # 主数据库（存索引和知识图谱）
  └── long_term_memory/       # 长期记忆原始细节
        └── {agent_id}/      # 按 Agent 分目录
              ├── 2026-04-07.md  # 一天一个 markdown 文件，追加写入，人类可读
              ├── 2026-04-06.md
              └── ...
```

**优点**：
- ✅ 文件数量极少 → 一年才 365 个文件，完全不会多
- ✅ 原始细节人类可读 → 直接打开就能看今天所有对话
- ✅ append-only 写入 → 不覆盖历史，天然版本控制
- ✅ 迁移简单 → 整个 data 目录打包就带走

---

## ⚡ Runtime Memory 运行时记忆架构（2026-05-16 新增）

### 设计目标

为 Agent 运行时提供统一的记忆读写接口，实现三层架构 100% 复用：
- **PO 层**：`Memory` / `MemoryCreateParams` / `MemoryTrace` - 持久化实体
- **DAL 层**：`MemorySearch` / `MemoryQuery` - 业务查询参数
- **Runtime 层**：最薄封装，零重复定义

### 核心接口设计

```rust
#[async_trait]
pub trait RuntimeMemory: Send + Sync {
    /// 写入记忆（复用 DAL 层 MemoryCreateParams）
    async fn write(&self, ctx: RequestContext, params: &MemoryCreateParams) -> Result<Memory, AppError>;
    
    /// 搜索记忆（复用 DAL 层 MemorySearch）
    async fn search(&self, ctx: RequestContext, search: &MemorySearch) -> Result<Vec<Memory>, AppError>;
    
    /// 查询记忆（复用 DAL 层 MemoryQuery）
    async fn query(&self, ctx: RequestContext, query: &MemoryQuery) -> Result<Vec<Memory>, AppError>;
}
```

### 实现模式

| 模式 | 说明 |
|------|------|
| **最薄封装** | Runtime 层不新增任何结构体，全部复用 PO/DAL 层定义 |
| **零转换成本** | 参数直接透传给 DAL，无字段映射开销 |
| **100% 兼容** | PO/DAL 新增字段时，Runtime 层自动获得支持 |
| **单例访问** | `runtime_memory()` 返回 `&'static Arc<dyn RuntimeMemory>` |

### 架构优势

1. **职责清晰**：Runtime 层只做统一入口，业务逻辑完全在 DAL 层
2. **维护成本低**：一处修改，三层同步受益，无重复代码
3. **可扩展性强**：后续新增语法糖在 Runtime 层追加方法，不修改核心契约
4. **对齐规范**：与 `tool_execution` 实现风格完全一致，保持 Domain 层架构统一

---

## 事件总线架构

详见独立设计文档：[docs/event_design.md](./event_design.md)

### 核心设计要点

| 设计点 | 实现方案 |
|--------|----------|
| 持久化 | 所有事件先存入 SQLite `messages` 表，总线只存 `message_id` 元数据 |
| 崩溃恢复 | 服务启动自动从数据库恢复所有 pending 事件 |
| 优先级排序 | 按 `priority DESC, created_at ASC` 排序，高优先级先出队 |
| 顺序保证 | 相同 `order_key` 保证顺序处理，不同 `order_key` 可以并行 |
| 并发模型 | Tokio 任务调度，相同 key 顺序锁保证顺序 |

---

## 最终实体层次关系

```
Agent (po + brain: Option<Brain>)
  └─► Brain 🧠
       ├─► Cortex (model_provider: ModelProvider, cortex: Box<dyn CortexTrait>)
       └─► Memory
            ├─► CoreMemory (soul: String, capabilities: String)
            └─► working: Vec<MemoryTrace>

Project + Task + Artifact 聚合
  └─► Project
       ├─► po: ProjectPo
       └─► tasks: Vec<Task>
            └─► artifacts: Vec<Artifact>
```

---

## 最新架构完成状态（2026-05-16 更新）

### 总体完成度：**~80%** 🎯

| 层级 | 完成度 | 状态 | 关键进展 |
|------|--------|------|---------|
| **DAO 层** | 100% ✅ | 完成 | 20 个 DAO 全部实现并被使用，零闲置 |
| **DAL 层** | 100% ✅ | 完成 | 13 个 DAL 全部接入业务，无闲置 |
| **Domain 层** | 80% ✅ | 大部分完成 | 7 个领域，5 个完整实现 + 2 个待补 Handler |
| **Handler 层** | 50% ⚠️ | 进行中 | 3 个领域 API 已上线，3 个待补充 |

---

### 已完成架构里程碑

#### ✅ 1. Message Domain 全链路完成
- **消息管理 + 8 个渠道管理 + 多渠道投递** 全部实现
- **67/67 测试通过**，覆盖率 100%
- 纯 match 分发架构，无 trait，无循环依赖
- 所有渠道 DAO 完全独立平放在 dao/ 下
- DAL 统一整合配置管理 + 消息分发

#### ✅ 2. Project/Task/Artifact 领域完整实现
- **23 个 DAL 方法** + **Domain 层完整业务编排**
- PO 与业务实体分层：业务实体内部持有 PO，DAL 层转换
- 软删除模式：`status = 0` 视为已删除，常规查询自动过滤
- 聚合模式：Project 聚合 Task，Task 聚合 Artifact

#### ✅ 3. Tool Domain 完整实现
- **27 个 DAL 方法** + **management 业务逻辑**
- 混合模式工具调用：简单工具走 rig auto，关键工具走自建 manual 链路
- 工具调用记录复用消息表：工具调用本身是特殊 Message

#### ✅ 5. Runtime Memory 运行时记忆模块完成
- **Runtime Domain 新增 Memory 子模块**，实现记忆读写通用接口
- **最薄封装设计**：100% 复用 DAL 层方法和结构体，核心接口零重复定义
- **三层统一复用**：`MemoryCreateParams` → `MemorySearch` → `MemoryQuery` 全链路复用，PO/DAL 新增字段时 Runtime 层自动兼容
- **对齐 Domain 架构规范**：与 `tool_execution` 实现风格一致，采用 `Arc<dyn Trait>` 单例模式

#### ✅ 6. 分层架构执行质量 100% 合规
- 所有 Handler **严格通过 Domain 调用**，没有直接调用 DAL/DAO
- 依赖方向完全正确：上层依赖下层，无反向依赖
- Domain 通过 `Arc<dyn Trait>` 注入 DAL，完全符合 DIP 原则

---

### 待完善架构项

| 优先级 | 任务 | 说明 |
|--------|------|------|
| **P0** | Tool / MCP Tool 运行面状态一致性 | Tool 管理 API 与 MCP Tool sync/list 已接入；Batch H 设计采用 `ToolStatus::Stale`：远端删除/改名时保留本地 ToolPo/绑定/审计，但排除 Prompt 可见、Runtime 执行与默认正常业务 list/search；MCP Server tools 管理列表可显式查询 Stale |
| **P1** | Runtime Memory 补充上层语法糖 | 核心接口已完成，可根据业务需要追加便捷方法 |
| **P1** | message 消费推送全链路 | consumer → domain_message → dal_message_channel；Manual ToolCallRequest → Runtime → ToolCallResult 最小闭环已完成，后续补二次推理与更多 E2E |
| **P2** | Agent 思考记忆链路 | Agent 触发 → Runtime Memory 读写 |

---

## 分层职责清晰化

| 层级 | 模块 | 职责 |
|------|------|------|
| **models** | 实体定义 | 定义所有持久化对象和业务实体 |
| **service/dao** | 数据访问层 | 数据库访问，文件读写 |
| **service/dal** | 业务逻辑层 | 组合 dao 完成业务逻辑 |
| **service/domain** | 领域层 | 核心业务规则，编排 dal |
| **handlers** | HTTP 接口层 | 接收请求，调用 domain，返回响应 |

---

## 三层设计哲学

经过多轮开发实践，总结出各层的核心设计思想：

| 层级 | 核心 | 设计要点 |
|------|------|---------|
| **dao** | **注重多态** | 定义接口协议，不同存储有不同实现，上层业务面向接口编程。例如：`ToolDao` 定义接口，`ToolDaoSqliteImpl` 提供 SQLite 实现。 |
| **dal** | **注重继承** | 基础 dal 实现通用逻辑，特殊需求通过继承基础 dal 进行扩展。基础 dal 复用通用逻辑，特殊 dal 只需要实现差异部分。 |
| **domain** | **注重组合** | 一个领域组合多个 dal，通过编排完成业务逻辑，保持高内聚低耦合。上层 handler 通过组合不同领域完成具体业务。 |

**图示：**
```
dao:  [ToolDao ◇─── ToolDaoSqliteImpl]  (多态：接口 → 多种实现)
dal:  [ToolDalBase ◇─── ToolDalSpecial] (继承：基础 → 扩展特殊)
domain: [ToolDomain ←---- (ToolDal + AgentDal + MessageDal)] (组合：多个 dal 编排)
```

---

## 设计原则

1. **严格分层不跨级调用** → 遵循 `handlers → domain → dal → dao → models` 层级依赖
2. **所有 service 层方法必须传递 RequestContext** → 方便日志追踪和扩展
3. **原始细节不占内存** → 短期长期都在数据库，只在需要时检索
4. **渐进式演进** → 短期积累到一定数量触发归纳，不断更新核心记忆和知识图谱
5. **人类可读** → 原始细节按天 markdown 存储，不需要工具直接查看

---

## 分层架构最佳实践

> **重要**：经过工具绑定架构的重构实践，我们总结了完整的分层架构规范和陷阱规避指南。
>
> 📖 **详细实践记录**：请参考 [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md)

### 核心分层原则重申

```
Handler (API)
    │
    ▼
Domain (领域逻辑) ← 组合多个 DAL，编排业务流程
    │
    ▼
DAL (业务数据) ← 组合多个 DAO，组装业务实体
    │
    ▼
DAO (数据访问) ← 单一数据源 CRUD
```

### 绝对禁止的反模式

| 反模式 | 危害 |
|--------|------|
| ❌ DAO 层调用其他 DAO | 分层边界模糊，测试隔离困难 |
| ❌ DAL 层调用其他 DAL | 循环依赖风险，复杂度失控 |
| ❌ 跨层直接访问（如 Handler 直接调 DAO） | 业务逻辑散落，难以维护 |
| ❌ DAO 层做实体组装/装饰 | 业务逻辑泄露到数据层 |

---

### PO 与业务实体分层规范（2026-05-11 新增）

经过 Project/Task/Artifact 三大模块的完整重构，我们确定了 PO 与业务实体的分层边界设计。

#### 核心原则
**PO 仅在 DAO/DAL 层内部使用，绝对不对外暴露到 Domain 层及以上**

| 层级 | 可使用对象 | 数据转换 | 接口签名规范 |
|------|------------|----------|------------|
| **DAO 层** | 仅 PO | PO ↔ 数据库 | `fn create(&self, po: &XxxPo) -> Result<()>` |
| **DAL 层** | 内部：PO，对外：业务实体 | PO ↔ 业务实体 | `fn create(&self, ctx: RequestContext, entity: &Xxx) -> Result<()>` |
| **Domain 层** | 仅业务实体 | 业务实体 ↔ Command | 所有方法无 PO 依赖 |
| **Handler 层** | 业务实体 + DTO | DTO ↔ 业务实体 | HTTP 接口层 |

#### 业务实体标准设计

**模式：业务实体内部持有 PO 字段**
```rust
pub struct Project {
    pub po: ProjectPo,  // 内部持有 PO
    // 可选：额外业务方法和计算字段
}

impl Project {
    pub fn id(&self) -> &str { &self.po.id }
    pub fn status(&self) -> ProjectStatus { self.po.status }
    // 业务方法...
}
```

**设计优势：**
1. ✅ **零转换成本**：DAL 层直接通过 `&xxx.po` 传递给 DAO，无需字段逐一映射
2. ✅ **易维护**：修改 PO 字段时只需修改一处，业务实体自动兼容
3. ✅ **100% 向后兼容**：现有测试和业务逻辑无需修改
4. ✅ **高性能**：写操作使用引用传递 `&`，避免不必要的 clone

#### DAL 层接口设计范式

```rust
#[async_trait]
pub trait ProjectDal: Send + Sync {
    // 写操作：接收 &业务实体 引用
    async fn create(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;
    async fn update(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;
    
    // 读操作：返回 业务实体
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>, AppError>;
    async fn list_by_user(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<Project>, AppError>;
}
```

#### RequestContext 跨层传递规范

**统一使用 `ctx.clone()`：**
```rust
// ✅ 正确
self.project_dal.create(ctx.clone(), project).await?;
self.task_dal.create(ctx.clone(), task).await?;

// ❌ 错误：所有权移动后无法继续使用
// self.project_dal.create(ctx, project).await?;
```

**理由：**
- RequestContext 内部是 Arc 引用，clone 成本极低（仅指针复制）
- 避免所有权移动导致的编译错误
- 与 message domain 风格保持一致

#### 软删除设计范式

**`status = 0` 视为软删除，常规查询默认过滤：**

```rust
// DAO 层示例
async fn find_by_id(&self, id: &str) -> Result<Option<TaskPo>> {
    sqlx::query_as!(
        TaskPo,
        r#"SELECT ... FROM tasks WHERE id = ? AND "status" != 0"#,
        id
    ).fetch_optional(&self.pool).await.map_err(Into::into)
}
```

**典型场景：**
- `TaskStatus::Cancelled = 0` - 取消的任务视为已删除
- 需要查询历史/恢复时，使用 `query` 方法绕过过滤
- 测试适配：cancel 后 get 返回 None 是预期行为

> 📖 **完整重构记录和决策过程**：请参考 [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md) 和 [project_management_design.md](./project_management_design.md)

---

`rig-core` crate 的内部模块结构在版本升级时可能发生变化，导致编译错误。**解决方案：**

- 确保 `Cargo.toml` 使用 `edition = "2024"`
- 从正确路径导入：`use rig::tool::{ToolDyn, ToolError};`
- 避免从 `rig::completion::*` 导入工具相关类型
- 必要时锁定精确版本：`rig-core = "=0.34"`

完整问题记录和解决方案详见 [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md)

---

## 支持的模型提供商

| 提供商 | 实现文件 | 支持 |
|--------|----------|------|
| OpenAI 官方 | `service/dao/cortex/rig/openai.rs` | ✅ |
| DeepSeek | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| 阿里云通义千问 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| 字节跳动豆包 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| Ollama 本地 | `service/dao/cortex/rig/ollama.rs` | ✅ |
| 自定义 OpenAI 兼容接口 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |

---

## 类型安全设计

### 枚举类型安全

项目中所有存储为整数的枚举字段，现在都直接使用 Rust 枚举类型存储：

| PO 实体 | 枚举字段 | 枚举类型 | 说明 |
|---------|----------|----------|------|
| `AgentPo` | `status` | `AgentStatus` | 已完成 |
| `ModelProviderPo` | `status` | `ModelProviderStatus` | 已完成 |
| `ModelProviderPo` | `provider_type` | `ProviderType` | 已完成 |
| `OrganizationPo` | `status` | `OrganizationStatus` | 已完成 |
| `UserPo` | `role` | `UserRole` (common) | 已完成 |
| `UserPo` | `status` | `UserStatus` | 已完成 |

**实现方式：**
- `common` 中定义枚举，为枚举实现 `rusqlite::ToSql` 和 `rusqlite::FromSql` trait
- 存储到 SQLite 自动转换为 `i32`，读取自动转换为枚举
- 编译期类型检查，避免 magic number 错误
- serde 序列化保持整数输出，API 契约不变

## 数据库设计

所有建表语句都统一放在 `src/pkg/storage/sql.rs` 作为常量，每个常量注释对应到实体：

| 表名 | 对应实体 |
|------|----------|
| `agents` | `AgentPo` |
| `model_providers` | `ModelProviderPo` |
| `organizations` | `OrganizationPo` |
| `users` | `UserPo` |
| `messages` | `MessagePo` (事件总线消息) |
| `tasks` | `Task` |
| `short_term_memory_index` | `ShortTermMemoryIndexPo` |
| `long_term_knowledge_node` | `LongTermKnowledgeNodePo` |
| `knowledge_reference` | `KnowledgeReferencePo` |
| `knowledge_node_relation` | `KnowledgeNodeRelationPo` |

---

## 单元测试规范

- 每个 DAO/DAL/Domain 模块对应一个单元测试文件
- 每个单元测试独立，使用随机临时 SQLite 文件，互不干扰
- 每个测试在执行前重新初始化 storage，保证干净环境
- 所有建表使用定义好的常量，不重复写 SQL
- 当前项目总测试数：**158 个** → **全部通过** ✅

### 测试设计要点

| 问题 | 解决方案 |
|------|----------|
| OnceLock 只能初始化一次 | 每个测试重新初始化 storage，使用随机数据库文件名 → 完全独立 |
| 一个测试 panic 影响其他 | 每个测试独立运行，互不干扰 → 失败只影响自己 |
| 代码可读性 | 每个测试短小精悍，独立清晰 → 好维护 |
