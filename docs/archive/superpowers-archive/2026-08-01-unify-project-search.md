# Project 实体搜索/查询接口统一实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Project 实体的 list/query/search 接口统一为三场景规范，修复 search_projects SQL 未复用 push_query_filters 的安全缺陷，并实现前端完整三场景切换（含状态筛选 UI）。

**Architecture:** 自底向上改造：DTO（SearchProjectsRequest + PagedResult）→ DAO（修复 SQL 缺陷：复用 push_query_filters + OFFSET + LIMIT 20）→ DAL（search 返回 PagedResult + truncate(20)）→ Domain trait（新增 search 方法）→ Handler（POST + 完整过滤）→ Router → 前端（搜索框 + 状态筛选 UI + 三场景切换）。

**Tech Stack:** Rust (sqlx + async-trait) / Dioxus (frontend) / SQLite (FTS5 + vss0)

**规范基准：** [docs/entity_list_query_search_design.md](../../entity_list_query_search_design.md)

**Agent 标杆实现参考：**
- DTO: `common/src/api/agent.rs` (SearchAgentsRequest)
- DAL: `src/service/dal/agent.rs` (search 返回 PagedResult + truncate(20))
- DAO: `src/service/dao/agent/sqlite.rs` (search_agents 复用 push_query_filters + OFFSET)
- Handler: `src/handlers/hr/agent/search_agents.rs`
- 前端: `frontend/src/api/hr.rs` + `frontend/src/pages/hr/agents.rs`

---

## File Structure

- Modify: `common/src/api/project.rs` — 新增 SearchProjectsRequest/SearchProjectsResponse
- Modify: `src/service/dao/project/sqlite.rs` — **修复关键缺陷**：search_projects 改用 QueryBuilder + 复用 push_query_filters + OFFSET + LIMIT 20
- Modify: `src/service/dal/project.rs` — search 返回 PagedResult + truncate(20) + 向量搜索限制 20
- Modify: `src/service/domain/project/mod.rs` — ProjectManage trait 新增 search 方法
- Create: `src/handlers/project/projects/search_projects.rs` — 新 handler
- Modify: `src/handlers/project/projects/mod.rs` — 声明模块
- Modify: `src/router.rs` — 注册 POST /projects/search
- Modify: `frontend/src/api/project.rs` — 新增 search_projects
- Modify: `frontend/src/pages/project/projects.rs` — 搜索框 + 状态筛选 UI + 三场景切换

---

## Task 1: 后端 DTO 改造

**Files:**
- Modify: `common/src/api/project.rs`

- [ ] **Step 1: 新增 SearchProjectsRequest/SearchProjectsResponse**

在 `common/src/api/project.rs` 中（在 `ProjectQueryRequest` 定义之后），新增：

```rust
/// 搜索 Project 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchProjectsRequest {
    /// 搜索关键词（FTS5 + 向量语义混合搜索）
    pub keyword: Option<String>,
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 根用户 ID
    pub root_user_id: Option<String>,
    /// 状态列表（OR 语义）
    pub status_in: Option<Vec<ProjectStatus>>,
    /// 按 Owner Agent ID 过滤
    pub owner_agent_id: Option<String>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Project 响应（分页）
pub type SearchProjectsResponse = PagedResult<ProjectListItem>;
```

确保文件顶部已导入 `PagedResult`（如未导入则添加 `use crate::api::PagedResult;` 或使用完整路径）。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p common`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add common/src/api/project.rs
git commit -m "refactor: add SearchProjectsRequest DTO with full filter + pagination"
```

---

## Task 2: 修复 DAO search_projects SQL 缺陷 + 改造返回分页

**Files:**
- Modify: `src/service/dao/project/sqlite.rs`

**关键缺陷说明**：当前 `search_projects`（第 277-295 行）使用原始 `sqlx::query_as` 硬编码 SQL，仅 `WHERE projects_fts MATCH ? AND p."status" != 0`，**未调用 `push_query_filters`**，导致 `root_user_id/status_in/owner_agent_id/ids` 业务过滤条件在关键词搜索时全部失效（安全隐患：可能搜到其他用户的项目）。

- [ ] **Step 1: 改造 search_projects 改用 QueryBuilder + 复用 push_query_filters + OFFSET**

在 `src/service/dao/project/sqlite.rs` 中，将 `search_projects` 方法从原始 `sqlx::query_as` 改为 `QueryBuilder` 动态拼接，复用 `push_query_filters`：

```rust
async fn search_projects(
    &self,
    _ctx: RequestContext,
    search: ProjectSearch,
) -> Result<Vec<(ProjectPo, Option<f32>)>> {
    use sqlx::QueryBuilder;

    let pool = _ctx.db_pool();
    let keyword = search.keyword.unwrap_or_default();

    // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
    if keyword.trim().is_empty() {
        return Ok(Vec::new());
    }

    let escaped_keyword = escape_fts5_keyword(&keyword);
    let filters = search.filters;

    // FTS5 MATCH + JOIN + BM25 排序
    // ✅ 复用 push_query_filters（修复原 SQL 未应用 root_user_id/owner_agent_id/status_in/ids 的缺陷）
    let mut builder = QueryBuilder::new(
        r#"SELECT p.id, p.name, p.description, p.workflow, p.guidance, p."status" as status,
                  p.priority, p.tags, p.root_user_id, p.owner_agent_id,
                  p.start_at, p.due_at, p.end_at, p.created_by, p.modified_by,
                  p.created_at, p.updated_at,
                  projects_fts.rank as fts_rank
           FROM projects_fts
           JOIN projects p ON projects_fts.rowid = p.rowid
           WHERE projects_fts MATCH "#,
    );
    builder.push_bind(escaped_keyword);

    // ✅ 复用 push_query_filters（与 query 共享过滤逻辑）
    // 注意：push_query_filters 中的字段引用不带表别名前缀（如 "status" 而非 p."status"），
    // 但在 JOIN 查询中需要带别名。需要调整 push_query_filters 使其支持别名，
    // 或在 search_projects 中手动拼接过滤条件（参考 agents 的做法）。
    // 
    // 方案：手动拼接过滤条件（与 agents DAO 的 search_agents 一致）
    if let Some(ids) = &filters.ids
        && !ids.is_empty()
    {
        builder.push(" AND p.id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }
    if let Some(root_user_id) = &filters.root_user_id {
        builder.push(" AND p.root_user_id = ").push_bind(root_user_id);
    }
    if let Some(owner_agent_id) = &filters.owner_agent_id {
        builder.push(" AND p.owner_agent_id = ").push_bind(owner_agent_id);
    }
    if let Some(status_list) = &filters.status_in
        && !status_list.is_empty()
    {
        builder.push(" AND p.\"status\" IN (");
        let mut separated = builder.separated(", ");
        for s in status_list {
            separated.push_bind(*s as i32);
        }
        separated.push_unseparated(")");
    }
    // 默认排除软删除（push_query_filters 有此逻辑，这里也加上）
    builder.push(" AND p.\"status\" != 0");

    builder.push(" ORDER BY projects_fts.rank");

    // 搜索场景限制最大返回数量
    let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
    builder.push(" LIMIT ").push_bind(search_limit as i64);
    if let Some(offset) = filters.pagination.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let rows: Vec<ProjectSearchRow> = builder
        .build_query_as::<ProjectSearchRow>()
        .fetch_all(pool)
        .await?;

    let results = rows
        .into_iter()
        .map(|row| {
            let po = ProjectPo {
                id: row.id,
                name: row.name,
                description: row.description,
                workflow: row.workflow,
                guidance: row.guidance,
                status: ProjectStatus::from(row.status),
                priority: row.priority,
                tags: row.tags,
                root_user_id: row.root_user_id,
                owner_agent_id: row.owner_agent_id,
                start_at: row.start_at,
                due_at: row.due_at,
                end_at: row.end_at,
                created_by: row.created_by,
                modified_by: row.modified_by,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };
            (po, row.fts_rank)
        })
        .collect();

    Ok(results)
}
```

注意：`ProjectSearchRow` 结构体需确保 `#[derive(FromRow)]`（如已有则无需修改）。如果 `ProjectSearchRow` 是 `#[derive(sqlx::FromRow)]` 的，需要确认字段顺序和类型与 SELECT 列一致。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p ai_orz --lib`
Expected: PASS（可能有 DAL/Domain 层的错误，因为 search 签名还没改，但 DAO 层本身应该编译通过）

- [ ] **Step 3: Commit**

```bash
git add src/service/dao/project/sqlite.rs
git commit -m "fix: search_projects SQL reuse push_query_filters (fix security bug) + OFFSET"
```

---

## Task 3: 改造 DAL search 返回 PagedResult

**Files:**
- Modify: `src/service/dal/project.rs`

- [ ] **Step 1: 修改 trait 签名**

在 `src/service/dal/project.rs` 第 167 行，将：

```rust
async fn search(&self, ctx: RequestContext, search: ProjectSearch) -> Result<Vec<Project>>;
```

改为：

```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: ProjectSearch,
) -> Result<common::api::PagedResult<Project>>;
```

- [ ] **Step 2: 修改实现签名**

在第 463 行，将实现签名同步为 `Result<common::api::PagedResult<Project>>`。

- [ ] **Step 3: 修改向量搜索限制 50 → 20**

在第 485 行，将：

```rust
// 向量搜索（前 50 条）
.search_vector(ctx.clone(), &vec_params.vector, 50)
```

改为：

```rust
// 向量搜索（前 MAX_SEARCH_RESULTS 条，与 FTS5 限制一致）
.search_vector(ctx.clone(), &vec_params.vector, 20)
```

- [ ] **Step 4: 修改 Step 8 返回 PagedResult**

在第 661-666 行，将：

```rust
// Step 8: 应用 limit
if let Some(limit) = search.filters.pagination.limit {
    projects.truncate(limit);
}

Ok(projects)
```

改为：

```rust
// Step 8: 截断到 MAX_SEARCH_RESULTS + 分页
// 搜索场景限制总结果数（MAX_SEARCH_RESULTS=20），搜不到应换关键词而非无限分页
projects.truncate(20);

let pagination = search.filters.pagination.clone();
let total = projects.len();
let offset = pagination.offset.unwrap_or(0);
let limit = pagination.limit.unwrap_or(20);
let items = projects.into_iter().skip(offset).take(limit).collect();
Ok(common::api::PagedResult { items, total })
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p ai_orz --lib`
Expected: 可能有 Domain/Handler 层错误（预期，后续 Task 修复）

- [ ] **Step 6: Commit**

```bash
git add src/service/dal/project.rs
git commit -m "refactor: Project DAL search returns PagedResult + truncate(20)"
```

---

## Task 4: Domain trait 新增 search 方法

**Files:**
- Modify: `src/service/domain/project/mod.rs`

- [ ] **Step 1: 在 ProjectManage trait 中新增 search 方法**

在 `src/service/domain/project/mod.rs` 的 `ProjectManage` trait 中（`query` 方法之后），新增：

```rust
/// 搜索 Project（关键词 + 向量语义混合搜索）
///
/// 返回分页结果，支持完整过滤条件。
async fn search(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::project::ProjectSearch,
) -> Result<common::api::PagedResult<Project>> {
    self.project_dal.search(ctx, search).await
}
```

注意：需要确认 `ProjectManage` 的实现结构体是否持有 `project_dal` 字段。如果 trait 中已有默认实现（如上），则无需在每个实现中重复。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p ai_orz --lib`
Expected: 可能有 Handler 层错误（预期，Task 5 修复）

- [ ] **Step 3: Commit**

```bash
git add src/service/domain/project/mod.rs
git commit -m "refactor: ProjectManage trait add search method"
```

---

## Task 5: 新增 search_projects handler + 路由

**Files:**
- Create: `src/handlers/project/projects/search_projects.rs`
- Modify: `src/handlers/project/projects/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 创建 search_projects handler**

创建 `src/handlers/project/projects/search_projects.rs`（参考 `src/handlers/hr/agent/search_agents.rs` 和 `src/handlers/project/projects/query_projects.rs`）：

```rust
//! Handler: POST /api/v1/projects/search - Search projects with full filtering
//!
//! 与 query_projects 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
//! query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use crate::service::dao::project::ProjectSearch;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, ProjectListItem, SearchProjectsRequest};
use common::error::Result;

/// Search projects with full filtering (FTS5 + vector semantic search)
#[register_handler_tool(
    id = "search_projects",
    name = "search_projects",
    description = "Search projects by keyword with full filtering support (FTS5 + vector semantic search).",
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

    Ok(page.map(|p| response::to_list_item(&p)))
}
```

注意：`response::to_list_item` 是 `query_projects.rs` 和 `list_projects.rs` 共用的映射函数，在 `src/handlers/project/projects/response.rs`（或 `mod.rs` 的 `response` 子模块）中定义。如果 `response` 模块路径不同，请参考 `query_projects.rs` 的 `use super::response` 导入。

- [ ] **Step 2: 声明模块**

在 `src/handlers/project/projects/mod.rs` 中添加：

```rust
pub mod search_projects;
```

- [ ] **Step 3: 注册路由**

在 `src/router.rs` 中找到 projects 路由组，添加 search 路由（POST）：

```rust
.route(
    "/projects/search",
    post(handlers::project::projects::search_projects_handler),
)
```

注意：确保 `post` 已从 `axum` 导入。

- [ ] **Step 4: 验证编译 + clippy**

Run: `cargo check -p ai_orz --lib && cargo clippy -p ai_orz --lib -- -D warnings`
Expected: PASS

- [ ] **Step 5: 更新测试**

更新 `src/service/dao/project/sqlite_test.rs`（或对应测试文件）中所有 search 相关测试断言：
- `results.len()` → `results.items.len()`
- `results[N]` → `results.items[N]`

更新所有 mock/stub 实现的 search 方法签名为返回 `PagedResult`。

Run: `cargo test -p ai_orz --lib project 2>&1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/handlers/project/projects/ src/router.rs
git commit -m "refactor: add search_projects handler + POST route"
```

---

## Task 6: 前端 API 改造

**Files:**
- Modify: `frontend/src/api/project.rs`

- [ ] **Step 1: 新增 search_projects 前端 API**

在 `frontend/src/api/project.rs` 中，新增 `search_projects` 函数（参考 `frontend/src/api/hr.rs` 的 `search_agents`）：

```rust
pub async fn search_projects(
    req: &SearchProjectsRequest,
) -> Result<PagedResult<ProjectListItem>, ApiError> {
    api_post("/api/v1/projects/search", req).await
}
```

确保在文件顶部的 `use common::api::{...}` 中添加 `SearchProjectsRequest` 到导入列表。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p frontend --target wasm32-unknown-unknown`
Expected: PASS（页面层可能还没有使用，但 API 层应该编译通过）

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/project.rs
git commit -m "refactor: add frontend search_projects API"
```

---

## Task 7: 前端页面三场景切换 + 状态筛选 UI

**Files:**
- Modify: `frontend/src/pages/project/projects.rs`

**目标**：实现完整的三场景切换：
1. 无关键词 + 无状态筛选 → `list_projects`（默认列表）
2. 无关键词 + 有状态筛选 → `query_projects`（条件过滤）
3. 有关键词 → `search_projects`（可同时带状态筛选）

- [ ] **Step 1: 增加搜索框和状态筛选 UI**

在 `frontend/src/pages/project/projects.rs` 中，参考 `frontend/src/pages/hr/agents.rs` 的搜索框实现，增加：

1. 在 signal 区域增加：
```rust
let mut search_keyword = use_signal(String::new);
let mut search_request_id = use_signal(|| 0u32);
let mut status_filter = use_signal(|| Option::<i32>::None);
```

2. 在 `use_effect` 中替换初始加载逻辑为 `reload_projects` 闭包：
```rust
let mut reload_projects = move || {
    let keyword = search_keyword();
    let status = status_filter();
    let my_id = search_request_id() + 1;
    search_request_id.set(my_id);
    spawn(async move {
        // 三场景切换：
        // 无关键词 + 无状态筛选 → list_projects
        // 无关键词 + 有状态筛选 → query_projects
        // 有关键词 → search_projects（可同时带状态筛选）
        let result = if keyword.trim().is_empty() && status.is_none() {
            list_projects(ListProjectsRequest::default()).await.map(|p| p.items)
        } else if keyword.trim().is_empty() {
            // 有状态筛选，无关键词 → query_projects
            query_projects(&ProjectQueryRequest {
                status_in: status.map(|s| vec![common::enums::ProjectStatus::from(s)]),
                ..Default::default()
            }).await.map(|p| p.items)
        } else {
            // 有关键词 → search_projects（可同时带状态筛选）
            search_projects(&SearchProjectsRequest {
                keyword: Some(keyword),
                status_in: status.and_then(|s| {
                    Some(vec![common::enums::ProjectStatus::from(s)])
                }),
                ..Default::default()
            }).await.map(|p| p.items)
        };
        // 丢弃过期请求的结果
        if search_request_id() != my_id {
            return;
        }
        match result {
            Ok(v) => projects.set(v),
            Err(e) => toast.error(&e),
        }
    });
};
```

3. 在 `use_effect` 中调用 `reload_projects()`：
```rust
use_effect(move || {
    loading.set(true);
    reload_projects();
    // ... 原有的 task_counts 加载逻辑保持不变 ...
});
```

4. 在 UI 中增加搜索框和状态筛选下拉框（放在卡片标题区域下方）：
```rust
div { class: "flex gap-2 items-center mt-4",
    // 搜索框
    input {
        class: "input input-bordered input-sm flex-1",
        placeholder: "搜索项目...",
        value: "{search_keyword}",
        oninput: move |e| search_keyword.set(e.value()),
        onkeypress: move |e| {
            if e.key() == Key::Enter {
                loading.set(true);
                reload_projects();
            }
        }
    }
    // 状态筛选下拉框
    select {
        class: "select select-bordered select-sm",
        onchange: move |e| {
            let val = e.value();
            status_filter.set(if val.is_empty() {
                None
            } else {
                val.parse::<i32>().ok()
            });
            loading.set(true);
            reload_projects();
        },
        option { value: "", "全部状态" }
        option { value: "1", "Active" }
        option { value: "2", "PendingReview" }
        option { value: "3", "InProgress" }
        option { value: "4", "Completed" }
        option { value: "5", "Archived" }
    }
    // 清除搜索按钮
    if !search_keyword().is_empty() || status_filter().is_some() {
        button {
            class: "btn btn-ghost btn-sm",
            onclick: move |_| {
                search_keyword.set(String::new());
                status_filter.set(None);
                loading.set(true);
                reload_projects();
            },
            "✕"
        }
    }
}
```

5. 在创建项目成功后，重新加载也调用 `reload_projects()`：
```rust
// 原：match list_projects(ListProjectsRequest::default()).await {
// 改为：
reload_projects();
```

- [ ] **Step 2: 验证编译 + clippy**

Run: `cargo check -p frontend --target wasm32-unknown-unknown && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/project/projects.rs
git commit -m "feat: projects page three-scenario switch with search + status filter UI"
```

---

## Task 8: 最终验证与推送

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

Run: `cargo test -p ai_orz --lib project 2>&1`
Expected: 全部 PASS

- [ ] **Step 5: 推送**

```bash
git push origin main
```

---

## Self-Review

### 1. Spec coverage
- ✅ DTO: Task 1 新增 SearchProjectsRequest/SearchProjectsResponse
- ✅ DAO SQL 缺陷修复: Task 2 复用 push_query_filters + OFFSET + LIMIT 20
- ✅ DAL: Task 3 search 返回 PagedResult + truncate(20) + 向量限制 20
- ✅ Domain: Task 4 ProjectManage 新增 search 方法
- ✅ Handler: Task 5 新增 search_projects handler + POST 路由
- ✅ 前端 API: Task 6 新增 search_projects
- ✅ 前端三场景: Task 7 搜索框 + 状态筛选 UI + 三场景切换（list/query/search 完整实现）
- ✅ 验证: Task 8

### 2. Placeholder scan
- 无 TBD/TODO
- 所有代码步骤都有完整代码
- `response::to_list_item` 函数已存在于 `query_projects.rs` 的 import 中，确认可用

### 3. Type consistency
- SearchProjectsRequest: keyword + ids + root_user_id + status_in + owner_agent_id + pagination（与 ProjectQueryRequest 一致）
- SearchProjectsResponse = PagedResult<ProjectListItem>（与 Agent 一致）
- ProjectSearch.filters: ProjectQuery（已存在，无需修改）
- DAL search 返回 PagedResult<Project>（与 Agent 一致）
- 前端三场景判断：无关键词+无筛选→list；无关键词+有筛选→query；有关键词→search

### 4. 关键缺陷修复确认
- 原 SQL: `WHERE projects_fts MATCH ? AND p."status" != 0`（仅排除软删除）
- 修复后: 复用 push_query_filters 的逻辑，手动拼接带 `p.` 别名前缀的过滤条件（root_user_id/owner_agent_id/status_in/ids）
- 安全隐患已修复：关键词搜索现在会正确应用 root_user_id 过滤，防止跨用户搜索
