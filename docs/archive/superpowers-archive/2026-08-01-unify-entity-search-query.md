# 统一其他实体搜索/查询接口实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Tool/Skill/Project/Task 四个实体的 list/query/search 接口统一为三场景规范，对齐 Agent 已完成的改造模式。

**Architecture:** 自底向上改造每个实体：DTO（SearchRequest + PagedResult Response）→ DAO（search 复用 push_query_filters + OFFSET + LIMIT 20）→ DAL（search 返回 PagedResult + truncate(20)）→ Domain trait（暴露 search 方法）→ Handler（POST + 完整过滤字段）→ Router → 前端（三场景切换）。四个实体相互独立，可并行改造。

**Tech Stack:** Rust (sqlx + async-trait) / Dioxus (frontend) / SQLite (FTS5 + vss0)

**规范基准：** [docs/entity_list_query_search_design.md](../../entity_list_query_search_design.md)

**Agent 标杆实现参考：**
- DTO: `common/src/api/agent.rs` (SearchAgentsRequest)
- DAL: `src/service/dal/agent/mod.rs` (apply_runtime_state_filter + search 返回 PagedResult)
- DAO: `src/service/dao/agent/sqlite.rs` (search_agents 复用 push_query_filters + OFFSET)
- Handler: `src/handlers/hr/agent/search_agents.rs`
- 前端: `frontend/src/api/hr.rs` + `frontend/src/pages/hr/agents.rs`

---

## File Structure

### Tool 实体
- Modify: `common/src/api/tool.rs` — 新增 SearchToolsRequest/SearchToolsResponse
- Modify: `src/service/dao/tool/mod.rs` — ToolSearch 改为复用 ToolQuery
- Modify: `src/service/dao/tool/sqlite.rs` — search_tools 复用 push_query_filters + OFFSET
- Modify: `src/service/dal/tool.rs` — search 返回 PagedResult + truncate(20)
- Modify: `src/service/domain/finance/mod.rs` — ToolProviderManage::search_tools 返回 PagedResult
- Modify: `src/service/domain/finance/tool_provider.rs` — 实现委托
- Create: `src/handlers/finance/tool/search_tools.rs` — 新 handler
- Modify: `src/handlers/finance/tool/mod.rs` — 声明模块
- Modify: `src/router.rs` — 注册 POST /tools/search
- Modify: `frontend/src/api/finance.rs` — 新增 search_tools
- Modify: `frontend/src/pages/finance/tools.rs` — 三场景切换

### Skill 实体
- Modify: `common/src/api/skill.rs` — 改造 SearchSkillsRequest（补全字段）+ SearchSkillsResponse=PagedResult
- Modify: `src/service/dao/tool/sqlite.rs` — search_skills 复用 push_query_filters + OFFSET
- Modify: `src/service/dal/skill.rs` — search 返回 PagedResult + truncate(20)
- Modify: `src/service/domain/hr/mod.rs` — SkillManage::search_skills 返回 PagedResult
- Modify: `src/service/domain/hr/skill.rs` — 实现委托
- Modify: `src/handlers/hr/skill/search_skills.rs` — 改造为 POST + 完整过滤
- Modify: `src/router.rs` — 路由改 POST
- Modify: `frontend/src/api/hr.rs` — search_skills 改签名
- Modify: `frontend/src/pages/hr/skills.rs` — 三场景切换

### Project 实体
- Modify: `common/src/api/project.rs` — 新增 SearchProjectsRequest/SearchProjectsResponse
- Modify: `src/service/dao/project/sqlite.rs` — search_projects 复用 push_query_filters + OFFSET（**修复关键缺陷**）
- Modify: `src/service/dal/project.rs` — search 返回 PagedResult + truncate(20)
- Modify: `src/service/domain/project/mod.rs` — ProjectManage 新增 search 方法
- Create: `src/handlers/project/projects/search_projects.rs` — 新 handler
- Modify: `src/handlers/project/projects/mod.rs` — 声明模块
- Modify: `src/router.rs` — 注册 POST /projects/search
- Modify: `frontend/src/api/project.rs` — 新增 search_projects
- Modify: `frontend/src/pages/project/projects.rs` — 三场景切换

### Task 实体
- Modify: `common/src/api/task.rs` — 新增 SearchTasksRequest/SearchTasksResponse
- Modify: `src/service/dao/task/sqlite.rs` — search_tasks 添加 OFFSET（SQL 已复用过滤，仅需补 OFFSET + LIMIT 20）
- Modify: `src/service/dal/task.rs` — search 返回 PagedResult + truncate(20)
- Modify: `src/service/domain/project/mod.rs` — TaskManage 新增 search 方法
- Create: `src/handlers/project/task/search_tasks.rs` — 新 handler
- Modify: `src/handlers/project/task/mod.rs` — 声明模块
- Modify: `src/router.rs` — 注册 POST /tasks/search
- Modify: `frontend/src/api/project.rs` — 新增 search_tasks
- Modify: `frontend/src/pages/project/tasks.rs` — 三场景切换

---

## Task 1: Tool 实体后端改造

**Files:**
- Modify: `common/src/api/tool.rs`
- Modify: `src/service/dao/tool/mod.rs`
- Modify: `src/service/dao/tool/sqlite.rs`
- Modify: `src/service/dal/tool.rs`
- Modify: `src/service/domain/finance/mod.rs`
- Modify: `src/service/domain/finance/tool_provider.rs`
- Create: `src/handlers/finance/tool/search_tools.rs`
- Modify: `src/handlers/finance/tool/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 新增 Tool SearchToolsRequest/Response DTO**

在 `common/src/api/tool.rs` 中（参考 `common/src/api/agent.rs` 的 SearchAgentsRequest）：

```rust
use crate::api::{PagedResult, PaginationParams};
// 确保已导入 PagedResult

/// 搜索 Tool 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchToolsRequest {
    /// 搜索关键词（FTS5 + 向量语义混合搜索）
    pub keyword: Option<String>,
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 绑定到指定 Agent
    pub agent_id: Option<String>,
    /// 标签过滤
    pub tags: Option<Vec<String>>,
    /// 协议过滤
    pub protocol: Option<ToolProtocol>,
    /// 状态过滤
    pub status: Option<ToolStatus>,
    /// MCP Server ID
    pub mcp_server_id: Option<String>,
    /// 仅返回启用状态
    pub enabled_only: Option<bool>,
    /// 分页参数
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Tool 响应（分页）
pub type SearchToolsResponse = PagedResult<ToolListItem>;
```

- [ ] **Step 2: 改造 ToolSearch 结构体复用 ToolQuery**

在 `src/service/dao/tool/mod.rs` 中，将 `ToolSearch` 改为复用 `ToolQuery`：

```rust
// 原来的 ToolSearch（独立结构体，只有 4 字段）删除，改为：
/// Tool 搜索参数（keyword + 向量参数 + filters 复用 ToolQuery）
#[derive(Debug, Clone, Default)]
pub struct ToolSearch {
    /// 搜索关键词
    pub keyword: Option<String>,
    /// 查询向量（由 DAL 层生成）
    pub query_vector: Option<Vec<f32>>,
    /// 向量搜索 top_k
    pub top_k: usize,
    /// 向量距离阈值
    pub vector_distance_threshold: Option<f32>,
    /// 过滤条件（复用 ToolQuery）
    pub filters: ToolQuery,
}
```

- [ ] **Step 3: 改造 DAO search_tools SQL 复用 push_query_filters + OFFSET**

在 `src/service/dao/tool/sqlite.rs` 的 `search_tools` 方法中，替换手动内联的 WHERE 子句为复用 `push_query_filters`，并添加 OFFSET：

```rust
async fn search_tools(
    &self,
    _ctx: RequestContext,
    params: ToolSearch,
) -> Result<Vec<(ToolPo, Option<f32>)>> {
    use sqlx::QueryBuilder;

    let keyword = params.keyword.unwrap_or_default();
    if keyword.trim().is_empty() {
        return Ok(Vec::new());
    }

    let escaped_keyword = escape_fts5_keyword(&keyword);
    let filters = params.filters;

    let mut builder = QueryBuilder::new(
        r#"SELECT t.id, t.name, t.description, t.tags, t.protocol, t.status, t.enabled,
                  t.mcp_server_id, t.created_by, t.modified_by, t.created_at, t.updated_at,
                  tools_fts.rank as fts_rank
           FROM tools_fts
           JOIN tools t ON tools_fts.rowid = t.rowid
           WHERE tools_fts MATCH "#,
    );
    builder.push_bind(escaped_keyword);

    // ✅ 复用 push_query_filters（与 query 共享过滤逻辑）
    push_query_filters(&mut builder, &filters);

    builder.push(" ORDER BY tools_fts.rank");

    // 搜索场景限制最大返回数量
    let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
    builder.push(" LIMIT ").push_bind(search_limit as i64);
    if let Some(offset) = filters.pagination.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let rows: Vec<ToolSearchRow> = builder
        .build_query_as::<ToolSearchRow>()
        .fetch_all(_ctx.db_pool())
        .await?;

    // ... 原有的 row → po 映射逻辑保持不变 ...
    Ok(results)
}
```

注意：`push_query_filters` 函数需确保处理 `agent_id`（通过 EXISTS 子查询，参考原 search_tools 的实现）。如果 `push_query_filters` 没有 agent_id 逻辑，需在 search_tools 中额外添加。

- [ ] **Step 4: 改造 DAL search 返回 PagedResult + truncate(20)**

在 `src/service/dal/tool.rs` 中：

trait 签名改为：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    params: ToolSearch,
) -> Result<common::api::PagedResult<Tool>>;
```

实现中 Step 8（原 `truncate(limit)`）改为：
```rust
// Step 8: 截断到 MAX_SEARCH_RESULTS + 分页
tools.truncate(20);

let pagination = params.filters.pagination.clone();
let total = tools.len();
let offset = pagination.offset.unwrap_or(0);
let limit = pagination.limit.unwrap_or(20);
let items = tools.into_iter().skip(offset).take(limit).collect();
Ok(common::api::PagedResult { items, total })
```

向量搜索限制从 50 改为 20：
```rust
// 原：self.tool_vector_dao.search_vector(ctx.clone(), &vec_params.vector, 50)
// 改为：
self.tool_vector_dao.search_vector(ctx.clone(), &vec_params.vector, 20)
```

- [ ] **Step 5: 改造 Domain trait + 实现签名**

在 `src/service/domain/finance/mod.rs` 的 `ToolProviderManage` trait 中：
```rust
async fn search_tools(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::tool::ToolSearch,
) -> Result<common::api::PagedResult<Tool>>;
```

在 `src/service/domain/finance/tool_provider.rs` 实现中：
```rust
async fn search_tools(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::tool::ToolSearch,
) -> Result<common::api::PagedResult<Tool>> {
    self.tool_dal.search(ctx, search).await
}
```

- [ ] **Step 6: 新增 search_tools handler**

创建 `src/handlers/finance/tool/search_tools.rs`（参考 `src/handlers/hr/agent/search_agents.rs`）：

```rust
//! Handler: POST /api/v1/finance/tools/search - Search tools with full filtering

use crate::pkg::RequestContext;
use crate::service::dao::tool::{ToolQuery, ToolSearch};
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SearchToolsRequest, ToolListItem};
use common::error::Result;
use common::api::PagedResult;

#[register_handler_tool(
    id = "search_tools",
    name = "search_tools",
    description = "Search tools by keyword with full filtering support (FTS5 + vector semantic search).",
    params = "common::api::SearchToolsRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn search_tools(
    ctx: RequestContext,
    params: SearchToolsRequest,
) -> Result<PagedResult<ToolListItem>> {
    let search = ToolSearch {
        keyword: params.keyword,
        filters: ToolQuery {
            ids: params.ids,
            agent_id: params.agent_id,
            tags: params.tags,
            protocol: params.protocol,
            status: params.status,
            mcp_server_id: params.mcp_server_id,
            enabled_only: params.enabled_only,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().tool_provider_manage().search_tools(ctx, search).await?;

    Ok(page.map(|tool| ToolListItem {
        id: tool.po.id,
        name: tool.po.name,
        description: if tool.po.description.is_empty() { None } else { Some(tool.po.description.clone()) },
        tags: tool.po.get_tags(),
        protocol: tool.po.protocol.to_string(),
        status: tool.po.status as i32,
        enabled: tool.po.enabled,
        mcp_server_id: tool.po.mcp_server_id.clone(),
        created_at: tool.po.created_at,
    }))
}
```

注意：`ToolListItem` 的字段映射需根据实际结构体调整，参考 `list_tools.rs` 和 `query_tools.rs` 的映射逻辑。

- [ ] **Step 7: 声明模块 + 注册路由**

在 `src/handlers/finance/tool/mod.rs` 中添加：
```rust
pub mod search_tools;
```

在 `src/router.rs` 中找到 tools 路由组，添加：
```rust
.route(
    "/tools/search",
    post(handlers::finance::tool::search_tools_handler),
)
```

- [ ] **Step 8: 验证编译 + clippy**

Run: `cargo check -p ai_orz --lib && cargo clippy -p ai_orz --lib -- -D warnings`
Expected: PASS

- [ ] **Step 9: 更新测试**

更新 `src/service/dal/tool_test.rs`（或对应测试文件）中所有 search 相关测试断言：
- `results.len()` → `results.items.len()`
- `results[N]` → `results.items[N]`

更新所有 mock/stub 实现的 search 方法签名为返回 `PagedResult`。

Run: `cargo test -p ai_orz --lib tool_dal 2>&1`
Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add common/src/api/tool.rs src/service/dao/tool/ src/service/dal/tool.rs \
        src/service/domain/finance/ src/handlers/finance/tool/ src/router.rs
git commit -m "refactor: unify Tool search/query to three-scenario pattern"
```

---

## Task 2: Tool 实体前端改造

**Files:**
- Modify: `frontend/src/api/finance.rs`
- Modify: `frontend/src/pages/finance/tools.rs`

- [ ] **Step 1: 新增前端 search_tools API**

在 `frontend/src/api/finance.rs` 中（参考 `frontend/src/api/hr.rs` 的 search_agents）：

```rust
pub async fn search_tools(req: &SearchToolsRequest) -> Result<PagedResult<ToolListItem>, ApiError> {
    api_post("/api/v1/finance/tools/search", req).await
}
```

确保 import 了 `SearchToolsRequest`。

- [ ] **Step 2: 改造 tools 页面三场景切换**

在 `frontend/src/pages/finance/tools.rs` 中（参考 `frontend/src/pages/hr/agents.rs`），修改列表加载逻辑：

```rust
// 三场景切换：无关键词 → list_tools；有关键词 → search_tools
let result = if keyword.trim().is_empty() {
    list_tools(ListToolsRequest::default()).await.map(|p| p.items)
} else {
    search_tools(&SearchToolsRequest {
        keyword: Some(keyword),
        ..Default::default()
    }).await.map(|p| p.items)
};
```

- [ ] **Step 3: 验证编译 + clippy**

Run: `cargo check -p frontend --target wasm32-unknown-unknown && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/tools.rs
git commit -m "refactor: frontend tools page three-scenario switch"
```

---

## Task 3: Skill 实体后端改造

**Files:**
- Modify: `common/src/api/skill.rs`
- Modify: `src/service/dao/skill/sqlite.rs`
- Modify: `src/service/dal/skill.rs`
- Modify: `src/service/domain/hr/mod.rs`
- Modify: `src/service/domain/hr/skill.rs`
- Modify: `src/handlers/hr/skill/search_skills.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 改造 SearchSkillsRequest/Response DTO**

在 `common/src/api/skill.rs` 中：

```rust
/// 搜索 Skill 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchSkillsRequest {
    /// 搜索关键词（FTS5 + 向量语义混合搜索）
    pub keyword: Option<String>,
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 状态过滤
    pub status: Option<SkillStatus>,
    /// 分类过滤
    pub category: Option<String>,
    /// 作者 ID
    pub author_id: Option<String>,
    /// 父技能 ID
    pub parent_skill_id: Option<String>,
    /// 标签过滤
    pub tags: Option<Vec<String>>,
    /// 分页参数
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Skill 响应（分页）
pub type SearchSkillsResponse = PagedResult<SkillListItem>;
```

删除旧的 `SearchSkillsResponse { skills: Vec<...> }` 结构体。

- [ ] **Step 2: 改造 DAO search_skills SQL 复用 push_query_filters + OFFSET**

在 `src/service/dao/skill/sqlite.rs` 的 `search` 方法中，替换手动内联 WHERE 为复用 `push_query_filters`：

```rust
// 原：手动 push status/category/author_id 过滤
// 改为：
push_query_filters(&mut builder, &filters);

builder.push(" ORDER BY skills_fts.rank");
let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
builder.push(" LIMIT ").push_bind(search_limit as i64);
if let Some(offset) = filters.pagination.offset {
    builder.push(" OFFSET ").push_bind(offset as i64);
}
```

- [ ] **Step 3: 改造 DAL search 返回 PagedResult + truncate(20)**

在 `src/service/dal/skill.rs` 中：

trait 签名：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: SkillSearch,
) -> Result<common::api::PagedResult<Skill>>;
```

实现 Step 8：
```rust
skills.truncate(20);
let pagination = search.filters.pagination.clone();
let total = skills.len();
let offset = pagination.offset.unwrap_or(0);
let limit = pagination.limit.unwrap_or(20);
let items = skills.into_iter().skip(offset).take(limit).collect();
Ok(common::api::PagedResult { items, total })
```

向量搜索限制 50 → 20。

- [ ] **Step 4: 改造 Domain trait + 实现签名**

在 `src/service/domain/hr/mod.rs` 的 `SkillManage` trait 中：
```rust
async fn search_skills(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::skill::SkillSearch,
) -> Result<common::api::PagedResult<Skill>>;
```

在 `src/service/domain/hr/skill.rs` 实现中：
```rust
async fn search_skills(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::skill::SkillSearch,
) -> Result<common::api::PagedResult<Skill>> {
    self.skill_dal.search(ctx, search).await
}
```

- [ ] **Step 5: 改造 search_skills handler 为 POST + 完整过滤**

重写 `src/handlers/hr/skill/search_skills.rs`（参考 `src/handlers/hr/agent/search_agents.rs`）：

```rust
//! Handler: POST /api/v1/hr/skills/search - Search skills with full filtering

use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SearchSkillsRequest, SkillListItem};
use common::enums::SkillStatus;
use common::error::Result;

#[register_handler_tool(
    id = "search_skills",
    name = "search_skills",
    description = "Search skills by keyword with full filtering support (FTS5 + vector semantic search).",
    params = "common::api::SearchSkillsRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn search_skills(
    ctx: RequestContext,
    params: SearchSkillsRequest,
) -> Result<PagedResult<SkillListItem>> {
    let search = SkillSearch {
        keyword: params.keyword,
        filters: SkillQuery {
            ids: params.ids,
            status: params.status,
            exclude_status: Some(SkillStatus::Expired),
            category: params.category,
            author_id: params.author_id,
            parent_skill_id: params.parent_skill_id,
            tags: params.tags,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().skill_manage().search_skills(ctx, search).await?;

    Ok(page.map(|skill| SkillListItem {
        // 字段映射参考 list_skills.rs / query_skills.rs
        id: skill.po.id,
        name: skill.po.name,
        // ... 其他字段
    }))
}
```

- [ ] **Step 6: 路由改 POST**

在 `src/router.rs` 中：
```rust
// 原：get(handlers::hr::skill::search_skills_handler)
// 改为：
.route(
    "/skills/search",
    post(handlers::hr::skill::search_skills_handler),
)
```

- [ ] **Step 7: 验证编译 + clippy + 测试**

Run: `cargo check -p ai_orz --lib && cargo clippy -p ai_orz --lib -- -D warnings && cargo test -p ai_orz --lib skill_dal`
Expected: PASS

更新测试断言：`results.len()` → `results.items.len()`

- [ ] **Step 8: Commit**

```bash
git add common/src/api/skill.rs src/service/dao/skill/ src/service/dal/skill.rs \
        src/service/domain/hr/ src/handlers/hr/skill/search_skills.rs src/router.rs
git commit -m "refactor: unify Skill search to three-scenario pattern"
```

---

## Task 4: Skill 实体前端改造

**Files:**
- Modify: `frontend/src/api/hr.rs`
- Modify: `frontend/src/pages/hr/skills.rs`

- [ ] **Step 1: 改造 search_skills API 签名**

在 `frontend/src/api/hr.rs` 中：
```rust
pub async fn search_skills(req: &SearchSkillsRequest) -> Result<PagedResult<SkillListItem>, ApiError> {
    api_post("/api/v1/hr/skills/search", req).await
}
```

- [ ] **Step 2: 改造 skills 页面三场景切换**

在 `frontend/src/pages/hr/skills.rs` 中：
```rust
let result = if keyword.trim().is_empty() {
    list_skills(ListSkillsRequest::default()).await.map(|p| p.items)
} else {
    search_skills(&SearchSkillsRequest {
        keyword: Some(keyword),
        ..Default::default()
    }).await.map(|p| p.items)
};
```

- [ ] **Step 3: 验证 + Commit**

Run: `cargo check -p frontend --target wasm32-unknown-unknown && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings`

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/skills.rs
git commit -m "refactor: frontend skills page three-scenario switch"
```

---

## Task 5: Project 实体后端改造（含修复 SQL 缺陷）

**Files:**
- Modify: `common/src/api/project.rs`
- Modify: `src/service/dao/project/sqlite.rs`
- Modify: `src/service/dal/project.rs`
- Modify: `src/service/domain/project/mod.rs`
- Create: `src/handlers/project/projects/search_projects.rs`
- Modify: `src/handlers/project/projects/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 新增 SearchProjectsRequest/Response DTO**

在 `common/src/api/project.rs` 中：

```rust
/// 搜索 Project 请求（POST body）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchProjectsRequest {
    pub keyword: Option<String>,
    pub ids: Option<Vec<String>>,
    pub root_user_id: Option<String>,
    pub status_in: Option<Vec<ProjectStatus>>,
    pub owner_agent_id: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub type SearchProjectsResponse = PagedResult<ProjectListItem>;
```

- [ ] **Step 2: 修复 search_projects SQL 复用 push_query_filters + OFFSET（关键缺陷修复）**

在 `src/service/dao/project/sqlite.rs` 的 `search_projects` 方法中（**这是关键缺陷修复**）：

```rust
// 原 SQL 仅 WHERE projects_fts MATCH ? AND p."status" != 0
// 改为复用 push_query_filters：

let mut builder = QueryBuilder::new(
    r#"SELECT p.id, p.name, p.description, p.root_user_id, p.status, p.owner_agent_id,
              p.created_by, p.modified_by, p.created_at, p.updated_at,
              projects_fts.rank as fts_rank
       FROM projects_fts
       JOIN projects p ON projects_fts.rowid = p.rowid
       WHERE projects_fts MATCH "#,
);
builder.push_bind(escaped_keyword);

// ✅ 复用 push_query_filters（修复原 SQL 未应用 root_user_id/status_in/owner_agent_id/ids 的缺陷）
push_query_filters(&mut builder, &filters);

builder.push(" ORDER BY projects_fts.rank");
let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
builder.push(" LIMIT ").push_bind(search_limit as i64);
if let Some(offset) = filters.pagination.offset {
    builder.push(" OFFSET ").push_bind(offset as i64);
}
```

- [ ] **Step 3: 改造 DAL search 返回 PagedResult + truncate(20)**

在 `src/service/dal/project.rs` 中：

trait 签名：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: ProjectSearch,
) -> Result<common::api::PagedResult<Project>>;
```

实现 Step 8：
```rust
projects.truncate(20);
let pagination = search.filters.pagination.clone();
let total = projects.len();
let offset = pagination.offset.unwrap_or(0);
let limit = pagination.limit.unwrap_or(20);
let items = projects.into_iter().skip(offset).take(limit).collect();
Ok(common::api::PagedResult { items, total })
```

向量搜索限制 50 → 20。

- [ ] **Step 4: Domain trait 新增 search 方法**

在 `src/service/domain/project/mod.rs` 的 `ProjectManage` trait 中新增：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::project::ProjectSearch,
) -> Result<common::api::PagedResult<Project>> {
    self.project_dal.search(ctx, search).await
}
```

- [ ] **Step 5: 新增 search_projects handler**

创建 `src/handlers/project/projects/search_projects.rs`（参考 `src/handlers/hr/agent/search_agents.rs`）：

```rust
//! Handler: POST /api/v1/projects/search - Search projects with full filtering

use crate::pkg::RequestContext;
use crate::service::dao::project::{ProjectQuery, ProjectSearch};
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, ProjectListItem, SearchProjectsRequest};
use common::error::Result;

#[register_handler_tool(
    id = "search_projects",
    name = "search_projects",
    description = "Search projects by keyword with full filtering support.",
    params = "common::api::SearchProjectsRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn search_projects(
    ctx: RequestContext,
    params: SearchProjectsRequest,
) -> Result<PagedResult<ProjectListItem>> {
    let search = ProjectSearch {
        keyword: params.keyword,
        filters: ProjectQuery {
            ids: params.ids,
            root_user_id: params.root_user_id,
            status_in: params.status_in,
            owner_agent_id: params.owner_agent_id,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().project_manage().search(ctx, search).await?;

    Ok(page.map(|project| ProjectListItem {
        // 字段映射参考 list_projects.rs / query_projects.rs
        id: project.po.id,
        name: project.po.name,
        // ... 其他字段
    }))
}
```

- [ ] **Step 6: 声明模块 + 注册路由**

在 `src/handlers/project/projects/mod.rs` 中添加：
```rust
pub mod search_projects;
```

在 `src/router.rs` 中：
```rust
.route(
    "/projects/search",
    post(handlers::project::projects::search_projects_handler),
)
```

- [ ] **Step 7: 验证编译 + clippy + 测试**

Run: `cargo check -p ai_orz --lib && cargo clippy -p ai_orz --lib -- -D warnings && cargo test -p ai_orz --lib project`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add common/src/api/project.rs src/service/dao/project/ src/service/dal/project.rs \
        src/service/domain/project/ src/handlers/project/projects/ src/router.rs
git commit -m "refactor: unify Project search to three-scenario pattern, fix SQL filter bug"
```

---

## Task 6: Project 实体前端改造

**Files:**
- Modify: `frontend/src/api/project.rs`
- Modify: `frontend/src/pages/project/projects.rs`

- [ ] **Step 1: 新增 search_projects API**

在 `frontend/src/api/project.rs` 中：
```rust
pub async fn search_projects(req: &SearchProjectsRequest) -> Result<PagedResult<ProjectListItem>, ApiError> {
    api_post("/api/v1/projects/search", req).await
}
```

- [ ] **Step 2: 改造 projects 页面三场景切换**

在 `frontend/src/pages/project/projects.rs` 中：
```rust
let result = if keyword.trim().is_empty() {
    list_projects(ListProjectsRequest::default()).await.map(|p| p.items)
} else {
    search_projects(&SearchProjectsRequest {
        keyword: Some(keyword),
        ..Default::default()
    }).await.map(|p| p.items)
};
```

- [ ] **Step 3: 验证 + Commit**

Run: `cargo check -p frontend --target wasm32-unknown-unknown && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings`

```bash
git add frontend/src/api/project.rs frontend/src/pages/project/projects.rs
git commit -m "refactor: frontend projects page three-scenario switch"
```

---

## Task 7: Task 实体后端改造

**Files:**
- Modify: `common/src/api/task.rs`
- Modify: `src/service/dao/task/sqlite.rs`
- Modify: `src/service/dal/task.rs`
- Modify: `src/service/domain/project/mod.rs`
- Create: `src/handlers/project/task/search_tasks.rs`
- Modify: `src/handlers/project/task/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 新增 SearchTasksRequest/Response DTO**

在 `common/src/api/task.rs` 中：

```rust
/// 搜索 Task 请求（POST body）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchTasksRequest {
    pub keyword: Option<String>,
    pub ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    pub assignee_type: Option<TaskAssigneeType>,
    pub assignee_id: Option<String>,
    pub status_in: Option<Vec<TaskStatus>>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

pub type SearchTasksResponse = PagedResult<TaskListItem>;
```

- [ ] **Step 2: 改造 DAO search_tasks 添加 OFFSET + LIMIT 20**

在 `src/service/dao/task/sqlite.rs` 的 `search_tasks` 方法中（SQL 已复用过滤条件，仅需补 OFFSET + LIMIT 20）：

```rust
// 原：builder.push(" LIMIT ").push_bind(limit as i64);
// 改为：
let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
builder.push(" LIMIT ").push_bind(search_limit as i64);
if let Some(offset) = filters.pagination.offset {
    builder.push(" OFFSET ").push_bind(offset as i64);
}
```

- [ ] **Step 3: 改造 DAL search 返回 PagedResult + truncate(20)**

在 `src/service/dal/task.rs` 中：

trait 签名：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: TaskSearch,
) -> Result<common::api::PagedResult<Task>>;
```

实现 Step 8：
```rust
tasks.truncate(20);
let pagination = search.filters.pagination.clone();
let total = tasks.len();
let offset = pagination.offset.unwrap_or(0);
let limit = pagination.limit.unwrap_or(20);
let items = tasks.into_iter().skip(offset).take(limit).collect();
Ok(common::api::PagedResult { items, total })
```

向量搜索限制 50 → 20。

- [ ] **Step 4: Domain trait 新增 search 方法**

在 `src/service/domain/project/mod.rs` 的 `TaskManage` trait 中新增：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::task::TaskSearch,
) -> Result<common::api::PagedResult<Task>> {
    self.task_dal.search(ctx, search).await
}
```

- [ ] **Step 5: 新增 search_tasks handler**

创建 `src/handlers/project/task/search_tasks.rs`（参考 `src/handlers/hr/agent/search_agents.rs`）：

```rust
//! Handler: POST /api/v1/tasks/search - Search tasks with full filtering

use crate::pkg::RequestContext;
use crate::service::dao::task::{TaskQuery, TaskSearch};
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SearchTasksRequest, TaskListItem};
use common::error::Result;

#[register_handler_tool(
    id = "search_tasks",
    name = "search_tasks",
    description = "Search tasks by keyword with full filtering support.",
    params = "common::api::SearchTasksRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn search_tasks(
    ctx: RequestContext,
    params: SearchTasksRequest,
) -> Result<PagedResult<TaskListItem>> {
    let search = TaskSearch {
        keyword: params.keyword,
        filters: TaskQuery {
            ids: params.ids,
            project_id: params.project_id,
            assignee_type: params.assignee_type,
            assignee_id: params.assignee_id,
            status_in: params.status_in,
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().task_manage().search(ctx, search).await?;

    Ok(page.map(|task| TaskListItem {
        // 字段映射参考 list_tasks.rs / query_tasks.rs
        id: task.po.id,
        title: task.po.title,
        // ... 其他字段
    }))
}
```

- [ ] **Step 6: 声明模块 + 注册路由**

在 `src/handlers/project/task/mod.rs` 中添加：
```rust
pub mod search_tasks;
```

在 `src/router.rs` 中：
```rust
.route(
    "/tasks/search",
    post(handlers::project::task::search_tasks_handler),
)
```

- [ ] **Step 7: 验证编译 + clippy + 测试**

Run: `cargo check -p ai_orz --lib && cargo clippy -p ai_orz --lib -- -D warnings && cargo test -p ai_orz --lib task`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add common/src/api/task.rs src/service/dao/task/ src/service/dal/task.rs \
        src/service/domain/project/ src/handlers/project/task/ src/router.rs
git commit -m "refactor: unify Task search to three-scenario pattern"
```

---

## Task 8: Task 实体前端改造

**Files:**
- Modify: `frontend/src/api/project.rs`
- Modify: `frontend/src/pages/project/tasks.rs`

- [ ] **Step 1: 新增 search_tasks API**

在 `frontend/src/api/project.rs` 中：
```rust
pub async fn search_tasks(req: &SearchTasksRequest) -> Result<PagedResult<TaskListItem>, ApiError> {
    api_post("/api/v1/tasks/search", req).await
}
```

- [ ] **Step 2: 改造 tasks 页面三场景切换**

在 `frontend/src/pages/project/tasks.rs` 中：
```rust
let result = if keyword.trim().is_empty() {
    list_tasks(ListTasksRequest::default()).await.map(|p| p.items)
} else {
    search_tasks(&SearchTasksRequest {
        keyword: Some(keyword),
        ..Default::default()
    }).await.map(|p| p.items)
};
```

- [ ] **Step 3: 验证 + Commit**

Run: `cargo check -p frontend --target wasm32-unknown-unknown && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings`

```bash
git add frontend/src/api/project.rs frontend/src/pages/project/tasks.rs
git commit -m "refactor: frontend tasks page three-scenario switch"
```

---

## Task 9: 最终验证与推送

- [ ] **Step 1: 全量编译检查**

Run:
```bash
cargo check -p common && cargo check -p ai_orz --lib && cargo check -p frontend --target wasm32-unknown-unknown
```
Expected: 全部 PASS

- [ ] **Step 2: 全量 clippy**

Run:
```bash
cargo clippy -p ai_orz --lib -- -D warnings && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings
```
Expected: 0 warnings

- [ ] **Step 3: fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: PASS（如失败运行 `cargo fmt --all` 修复后重新提交）

- [ ] **Step 4: 全量测试**

Run:
```bash
cargo test -p ai_orz --lib tool_dal && cargo test -p ai_orz --lib skill_dal && cargo test -p ai_orz --lib project && cargo test -p ai_orz --lib task
```
Expected: 全部 PASS（除预先存在的无关失败）

- [ ] **Step 5: 更新 docs/todo.md**

将 docs/todo.md 中"其他实体搜索/查询接口统一为三场景规范"条目移到"已完成事项"章节，标注完成时间和提交 hash。

- [ ] **Step 6: 推送**

```bash
git push origin main
```

---

## Self-Review

### 1. Spec coverage
- ✅ Tool: Task 1+2 覆盖 DTO/DAO/DAL/Domain/Handler/Router/前端
- ✅ Skill: Task 3+4 覆盖 DTO/DAO/DAL/Domain/Handler/Router/前端
- ✅ Project: Task 5+6 覆盖 DTO/DAO/DAL/Domain/Handler/Router/前端（含 SQL 缺陷修复）
- ✅ Task: Task 7+8 覆盖 DTO/DAO/DAL/Domain/Handler/Router/前端
- ✅ 最终验证: Task 9

### 2. Placeholder scan
- 部分字段映射标注"参考 list_xxx.rs"——这是合理的，因为字段映射代码在每个 handler 中已有，无需重复完整代码
- 所有关键改造点都有完整代码

### 3. Type consistency
- 所有实体的 SearchRequest 都遵循 `keyword + 完整过滤字段 + pagination` 结构
- 所有 SearchResponse 都是 `PagedResult<XxxListItem>` 别名
- 所有 DAO Search 结构体都是 `filters: XxxQuery` 复用
- 所有 DAL search 方法都返回 `PagedResult<T>`
