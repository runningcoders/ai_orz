# Task 实体搜索/查询接口统一实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Task 实体的 list/query/search 接口统一为三场景规范，修复 search_tasks SQL 未复用 push_query_filters 的缺陷，并实现前端完整三场景切换（已有筛选 UI + 新增搜索框）。

**Architecture:** 自底向上改造：DTO（SearchTasksRequest + PagedResult）→ DAO（修复 SQL：复用 push_query_filters + OFFSET + LIMIT 20）→ DAL（search 返回 PagedResult + truncate(20)）→ Domain trait（新增 search 方法）→ Handler（POST + 完整过滤）→ Router → 前端（搜索框 + 三场景切换）。

**Tech Stack:** Rust (sqlx + async-trait) / Dioxus (frontend) / SQLite (FTS5 + vss0)

**规范基准：** [docs/entity_list_query_search_design.md](../../entity_list_query_search_design.md)

**Project 标杆实现参考（已完成）：**
- DTO: `common/src/api/project.rs` (SearchProjectsRequest)
- DAO: `src/service/dao/project/sqlite.rs` (search_projects 复用业务过滤 + OFFSET)
- DAL: `src/service/dal/project.rs` (search 返回 PagedResult + truncate(20))
- Handler: `src/handlers/project/projects/search_projects.rs`
- 前端: `frontend/src/pages/project/projects.rs` (三场景切换 + 搜索框 + 状态筛选)

**Task 前端差异**：Task 页面**已有完整筛选 UI**（项目/状态/负责人下拉），使用 `query_tasks` 加载。改造时保留现有筛选 UI，新增搜索框，并将加载逻辑改为三场景切换（无关键词+无筛选→list；无关键词+有筛选→query；有关键词→search）。

---

## File Structure

- Modify: `common/src/api/task.rs` — 新增 SearchTasksRequest/SearchTasksResponse
- Modify: `src/service/dao/task/sqlite.rs` — search_tasks 改用 QueryBuilder + 复用业务过滤 + OFFSET + LIMIT 20
- Modify: `src/service/dal/task.rs` — search 返回 PagedResult + truncate(20) + 向量搜索限制 20
- Modify: `src/service/domain/project/mod.rs` — TaskManage trait 新增 search 方法
- Create: `src/handlers/project/task/search_tasks.rs` — 新 handler
- Modify: `src/handlers/project/task/mod.rs` — 声明模块
- Modify: `src/router.rs` — 注册 POST /tasks/search
- Modify: `frontend/src/api/project.rs` — 新增 search_tasks
- Modify: `frontend/src/pages/project/tasks.rs` — 搜索框 + 三场景切换

---

## Task 1: 后端 DTO 改造

**Files:**
- Modify: `common/src/api/task.rs`

- [ ] **Step 1: 新增 SearchTasksRequest/SearchTasksResponse**

在 `TaskQueryRequest` 定义之后（约第 266 行），新增：

```rust
/// 搜索 Task 请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchTasksRequest {
    /// 搜索关键词（FTS5 + 向量语义混合搜索）
    pub keyword: Option<String>,
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 项目 ID
    pub project_id: Option<String>,
    /// 负责人类型
    pub assignee_type: Option<AssigneeType>,
    /// 负责人 ID
    pub assignee_id: Option<String>,
    /// 状态列表（OR 语义）
    pub status_in: Option<Vec<TaskStatus>>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// 搜索 Task 响应（分页）
pub type SearchTasksResponse = PagedResult<TaskListItem>;
```

确保 `PagedResult` 已导入（参考 `common/src/api/project.rs` 的导入方式）。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p common`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add common/src/api/task.rs
git commit -m "refactor: add SearchTasksRequest DTO with full filter + pagination"
```

---

## Task 2: 修复 DAO search_tasks SQL + OFFSET + LIMIT 20

**Files:**
- Modify: `src/service/dao/task/sqlite.rs`

**关键缺陷**：当前 `search_tasks`（第 171-262 行）手工拼接过滤条件，未复用 `push_query_filters`，且无 OFFSET，默认 LIMIT 50。

- [ ] **Step 1: 改造 search_tasks 改用 QueryBuilder + OFFSET + LIMIT 20**

参考 Project 的 `search_projects` 改造模式（`src/service/dao/project/sqlite.rs` 第 257-366 行），将 `search_tasks` 方法改造为：

```rust
async fn search_tasks(
    &self,
    _ctx: RequestContext,
    search: TaskSearch,
) -> Result<Vec<(TaskPo, Option<f32>)>> {
    use sqlx::QueryBuilder;

    let pool = _ctx.db_pool();
    let keyword = search.keyword.unwrap_or_default();

    if keyword.trim().is_empty() {
        return Ok(Vec::new());
    }

    let escaped_keyword = escape_fts5_keyword(&keyword);
    let filters = search.filters;

    let mut builder = QueryBuilder::new(
        r#"SELECT t.id, t.title, t.description, t.project_id, t."status" as status,
                  t.priority, t.tags, t.assignee_type, t.assignee_id, t.root_user_id,
                  t.thinking_depth, t.progress, t.parent_task_id, t.created_by, t.modified_by,
                  t.created_at, t.updated_at,
                  tasks_fts.rank as fts_rank
           FROM tasks_fts
           JOIN tasks t ON tasks_fts.rowid = t.rowid
           WHERE tasks_fts MATCH "#,
    );
    builder.push_bind(escaped_keyword);

    // 手动拼接业务过滤条件（带 t. 别名前缀，因为 JOIN 查询需要表别名）
    if let Some(ids) = &filters.ids
        && !ids.is_empty()
    {
        builder.push(" AND t.id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }
    if let Some(project_id) = &filters.project_id {
        builder.push(" AND t.project_id = ").push_bind(project_id);
    }
    if let Some(assignee_type) = &filters.assignee_type {
        builder.push(" AND t.assignee_type = ").push_bind(*assignee_type as i32);
    }
    if let Some(assignee_id) = &filters.assignee_id {
        builder.push(" AND t.assignee_id = ").push_bind(assignee_id);
    }
    if let Some(status_list) = &filters.status_in
        && !status_list.is_empty()
    {
        builder.push(" AND t.\"status\" IN (");
        let mut separated = builder.separated(", ");
        for s in status_list {
            separated.push_bind(*s as i32);
        }
        separated.push_unseparated(")");
    }
    // 默认排除软删除
    builder.push(" AND t.\"status\" != 0");

    builder.push(" ORDER BY tasks_fts.rank");

    let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
    builder.push(" LIMIT ").push_bind(search_limit as i64);
    if let Some(offset) = filters.pagination.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }

    // ... 保留原有的 row → po 映射逻辑 ...
}
```

注意：需先读取当前 `search_tasks` 方法和 `TaskSearchRow` 结构体，确认字段顺序和类型。SELECT 列必须与 `TaskSearchRow` 的 `#[derive(FromRow)]` 字段一致。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p ai_orz --lib`
Expected: PASS（或仅有 DAL/Domain 层预期错误）

- [ ] **Step 3: Commit**

```bash
git add src/service/dao/task/sqlite.rs
git commit -m "fix: search_tasks SQL reuse business filters + OFFSET + LIMIT 20"
```

---

## Task 3: 改造 DAL search 返回 PagedResult

**Files:**
- Modify: `src/service/dal/task.rs`

- [ ] **Step 1: 修改 trait 签名（第 149 行）**

原：`async fn search(&self, ctx: RequestContext, search: TaskSearch) -> Result<Vec<Task>>;`
改为：
```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: TaskSearch,
) -> Result<common::api::PagedResult<Task>>;
```

- [ ] **Step 2: 修改实现签名（第 362 行）**

同步为 `Result<common::api::PagedResult<Task>>`。

- [ ] **Step 3: 向量搜索限制 50 → 20（第 386 行）**

原：`.search_vector(ctx.clone(), &vec_params.vector, 50)`
改为：`.search_vector(ctx.clone(), &vec_params.vector, 20)`

- [ ] **Step 4: 修改 Step 8 返回 PagedResult（第 555-558 行）**

原：
```rust
if let Some(limit) = search.filters.pagination.limit {
    tasks.truncate(limit);
}
Ok(tasks)
```
改为：
```rust
tasks.truncate(20);
let pagination = search.filters.pagination.clone();
let total = tasks.len();
let offset = pagination.offset.unwrap_or(0);
let limit = pagination.limit.unwrap_or(20);
let items = tasks.into_iter().skip(offset).take(limit).collect();
Ok(common::api::PagedResult { items, total })
```

- [ ] **Step 5: 验证编译**

Run: `cargo check -p ai_orz --lib`
Expected: 可能有 Domain/Handler 层预期错误

- [ ] **Step 6: Commit**

```bash
git add src/service/dal/task.rs
git commit -m "refactor: Task DAL search returns PagedResult + truncate(20)"
```

---

## Task 4: Domain trait 新增 search 方法

**Files:**
- Modify: `src/service/domain/project/mod.rs`（trait 声明）
- Modify: `src/service/domain/project/service.rs`（impl 实现，参考 Project 的模式）

- [ ] **Step 1: 在 TaskManage trait 中新增 search 方法声明**

在 `src/service/domain/project/mod.rs` 的 `TaskManage` trait 中（`query` 方法之后，约第 294 行），新增：

```rust
/// 搜索 Task（关键词 + 向量语义混合搜索）
///
/// 返回分页结果，支持完整过滤条件。
async fn search(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::task::TaskSearch,
) -> Result<common::api::PagedResult<Task>>;
```

- [ ] **Step 2: 在 impl 块中新增 search 实现**

在 `src/service/domain/project/service.rs` 的 `impl TaskManage for ...` 块中（`query` 实现之后），新增：

```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::task::TaskSearch,
) -> Result<common::api::PagedResult<Task>> {
    self.task_dal.search(ctx, search).await
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p ai_orz --lib`
Expected: PASS（或仅有 Handler 层预期错误）

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/project/mod.rs src/service/domain/project/service.rs
git commit -m "refactor: TaskManage trait add search method"
```

---

## Task 5: 新增 search_tasks handler + 路由

**Files:**
- Create: `src/handlers/project/task/search_tasks.rs`
- Modify: `src/handlers/project/task/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 创建 search_tasks handler**

参考 `src/handlers/project/projects/search_projects.rs`（Project 标杆）和 `src/handlers/project/task/query_tasks.rs`（Task 的 response 映射）：

```rust
//! Handler: POST /api/v1/tasks/search - Search tasks with full filtering

use super::response;
use crate::pkg::RequestContext;
use crate::service::dao::task::{TaskQuery, TaskSearch};
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, SearchTasksRequest, TaskListItem};
use common::error::Result;

#[register_handler_tool(
    id = "search_tasks",
    name = "search_tasks",
    description = "Search tasks by keyword with full filtering support (FTS5 + vector semantic search).",
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

    Ok(page.map(|t| response::to_list_item(&t)))
}
```

- [ ] **Step 2: 声明模块 + 重导出 handler**

在 `src/handlers/project/task/mod.rs` 中添加：
```rust
pub mod search_tasks;
pub use search_projects::search_tasks_handler;
```

- [ ] **Step 3: 注册路由**

在 `src/router.rs` 的 `task_routes()` 函数中（`/tasks/query` 之后），添加：
```rust
.route(
    "/tasks/search",
    post(handlers::project::task::search_tasks_handler),
)
```

- [ ] **Step 4: 验证编译 + clippy**

Run: `cargo check -p ai_orz --lib && cargo clippy -p ai_orz --lib -- -D warnings`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/handlers/project/task/search_tasks.rs src/handlers/project/task/mod.rs src/router.rs
git commit -m "refactor: add search_tasks handler + POST route"
```

---

## Task 6: 前端 API 改造

**Files:**
- Modify: `frontend/src/api/project.rs`

- [ ] **Step 1: 新增 search_tasks 前端 API**

在 `frontend/src/api/project.rs` 中，新增 `search_tasks` 函数（参考 `search_projects`）：

```rust
pub async fn search_tasks(
    req: &SearchTasksRequest,
) -> Result<PagedResult<TaskListItem>, ApiError> {
    api_post("/api/v1/tasks/search", req).await
}
```

确保在 `use common::api::{...}` 中添加 `SearchTasksRequest`。

- [ ] **Step 2: 验证编译**

Run: `cargo check -p frontend --target wasm32-unknown-unknown`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/project.rs
git commit -m "refactor: add frontend search_tasks API"
```

---

## Task 7: 前端页面三场景切换 + 搜索框

**Files:**
- Modify: `frontend/src/pages/project/tasks.rs`

**Task 前端差异**：Task 页面**已有完整筛选 UI**（项目/状态/负责人下拉），使用 `query_tasks` 加载。改造时：
1. 保留现有筛选 UI
2. 新增搜索框
3. 将 `load_data` 改为三场景切换：
   - 无关键词 + 无筛选（filter_status=-1 && filter_project_id="" && filter_assignee_type=-1）→ `list_tasks`
   - 无关键词 + 有筛选 → `query_tasks`（现有逻辑）
   - 有关键词 → `search_tasks`（带现有筛选 + keyword）

- [ ] **Step 1: 增加搜索框 signal + 三场景切换逻辑**

在 `frontend/src/pages/project/tasks.rs` 中：

1. 新增 signal：
```rust
let mut search_keyword = use_signal(String::new);
let mut search_request_id = use_signal(|| 0u32);
```

2. 改造 `load_data` 为三场景切换（参考 Project 的 `reload_projects` 模式）：
```rust
let load_data = move || {
    let keyword = search_keyword();
    let project_id = filter_project_id();
    let status = filter_status();
    let assignee_type = filter_assignee_type();
    let my_id = search_request_id() + 1;
    search_request_id.set(my_id);
    spawn(async move {
        let has_filter = !project_id.is_empty() || status >= 0 || assignee_type >= 0;
        // 三场景切换：
        // 无关键词 + 无筛选 → list_tasks
        // 无关键词 + 有筛选 → query_tasks
        // 有关键词 → search_tasks（可同时带筛选）
        let result = if keyword.trim().is_empty() && !has_filter {
            list_tasks(ListTasksRequest::default()).await.map(|p| p.items)
        } else if keyword.trim().is_empty() {
            query_tasks(&TaskQueryRequest {
                project_id: if project_id.is_empty() { None } else { Some(project_id) },
                status_in: if status >= 0 { Some(vec![TaskStatus::from(status)]) } else { None },
                assignee_type: if assignee_type >= 0 { Some(AssigneeType::from(assignee_type)) } else { None },
                ..Default::default()
            }).await.map(|p| p.items)
        } else {
            search_tasks(&SearchTasksRequest {
                keyword: Some(keyword),
                project_id: if project_id.is_empty() { None } else { Some(project_id) },
                status_in: if status >= 0 { Some(vec![TaskStatus::from(status)]) } else { None },
                assignee_type: if assignee_type >= 0 { Some(AssigneeType::from(assignee_type)) } else { None },
                ..Default::default()
            }).await.map(|p| p.items)
        };
        if search_request_id() != my_id {
            return;
        }
        match result {
            Ok(v) => tasks.set(v),
            Err(e) => toast.error(&e),
        }
        loading.set(false);
    });
};
```

3. 在 UI 中增加搜索框（放在筛选下拉区域）：
```rust
input {
    class: "input input-bordered input-sm flex-1",
    placeholder: "搜索任务...",
    value: "{search_keyword}",
    oninput: move |e| search_keyword.set(e.value()),
    onkeypress: move |e| {
        if e.key() == Key::Enter {
            loading.set(true);
            load_data();
        }
    }
}
```

4. 调整导入：添加 `list_tasks`、`search_tasks`、`ListTasksRequest`、`SearchTasksRequest`。

**重要**：参考 Project 改造的经验，信号读取应在 `spawn` 内部进行，避免 `use_effect` 订阅死循环。

- [ ] **Step 2: 验证编译 + clippy**

Run: `cargo check -p frontend --target wasm32-unknown-unknown && cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/project/tasks.rs
git commit -m "feat: tasks page three-scenario switch with search box"
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
Expected: PASS

- [ ] **Step 4: 测试**

Run: `cargo test -p ai_orz --lib task`
Expected: 全部 PASS（如有 search 断言错误，更新 `results.len()` → `results.items.len()` 等）

- [ ] **Step 5: 推送**

```bash
git push origin main
```

---

## Self-Review

### 1. Spec coverage
- ✅ DTO: Task 1
- ✅ DAO SQL 修复: Task 2（复用业务过滤 + OFFSET + LIMIT 20）
- ✅ DAL: Task 3（PagedResult + truncate(20) + 向量限制 20）
- ✅ Domain: Task 4（TaskManage 新增 search）
- ✅ Handler: Task 5（search_tasks handler + POST 路由）
- ✅ 前端 API: Task 6
- ✅ 前端三场景: Task 7（搜索框 + 三场景切换，保留现有筛选 UI）
- ✅ 验证: Task 8

### 2. Type consistency
- SearchTasksRequest: keyword + ids + project_id + assignee_type + assignee_id + status_in + pagination（与 TaskQueryRequest 一致）
- SearchTasksResponse = PagedResult<TaskListItem>
- TaskSearch.filters: TaskQuery（已存在）
- DAL search 返回 PagedResult<Task>

### 3. 关键差异
- Task 前端已有筛选 UI（Project 改造时需新增）→ Task 7 保留现有筛选，仅增加搜索框
- Task 前端原本用 query_tasks 加载（Project 原本用 list_projects）→ Task 7 需改为三场景切换
