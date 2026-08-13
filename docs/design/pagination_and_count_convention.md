# 查询接口分页与通用 count 规范

> 📌 **决策快照**：2026-07-24（分页规范）/ 2026-07-25（通用 count 规范）写定，从 AGENTS.md 4.9/4.10 迁移而来。
> AGENTS.md 只保留红线摘要，完整实现模式以本文档为准；现状描述以 wiki 为准。

## 核心原则

- **query 是核心查询能力，list 是语法糖**；两者统一返回 `PagedResult<T>`
- **count 与 query 复用查询结构体和 SQL 拼接逻辑**；特定 `count_*` 方法退化为语法糖直接调用通用 count

## 一、query / list 接口设计

| 接口类型 | 职责 | HTTP 方法 | 参数位置 | 返回 |
|---------|------|----------|---------|------|
| **query（核心）** | 完整查询条件 + 分页 | POST body | `XxxQueryRequest { ...查询条件..., pagination }` | `PagedResult<T>` |
| **list（语法糖）** | 只接受分页，内部固定默认过滤和排序 | GET query param | `?limit=10&offset=0` | `PagedResult<T>` |

**list 的"语法糖"含义**：

- 只接受分页参数（limit/offset），**不接受任何查询功能**（ids/status/keyword 等）
- 内部固定默认过滤（如排除 Deleted/Expired 状态）和默认排序（如 created_at DESC）
- 面向"给我第一页数据"的简单列表场景
- **任何涉及查询的操作（ids 批量查询、status 过滤、keyword 搜索等）必须走 query 接口**

## 二、分页基础设施（common/src/api/mod.rs）

```rust
/// 统一分页参数
pub struct PaginationParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 统一分页结果
pub struct PagedResult<T> {
    pub items: Vec<T>,       // 当前页数据
    pub total: usize,        // 总条数（忽略分页）
}

impl<T> PagedResult<T> {
    /// 转换 items 类型，保留 total（用于 PO → 业务实体 → ListItem 链式转换）
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> PagedResult<U> { ... }
}
```

## 三、全链路分页参数传递

```
Handler                   Domain                   DAL                   DAO
   │                        │                       │                     │
   ├─ XxxQueryRequest ──► XxxQuery ──────────────► XxxQuery ──────────► SQL
   │  (含 pagination)       (含 pagination)         (含 pagination)        │
   │                        │                       │                     │
   ◄─ PagedResult<T> ── ◄─ PagedResult.map(from_po) ◄─ PagedResult<Po> ◄─ COUNT + LIMIT/OFFSET
```

**关键约束**：

- pagination 字段随 Query 结构体一起传递，不需要单独的方法参数
- 每层用 `PagedResult::map()` 转换内部类型，保留 total
- DAO 层的 `query` 方法签名统一返回 `Result<PagedResult<Po>>`

## 四、DAO 层实现模式

每个 DAO 的 sqlite.rs 必须抽取 `push_query_filters` 函数，COUNT 和 LIST 查询复用同一套 WHERE 条件：

```rust
/// 推送查询过滤条件（COUNT 和 LIST 复用）
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &XxxQuery,
) {
    if let Some(ids) = &query.ids { /* ... */ }
    if let Some(status) = &query.status { /* ... */ }
    // ... 其他过滤条件
}

async fn query(&self, ctx: RequestContext, query: XxxQuery)
    -> Result<common::api::PagedResult<XxxPo>>
{
    // 1. COUNT 查询（复用 filters）
    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM xxx WHERE 1=1");
    push_query_filters(&mut count_builder, &query);
    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

    // 2. LIST 查询（复用 filters + LIMIT/OFFSET）
    let mut list_builder = sqlx::QueryBuilder::new("SELECT ... FROM xxx WHERE 1=1");
    push_query_filters(&mut list_builder, &query);
    list_builder.push(" ORDER BY created_at DESC");

    if let Some(limit) = query.pagination.limit {
        list_builder.push(" LIMIT ").push_bind(limit as i64);
    } else if query.pagination.offset.is_some() {
        list_builder.push(" LIMIT -1");  // SQLite: offset 单独使用需 LIMIT -1
    }
    if let Some(offset) = query.pagination.offset {
        list_builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let items = list_builder.build_query_as().fetch_all(pool).await?;
    Ok(common::api::PagedResult { items, total: total as usize })
}
```

## 五、Handler 层模式

**query handler**（POST，接受完整查询条件 + pagination）：

```rust
pub async fn query_agents(
    ctx: RequestContext,
    params: AgentQueryRequest,
) -> Result<common::api::PagedResult<AgentListItem>> {
    let page = domain().agent_manage().query(ctx, AgentQuery {
        ids: params.ids,
        status: params.status,
        pagination: params.pagination,  // 透传分页参数
        ..Default::default()
    }).await?;

    Ok(page.map(|agent| AgentListItem { ... }))  // 用 map 转换类型
}
```

**list handler**（GET，只接受分页，内部固定默认过滤）：

```rust
pub async fn list_agents(
    ctx: RequestContext,
    params: ListAgentsRequest,  // 只含 pagination 字段
) -> Result<common::api::PagedResult<AgentListItem>> {
    // list 是语法糖：内部固定排除 Deleted
    let page = domain().agent_manage().query(ctx, AgentQuery {
        exclude_status: Some(AgentStatus::Deleted),  // 固定默认过滤
        pagination: params.pagination,
        ..Default::default()
    }).await?;

    Ok(page.map(|agent| AgentListItem { ... }))
}
```

## 六、各实体的 list 默认过滤和排序

| 实体 | list 默认过滤 | list 默认排序 |
|------|-------------|-------------|
| Agent | `exclude_status = Deleted` | `created_at DESC` |
| Project | `status != 0`（软删除） | `priority DESC, created_at DESC` |
| Task | `status != 0`（软删除） | `priority DESC, created_at DESC` |
| Tool | 无 | `created_at DESC` |
| Skill | `exclude_status = Expired` | `updated_at DESC` |

## 七、通用 count 方法

| 接口类型 | 职责 | SQL 复用 | 返回 |
|---------|------|---------|------|
| **count（核心）** | 统计符合 Query 条件的总数 | 复用 `push_query_filters`，只跑 `SELECT COUNT(*)` 不跑 LIST | `u64` |
| **count_by_xxx（语法糖）** | 针对单字段条件的快捷方法 | 内部构造 Query 后调用通用 count | `u64` |

**与分页规范的关系**：`query` 返回 `PagedResult<T>`（含 items + total），其中 total 来自 COUNT 查询；通用 count 将 COUNT 抽取为独立方法，避免每次只为拿 total 跑完整 query。

### 三层透传链路

```
Handler                   Domain                   DAL                   DAO
   │                        │                       │                     │
   ├─ XxxQuery ──────────► count_xxx(ctx, query) ─► count(ctx, query) ─► SELECT COUNT(*)
   │  (复用 Query 结构体)    (透传 DAL)              (透传 DAO)            │
   │                                                │                     │
   ◄─ u64 ────────────── ◄─ u64 ──────────────── ◄─ u64 ───────────── ◄─ COUNT(*) AS total
```

**关键约束**：

- 三层 count 方法的签名统一：`async fn count(&self, ctx: RequestContext, query: XxxQuery) -> Result<u64>`
- Domain 层方法命名可以叫 `count_agents` / `count_projects` 等（更贴近业务语义），但内部只透传 DAL 的 `count`
- 特定的 `count_by_xxx` 方法（如 `count_by_assignee`、`count_by_root_user_and_status`）一律改为构造 Query 后调用通用 count

### DAO 层实现模式

每个 DAO 的 sqlite.rs 必须复用 `push_query_filters`（与 `query` 方法共享同一套 WHERE 条件）：

```rust
async fn count(&self, ctx: RequestContext, query: XxxQuery) -> Result<u64> {
    let pool = ctx.db_pool();
    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM xxx WHERE 1=1");
    push_query_filters(&mut count_builder, &query);  // 复用与 query 相同的过滤逻辑
    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;
    Ok(total as u64)
}

/// 语法糖：按 assignee 统计
async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64> {
    // 语法糖：调用通用 count
    self.count(ctx, XxxQuery {
        assignee_id: Some(assignee_id.to_string()),
        ..Default::default()
    }).await
}
```

### 各层实现要点

| 层级 | 实现要求 |
|------|---------|
| **DAO** | `count(ctx, query)` 复用 `push_query_filters`；所有 `count_by_xxx` 改为构造 Query 后调用 `self.count(...)` |
| **DAL** | `count(ctx, query)` 透传 DAO；所有 `count_by_xxx` 改为构造 Query 后调用 `self.count(...)` |
| **Domain** | `count_xxx(ctx, query)` 透传 DAL；特定 `count_by_xxx` 同样构造 Query 后调用通用 `count_xxx` |

## 八、禁止的写法

```rust
// ❌ 禁止：list 接口接受查询字段
pub struct ListAgentsRequest {
    pub status: Option<AgentStatus>,    // 应移除，走 query 接口
    pub ids: Option<Vec<String>>,       // 应移除，走 query 接口
}

// ❌ 禁止：DAO 层 query 方法返回 Vec 而非 PagedResult
async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<AgentPo>>;

// ❌ 禁止：在 DAL/Domain 层手工拼接 limit/offset 而不用 PaginationParams
let limit = query.limit;  // 应为 query.pagination.limit

// ❌ 禁止：Handler 层把 PagedResult 当 Vec 用
let agents: Vec<Agent> = domain().agent_manage().query(ctx, q).await?;  // 应取 .items

// ❌ 禁止：在 count 方法中独立拼接 WHERE 条件，不复用 push_query_filters
async fn count(&self, ctx: RequestContext, query: XxxQuery) -> Result<u64> {
    let mut sql = String::from("SELECT COUNT(*) FROM xxx WHERE 1=1");
    if query.assignee_id.is_some() { sql.push_str(" AND assignee_id = ?"); }  // 应复用 push_query_filters
}

// ❌ 禁止：count_by_xxx 方法独立实现 SQL，不调用通用 count
async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64> {
    let count = sqlx::query!("SELECT COUNT(*) FROM xxx WHERE assignee_id = ?", assignee_id)
        .fetch_one(ctx.db_pool()).await?;
    Ok(count.count as u64)  // 应改为 self.count(ctx, XxxQuery { ... }).await
}

// ❌ 禁止：DAL/Domain 层独立实现 count 逻辑，不透传到 DAO
async fn count_by_xxx(&self, ctx: RequestContext, ...) -> Result<u64> {
    let items = self.query(ctx, ...).await?;  // 不能用 query 然后取 len()
    Ok(items.len() as u64)
}
```

## 九、参考实现

- **基础设施**：`common/src/api/mod.rs` 的 `PaginationParams` 和 `PagedResult<T>`
- **DAO 分页参考**：`src/service/dao/mcp_server/sqlite.rs`（首个完成分页改造的 DAO）
- **DAO count 参考**：`src/service/dao/project/sqlite.rs` 的 `count` + `count_by_root_user` + `count_by_root_user_and_status`
- **DAL count 参考**：`src/service/dal/project.rs` 的 `count` + `count_by_root_user`（语法糖）
- **Domain count 参考**：`src/service/domain/project/project.rs` 的 `count_projects`（透传 DAL）
- **已改造实体（分页）**：Agent / Project / Task / Tool / Skill（DAO + Domain + Handler 全链路）+ McpServer / MessageChannel / Artifact / ModelProvider / User（仅 DAO query 通用化，Handler 按需适配）
- **已落地实体（count）**：Agent / Project / Task / Message / Artifact / User / Organization

## 十、设计动机

- **分页**：统一分页接口避免每个实体自定义分页逻辑，list 作为语法糖降低简单场景的使用成本，query 作为核心能力覆盖所有复杂查询需求。前端只需处理统一的 `PagedResult<T>` 结构。
- **count**：将 count 与 query 的 WHERE 条件统一到 `push_query_filters` 一处，避免「count 漏掉某个过滤条件」的常见 bug。特定 count 方法退化为语法糖后，新增查询条件时只需改 `push_query_filters` 一处，所有 count_by_xxx 自动同步。
