# RequestContext 设计文档

> 最后更新：2026-07-04

## 一、定位与核心原则

`RequestContext` 是贯穿整个请求生命周期的上下文对象，承载用户身份、组织信息、业务维度等元数据，以及数据库、向量存储、统计模块等基础设施引用。

**核心原则**：**不可变优先，重建替代修改**

- 上下文一旦构建完成，即为不可变对象
- 任何维度的变更都通过 Builder 重建新的上下文
- 通过类型系统保证"构建完成"和"构建中"两个阶段的语义隔离

---

## 二、树形扩散模型

### 2.1 核心思想

上下文在分层架构中沿调用链**向下传递时越来越丰富**，不同分支互不干扰，上层不受下层影响。

```
[Handler 层 ctx]         user_id, organization_id
    │
    ├─ 查 Agent 实体 → 注入 agent_id → 新 ctx 往下传
    │     │
    │     ├─ 调用 Project Domain
    │     │    │
    │     │    └─ 查 Project → 注入 project_id → 新 ctx 继续往下
    │     │           │
    │     │           └─ 发消息（delivery 层）
    │     │                 ↓
    │     │            ctx 里已有 project_id，
    │     │            cmd 没传也没关系，ctx 兜底
    │     │
    │     └─ 调用 Brain Domain
    │          └─ 注入 model_provider_id → 新 ctx
    │
    └─ 另一条分支...
        （ctx 不受上面那条分支的影响）
```

### 2.2 关键性质

| 性质 | 说明 |
|------|------|
| **不可变 + 写时复制** | 每层基于上层 ctx 克隆后注入新维度，生成新 ctx 往下传，上层 ctx 不受影响 |
| **越往下越具体** | 调用链越深，ctx 携带的维度信息越丰富 |
| **分支互不干扰** | 不同业务分支的 ctx 是独立的，不会互相污染 |
| **下游防御兜底** | delivery 等底层模块，cmd 参数优先，ctx 兜底，确保上下文不丢失 |

### 2.3 优先级规则

越靠近具体业务逻辑层的信息时效性越高，优先级越高：

1. **Command 参数**（最高优先级）—— 调用方显式指定的参数
2. **当前层实体信息** —— 当前逻辑层查到的实体信息，通过 `to_builder()` 注入
3. **上层传递的 ctx**（兜底优先级）—— 上游已经有的上下文信息

---

## 三、字段分类

| 分类 | 字段 | 来源 | 说明 |
|------|------|------|------|
| **追踪标识** | `log_id` | 自动生成 / Header | 链路追踪 ID |
| **用户身份** | `user_id`, `username` | HTTP Header / JWT | 当前操作用户 |
| **组织维度** | `organization_id` | HTTP Header / JWT | 当前操作所在组织 |
| **业务维度** | `agent_id` | Runtime 设置 | 当前操作的 Agent |
| | `project_id` | 业务层设置 | 当前关联的项目 |
| | `task_id` | 业务层设置 | 当前关联的任务 |
| **模型维度** | `model_provider_id` | Cortex 创建时 | 当前使用的模型提供商 |
| | `model_name` | Cortex 创建时 | 当前使用的模型名称 |
| **基础设施** | `storage` | 全局单例 | SQLite + Vector + Stats 统一门面 |

---

## 四、Builder 模式设计

### 4.1 类型关系

```
RequestContext            (不可变，只有 getter)
    │
    ├─ RequestContext::builder()   → 从零开始构建
    └─ ctx.to_builder()            → 克隆现有上下文后继续构建
          │
          ▼
RequestContextBuilder     (可变，有 with_* 方法)
          │
          └─ .build()             →  RequestContext（新的不可变实例）
```

### 4.2 RequestContextBuilder API

```rust
// 构建器（可变阶段）
impl RequestContextBuilder {
    // 必填方法：直接设置值
    pub fn log_id(mut self, log_id: impl Into<String>) -> Self;
    pub fn user_id(mut self, user_id: impl Into<String>) -> Self;
    pub fn username(mut self, username: impl Into<String>) -> Self;
    pub fn organization_id(mut self, org_id: impl Into<String>) -> Self;
    pub fn agent_id(mut self, agent_id: impl Into<String>) -> Self;
    pub fn project_id(mut self, project_id: impl Into<String>) -> Self;
    pub fn task_id(mut self, task_id: impl Into<String>) -> Self;
    pub fn model_provider_id(mut self, id: impl Into<String>) -> Self;
    pub fn model_name(mut self, name: impl Into<String>) -> Self;
    pub fn storage(mut self, storage: Storage) -> Self;

    // try_* 方法：Some 时覆盖，None 时跳过（保留已有值）
    pub fn try_user_id(mut self, user_id: Option<impl Into<String>>) -> Self;
    pub fn try_username(mut self, username: Option<impl Into<String>>) -> Self;
    pub fn try_organization_id(mut self, org_id: Option<impl Into<String>>) -> Self;
    pub fn try_agent_id(mut self, agent_id: Option<impl Into<String>>) -> Self;
    pub fn try_project_id(mut self, project_id: Option<impl Into<String>>) -> Self;
    pub fn try_task_id(mut self, task_id: Option<impl Into<String>>) -> Self;
    pub fn try_model_provider_id(mut self, id: Option<impl Into<String>>) -> Self;
    pub fn try_model_name(mut self, name: Option<impl Into<String>>) -> Self;

    pub fn build(self) -> RequestContext;
}
```

**`build()` 行为**：
- 如果 `log_id` 未设置，自动生成
- `storage` 未设置时使用全局单例
- 其他字段默认为 `None`

**`try_*` 方法用途**：实体字段为 `Option<String>` 时，无需手动判空，直接传给 `try_*` 方法。有值则覆盖，None 则跳过，保留 builder 中已有的值。典型场景是 `EnrichContext` 实现。

### 4.3 RequestContext 上的构造入口

```rust
// 从零构建
pub fn builder() -> RequestContextBuilder;

// 从现有上下文克隆后构建（最常用）
pub fn to_builder(&self) -> RequestContextBuilder;
```

---

## 五、典型使用场景

### 5.1 HTTP 请求入口（from_headers）

```rust
// 中间件中从 Header 构建初始上下文
let ctx = RequestContext::builder()
    .log_id(log_id)
    .user_id(user_id)
    .username(username)
    .organization_id(org_id)
    .build();
```

### 5.2 业务层扩展上下文（查到实体后回填）

**手动方式**（适合单实体、字段明确的场景）：

```rust
// 查到 Agent 实体后，回填到 ctx
let agent = self.agent_dal.find_by_id(ctx.clone(), agent_id).await?;
let ctx = ctx.to_builder()
    .agent_id(&agent.po.id)
    .model_provider_id(&agent.po.model_provider_id)
    .build();

// 继续往下传递新的 ctx
self.brain_domain.think(ctx, ...).await?;
```

**EnrichContext 方式**（适合多实体串联、字段映射集中的场景）：

```rust
// 一行代码完成多实体上下文补充
let ctx = enrich_ctx!(&ctx, &agent, &project, &task);
```

等价于：

```rust
let mut builder = ctx.to_builder();
builder = agent.enrich(builder);   // 注入 agent_id, model_provider_id
builder = project.enrich(builder); // 注入 project_id, try agent_id
builder = task.enrich(builder);    // 注入 task_id, try project_id
let ctx = builder.build();
```

### 5.3 Consumer 异步场景重建上下文

```rust
// 从 message 元数据重建完整上下文
let ctx = RequestContext::builder()
    .organization_id(message.po.organization_id.clone())
    .agent_id(tool_call.from_id.clone())
    .project_id(tool_call.project_id.clone())
    .task_id(tool_call.task_id.clone())
    .build();
```

### 5.4 Cortex 创建时注入模型维度

```rust
let ctx = ctx.to_builder()
    .model_provider_id(&provider.po.id)
    .model_name(&provider.po.model_name)
    .build();
```

### 5.5 Delivery 层 ctx 兜底（下游防御）

在 `send_to_agent` / `send_to_user` / `send_tool_call_request` 中：

- `project_id`：cmd 传了用 cmd 的，没传用 ctx 里的
- `task_id`：cmd 传了用 cmd 的，没传用 ctx 里的
- `organization_id`：直接从 ctx 取（之前已实现）

这样即使调用方忘了在 cmd 里传，只要 ctx 里有，消息也不会丢维度。

---

## 六、与旧 API 的兼容与迁移

### 6.1 迁移状态

- ✅ **已完成**：Builder API 上线
- ✅ **已完成**：全部 `set_*` 方法删除，彻底锁死不可变性
- ✅ **已完成**：生产代码和测试代码全部迁移

### 6.2 保留的构造方法（向后兼容）

- `RequestContext::new(user_id, username)` — 内部委托给 builder
- `RequestContext::from_headers(headers)` — 内部委托给 builder
- `RequestContext::from_storage(user_id, storage)` — 内部委托给 builder

---

## 七、EnrichContext：实体到上下文的自动映射

### 7.1 设计动机

业务层扩展上下文时，经常需要把查到的实体字段回填到 ctx。如果每次都手写 `to_builder().xxx_id().build()`，会产生大量重复代码，且字段映射规则散落各处。

`EnrichContext` trait 让实体自己声明如何注入上下文，字段映射规则集中在实体定义处，调用方通过 `enrich_ctx!` 宏串联多个实体。

### 7.2 EnrichContext trait

```rust
/// 定义在 src/pkg/request_context.rs
pub trait EnrichContext {
    fn enrich(&self, builder: RequestContextBuilder) -> RequestContextBuilder;
}
```

**覆盖规则**（符合树形扩散模型）：
- 实体字段有值（Some）时，覆盖 builder 中已有的值
- 实体字段为 None 时，跳过，保留 builder 中已有值

### 7.3 enrich_ctx! 宏

```rust
let new_ctx = enrich_ctx!(&ctx, &agent, &project, &task);
```

依次调用每个实体的 `enrich` 方法，最后 `build()` 生成新的不可变 `RequestContext`。

### 7.4 已实现的实体

| 实体 | 注入字段 | 说明 |
|------|----------|------|
| `Agent` | `agent_id`, `model_provider_id` | 必填字段直接注入 |
| `Project` | `project_id`, `try_agent_id` | owner_agent_id 可选，用 try_* |
| `Task` | `task_id`, `try_project_id` | project_id 可选，用 try_* |
| `ModelProvider` | `model_provider_id`, `model_name` | 必填字段直接注入 |

### 7.5 设计约束

> **上下文只存简单信息（ID、名称等），业务实体通过方法参数显式传递。**

- `RequestContext` 永远不依赖 `models` 模块，只持有 String 类型的简单信息
- `EnrichContext` trait 定义在 `request_context.rs` 中，实体在自己的文件中实现
- 依赖方向单向：`models/*` → `pkg/request_context`，无循环引用风险

---

## 八、设计决策

### 8.1 为什么不用 with_* 直接在 RequestContext 上？

`ctx.clone().with_agent_id(...).with_project_id(...)` 也能工作，但有以下问题：

- 没有明确的"构建完成"边界，无法做统一校验
- `with_*` 方法会让 RequestContext 本身的 API 面膨胀
- builder 作为独立类型，语义更清晰："正在构建中" vs "已经构建好"

### 8.2 为什么用 owned 而不是引用？

- 上下文对象是要被移动和克隆的，所有字段用 owned（String 而非 &str）
- `with_*` 方法参数用 `impl Into<String>`，调用方可以传 `&str` 或 `String`
- 内部 `Arc<Storage>` 天然支持浅克隆

### 8.3 build() 时的校验（预留）

当前版本 `build()` 不做强校验，但预留了扩展位。未来可根据需要增加：

- `agent_id` 设置时必须有 `organization_id`
- `task_id` 设置时必须有 `project_id`
- 等等...

### 8.4 为什么 delivery 层要做 ctx 兜底？

- **防御式设计**：上游可能忘记传维度，兜底确保数据完整性
- **减少重复传参**：ctx 里已经有的信息，调用方不需要在 cmd 里再传一遍
- **消费端可重建**：消息里存的维度越全，异步消费时重建的 ctx 越完整

---

## 八、改动影响范围

### 不需要改的

- 所有 getter 方法（`agent_id()`, `project_id()` 等）不变
- `clone()` 行为不变
- `db_pool()`, `storage()`, `stats()` 等基础设施访问方法不变

### 已完成迁移的

- 所有 `set_*` 方法 → `to_builder().with_*().build()`
- 生产代码：cortex/rig.rs、consumer/message.rs
- 测试代码：request_context_test.rs、tool_call_entry_test.rs、tool_execution_test.rs

### 新增的

- `RequestContextBuilder` 结构体
- `RequestContext::builder()` 静态方法
- `RequestContext::to_builder()` 实例方法
- `try_*` 系列 Builder 方法（8 个），支持 Option 字段的条件覆盖
- `EnrichContext` trait + `enrich_ctx!` 宏，实体到上下文的自动映射
- 四个核心实体已实现 `EnrichContext`：Agent、Project、Task、ModelProvider
