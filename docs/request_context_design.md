# RequestContext 设计文档

> 最后更新：2026-07-03

## 一、定位与核心原则

`RequestContext` 是贯穿整个请求生命周期的上下文对象，承载用户身份、组织信息、业务维度等元数据，以及数据库、向量存储、统计模块等基础设施引用。

**核心原则**：**不可变优先，重建替代修改**

- 上下文一旦构建完成，即为不可变对象
- 任何维度的变更都通过 Builder 重建新的上下文
- 通过类型系统保证"构建完成"和"构建中"两个阶段的语义隔离

---

## 二、字段分类

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

## 三、Builder 模式设计

### 3.1 类型关系

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

### 3.2 RequestContextBuilder API

```rust
// 构建器（可变阶段）
impl RequestContextBuilder {
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
    pub fn build(self) -> RequestContext;
}
```

**`build()` 行为**：
- 如果 `log_id` 未设置，自动生成
- `storage` 未设置时使用全局单例
- 其他字段默认为 `None`

### 3.3 RequestContext 上的构造入口

```rust
// 从零构建
pub fn builder() -> RequestContextBuilder;

// 从现有上下文克隆后构建（最常用）
pub fn to_builder(&self) -> RequestContextBuilder;
```

---

## 四、典型使用场景

### 4.1 HTTP 请求入口（from_headers）

```rust
// 中间件中从 Header 构建初始上下文
let ctx = RequestContext::builder()
    .log_id(log_id)
    .user_id(user_id)
    .username(username)
    .organization_id(org_id)
    .build();
```

### 4.2 业务层扩展上下文

**旧方式（set_*，可变）**：
```rust
let mut ctx = ctx.clone();
ctx.set_agent_id(agent.id.clone());
ctx.set_project_id(project.id.clone());
```

**新方式（builder，不可变）**：
```rust
let ctx = ctx.to_builder()
    .agent_id(&agent.po.id)
    .project_id(&project.po.id)
    .build();
```

### 4.3 Consumer 异步场景重建上下文

```rust
// 从 message 元数据重建完整上下文
let ctx = RequestContext::builder()
    .organization_id(message.po.organization_id.clone())
    .agent_id(tool_call.from_id.clone())
    .project_id(tool_call.project_id.clone())
    .task_id(tool_call.task_id.clone())
    .build();
```

### 4.4 Cortex 创建时注入模型维度

```rust
let ctx = ctx.to_builder()
    .model_provider_id(&provider.po.id)
    .model_name(&provider.po.model_name)
    .build();
```

---

## 五、与旧 API 的兼容与迁移

### 5.1 过渡期策略

- **第一阶段**：新增 Builder API，保留所有 `set_*` 方法
- **第二阶段**：逐步将各模块的 `set_*` 调用迁移到 builder 模式
- **第三阶段**：确认无遗漏后，删除 `set_*` 方法，彻底锁死不可变性

### 5.2 保留的构造方法（向后兼容）

- `RequestContext::new(user_id, username)` — 内部委托给 builder
- `RequestContext::from_headers(headers)` — 内部委托给 builder
- `RequestContext::from_storage(user_id, storage)` — 内部委托给 builder

---

## 六、设计决策

### 6.1 为什么不用 with_* 直接在 RequestContext 上？

`ctx.clone().with_agent_id(...).with_project_id(...)` 也能工作，但有以下问题：

- 没有明确的"构建完成"边界，无法做统一校验
- `with_*` 方法会让 RequestContext 本身的 API 面膨胀
- builder 作为独立类型，语义更清晰："正在构建中" vs "已经构建好"

### 6.2 为什么用 owned 而不是引用？

- 上下文对象是要被移动和克隆的，所有字段用 owned（String 而非 &str）
- `with_*` 方法参数用 `impl Into<String>`，调用方可以传 `&str` 或 `String`
- 内部 `Arc<Storage>` 天然支持浅克隆

### 6.3 build() 时的校验（预留）

当前版本 `build()` 不做强校验，但预留了扩展位。未来可根据需要增加：

- `agent_id` 设置时必须有 `organization_id`
- `task_id` 设置时必须有 `project_id`
- 等等...

---

## 七、改动影响范围

### 不需要改的

- 所有 getter 方法（`agent_id()`, `project_id()` 等）不变
- `clone()` 行为不变
- `db_pool()`, `storage()`, `stats()` 等基础设施访问方法不变

### 需要逐步迁移的

- `set_*` 方法 → `to_builder().with_*().build()`
- 所有显式构造 `RequestContext { ... }` 结构体字面量的地方

### 新增的

- `RequestContextBuilder` 结构体
- `RequestContext::builder()` 静态方法
- `RequestContext::to_builder()` 实例方法
