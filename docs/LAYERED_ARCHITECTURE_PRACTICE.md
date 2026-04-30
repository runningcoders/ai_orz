# 分层架构重构实践记录

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
    async fn wake(&self, ctx: &RequestContext, brain_id: Uuid) -> Result<Brain> {
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
Handler (API 层)
    │
    ▼
Domain (领域层) → 组合多个 DAL，实现业务逻辑
    │
    ▼
DAL (业务数据层) → 组合多个 DAO，提供业务级数据操作
    │
    ▼
DAO (数据访问层) → 单一数据源操作，只做 CRUD
```

### 各层职责边界

| 层级 | 职责 | 禁止做 |
|------|------|--------|
| **DAO** | 单一数据源的 CRUD 操作<br>SQL 拼接<br>数据库实体 PO 转换 | ❌ 跨 DAO 依赖<br>❌ 业务逻辑<br>❌ 实体组装/装饰 |
| **DAL** | 组合多个 DAO<br>提供业务级数据接口<br>业务实体组装/过滤 | ❌ 跨 DAL 依赖<br>❌ 复杂业务编排 |
| **Domain** | 组合多个 DAL<br>核心业务逻辑编排<br>跨领域事务 | ❌ 直接操作数据库 |
| **Handler** | HTTP 路由<br>参数校验<br>调用 Domain 服务 | ❌ 业务逻辑 |

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
    async fn get_tools_by_agent(&self, ctx: &RequestContext, agent_id: Uuid) -> Result<Vec<ToolPo>> {
        // 纯 SQL 查询
    }
}

// ✅ DAL 层：负责业务组装
impl ToolDalImpl {
    async fn list_tools_by_agent(&self, ctx: &RequestContext, agent_id: Uuid) -> Result<Vec<Tool>> {
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
    
    async fn call(&self, _ctx: &RequestContext, _args: Value) -> Result<Value, ToolError> {
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
async fn wake(&self, ctx: &RequestContext, brain_id: Uuid) -> Result<Brain> {
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
async fn get_tool(&self, ctx: &RequestContext, id: Uuid) -> Result<Option<Tool>> {
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
| **API DTO** | Handler 层 | `common/src/api/**` | HTTP 请求/响应结构，前后端复用 | `CreateMessageRequest`, `MessageSummary` | ✅ 必须实现 Serialize/Deserialize |
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
Handler: 解析 JSON → API DTO → 转换为 Command/Query
    │
    ▼
Domain: 接收 Command → 执行业务逻辑 → 返回业务实体
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
| **Handler** | API DTO (JSON) | 业务命令/查询对象 | API 协议 → 业务概念 |
| **Domain** | 业务命令/查询对象 | 业务实体 | 业务逻辑编排 |
| **DAL** | 业务实体 | PO | 业务对象 → 持久化对象 |
| **DAO** | PO | PO | 纯数据读写 |

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
        ctx: &RequestContext,
        cmd: CreateMessageCommand,
    ) -> Result<Message>;
    
    // ✅ 使用查询对象
    async fn list_messages(
        &self,
        ctx: &RequestContext,
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

#### 陷阱 2：Domain 层方法参数爆炸
```rust
// ❌ 错误：10+ 个零散参数
async fn create_message(
    &self,
    ctx: &RequestContext,
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

#### 陷阱 3：PO 构造逻辑放在 DAO 层
```rust
// ❌ 错误：DAO 层做 PO 构造
impl MessageDao for MessageDaoSqliteImpl {
    async fn create(
        &self,
        ctx: &RequestContext,
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
    async fn create(&self, ctx: &RequestContext, po: MessagePo) -> Result<MessagePo> {
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

**文档维护者**：架构组  
**下次更新**：Domain 层完成后
