# 分层架构重构实践记录

> 🎯 **本文档定位**：开发者实操手册——怎么正确写分层代码、哪些坑不能踩、反模式长什么样（CODE_STANDARDS.md §2 数据对象分层 + §3 Trait 位置规范的配套示例化展开）
>
> 状态：持续同步，随开发中发现的新反模式与正确写法追加更新
>
> 查阅场景：
> - 写新 DAO/DAL/Domain/Handler 前不确定职责边界 → 查对应章节「✅ 正确 / ❌ 反模式」
> - Code Review 发现职责混淆时，把对应章节链接作为评审依据附上
> - 排查跨层依赖/同层互调导致的测试难隔离问题时，按 §典型反模式定位
>
> 关联文档：
> - [CODE_STANDARDS.md](./CODE_STANDARDS.md) — 编码规范 SSOT（分层边界、数据对象、Trait 位置权威定义）
> - [AGENTS.md](../AGENTS.md) — Agent 快速入门手册
> - [ARCHITECTURE.md](./ARCHITECTURE.md) — 唯一权威架构总纲

> 记录日期：2024-04-28
> 背景：工具绑定架构从 DAO 层组装重构为严格分层调用

---

## 📋 问题背景

最初的设计存在**职责混淆**问题：
- `CortexDao` 直接依赖 `ToolCallDao` 进行工具实体组装
- DAO 层承担了超出"数据持久化"的业务逻辑
- 跨 DAO 依赖导致分层边界模糊，测试隔离困难

**反模式示例：**
```rust
// ❌ 错误：DAO 层跨领域依赖其他 DAO
impl CortexDao for CortexDaoSqliteImpl {
    async fn wake(&self, ctx: RequestContext, brain_id: Uuid) -> Result<Brain> {
        // ToolCallDao 被直接在 DAO 层调用
        let tool_dao = ToolCallDaoSqliteImpl::new();
        let tools = tool_dao.get_enabled_tools(ctx).await?;
        // ...
    }
}
```

---

## ✅ 最终分层架构方案

### 核心原则

**单向依赖 + 逐层调用 + 单一职责**

```
Adapter (适配层：HTTP Handler + AOP Producer)
    │  所有外部输入的入口：用户 HTTP API、外部系统回调、WS 事件、定时轮询
    │  职责：协议解析、校验、ID 映射 → 直接调用 Domain 方法
    ▼
Domain (领域层) → 组合多个 DAL，实现业务逻辑，产生内部事件
    │
    ▼
DAL (业务数据层) → 组合多个 DAO，提供业务级数据操作
    │
    ▼
DAO (数据访问层) → 单一数据源操作（本地 DB CRUD + 外部 API 出站调用）
```

### 各层职责边界

| 层级 | 可以做 | 禁止做 |
|------|--------|--------|
| **DAO** | 单一数据源的数据访问<br>本地 DB：SQL 拼接、PO 读写<br>外部 API：出站调用、出站格式转换（如 Markdown→飞书卡片） | ❌ **同层 DAO 互调**<br>❌ 向上调用 DAL/Domain<br>❌ 业务逻辑<br>❌ 实体组装/装饰 |
| **DAL** | ✅ **依赖多个 DAO**（业务决定）<br>提供业务级数据接口<br>PO ↔ Entity 转换 | ❌ **同层 DAL 互调**<br>❌ 向上调用 Domain |
| **Domain** | ✅ **依赖多个 DAL**（业务决定）<br>核心业务逻辑编排<br>跨领域事务<br>产生内部事件（如 MessageCreatedEvent） | ❌ **同层 Domain 互调**<br>❌ 直接调用 DAO（跨层）<br>❌ 直接调用外部 API（应通过外部 DAO） |
| **Adapter（适配层）** | 外部输入适配：HTTP Handler（用户 API）、公开回调 Handler、AOP Producer（WS/轮询）<br>协议解析、参数校验、鉴权<br>DTO/外部结构 ↔ Command 转换<br>外部 ID ↔ 内部 ID 映射<br>幂等检查<br>按 Action 编排 Domain 调用<br>组装响应（HTTP Handler） | ❌ 直接调用 DAL/DAO（跨层）<br>❌ 承载核心业务规则<br>❌ 把外部协议包装成内部事件投递<br>❌ Handler/Producer 之间互调<br>❌ 抽象通用 Adapter 框架 |

> 💡 **设计哲学**：Handler（面向用户 HTTP API）、公开回调 Handler（面向外部系统 HTTP 回调）、AOP Producer（面向外部 WS 事件/定时轮询）三者**同属适配层（Adapter）**，职责完全相同——把外部世界的输入适配成内部 Domain 方法调用。它们的区别仅在于对接的外部对象不同、触发方式不同。出站外部调用统一封装在外部 DAO 中，由 DAL 组合、Domain 编排。详见「实践 7：适配层架构原则」。

---

## 🔧 具体重构实践

### 1. Tool 实体组装重构

**问题**：`Tool` 实体需要 `ToolPo` + `Box<dyn CoreTool>` 两部分组合而成。最初想在 DAO 层完成组装。

**解决方案**：分层组装
- **DAO 层 (`ToolCallDao`)**：只负责从数据库读取 `ToolPo`，不做任何组装
- **DAL 层 (`ToolDal`)**：调用 `ToolCallDao` 获取 PO，然后通过 `ToolRegistry` 查找注册的工厂，组装完整 `Tool` 实体
- **Domain 层**：调用 `ToolDal` 获取完整工具列表，注入到 Agent/Brain

**代码示例：**
```rust
// ✅ DAO 层：只做数据读取
impl ToolCallDao for ToolCallDaoSqliteImpl {
    async fn get_tools_by_agent(&self, ctx: RequestContext, agent_id: Uuid) -> Result<Vec<ToolPo>> {
        // 纯 SQL 查询
    }
}

// ✅ DAL 层：负责业务组装
impl ToolDalImpl {
    async fn list_tools_by_agent(&self, ctx: RequestContext, agent_id: Uuid) -> Result<Vec<Tool>> {
        let pos = self.tool_call_dao.get_tools_by_agent(ctx, agent_id).await?;
        
        // 通过注册表过滤并组装完整实体
        let registry = get_registry();
        let mut tools = Vec::new();
        for po in pos {
            if let Some(factory) = registry.get_factory(&po.id) {
                let our_tool = factory.create(po.clone());
                tools.push(Tool { po, our_tool });
            }
        }
        Ok(tools)
    }
}
```

### 2. 工具注册表模式

引入 `ToolRegistry` 单例模式，解决工具实例化问题：

```rust
// 工具工厂 trait
pub trait BuiltinToolFactory: Send + Sync {
    fn id(&self) -> &str;
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool>;
}

// 注册表（可扩展支持动态注册）
pub struct ToolRegistry {
    factories: RwLock<HashMap<String, Box<dyn BuiltinToolFactory>>>,
}

impl ToolRegistry {
    pub fn register_builtin_factory(&self, factory: Box<dyn BuiltinToolFactory>) {
        self.factories.write().insert(factory.id().to_string(), factory);
    }
    
    pub fn get_factory(&self, id: &str) -> Option<Box<dyn BuiltinToolFactory>> {
        self.factories.read().get(id).cloned()
    }
}
```

**设计优点：**
- ✅ 支持编译期静态注册 + 运行期动态注册
- ✅ 解耦工具定义和实例化
- ✅ 支持热插拔和扩展

### 3. 附带信息模式（Fetch Options）

**问题**：业务实体（如 Agent、Project、Task）除了核心 PO 数据外，还经常需要补充各种"附带信息"——运行时状态、统计数据、绑定的工具/技能等。如果每种附带信息都写一个单独的查询方法，接口会爆炸；如果每次都全量加载，性能又不好。

**解决方案**：在 DAL 层引入 `XxxFetchOptions` 结构体，通过布尔标志控制是否加载某项附带信息，由调用方按需选择。

```
调用方（Handler / Domain / 消费者）
    │
    ▼
DAL 层：find_by_id(id, fetch_options)
    │
    ├── 主查询：加载核心 PO（必选）
    ├── with_runtime_state → 从 AgentRuntimeStateManager 注入
    ├── with_stats → 调用 StatsDao 查询统计数据
    ├── with_skills → 调用 SkillDao 查询绑定技能
    └── ...（未来扩展）
```

#### 各层职责划分

| 层级 | 查询结构体 | 职责 |
|------|-----------|------|
| **DAO 层** | `XxxQuery` / `XxxStatsQuery` | 单一数据源的查询参数，只针对一张表 |
| **DAL 层** | `XxxFetchOptions` | 复合选项，控制从哪些 DAO 加载什么附带信息 |
| **Domain 层** | Command / Query | 表达业务意图，不直接操作查询参数 |

#### 设计原则

1. **按需加载**：所有附带信息默认不加载（None 表示 false），调用方显式开启
2. **单一入口**：所有附带信息的开关都在同一个 `XxxFetchOptions` 结构体里
3. **过滤条件下传**：附带信息的过滤条件（如统计的 task_id 过滤）作为 options 的子字段下传到对应 DAO
4. **可扩展**：新增附带信息只需加一个 `with_xxx: Option<bool>` 字段，不破坏现有接口

#### 代码示例

```rust
// ✅ DAL 层：复合选项结构体
#[derive(Debug, Clone, Default)]
pub struct AgentFetchOptions {
    /// 是否加载运行时状态
    pub with_runtime_state: Option<bool>,
    /// 是否加载统计信息
    pub with_stats: Option<bool>,
    /// 统计过滤条件（with_stats=true 时生效）
    pub stats_filter: Option<AgentStatsFilter>,
    // 未来扩展：
    // pub with_skills: Option<bool>,
    // pub with_tools: Option<bool>,
}

// ✅ DAL 层：实现按需注入
impl AgentDal for AgentDalImpl {
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AgentFetchOptions,
    ) -> Result<Option<Agent>> {
        let opt = self.agent_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut agent) = opt.map(Agent::from_po) else {
            return Ok(None);
        };

        // 按需注入运行时状态
        if options.with_runtime_state.unwrap_or(true) {
            agent = Self::inject_runtime_state(agent);
        }

        // 按需注入统计信息
        if options.with_stats.unwrap_or(false) {
            let stats = self.agent_stats_dao.get_stats(
                ctx,
                AgentStatsQuery {
                    agent_id: agent.po.id.clone(),
                    filters: options.stats_filter.unwrap_or_default().into_filters(),
                    ..Default::default()
                },
                StatsFetchOptions {
                    with_call_summary: true,
                    ..Default::default()
                },
            ).await?;
            agent.stats = stats;
        }

        Ok(Some(agent))
    }
}
```

#### 两种使用方式

```rust
// 方式 1：只需要统计 → 直接调用统计方法
let stats = agent_dal.get_stats(ctx, &agent_id, options).await?;

// 方式 2：已经在获取实体 → 通过 options 带回去
let agent = agent_dal.find_by_id(
    ctx,
    &agent_id,
    AgentFetchOptions {
        with_stats: Some(true),
        stats_filter: Some(AgentStatsFilter {
            task_id: Some(task_id.to_string()),
            ..Default::default()
        }),
        ..Default::default()
    },
).await?;
```

---

## 🧪 测试最佳实践

### 1. 测试隔离原则

**每个测试都是独立的，不依赖其他测试的副作用**

```rust
#[sqlx::test]
async fn test_add_tool_to_agent(pool: SqlitePool) {
    // ✅ 每个测试独立初始化各层
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    register_test_factory(); // 测试专用工厂
    
    let tool_dal = tool::get_dal();
    // ... 测试逻辑
}
```

### 2. 测试专用 Mock 工厂

在测试环境中，使用简单的测试工厂验证组装逻辑，不依赖真实工具实现：

```rust
// 测试工具工厂
#[derive(Clone)]
struct TestToolFactory;

impl BuiltinToolFactory for TestToolFactory {
    fn id(&self) -> &str {
        "test_tool"
    }
    
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(TestTool { po })
    }
}

// 测试工具实现
#[derive(Clone)]
struct TestTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for TestTool {
    fn po(&self) -> &ToolPo {
        &self.po
    }
    
    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value, ToolError> {
        Ok(Value::Null) // 测试用空实现
    }
}
```

### 3. 分层测试策略

| 层级 | 测试重点 |
|------|----------|
| DAO | SQL 正确性、数据转换、事务边界 |
| DAL | 组合逻辑、实体组装、业务规则过滤 |
| Domain | 业务流程、跨领域协调、事务一致性 |
| Handler | API 契约、参数校验、错误码映射 |

---

## ❌ 常见陷阱

### 陷阱 1：DAO 层跨 DAO 依赖

**错误**：
```rust
// ❌ CortexDao 直接调用 ToolCallDao
async fn wake(&self, ctx: RequestContext, brain_id: Uuid) -> Result<Brain> {
    let tool_dao = ToolCallDaoSqliteImpl::new();
    let tools = tool_dao.get_enabled_tools(ctx).await?;
    // ...
}
```

**危害**：
- 分层边界模糊
- 测试时需要 mock 多层依赖
- 难以单独优化和替换

**正确做法**：上层注入/调用，DAO 只做自己的事。

---

### 陷阱 2：组装逻辑位置错误

**错误**：在 DAO 层完成实体组装
```rust
// ❌ DAO 层承担组装职责
async fn get_tool(&self, ctx: RequestContext, id: Uuid) -> Result<Option<Tool>> {
    let po = ...; // 从数据库读取
    // 这里做组装... ❌
}
```

**为什么错**：
- DAO 层不应该依赖业务逻辑（注册表、工厂）
- 不同业务场景可能需要不同的组装策略
- 难以测试不同组装逻辑

**正确做法**：组装逻辑放在 DAL 或 Domain 层。

---

### 陷阱 3：循环依赖

**错误**：
```
DaoA → DaoB → DaoA
```

**预防**：
- 严格禁止同层互调
- 依赖方向只能是向上（上层调用下层）
- 跨领域组合放在 Domain 层

---

## 📦 Rig 包名问题记录

### 问题描述

`rig-core` crate 在 `0.34` 版本中，`ToolDefinition` 的位置发生变化，导致编译错误：

```
error[E0670]: `async fn` is not permitted in Rust 2015
error[E0432]: unresolved import `rig::completion::ToolDefinition`
```

### 根本原因

1. **Edition 配置问题**：Rust 2015 edition 不支持 `async fn` 在 trait 中，即使使用了 `#[async_trait]`。

2. **Rig 内部模块重构**：
   - `rig-core 0.34` 内部模块结构调整
   - `ToolDefinition` 的导出路径变化
   - 部分类型从 `rig::completion::*` 移动到 `rig::tool::*`

### 解决方案

**方案 1：确认 Edition 配置**

确保 `Cargo.toml` 使用正确的 edition：
```toml
[package]
edition = "2024"  # 推荐使用最新版
```

**方案 2：正确的导入路径**

```rust
// ✅ 正确：从 rig::tool 导入
use rig::tool::{ToolDyn, ToolError, ToolDefinition};

// ❌ 错误：旧路径
// use rig::completion::ToolDefinition;
```

**方案 3：版本锁定**

如果依赖特定版本的 rig，在 `Cargo.toml` 中明确锁定：
```toml
[dependencies]
rig-core = "=0.34"  # 锁定精确版本
```

### 经验教训

1. **外部 crate 升级需要谨慎**：每次升级前检查 CHANGELOG
2. **类型重导出不稳定**：避免过度依赖 crate 的 re-export 路径
3. **Edition 全局影响**：项目级别的 edition 配置会影响所有依赖
4. **最小化依赖暴露**：内部 trait 尽量封装，不要直接暴露外部 crate 类型

---

## ✅ 重构验证标准

完成分层重构后，通过以下标准验证正确性：

### 1. 编译检查
```bash
cargo check --tests
# 期望：0 错误
```

### 2. 测试覆盖率
```bash
cargo test
# 期望：所有测试通过
```

**当前结果**：✅ 165 passed; 0 failed

### 3. 架构守护检查

- [ ] 没有 `use super::super::` 跨层直接访问
- [ ] DAO 层不依赖其他 DAO
- [ ] DAL 层不依赖其他 DAL
- [ ] 所有业务逻辑在 Domain/DAL/DAO 正确分层

---

## 📝 总结

### 这次重构的核心收获

1. **单一职责是根本**：每个层、每个对象只做一件事
2. **依赖方向要正确**：永远是上层调用下层，不要反向
3. **测试隔离很重要**：可测试的设计才是好设计
4. **注册表模式通用**：遇到"需要动态创建不同类型实例"的场景，注册表/工厂模式几乎总是正确答案

### 后续演进方向

1. 引入 `tower::Layer` 模式实现横切关注点（日志、指标、缓存）
2. 考虑引入 `Repository` 模式进一步抽象数据访问
3. 实现编译期分层检查（通过 cargo 工具或自定义 linter）

---

## 📦 数据对象分层与参数优化规范

> 记录日期：2024-04-30  
> 背景：解决 domain/dal/dao 层创建实体方法参数过多问题，统一各层数据对象定义

---

### 🎯 问题背景

**典型症状**：
- `MessagePo::new` 有 13 个参数
- `Message::new_with_context` 有 13 个参数
- `ToolCallMessage::new_request` 有 10 个参数
- 方法签名冗长，调用时容易传错参数顺序
- 新增字段时需要修改所有调用点

**根本原因**：
1. 缺乏清晰的数据对象分类，所有结构混为一谈
2. PO 实体直接使用位置参数构造
3. Domain 层方法直接使用零散参数而非业务命令对象

---

### ✅ 四层数据对象清晰定义

| 对象类型 | 所属层级 | 定义位置 | 用途 | 示例 | 序列化 |
|----------|----------|----------|------|------|--------|
| **API DTO** | Handler 层 | `common/src/api/**` | HTTP 请求/响应结构，前后端复用；通用响应包装统一使用 `common::api::ApiResponse<T>` | `CreateMessageRequest`, `MessageSummary`, `ApiResponse<T>` | ✅ 必须实现 Serialize/Deserialize |
| **业务命令/查询对象** | Domain 层 | `src/service/domain/*/mod.rs` | Domain 层方法的输入参数，表达业务意图 | `CreateMessageCommand`, `MessageQuery` | ❌ 不实现序列化 |
| **业务实体** | Domain 层 | `src/models/*.rs` | 核心业务对象，包含行为和状态 | `Message`, `Agent`, `Tool` | ❌ 不实现序列化 |
| **PO (持久化对象)** | DAO 层 | `src/models/*.rs` | 数据库映射对象，1:1 对应表结构 | `MessagePo`, `AgentPo` | ✅ 实现 sqlx::FromRow |

---

### 🔄 数据传递规范

#### 调用链数据流
```
HTTP Request
    │
    ▼
Handler: 解析 JSON → API DTO → 补全请求上下文 → 转换为 Command/Query → 按用户 Action 编排 Domain 调用
    │
    ▼
Domain: 接收 Command/Query → 执行业务逻辑 → 返回业务实体
    │
    ▼
DAL: 接收业务实体 → 转换为 PO
    │
    ▼
DAO: 接收 PO → 持久化
```

#### 各层职责边界（数据角度）

| 层级 | 输入 | 输出 | 转换职责 |
|------|------|------|----------|
| **Handler** | API DTO (JSON) | 业务命令/查询对象 + 响应 DTO | API 协议 → 业务概念；补全 `RequestContext` 派生参数；按用户 Action 编排 Domain；Entity → Response DTO |
| **Domain** | 业务命令/查询对象 | 业务实体 | 业务逻辑编排 |
| **DAL** | 业务实体 | PO | 业务对象 → 持久化对象 |
| **DAO** | PO | PO | 纯数据读写 |

> API 契约统一约定：Handler 的响应包装必须从 `common::api::ApiResponse` 导入；`src/handlers` 不再定义本地 `ApiResponse`，避免前后端共享 DTO 与后端本地响应结构分叉。

---

### 🏗️ PO 实体构造优化：Builder 模式

#### 方案选择

| 方案 | 优点 | 缺点 | 适用场景 |
|------|------|------|----------|
| **derive_builder** | 零成本开箱即用<br>功能完善（可选字段、默认值）<br>社区成熟 | 运行时错误（缺失必填字段） | 90% 常规场景（推荐） |
| **自定义 Typestate 宏** | 真正的编译期检查<br>零运行时开销 | 开发维护成本高<br>复杂度高 | 复杂领域模型 |
| **位置参数构造** | 简单直接 | 参数过多时难以维护 | 字段 ≤ 5 个的简单结构 |

**最终决策**：优先使用 `derive_builder` crate，性价比最高。

#### 实施规范

**Step 1: 添加依赖**
```toml
# Cargo.toml
[dependencies]
derive_builder = "0.20"
```

**Step 2: PO 实体实现**
```rust
// ✅ 正确：使用 Builder 模式
#[derive(Debug, Clone, sqlx::FromRow, derive_builder::Builder)]
#[builder(setter(into))]
pub struct MessagePo {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_type: SenderType,
    pub sender_id: Option<Uuid>,
    // ... 其他字段
    
    #[builder(default)]
    pub created_at: DateTime<Utc>,
}

// 可选：提供便捷的 default 实现
impl Default for MessagePo {
    fn default() -> Self {
        Self {
            id: Uuid::now_v7(),
            created_at: Utc::now(),
            // ...
        }
    }
}
```

**Step 3: 调用方式**
```rust
// ✅ Builder 模式，命名参数清晰
let po = MessagePoBuilder::default()
    .conversation_id(conversation_id)
    .sender_type(SenderType::User)
    .sender_id(Some(user_id))
    .content(content)
    .build()
    .expect("required fields missing");

// ✅ 或者配合 Default 局部修改
let po = MessagePo {
    conversation_id,
    sender_type: SenderType::User,
    ..Default::default()
};
```

**注意事项**：
- DAL/DAO 层的 `create` 方法签名保持不变（接收完整 PO）
- 这一层不需要简化参数，因为：
  - 调用方已经在上层完成了 PO 构造
  - 完整 PO 传递保证语义清晰，避免部分更新问题

---

### 📝 Domain 层输入对象规范

#### 与 Handler 的协作边界

Handler 是用户 Action 的入口，Domain 输入对象是 Handler 与 Domain 之间的业务契约。设计时优先让每个 Handler 按接口需求组装明确的 Command/Query，而不是抽象出通用 Handler 或把 API DTO 直接下传。

- **Handler 负责**：解析 API DTO、从 `RequestContext` 补全组织/用户等请求上下文、把前端协议字段转换为业务语义、根据这个 Action 编排一个或多个 Domain 调用、把业务实体转换为响应 DTO。
- **Domain 负责**：承载可复用业务能力、状态规则、权限语义、跨 DAL 编排；同一 Domain 方法可以被多个 Handler 或 Consumer 复用。
- **复用方式**：优先复用 Command/Query 参数结构和 Domain 方法，不通过 Handler 互调、`BaseHandler`、`GenericActionHandler` 等提前抽象来复用。

#### 定义位置
业务命令和查询对象**必须**定义在对应 domain 模块的 `mod.rs` 中，与 trait 定义放在一起：

```rust
// src/service/domain/message/mod.rs

// ✅ 业务命令：表达创建意图
#[derive(Debug, Clone)]
pub struct CreateMessageCommand {
    pub conversation_id: Uuid,
    pub sender_type: SenderType,
    pub sender_id: Option<Uuid>,
    pub content: String,
    pub message_type: MessageType,
    pub metadata: Option<Value>,
    pub reply_to_id: Option<Uuid>,
}

// ✅ 业务查询：表达查询意图
#[derive(Debug, Clone, Default)]
pub struct MessageQuery {
    pub conversation_id: Option<Uuid>,
    pub sender_id: Option<Uuid>,
    pub message_type: Option<MessageType>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[async_trait]
pub trait MessageDomain: Send + Sync {
    // ✅ 使用命令对象而非 10+ 个参数
    async fn create_message(
        &self,
        ctx: RequestContext,
        cmd: CreateMessageCommand,
    ) -> Result<Message>;
    
    // ✅ 使用查询对象
    async fn list_messages(
        &self,
        ctx: RequestContext,
        query: MessageQuery,
    ) -> Result<Vec<Message>>;
}
```

#### 设计原则

1. **命名清晰**：
   - 动词 + 名词 + `Command`：`CreateMessageCommand`
   - 名词 + `Query`：`MessageQuery`
   
2. **不实现序列化**：
   - Command/Query 是纯粹的业务输入，不是 API 契约
   - 避免被错误地直接用于 HTTP 响应

3. **可选字段合理使用**：
   - Command：必填字段不要用 Option
   - Query：大部分字段可以是 Option（动态查询）

---

### ❌ 常见反模式与陷阱

#### 陷阱 1：DTO 污染 Domain 层
```rust
// ❌ 错误：把 API 响应结构放在 Domain 层
// src/service/domain/message/mod.rs
#[derive(Serialize)]  // ❌ Domain 层不需要序列化
pub struct MessageSummary {  // ❌ 这是视图 DTO，不属于 Domain
    pub id: Uuid,
    pub content: String,
}
```

**正确做法**：
```rust
// common/src/api/message.rs  ✅ DTO 放在 common 包
#[derive(Serialize, Deserialize)]
pub struct MessageSummaryResponse {
    pub id: Uuid,
    pub content: String,
}

// Handler 层做转换 ✅
let summary = MessageSummaryResponse {
    id: message.id(),
    content: message.content().to_string(),
};
```

---

#### 陷阱 2：Handler 过度抽象
```rust
// ❌ 错误：为了复用提前抽象通用 Handler，隐藏了用户 Action 的真实语义
pub async fn generic_action_handler<TReq, TCmd, TResp>(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<TReq>,
) -> Result<Json<TResp>, AppError> {
    let cmd = req.into_command(ctx);
    let entity = state.dispatch(cmd).await?;
    Ok(Json(entity.into_response()))
}
```

**问题**：
- 接口语义被泛型调度隐藏，可读性下降
- 不同 Action 的请求级编排差异被迫塞进 trait/回调
- 后续需要组合多个 Domain 时，通用框架会变成新的耦合点

**正确做法**：一个 Handler 对应一个用户 Action，直接组织参数并调用 Domain：
```rust
pub async fn send_user_message_handler(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<SendUserMessageRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    let command = SendUserMessageCommand {
        agent_id: req.agent_id,
        content: req.content,
        organization_id: ctx.organization_id,
        user_id: ctx.user_id,
    };

    let message = state
        .message_domain
        .send_user_message(ctx.clone(), command)
        .await?;

    if req.auto_awaken {
        state
            .runtime_domain
            .awaken_agent(ctx, AwakenAgentCommand {
                agent_id: req.agent_id,
                message_id: message.id(),
            })
            .await?;
    }

    Ok(Json(MessageResponse::from(message)))
}
```

---

#### 陷阱 3：Handler 间互调
```rust
// ❌ 错误：用已有 Handler 复用逻辑
pub async fn create_project_and_task_handler(...) -> Result<Json<ProjectResponse>, AppError> {
    let project = create_project_handler(...).await?;
    create_task_handler(...).await?;
    Ok(project)
}
```

**正确做法**：把可复用逻辑沉到 Domain，Handler 只编排 Domain 方法或调用一个更明确的 Domain 流程方法。

---

#### 陷阱 4：Domain 层方法参数爆炸
```rust
// ❌ 错误：10+ 个零散参数
async fn create_message(
    &self,
    ctx: RequestContext,
    conversation_id: Uuid,
    sender_type: SenderType,
    sender_id: Option<Uuid>,
    content: String,
    message_type: MessageType,
    metadata: Option<Value>,
    reply_to_id: Option<Uuid>,
    // ... 还有更多
) -> Result<Message>;
```

**正确做法**：使用 Command 对象封装（见上文规范）。

---

#### 陷阱 5：PO 构造逻辑放在 DAO 层
```rust
// ❌ 错误：DAO 层做 PO 构造
impl MessageDao for MessageDaoSqliteImpl {
    async fn create(
        &self,
        ctx: RequestContext,
        conversation_id: Uuid,  // ❌ 零散参数
        sender_type: SenderType,
        // ...
    ) -> Result<MessagePo> {
        // 这里做构造...
    }
}
```

**正确做法**：
```rust
// ✅ DAO 层只接收完整 PO
impl MessageDao for MessageDaoSqliteImpl {
    async fn create(&self, ctx: RequestContext, po: MessagePo) -> Result<MessagePo> {
        // 纯 SQL INSERT
    }
}

// ✅ PO 构造在上层（DAL/Domain）完成
```

---

### ✅ 重构步骤（以 Message 模块为例）

1. **Step 1**：添加 `derive_builder` 依赖到 `Cargo.toml`
2. **Step 2**：为 `MessagePo` 实现 Builder 模式
3. **Step 3**：在 `message/domain/mod.rs` 中定义 `CreateMessageCommand` 和 `MessageQuery`
4. **Step 4**：重构 Domain trait 方法签名，使用 Command/Query 替换多参数
5. **Step 5**：更新 DAL 实现和所有调用点
6. **Step 6**：运行 `cargo test` 验证所有测试通过
7. **Step 7**：总结模式，推广到 Agent、Tool 等其他模块

---

## 🔄 实践 4：Organization Domain 跨 DAL 依赖拆分

> 记录日期：2024-04-30
> 背景：修正 DAL 层跨领域依赖，强化分层边界

### 🎯 问题发现

**原设计违反 DAL 单一职责原则**：
```rust
// ❌ 错误：Organization DAL 直接依赖 UserDao，跨领域操作
pub struct OrganizationDalImpl {
    dao: Arc<dyn OrganizationDao + Send + Sync>,
    user_dao: Arc<dyn UserDao + Send + Sync>,  // 跨领域 DAO 依赖
}

impl OrganizationDalImpl {
    async fn initialize_system(&self, ...) -> Result<(String, String)> {
        // 同时操作 organizations 和 users 两张表
        // 跨领域逻辑下沉到 DAL 层，职责混淆
    }
}
```

### ✅ 修正后的架构方案

**核心修正：DAL 层不跨领域，Domain 层负责编排**

| 层级 | 修正前 | 修正后 |
|------|--------|--------|
| **DAL** | OrganizationDal 注入 UserDao，跨表操作 | OrganizationDal 仅依赖自身 OrganizationDao<br>**新增独立 UserDal** 封装用户领域操作 |
| **Domain** | 只注入 OrganizationDal | OrganizationDomain **同时注入 OrganizationDal + UserDal**<br>在 `initialize_system` 方法中编排跨 DAL 调用 |

**修正后的分层边界更新：**

| 层级 | 可以做 | 禁止做 |
|------|--------|--------|
| **DAO** | 单一/多个数据源的 CRUD 操作<br>SQL 拼接<br>数据库实体 PO 转换 | ❌ **同层 DAO 互调**<br>❌ 向上调用 DAL/Domain<br>❌ 业务逻辑<br>❌ 实体组装/装饰 |
| **DAL** | ✅ **依赖多个 DAO**（业务决定）<br>提供业务级数据接口<br>PO → Entity 转换 | ❌ **同层 DAL 互调**<br>❌ 向上调用 Domain |
| **Domain** | ✅ **依赖多个 DAL**（业务决定）<br>核心业务逻辑编排<br>跨领域事务 | ❌ **同层 Domain 互调**<br>❌ 直接调用 DAO（跨层） |
| **Handler** | HTTP 路由<br>参数校验<br>调用 Domain 服务 | ❌ 直接调用 DAL/DAO（跨层） |

### 📝 本次重构的真实意图

本次重构的**核心不是禁止 DAL 多 DAO 依赖**，而是：
1. **User 是独立领域**，应该有自己独立的 DAL 封装，而不是作为 Organization DAL 的内部依赖
2. **User DAL 可被其他领域复用**（HR、权限等），不应该被 Organization 独占
3. **保持 DAL 的领域内聚性**：一个 DAL 应该服务于一个完整的业务领域，而不是多个领域的混合

**这是一个领域边界划分的设计决策，不是对 DAL 多 DAO 依赖的禁止。**

### 📝 代码示例

**✅ User DAL 独立封装**
```rust
pub struct UserDalImpl {
    dao: Arc<dyn UserDao + Send + Sync>,
}

#[async_trait]
impl UserDal for UserDalImpl {
    async fn create(&self, ctx: RequestContext, user: &UserPo) -> Result<UserPo, AppError> {
        self.dao.create(ctx, user).await
    }
    
    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<Vec<UserPo>, AppError> {
        self.dao.query(ctx, query).await
    }
    // ... 仅用户领域的方法
}
```

**✅ Organization DAL 保持单一职责**
```rust
pub struct OrganizationDalImpl {
    dao: Arc<dyn OrganizationDao + Send + Sync>,
    // ✅ 不再注入 UserDao
}

#[async_trait]
impl OrganizationDal for OrganizationDalImpl {
    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<OrganizationPo, AppError> {
        self.dao.create(ctx, org).await
    }
    // ✅ 移除了跨表的 initialize_system 方法
    // ... 仅组织领域的方法
}
```

**✅ Organization Domain 实现跨 DAL 编排**
```rust
pub struct OrganizationDomainImpl {
    org_dal: Arc<dyn OrganizationDal + Send + Sync>,
    user_dal: Arc<dyn UserDal + Send + Sync>,  // ✅ 注入同级 DAL
}

#[async_trait]
impl OrganizationManage for OrganizationDomainImpl {
    async fn initialize_system(&self, ...) -> Result<(String, String), AppError> {
        // 1. 调用 Organization DAL 创建组织
        let org_id = generate_org_id();
        let org = OrganizationPo::new(org_id.clone(), ...);
        self.org_dal.create(ctx.clone(), &org).await?;

        // 2. 调用 User DAL 创建超级管理员
        let user_id = generate_user_id();
        let user = UserPo::new(user_id.clone(), org_id.clone(), ...);
        self.user_dal.create(ctx, &user).await?;

        Ok((org_id, user_id))
    }
}
```

### 🎯 重构收益

1. **职责清晰**：每个 DAL 只负责自己领域的数据操作，边界明确
2. **复用性提升**：User DAL 可被其他领域（如 HR、权限系统）独立复用
3. **测试隔离**：DAL 层测试无需考虑跨领域依赖，更易编写和维护
4. **扩展性增强**：新增领域时只需提供对应 DAL，由 Domain 层自由组合

### ✅ 验证结果

- 编译通过 ✓
- **167 个测试全部通过** ✓
- 无破坏性变更 ✓

---

## 🔄 实践 5：Message Domain 全链路完成（2026-05-15）

> **背景**：从 0 到 1 完成 Message 领域，涵盖消息存储 + 8 个渠道管理 + 多渠道投递全链路

### 🎯 完成的核心内容

| 模块 | 完成情况 | 测试 |
|------|---------|------|
| Message DAL | ✅ 完整实现：CRUD + 状态流转 + 分发 | 28/28 通过 |
| Message Channel DAL | ✅ 完整实现：8 个渠道管理 + 多渠道投递 | 39/39 通过 |
| Message Domain | ✅ 完整实现：注入 DAL，12 个业务方法 | 全链路验证 |

**总计：67/67 测试通过** ✅

---

### 📐 最终架构模式（经过 5 轮讨论确认）

#### 核心设计原则（已验证可落地）

| 原则 | 说明 | 实际效果 |
|------|------|---------|
| **无 trait，纯 match** | 渠道推送不使用 trait 约束，最简单直接 | ✅ 没有复杂的 trait 对象转换 |
| **DAL 统一整合** | 渠道配置管理 + 消息分发统一在 `MessageChannelDal` | ✅ 对外只暴露 8 个公共方法 |
| **严格分层封装** | 所有 DAO 都是 DAL 私有字段，Domain 完全看不到 DAO | ✅ 依赖方向 100% 正确 |
| **无循环依赖** | DAL 依赖 DAO，DAO 不依赖 DAL，单向依赖 | ✅ 编译零警告 |
| **错误统一** | 不创建独立错误类型，统一到 `AppError` | ✅ 错误处理一致 |

---

### 📂 最终目录结构

```
src/service/
├── dao/
│   ├── mod.rs
│   ├── message.rs              # 消息存储 CRUD
│   ├── message_channel.rs      # 渠道配置 CRUD
│   ├── lark_dao.rs             # 飞书推送 DAO
│   ├── wechat_dao.rs           # 微信推送 DAO
│   ├── slack_dao.rs            # Slack 推送 DAO
│   ├── email_dao.rs            # 邮件推送 DAO
│   └── webhook_dao.rs          # Webhook 推送 DAO
│
└── dal/
    ├── mod.rs
    ├── message_dal.rs          # 消息管理 DAL
    └── message_channel_dal.rs  # ✅ 统一整合：配置管理 + 消息分发
```

---

### 🎯 各渠道 DAO 设计模式（完全独立，无 trait）

以 `lark_dao.rs` 为例：

```rust
#[derive(Clone, Default)]
pub struct LarkDao;

impl LarkDao {
    /// 推送消息（约定方法名）
    pub async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
    ) -> Result<(), String> {
        // 实现飞书推送逻辑
    }
    
    /// 测试连接（约定方法名）
    pub async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> Result<(), String> {
        // 实现飞书连接测试逻辑
    }
}
```

**关键点总结：**
1. ✅ 完全独立，不实现任何 trait
2. ✅ `push()` 和 `test_connection()` 只是约定的方法名
3. ✅ 可以自由添加其他渠道特有方法
4. ✅ 测试时可以独立 Mock

---

### 🧩 MessageChannelDal 核心分发逻辑

```rust
pub struct MessageChannelDal {
    // ✅ 所有 DAO 都是私有，不对外暴露！
    message_channel_dao: Arc<dyn MessageChannelDao>,
    lark_dao: Arc<LarkDao>,
    wechat_dao: Arc<WechatDao>,
    slack_dao: Arc<SlackDao>,
    email_dao: Arc<EmailDao>,
    webhook_dao: Arc<WebhookDao>,
}

impl MessageChannelDal {
    // ... 配置管理的公共方法 ...
    
    /// ✅ 测试渠道连接（公共方法）
    pub async fn test_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<()> {
        let channel = self.get_channel(ctx.clone(), channel_id).await?;
        
        // 🎯 核心：纯 match 分发！无 trait！
        match channel.channel_type() {
            ChannelType::Lark => self.lark_dao.test_connection(ctx, &channel).await,
            ChannelType::Wechat => self.wechat_dao.test_connection(ctx, &channel).await,
            ChannelType::Slack => self.slack_dao.test_connection(ctx, &channel).await,
            ChannelType::Email => self.email_dao.test_connection(ctx, &channel).await,
            ChannelType::Webhook => self.webhook_dao.test_connection(ctx, &channel).await,
        }.map_err(|e| AppError::ChannelPushError(e))
    }
}
```

---

### 💡 本次实践的关键架构洞见

#### 洞见 1：Trait 不是银弹，有时候纯 match 更好

**问题场景**：多个渠道实现相似但不相同的方法
- ❌ 使用 trait：需要 `Box<dyn PushTrait>`，但每个渠道有特有配置和方法，trait 难以统一
- ✅ 纯 match：每个 DAO 独立，DAL 内部 match 分发，灵活且简单

**适用场景**：实现数量固定（< 10 个），每个实现略有差异，不要求动态扩展

---

#### 洞见 2：DAO 层可以承载外部 API 调用

DAO 不仅是数据库持久化：
- ✅ **数据库 DAO**：SQLite CRUD（如 message_dao）
- ✅ **外部 API DAO**：调用三方 API（如 lark_dao, wechat_dao）

**统一抽象**：DAO = "数据访问对象"，不论是本地数据库还是外部 API，都是"访问数据"

---

#### 洞见 3：DAL 层是"组合器"

MessageChannelDal 做了三件事：
1. **组合多个 DAO**：配置 DAO + 5 个渠道 DAO
2. **隐藏实现细节**：所有 DAO 都是私有，对外只暴露 8 个业务方法
3. **错误转换**：把 DAO 层的 String 错误统一转换为 AppError

**这就是 DAL 层的真正价值：把多个底层数据源组合成业务语义清晰的操作**

---

### 📋 本次实践的可复用模式

| 模式 | 适用场景 | 关键点 |
|------|---------|--------|
| **纯 match 分发** | 固定数量的相似实现（< 10 个） | 无 trait，直接在 DAL 层 match |
| **DAO 私有封装** | 任何需要隐藏底层实现的场景 | DAL 字段私有，对外只暴露方法 |
| **约定方法名** | 多个 DAO 有相似操作但无需统一 trait | push(), test_connection() 等约定命名 |
| **错误统一转换** | 跨模块调用时 | DAO 层返回简单错误，DAL 层统一转换 |

---

## 🔄 实践 6：PO 与业务实体分层边界（2026-05-11）

> **背景**：Project/Task/Artifact 模块重构，明确 PO 与业务实体的边界

### 核心原则：PO 仅在 DAO/DAL 层内部使用，绝对不对外暴露到 Domain 层及以上

#### 分层边界定义

| 层级 | 可使用对象 | 数据传递方式 | 说明 |
|------|------------|------------|------|
| **DAO 层** | 仅 PO | PO ↔ 数据库 | 单一数据源 CRUD，SQL 拼接，无业务逻辑 |
| **DAL 层** | 内部：PO，对外：业务实体 | PO ↔ 业务实体 双向转换 | 组合 DAO，完成业务级数据操作 |
| **Domain 层** | 仅业务实体 | 业务实体 ↔ Command | 核心业务逻辑编排，无 PO 依赖 |
| **Handler 层** | 业务实体 + DTO | DTO ↔ 业务实体 | HTTP 接口，参数校验 |

#### 业务实体内部设计

**标准模式：业务实体内部持有 PO**
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
```

#### RequestContext 跨层传递规范

**所有跨层 ctx 传递统一使用 `ctx.clone()`：**
```rust
// ✅ 正确：clone 后传递，避免所有权移动问题
self.project_dal.create(ctx.clone(), project).await?;
```

**理由：**
- RequestContext 内部是 Arc 引用，clone 成本极低（仅指针复制）
- 避免所有权移动导致的编译错误
- 与 message domain 风格保持一致

#### 软删除设计规范

**`status = 0` 视为软删除，常规查询自动过滤**：
- DAO 层的 `find_by_id`、`list_by_*` 等方法自动加 `WHERE status != 0`
- 需要查询已删除数据时，使用独立方法如 `find_by_id_include_deleted`
- Domain 层完全不需要感知软删除的存在

---

## 🔄 实践 7：适配层（Handler/Adapter）架构原则（2026-07-21）

> **背景**：飞书 P2P 消息入站、A2A Remote Agent 异步回调/轮询等外部接入场景完成后，重新审视分层架构，明确 Handler 本质上也是 Adapter——所有外部输入入口都是同层的适配器。

### 🎯 核心认知：Handler = Adapter

传统上我们把"Handler 层"单独列一层，但本质上 **Handler 就是面向用户/前端 HTTP API 的 Adapter**，和面向飞书、A2A Agent 等外部系统的 Adapter 是**同级别、同职责**的组件——它们都是把外部世界的输入适配成内部 Domain 方法调用。

因此正确的分层不是"边界层在 Handler 之外"，而是**原来的 Handler 层改名为适配层（Adapter 层）**，它包含了所有外部输入的适配器：

```
                          ┌──────────────────────┐
                          │      外部世界         │
                          └──────────┬───────────┘
                                     │
           ┌─────────────────────────┼─────────────────────────┐
           │                         │                         │
           ▼                         ▼                         ▼
  ┌─────────────────┐     ┌─────────────────────┐    ┌─────────────────────┐
  │ 用户/前端        │     │ 外部系统（飞书等）   │    │ 外部系统（A2A等）   │
  │ HTTP API 调用   │     │ WebSocket 事件推送  │    │ HTTP 回调 / 定时轮询│
  └────────┬────────┘     └──────────┬──────────┘    └──────────┬──────────┘
           │                         │                         │
           ▼                         ▼                         ▼
  ┌─────────────────┐     ┌─────────────────────┐    ┌─────────────────────┐
  │ HTTP Handler    │     │ AOP Producer        │    │ HTTP Callback Handler│
  │ (API Adapter)   │     │ (WS/轮询 Adapter)   │    │ (回调 Adapter)      │
  └────────┬────────┘     └──────────┬──────────┘    └──────────┬──────────┘
           │                         │                         │
           └─────────────────────────┼─────────────────────────┘
                                     │
                        ┌────────────▼────────────┐
                        │   适配层（Adapter 层）    │
                        │  统一职责：协议转换+校验  │
                        │  → 直接调用 Domain 方法  │
                        └────────────┬────────────┘
                                     │ Domain 方法调用
                                     ▼
                        ┌─────────────────────────┐
                        │      Domain 层           │
                        │  核心业务逻辑             │
                        │  内部事件在这里产生       │
                        └────────────┬────────────┘
                                     │
                                     ▼
                        ┌─────────────────────────┐
                        │        DAL 层            │
                        │  组合 DAO，PO↔Entity 转换 │
                        └────────────┬────────────┘
                                     │
                        ┌────────────▼────────────┐
                        │        DAO 层            │
                        │  ├─ 本地数据库 CRUD      │
                        │  └─ 外部 API 出站调用    │
                        │     （LarkDao.push 等） │
                        └─────────────────────────┘
```

### 📋 适配层包含哪些组件？

| 组件类型 | 对接对象 | 触发方式 | 示例 |
|---------|---------|---------|------|
| **HTTP Handler** | 用户/前端 | HTTP 请求（需要 JWT 鉴权） | `create_agent`、`send_message` 等所有 `/api/v1/...` 接口 |
| **公开 HTTP Handler** | 外部系统回调 | HTTP 回调（无 JWT，需自身校验） | `POST /a2a/callback/:task_id` |
| **AOP Producer** | 外部系统事件/定时轮询 | AOP 框架定时触发/事件监听 | `A2aPollingProducer`（30秒轮询）、`MessageChannelProducer`（飞书 WS） |
| **Consumer** | 内部事件 | 事件中心内部事件（`MessageCreatedEvent` 等） | `MessageConsumer`（注意：Consumer 处理的是**内部**事件，不是外部输入） |

> **注意**：Consumer 不在适配层——它消费的是 Domain 产生的内部事件，是内部链路的一部分，已经完全脱离外部协议。

### ✅ 适配层核心原则：外部协议不进入内部

| 原则 | 说明 |
|------|------|
| **协议转换在适配层完成** | 外部数据结构（HTTP JSON、飞书事件、A2A JSON-RPC）→ 内部业务 Command/方法参数 的映射，全部在适配层完成 |
| **直接调用 Domain** | 适配层拿到外部数据后，**直接调用 Domain 层方法**执行业务操作，不包装成"外部事件"投递到事件中心 |
| **内部事件纯内部** | 事件中心里流通的事件（如 `MessageCreatedEvent`、`CronTriggerEvent`）全部由 Domain 方法内部产生，Consumer 不需要感知任何外部协议 |
| **出站调用在 DAO** | 推送到外部系统的逻辑（如发飞书消息、调用 A2A tasks/send）封装在外部 DAO 中（如 `LarkDao.push`、`A2aRuntimeDao.send_task`），由 DAL 组合、Domain 编排 |

### 📋 适配层职责清单

**入站适配器（所有类型的 Handler/Producer）可以做**：
- 接收/监听外部触发（HTTP 请求、WebSocket 事件、定时轮询）
- 外部协议校验、签名验证、鉴权（JWT、签名、token 等）
- 幂等检查（如"任务已在终态则跳过"）
- ID 映射（外部 ID ↔ 内部 ID，如 `open_id → user_id`、`a2a_task_id → task_id`）
- 外部数据解析与过滤（如只处理飞书 P2P 文本消息、只处理 A2A agent 角色消息）
- DTO ↔ Command 转换（HTTP Handler 的标准职责）
- 直接调用 Domain 方法（`send_to_user()`、`send_to_agent()`、`transition_status()` 等）
- 记录日志，处理外部渠道特有错误
- 组装响应（HTTP Handler 返回 JSON，其他类型适配器无需响应）

**入站适配器禁止做**：
- ❌ 直接操作 DAL/DAO（跨层，必须通过 Domain）
- ❌ 把外部协议原始 JSON 包装成事件投递到事件中心
- ❌ 把外部协议结构持久化到内部事件队列
- ❌ 在 Consumer 里解析外部协议数据（到了 Consumer 就应该是纯内部语义了）
- ❌ 实现核心业务规则（业务规则在 Domain 层）
- ❌ Handler 之间互调（复用逻辑沉到 Domain）

**出站适配器（推送到外部）**：
- 统一在外部 DAO 层实现（如 `LarkDao.push`、`A2aRuntimeDao.send_task`、`A2aRuntimeDao.fetch_task`）
- 外部 DAO 由对应 DAL 持有并组合调用
- 出站消息格式转换在外部 DAO 内部完成（如 Markdown → 飞书卡片、内部消息 → A2A Message）

### 🔍 入站适配器实现对照

| 外部输入源 | 适配层组件 | 协议转换 | 业务动作 |
|-----------|-----------|---------|---------|
| 用户/前端 HTTP API | `src/handlers/...` 各 handler | API DTO (JSON) → Command | 调用对应 Domain 方法，返回 Response DTO |
| 飞书 P2P 消息 | `src/producer/message_channel.rs` (Producer) | `LarkMessageEvent` → `AdaptedMessage` → `SendToAgentCommand` | `MessageDomain.send_to_agent()` → 内部创建消息、发布 `MessageCreatedEvent`、唤醒 Agent |
| A2A 回调推送 | `src/handlers/a2a/callback.rs` (公开 HTTP Handler) | `A2aTask` → 提取消息文本/状态 → `SendToUserCommand` | `MessageDomain.send_to_user()` + `TaskManage.transition_status()` → 内部创建消息、发布 `MessageCreatedEvent`、SSE 推送 |
| A2A 轮询兜底 | `src/producer/a2a_polling.rs` (AOP Producer) | 同上（fetch_task 获取 A2aTask 后同样处理） | 同上 |

### ❌ 反模式：外部事件包装进事件中心

```rust
// ❌ 错误：把外部协议 JSON 包装成内部事件投递
pub struct A2aTaskUpdateEvent {
    pub task_json: String,  // 直接存外部协议 JSON
    // ...
}
// → 导致 Consumer 需要解析外部 JSON、理解外部协议
// → 外部协议变更时需要改 Consumer
// → 事件中心被外部协议污染
// → 这本质上是"绕过适配层，把协议解析下沉到了 Consumer"
```

```rust
// ✅ 正确：适配层直接调用 Domain，事件中心只看到内部事件
// 适配层（handler/producer）:
let cmd = SendToUserCommand { content: extract_text(&msg.parts), ... };
message_domain.send_to_user(ctx, cmd).await?;  // 内部会发布 MessageCreatedEvent

// Consumer 只看到 MessageCreatedEvent，完全不需要知道消息来自飞书、A2A 还是用户在前端发的
```

### 💡 架构洞见

**为什么 Handler 和 Producer/回调 是同级别？** 因为它们的职责完全一致——都是"外部世界 → 内部系统"的适配器：
- HTTP Handler 适配来自**用户**的外部输入（HTTP 请求）
- 飞书 Producer 适配来自**飞书**的外部输入（WebSocket 事件）
- A2A 回调 Handler 适配来自**外部 A2A Agent**的外部输入（HTTP 回调）
- A2A 轮询 Producer 适配来自**外部 A2A Agent**的外部输入（主动轮询拉取）

区别只在于它们对接的外部世界不同、触发方式不同，但在架构中的**位置和职责完全相同**——把外部语言翻译成内部语言，然后调用 Domain。

这正是 Hexagonal Architecture（端口与适配器架构）的核心思想：
- **内部（Domain）**：核心业务逻辑，定义端口（trait/方法签名），不依赖任何外部
- **外部（Adapters）**：各种适配器（HTTP、WS、消息队列、定时任务、外部 API），实现与外部世界的对接，调用内部端口
- 外部系统的变化只影响适配器，不影响内部核心

### 📐 修正后的分层架构表

| 层级 | 职责 | 组件 | 依赖方向 |
|------|------|------|---------|
| **适配层 (Adapter)** | 外部输入适配、协议转换、校验、直接调 Domain | HTTP Handler（含公开回调）、AOP Producer | → Domain |
| **Domain 层** | 核心业务逻辑、跨 DAL 编排、内部事件产生 | 各 Domain（Message、Project、HR 等） | → DAL |
| **DAL 层** | 组合 DAO、业务级数据操作、PO↔Entity 转换 | 各 Dal | → DAO |
| **DAO 层** | 数据访问（本地 DB CRUD + 外部 API 出站调用） | 本地 DB DAO、外部 API DAO（LarkDao、A2aRuntimeDao） | → DB / 外部 API |

---

## 🏆 分层架构实践总结（截至 2026-07-21）

### ✅ 已验证的最佳实践

| 实践 | 验证模块 | 效果 |
|------|---------|------|
| **严格单向调用** | 全部 6+ 个领域 | ✅ 零跨层调用，零反向依赖 |
| **DAO 职责单一** | 20+ 个 DAO | ✅ 每个 DAO 仅负责一个数据源 |
| **纯 match 分发** | Message Channel | ✅ 简单灵活，无 trait 复杂度 |
| **业务实体持 PO** | Project/Task/Artifact | ✅ 减少转换代码，兼容性好 |
| **DAL 私有封装 DAO** | 全部 DAL | ✅ 隐藏实现细节，接口清晰 |
| **Arc<dyn Trait> 注入** | 全部 Domain | ✅ 符合 DIP，测试友好 |
| **适配层协议转换** | HTTP Handler、飞书入站、A2A 回调/轮询 | ✅ Handler=Adapter，外部协议不污染事件中心 |
| **外部 DAO 封装出站** | Lark 推送、A2A 调用 | ✅ 出站外部调用统一在 DAO 层 |

---

### ⚠️ 反模式避坑指南

| 反模式 | 危害 | 正确做法 |
|--------|------|---------|
| DAO 调 DAO | 破坏单一职责，循环依赖 | DAL 层组合 DAO |
| Domain 直接调 DAO | 跳过分层，职责混淆 | 必须通过 DAL 层 |
| PO 暴露到 Domain 层 | 分层边界失效，耦合数据库 | 业务实体包装 PO |
| 过度使用 trait | 复杂的对象转换，性能损耗 | 固定数量实现用纯 match |
| Handler 直接调 DAL | 跳过业务逻辑层 | 必须通过 Domain 层 |
| Handler 之间互调 | 耦合混乱 | 复用逻辑沉到 Domain |
| **外部事件包装进事件中心** | 外部协议污染内部，Consumer 需理解外部协议 | **适配层直接调 Domain，事件中心只流通内部事件** |
| **出站外部调用散落在 Domain** | Domain 耦合外部协议 | **出站调用封装在外部 DAO，由 DAL/Domain 编排** |

---

**文档维护者**：架构组
**上次更新**：2026-07-21（明确 Handler=Adapter，修正架构层级认知）
