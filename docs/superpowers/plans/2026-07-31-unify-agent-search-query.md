# 统一 Agent 搜索与查询接口实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `search_agents` 改造为支持完整过滤条件 + 分页返回，与 `query_agents` 形成统一查询能力；前端列表场景默认用 query，有关键词时切换到 search。

**Architecture:** 后端 `search_agents` 复用 `AgentSearch.filters: AgentQuery`（已存在）承载所有过滤条件，返回类型从 `Vec<Agent>` 改为 `PagedResult<Agent>`，DAL 层在内存过滤 runtime_state 后手动分页。前端 `search_agents` API 改为 POST body 传完整过滤条件，返回 `PagedResult`；agents 页面根据 keyword 是否为空切换 query/search。

**Tech Stack:** Rust (axum + sqlx + tokio), Dioxus (frontend), serde, schemars

---

## 背景与现状

### 当前架构问题

1. **search_agents 过滤能力不足**：当前 `SearchAgentsRequest` 只有 `keyword` + `limit`，无法复用 query 的完整过滤条件（roles、status、runtime_state 等）
2. **search_agents 无分页**：返回 `Vec<Agent>`，DAL 层用 `truncate` 截断，无法获取 total，前端无法实现完整分页
3. **前端切换逻辑割裂**：`search_agents(keyword)` 返回 `ListAgentsResponse`，`query_agents(req)` 返回 `PagedResult`，类型不统一

### 已有基础设施（复用）

- `AgentSearch.filters: AgentQuery` — search 已能复用 query 的所有过滤条件（包括新增的 `runtime_state`）
- `AgentQuery.runtime_state` — 已实现，DAL 层内存过滤
- `PagedResult<T>` — 统一分页响应结构（items + total）
- `PaginationParams` — 统一分页参数（limit + offset）

### 三种接口对应三种场景

| 接口 | HTTP | 场景 | 说明 |
|------|------|------|------|
| `list_agents` | GET `/agents` | 默认列表 | 无条件打开列表，最简场景 |
| `query_agents` | POST `/agents/query` | 条件过滤 | 有非关键词过滤条件（status、roles、runtime_state 等） |
| `search_agents` | POST `/agents/search` | 关键词搜索 | 用户输入关键词，需要 FTS5 + 向量语义混合搜索 |

三者各自独立，前端根据场景选择：无条件 → list；有过滤条件 → query；有关键词 → search。

### 设计决策

1. **search_agents 改为 POST**：与 query_agents 一致，支持复杂过滤条件通过 body 传递
2. **search 返回 PagedResult**：DAL 层 search 方法返回 `PagedResult<Agent>`，在内存中做 total 计数和分页
3. **runtime_state 过滤逻辑抽取为内部复用方法**：query 和 search 都需要按 runtime_state 内存过滤 + 手动分页，抽取为 `apply_runtime_state_filter` 私有方法避免重复
4. **前端统一类型**：`search_agents` 返回 `PagedResult<AgentListItem>`，与 `query_agents` / `list_agents` 类型一致
5. **前端三场景切换**：keyword 空 + 无过滤 → list_agents；有过滤条件 → query_agents；有 keyword → search_agents

## 文件结构

### 后端

- **修改** `common/src/api/agent.rs` — `SearchAgentsRequest` 增加完整过滤字段 + 分页；`SearchAgentsResponse` 改为 `PagedResult<AgentListItem>`
- **修改** `src/service/dao/agent/mod.rs` — 无需改动（AgentSearch 已复用 AgentQuery）
- **修改** `src/service/dal/agent.rs` — `AgentDal::search` 返回 `PagedResult<Agent>`，应用 runtime_state 内存过滤 + 分页
- **修改** `src/service/domain/hr/agent.rs` — `AgentManage::search_agents` trait 返回 `PagedResult<Agent>`
- **修改** `src/service/domain/hr/mod.rs` — trait 方法签名同步
- **修改** `src/handlers/hr/agent/search_agents.rs` — 透传完整过滤条件，返回 `PagedResult`
- **修改** `src/router.rs` — search 路由从 GET 改为 POST

### 前端

- **修改** `frontend/src/api/hr.rs` — `search_agents` 改为 POST + `SearchAgentsRequest`，返回 `PagedResult`
- **修改** `frontend/src/pages/hr/agents.rs` — 切换逻辑：keyword 空 → query，非空 → search；统一处理 `PagedResult`

### 测试

- **修改** `src/service/dal/agent_test.rs` — search 相关测试更新断言
- **新增** 针对 search_agents runtime_state 过滤的单元测试

---

## Task 1: 后端 SearchAgentsRequest/SearchAgentsResponse DTO 改造

**Files:**
- Modify: `common/src/api/agent.rs:295-311`

- [ ] **Step 1: 修改 SearchAgentsRequest 增加完整过滤字段**

将 `common/src/api/agent.rs` 中的 `SearchAgentsRequest` 改为：

```rust
/// 搜索 Agent 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchAgentsRequest {
    /// 搜索关键词（支持 FTS5 全文搜索 + 向量语义搜索）
    pub keyword: Option<String>,
    /// 状态筛选
    pub status: Option<AgentStatus>,
    /// 创建者 ID
    pub created_by: Option<String>,
    /// 模型供应商 ID
    pub model_provider_id: Option<String>,
    /// 角色列表
    pub roles: Option<Vec<String>>,
    /// 运行时状态筛选（0=Idle, 1=Resting, 2=Busy）
    pub runtime_state: Option<AgentRuntimeState>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}
```

注意：
- 移除 `#[param(source = "query")]` 属性（改为 POST body）
- 移除独立的 `limit` 字段，统一到 `pagination`
- 不再 derive `Params`（POST body 不需要）

- [ ] **Step 2: 修改 SearchAgentsResponse 改为 PagedResult 别名**

将 `SearchAgentsResponse` 改为：

```rust
/// 搜索 Agent 响应（分页）
pub type SearchAgentsResponse = PagedResult<AgentListItem>;
```

移除原有的 `pub struct SearchAgentsResponse { pub agents: Vec<AgentListItem> }`。

- [ ] **Step 3: 验证编译**

Run: `cargo check -p common 2>&1 | tail -5`
Expected: 编译通过，可能有下游 warnings（handler 还没改）

- [ ] **Step 4: Commit**

```bash
git add common/src/api/agent.rs
git commit -m "refactor: unify SearchAgentsRequest with full filter + pagination"
```

---

## Task 2: 抽取 runtime_state 过滤内部方法 + DAO/DAL search 改为返回 PagedResult

**Files:**
- Modify: `src/service/dao/agent/sqlite.rs:216-220` (DAO search_agents 增加 OFFSET)
- Modify: `src/service/dal/agent.rs:137` (DAL trait 签名)
- Modify: `src/service/dal/agent.rs:413-452` (DAL query 方法，复用抽取的方法)
- Modify: `src/service/dal/agent.rs:471-678` (DAL search 方法实现)

**背景**：
- runtime_state 是内存态，DAO 层无法 SQL 过滤。query 和 search 都需要"注入 runtime_info → 按状态过滤 → 手动分页"的逻辑，抽取为内部复用方法避免重复。
- DAO 层 query 方法已支持分页（LIMIT + OFFSET + COUNT）✅
- DAO 层 search_agents 方法只有 LIMIT 没有 OFFSET ❌，需补齐 OFFSET 支持
- DAL 层 search 方法返回 Vec 用 truncate 截断 ❌，需改为 PagedResult + 手动分页

**分页职责划分**：
- DAO 层 search_agents：通过 `filters.pagination` 感知 limit + offset，在 SQL 层限制 FTS5 结果数量（避免返回全量）
- DAL 层 search：聚合 FTS5 + 向量搜索结果后排序，**先截断到 MAX_SEARCH_RESULTS（20 条）**，再在内存中做最终分页

**搜索结果数量限制**：
搜索场景用户目标明确，如果在一定数量内搜不到应该换关键词而非疯狂分页。因此 search 限定总结果数为 `MAX_SEARCH_RESULTS = 20` 条，用户最多看到 20 条结果。分页在这个范围内进行（如 limit=10 最多翻 2 页）。这避免了关键词失控导致返回大量结果浪费性能且毫无意义。

```rust
/// 搜索场景最大返回结果数（搜索目标明确，搜不到应换关键词而非无限分页）
const MAX_SEARCH_RESULTS: usize = 20;
```

- [ ] **Step 0: DAO 层 search_agents 增加 OFFSET 支持 + 默认 limit**

在 `src/service/dao/agent/sqlite.rs` 第 216-220 行，将：

```rust
        builder.push(" ORDER BY agents_fts.rank");

        if let Some(limit) = filters.pagination.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }
```

改为：

```rust
        builder.push(" ORDER BY agents_fts.rank");

        // 搜索场景限制最大返回数量（避免关键词失控返回全量结果）
        // 用户传的 limit 若超过 MAX_SEARCH_RESULTS 则截断，未传则默认 MAX_SEARCH_RESULTS
        let search_limit = std::cmp::min(
            filters.pagination.limit.unwrap_or(20),
            20, // MAX_SEARCH_RESULTS，与 DAL 层常量保持一致
        );
        builder.push(" LIMIT ").push_bind(search_limit as i64);

        if let Some(offset) = filters.pagination.offset {
            builder.push(" OFFSET ").push_bind(offset as i64);
        }
```

**说明**：DAO 层 search_agents 现在感知完整的分页参数（limit + offset），且限制最大返回 20 条 FTS5 结果。DAL 层聚合 FTS5 + 向量结果后再次截断到 20 条并做最终分页。

- [ ] **Step 1: 新增 `apply_runtime_state_filter` 私有方法**

在 `src/service/dal/agent.rs` 的 `AgentDalImpl` impl 块中（建议放在 `inject_runtime_state` 方法附近），新增：

```rust
    /// 对已加载的 Agent 列表应用 runtime_state 内存过滤 + 分页
    ///
    /// runtime_state 是内存态（AgentRuntimeStateManager），DAO 层无法 SQL 过滤。
    /// 此方法在 DAL 层统一处理：注入 runtime_info → 按目标状态过滤 → 手动分页。
    /// query 和 search 方法复用此逻辑。
    fn apply_runtime_state_filter(
        agents: Vec<Agent>,
        target_state: common::enums::AgentRuntimeState,
        pagination: common::api::PaginationParams,
    ) -> common::api::PagedResult<Agent> {
        let filtered: Vec<Agent> = agents
            .into_iter()
            .filter(|agent| {
                let state = agent
                    .runtime_info
                    .as_ref()
                    .map(|info| info.state)
                    .unwrap_or(common::enums::AgentRuntimeState::Idle);
                state == target_state
            })
            .collect();
        let total = filtered.len();
        let offset = pagination.offset.unwrap_or(0);
        let limit = pagination.limit.unwrap_or(20);
        let items = filtered.into_iter().skip(offset).take(limit).collect();
        common::api::PagedResult { items, total }
    }
```

- [ ] **Step 2: query 方法复用 `apply_runtime_state_filter`**

将 `src/service/dal/agent.rs` 第 413-452 行的 `query` 方法简化为：

```rust
    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>> {
        // runtime_state 是内存态，DAO 层无法过滤。需内存过滤时查全量再手动分页。
        let runtime_state_filter = query.runtime_state;
        if let Some(target_state) = runtime_state_filter {
            let original_pagination = query.pagination.clone();
            let mut full_query = query;
            full_query.runtime_state = None;
            full_query.pagination = common::api::PaginationParams::default();

            let page = self.agent_dao.query(ctx, full_query).await?;
            let all_agents: Vec<Agent> = page
                .items
                .into_iter()
                .map(Agent::from_po)
                .map(Self::inject_runtime_state)
                .collect();

            return Ok(Self::apply_runtime_state_filter(
                all_agents,
                target_state,
                original_pagination,
            ));
        }

        let page = self.agent_dao.query(ctx, query).await?;
        Ok(page.map(Agent::from_po).map(Self::inject_runtime_state))
    }
```

- [ ] **Step 3: 修改 AgentDal trait search 方法签名**

将 `src/service/dal/agent.rs` 第 137 行的 trait 定义改为：

```rust
    /// 🔍 搜索 Agent（关键词 + 向量语义混合搜索）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果（三态匹配 + 综合排序）
    ///
    /// 返回分页结果，支持 runtime_state 内存过滤。
    async fn search(
        &self,
        ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>>;
```

- [ ] **Step 4: 修改 AgentDalImpl::search 实现**

对 `src/service/dal/agent.rs` 第 471 行起的 `search` 方法做三处定点修改，**其余逻辑（Step 1-7 的向量搜索 + FTS 搜索 + 聚合 + 排序）保持不变**：

**变更 A**：方法签名返回类型 `Vec<Agent>` → `PagedResult<Agent>`（第 471 行）

**变更 A2**：向量搜索限制从 50 改为 20（第 499 行）

原代码（第 498-499 行）：
```rust
                    // 向量搜索（前 50 条）
                    match self
                        .agent_vector_dao
                        .search_vector(ctx.clone(), &vec_params.vector, 50)
                        .await
```

改为：
```rust
                    // 向量搜索（前 MAX_SEARCH_RESULTS 条，与 FTS5 限制一致）
                    match self
                        .agent_vector_dao
                        .search_vector(ctx.clone(), &vec_params.vector, 20)
                        .await
```

**变更 B**：Step 6 构建 Agent 时注入 runtime_info（在第 619 行 `agents.push(agent);` 之前）

原代码（第 617-619 行）：
```rust
            let mut agent = Agent::from_po(po);
            agent.search_match = match_info;
            agents.push(agent);
```

改为：
```rust
            let mut agent = Agent::from_po(po);
            agent.search_match = match_info;
            // 注入 runtime_info（原实现缺失，search 结果也需要 runtime_state 供过滤和展示）
            agent = Self::inject_runtime_state(agent);
            agents.push(agent);
```

**变更 C**：Step 8 替换 truncate 为截断 + runtime_state 过滤 + 分页（第 672-677 行）

原代码（第 672-677 行）：
```rust
        // Step 8: 应用 limit
        if let Some(limit) = search.filters.pagination.limit {
            agents.truncate(limit);
        }

        Ok(agents)
```

改为：
```rust
        // Step 8: 截断到 MAX_SEARCH_RESULTS + runtime_state 内存过滤 + 分页
        // 搜索场景限制总结果数（MAX_SEARCH_RESULTS=20），搜不到应换关键词而非无限分页
        agents.truncate(20); // MAX_SEARCH_RESULTS

        let runtime_state_filter = search.filters.runtime_state;
        let pagination = search.filters.pagination.clone();
        let result = if let Some(target_state) = runtime_state_filter {
            Self::apply_runtime_state_filter(agents, target_state, pagination)
        } else {
            // 无 runtime_state 过滤，直接分页（total 最大为 MAX_SEARCH_RESULTS）
            let total = agents.len();
            let offset = pagination.offset.unwrap_or(0);
            let limit = pagination.limit.unwrap_or(20);
            let items = agents.into_iter().skip(offset).take(limit).collect();
            common::api::PagedResult { items, total }
        };

        Ok(result)
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p ai_orz --lib 2>&1 | tail -10`
Expected: 编译错误（domain handler 还没改），记录错误信息用于下一步

- [ ] **Step 6: Commit**

```bash
git add src/service/dal/agent.rs
git commit -m "refactor: extract apply_runtime_state_filter, search returns PagedResult"
```

---

## Task 3: 后端 Domain trait search_agents 签名同步

**Files:**
- Modify: `src/service/domain/hr/agent.rs:186-191`
- Modify: `src/service/domain/hr/mod.rs` (trait 定义，搜索 `search_agents`)

- [ ] **Step 1: 修改 AgentManage trait search_agents 方法签名**

在 `src/service/domain/hr/agent.rs` 和 `src/service/domain/hr/mod.rs` 中，将 `search_agents` 返回类型从 `Result<Vec<Agent>>` 改为 `Result<common::api::PagedResult<Agent>>`：

```rust
    async fn search_agents(
        &self,
        ctx: RequestContext,
        search: crate::service::dao::agent::AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.agent_dal.search(ctx, search).await
    }
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p ai_orz --lib 2>&1 | tail -10`
Expected: 编译错误仅在 handler 层（search_agents.rs）

- [ ] **Step 3: Commit**

```bash
git add src/service/domain/hr/agent.rs src/service/domain/hr/mod.rs
git commit -m "refactor: AgentManage::search_agents returns PagedResult"
```

---

## Task 4: 后端 search_agents handler 改造

**Files:**
- Modify: `src/handlers/hr/agent/search_agents.rs`

- [ ] **Step 1: 重写 search_agents handler**

将 `src/handlers/hr/agent/search_agents.rs` 改为：

```rust
//! Handler: POST /api/v1/hr/agents/search - Search agents with full filtering

use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentSearch};
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentListItem, PagedResult, SearchAgentsRequest};
use common::enums::AgentRuntimeState;
use common::error::Result;

/// Search AI agents with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_agents",
    name = "search_agents",
    description = "Search agents by keyword with full filtering support (FTS5 + vector semantic search).",
    params = "common::api::SearchAgentsRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn search_agents(
    ctx: RequestContext,
    params: SearchAgentsRequest,
) -> Result<PagedResult<AgentListItem>> {
    let search = AgentSearch {
        keyword: params.keyword,
        filters: AgentQuery {
            status: params.status,
            exclude_status: Some(common::enums::AgentStatus::Deleted),
            created_by: params.created_by,
            model_provider_id: params.model_provider_id,
            roles: params.roles,
            runtime_state: params.runtime_state,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().agent_manage().search_agents(ctx, search).await?;

    Ok(page.map(|agent| {
        let runtime_state = match &agent.runtime_info {
            Some(info) => info.state as i32,
            None => AgentRuntimeState::Idle as i32,
        };

        AgentListItem {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            roles: agent.po.get_roles(),
            description: if agent.po.description.is_empty() {
                None
            } else {
                Some(agent.po.description.clone())
            },
            kind: agent.po.kind.to_string(),
            model_provider_id: agent.po.model_provider_id.clone(),
            status: agent.po.status as i32,
            created_at: agent.po.created_at,
            runtime_state,
        }
    }))
}
```

- [ ] **Step 2: 修改 router.rs 中的 search 路由**

在 `src/router.rs` 中找到 `search_agents` 的路由注册，从 GET 改为 POST：

搜索 `/agents/search` 或 `search_agents` 在 router.rs 中的注册，将 HTTP 方法从 GET 改为 POST。

- [ ] **Step 3: 验证编译**

Run: `cargo check -p ai_orz --lib 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 4: 运行 clippy**

Run: `cargo clippy -p ai_orz --lib -- -D warnings 2>&1 | tail -5`
Expected: 0 warnings

- [ ] **Step 5: Commit**

```bash
git add src/handlers/hr/agent/search_agents.rs src/router.rs
git commit -m "refactor: search_agents handler supports full filter + pagination"
```

---

## Task 5: 后端测试更新

**Files:**
- Modify: `src/service/dal/agent_test.rs`

- [ ] **Step 1: 更新现有 search 相关测试**

搜索 `src/service/dal/agent_test.rs` 中所有调用 `search` 或 `search_agents` 的测试，将断言从 `Vec<Agent>` 改为 `PagedResult<Agent>`：

```rust
// 原断言
assert!(!results.is_empty());
// 改为
assert!(!results.items.is_empty());
assert!(results.total > 0);

// 原 len 断言
assert_eq!(agents.len(), 3);
// 改为
assert_eq!(agents.items.len(), 3);
assert_eq!(agents.total, 3);
```

- [ ] **Step 2: 新增 runtime_state 过滤测试**

在 `src/service/dal/agent_test.rs` 中新增测试：

```rust
#[tokio::test]
async fn test_search_agents_with_runtime_state_filter() {
    let ctx = test_context().await;
    let manager = AgentRuntimeStateManager::global();

    // 创建测试 Agent
    let agent1 = create_test_agent(&ctx, "agent-idle", "Idle Agent").await;
    let agent2 = create_test_agent(&ctx, "agent-busy", "Busy Agent").await;

    // 设置 runtime_state
    manager.set_idle(&agent1.id);
    manager.set_busy(&agent2.id, "msg-1");

    // 搜索 Idle Agent
    let search = AgentSearch {
        keyword: Some("Agent".to_string()),
        filters: AgentQuery {
            exclude_status: Some(AgentStatus::Deleted),
            runtime_state: Some(AgentRuntimeState::Idle),
            pagination: PaginationParams::default(),
            ..Default::default()
        },
        ..Default::default()
    };
    let result = domain().agent_manage().search_agents(ctx.clone(), search).await.unwrap();

    // 只返回 Idle Agent
    assert!(result.items.iter().all(|a| {
        a.runtime_info.as_ref().map(|i| i.state).unwrap_or(AgentRuntimeState::Idle)
            == AgentRuntimeState::Idle
    }));
    assert!(result.items.iter().any(|a| a.id() == "agent-idle"));
    assert!(!result.items.iter().any(|a| a.id() == "agent-busy"));

    // 清理
    manager.set_idle(&agent2.id);
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p ai_orz --lib agent_dal 2>&1 | grep "test result:"`
Expected: 所有测试通过

- [ ] **Step 4: Commit**

```bash
git add src/service/dal/agent_test.rs
git commit -m "test: update search tests for PagedResult + runtime_state filter"
```

---

## Task 6: 前端 search_agents API 改造

**Files:**
- Modify: `frontend/src/api/hr.rs:38-40`

- [ ] **Step 1: 修改 search_agents API 函数**

将 `frontend/src/api/hr.rs` 中的 `search_agents` 改为 POST + 完整请求体：

```rust
pub async fn search_agents(
    req: &SearchAgentsRequest,
) -> Result<PagedResult<AgentListItem>, ApiError> {
    api_post("/api/v1/hr/agents/search", req).await
}
```

同时在 `use common::api::{...}` 中导入 `SearchAgentsRequest`。

- [ ] **Step 2: 验证前端编译**

Run: `cargo check -p frontend --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 编译错误（agents.rs 页面还没改）

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/hr.rs
git commit -m "refactor: frontend search_agents API to POST with full filter"
```

---

## Task 7: 前端 agents 页面三场景切换逻辑改造

**Files:**
- Modify: `frontend/src/pages/hr/agents.rs`

**设计原则**：三种接口对应三种场景，前端根据状态选择：
- 无条件 + 无关键词 → `list_agents`（GET，默认列表）
- 有过滤条件（status、roles、runtime_state 等）+ 无关键词 → `query_agents`（POST，条件过滤）
- 有关键词 → `search_agents`（POST，关键词搜索）

当前 agents 页面只有搜索框（无过滤条件 UI），实际只用到 list 和 search 两种场景。但切换逻辑应预留 query 场景，供未来增加过滤条件 UI 时使用。

- [ ] **Step 1: 修改 agents 页面的数据加载逻辑**

将 `frontend/src/pages/hr/agents.rs` 中所有调用 `search_agents` 的位置（`reload_agents` 闭包、搜索框提交），从 `search_agents(&keyword)` 改为 `search_agents(&SearchAgentsRequest { keyword: Some(keyword), ..Default::default() })`，并从返回的 `PagedResult` 取 `.items`：

```rust
// reload_agents 闭包（约第 94-116 行）
let mut reload_agents = move || {
    let keyword = search_keyword();
    let my_id = search_request_id() + 1;
    search_request_id.set(my_id);
    spawn(async move {
        // 三场景切换：无关键词 → list_agents；有关键词 → search_agents
        // 未来增加过滤条件 UI 后：有过滤条件无关键词 → query_agents
        let result: Result<Vec<ListAgentsResponseItem>, ApiError> = if keyword.is_empty() {
            list_agents(ListAgentsRequest::default())
                .await
                .map(|p| p.items)
        } else {
            search_agents(&SearchAgentsRequest {
                keyword: Some(keyword),
                ..Default::default()
            })
            .await
            .map(|p| p.items)
        };
        if search_request_id() != my_id {
            return;
        }
        match result {
            Ok(v) => agents.set(v),
            Err(e) => toast.error(format!("{}", e)),
        }
    });
};
```

同样修改搜索框提交逻辑（约第 270-285 行）：

```rust
// 搜索框提交（约第 270-285 行）
let kw = search_keyword();
let my_id = search_request_id() + 1;
search_request_id.set(my_id);
spawn(async move {
    let result = if kw.is_empty() {
        list_agents(ListAgentsRequest::default())
            .await
            .map(|p| p.items)
    } else {
        search_agents(&SearchAgentsRequest {
            keyword: Some(kw),
            ..Default::default()
        })
        .await
        .map(|p| p.items)
    };
    if search_request_id() != my_id { return; }
    match result {
        Ok(v) => agents.set(v),
        Err(e) => toast.error(format!("{}", e)),
    }
    loading.set(false);
});
```

**关键变更**：
- `list_agents` 保持不变（默认场景）
- `search_agents(&keyword)` → `search_agents(&SearchAgentsRequest { keyword: Some(keyword), ..Default::default() })`
- `r.agents` → `p.items`（PagedResult 字段）
- 导入 `SearchAgentsRequest`

- [ ] **Step 2: 验证前端编译**

Run: `cargo check -p frontend --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 3: 运行前端 clippy**

Run: `cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings 2>&1 | tail -5`
Expected: 0 warnings

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/hr/agents.rs
git commit -m "refactor: agents page three-scenario switch (list/query/search)"
```

---

## Task 8: 技能文档更新

**Files:**
- Modify: `src/service/domain/system/seed/skills/communication/skill.md`
- Modify: `src/service/domain/system/seed/skills/project_management/skill.md`

- [ ] **Step 1: 更新沟通技能中的 Agent 查询工具描述**

在 `communication/skill.md` 的"发现协作伙伴"章节中，更新三种接口的描述，明确各自场景：

```markdown
### Agent 查询工具选择

| 工具 | 场景 | 说明 |
|------|------|------|
| `list_agents` | 默认列表 | 无条件获取 Agent 列表，最简场景 |
| `query_agents` | 条件过滤 | 按 status、roles、runtime_state 等条件精确筛选 |
| `search_agents` | 关键词搜索 | 按关键词 FTS5 + 向量语义混合搜索，也支持完整过滤条件 |

三者都返回 `PagedResult<AgentListItem>`（分页结果）。根据场景选择：无条件 → list；有过滤条件 → query；有关键词 → search。

### `search_agents` — 搜索 Agent

**能力**：按关键词搜索，支持 **FTS5 全文 + 向量语义混合搜索**，同时支持完整过滤条件（status、roles、runtime_state 等）。

**参数**（`SearchAgentsRequest`，POST body）：
- `keyword` — 搜索关键词（FTS5 + 向量语义）
- `status` / `created_by` / `model_provider_id` / `roles` — 过滤条件（与 query_agents 一致）
- `runtime_state` — 运行时状态过滤（0=Idle, 1=Resting, 2=Busy）
- `pagination` — 分页参数（limit + offset）

**返回**：`PagedResult<AgentListItem>`（分页结果，含 total）。

**与 `query_agents` 的区别**：`search_agents` 重在"语义相关性"（混合搜索），`query_agents` 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。
```

- [ ] **Step 2: 更新项目管理技能中的分配前查询描述**

在 `project_management/skill.md` 的"分配前查询空闲 Agent"章节中，更新查询方式描述，明确三种工具的选择：

```markdown
1. **查能力匹配**：用 `search_agents`（keyword 语义搜索，如"前端开发"）找到候选 Agent
2. **查运行时状态**：用 `query_agents` 或 `search_agents` 传 `runtime_state=0`（Idle）过滤出当前空闲的 Agent
3. **查串行约束**：
   - 分配项目前：用 `query_projects` 传 `owner_agent_id` + `status_in=[1,2,3]` 确认候选无未完结项目
   - 分配任务前：用 `list_agent_tasks` 传 `status=in_progress` 确认候选无进行中任务
```

- [ ] **Step 3: 验证 seed 测试**

Run: `cargo test -p ai_orz --lib seed 2>&1 | grep "test result:"`
Expected: 30 tests passed

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/system/seed/skills/communication/skill.md src/service/domain/system/seed/skills/project_management/skill.md
git commit -m "docs: update skills for three-scenario query/search/list"
```

---

## Task 9: 总结经验到 docs/todo.md

**Files:**
- Modify: `docs/todo.md`

**背景**：本次 Agent 搜索/查询统一改造的经验（三种场景 + runtime_state 过滤复用 + 分页统一）应推广到其他实体（工具、技能、项目、任务）。记录到 todo 便于后续逐步完善。

- [ ] **Step 1: 在 docs/todo.md 的"待办事项"章节新增条目**

在 `docs/todo.md` 第 33 行（第一个待办事项之后、`## 已完成事项` 之前）插入：

```markdown
### 2. 其他实体搜索/查询接口统一为三场景规范

**背景**：Agent 的 list/query/search 三种接口已统一为三种场景规范（list=默认列表、query=条件过滤、search=关键词搜索），都支持完整过滤条件和分页返回。其他拥有搜索和查询功能的实体应遵循同样规范。

**现状**：
- Agent: ✅ 已完成三场景统一（list/query/search 都返回 PagedResult，search 支持 runtime_state 过滤）
- Tool/Skill/Project/Task: ❌ search 返回 Vec 而非 PagedResult，且 search 不复用 query 的完整过滤条件

**待办**：
- [ ] Tool: search_tools 改为返回 PagedResult，复用 ToolQuery 的完整过滤条件
- [ ] Skill: search_skills 改为返回 PagedResult，复用 SkillQuery 的完整过滤条件
- [ ] Project: search_projects 改为返回 PagedResult，复用 ProjectQuery 的完整过滤条件（含 owner_agent_id）
- [ ] Task: search_tasks 改为返回 PagedResult，复用 TaskQuery 的完整过滤条件
- [ ] 每个实体的 search 方法都应支持所有 query 的过滤条件（包括内存态过滤如 runtime_state）
- [ ] 前端各实体列表页面统一为三场景切换：无条件 → list；有过滤条件 → query；有关键词 → search

**设计原则**（从 Agent 改造中总结）：
1. 三种接口对应三种场景：list（GET，默认列表）、query（POST，条件过滤）、search（POST，关键词搜索）
2. search 复用 query 的过滤条件（通过 `Search.filters: Query` 结构），在其基础上增加 keyword 搜索
3. 所有接口统一返回 `PagedResult<T>`，支持分页
4. 内存态过滤（如 runtime_state）抽取为内部复用方法（如 `apply_runtime_state_filter`），query 和 search 共享

**优先级**：中（架构一致性改进，非阻塞性）

**相关文件**：
- Agent 改造参考：`src/service/dal/agent.rs`（`apply_runtime_state_filter` + `query` + `search`）
- `src/handlers/hr/agent/`（list_agents / query_agents / search_agents handler）
- 待改造：`src/handlers/hr/tool/`、`src/handlers/hr/skill/`、`src/handlers/project/`（对应实体）

**关联计划**：[2026-07-31-unify-agent-search-query.md](superpowers/plans/2026-07-31-unify-agent-search-query.md)
```

- [ ] **Step 2: Commit**

```bash
git add docs/todo.md
git commit -m "docs: add todo for unifying other entities' search/query to three-scenario pattern"
```

---

## Task 10: 最终验证与推送

- [ ] **Step 1: 全量编译检查**

Run:
```bash
cargo check -p ai_orz --lib && \
cargo check -p frontend --target wasm32-unknown-unknown && \
cargo check -p common
```
Expected: 全部通过

- [ ] **Step 2: 全量 clippy 检查**

Run:
```bash
cargo clippy -p ai_orz --lib -- -D warnings && \
cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings
```
Expected: 0 warnings

- [ ] **Step 3: fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: 通过

- [ ] **Step 4: 全量测试**

Run:
```bash
cargo test -p ai_orz --lib agent_dal && \
cargo test -p ai_orz --lib seed && \
cargo test -p ai_orz --lib project
```
Expected: 全部通过

- [ ] **Step 5: 推送**

```bash
git push origin main
```
