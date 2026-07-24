# Query 接口分页改造 + list 接口简化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 5 个实体（Agent/Project/Task/Tool/Skill）的 query 接口补齐 offset 分页 + total 返回，同时简化 list 接口为纯分页语法糖（只接受分页参数，不接受查询功能），统一返回 `PagedResult<T>`。

**Architecture:** 复用 `common::api::PaginationParams`（limit + offset）和 `common::api::PagedResult<T>`（items + total）。
- **query 是核心**：接受完整查询条件 + pagination（POST body），返回 `PagedResult<T>`
- **list 是语法糖**：只接受 pagination（GET query param），内部固定默认过滤和默认排序，返回 `PagedResult<T>`
- DAO 层抽取 `push_query_filters` 函数复用 WHERE 条件，COUNT + LIMIT/OFFSET 双查询
- Domain 层用 `PagedResult::map` 转换 Po→业务实体

**Tech Stack:** Rust, sqlx::QueryBuilder, SQLite, axum, utoipa, Dioxus (前端)

---

## 设计原则

1. **query 是核心，list 是语法糖**：
   - query 接口：完整查询条件 + pagination（POST body）→ `PagedResult<T>`
   - list 接口：只接受 pagination（GET query param）→ `PagedResult<T>`
2. **list 的"语法糖"含义**：
   - 只接受分页参数（limit/offset），**不接受任何查询功能**（ids/status/keyword 等）
   - 内部固定默认过滤（如排除 Deleted/Expired）和默认排序（如 created_at DESC）
   - 简单易用，面向"给我第一页数据"的简单列表场景
3. **查询操作统一走 query 接口**：涉及 ids 批量查询、status 过滤、keyword 搜索等，都走 query 接口
4. **统一返回 PagedResult<T>**：list 和 query 都返回 `{items: Vec<T>, total: usize}`，前端结构统一
5. **复用现有基础设施**：直接使用 `common::api::PaginationParams` 和 `common::api::PagedResult<T>`，与 McpServer 模式对齐
6. **filter 复用**：每个 DAO 抽取 `push_query_filters` 函数，COUNT 和 LIST 查询共用

## 文件结构

### 后端修改

| 层 | 文件 | 职责 |
|----|------|------|
| DTO | `common/src/api/agent.rs` | AgentQueryRequest 加 pagination；ListAgentsRequest 简化为只含 pagination |
| DTO | `common/src/api/project.rs` | ProjectQueryRequest 加 pagination；ListProjectsRequest 简化 |
| DTO | `common/src/api/task.rs` | TaskQueryRequest 加 pagination；ListTasksRequest 简化 |
| DTO | `common/src/api/tool.rs` | ToolQueryRequest 加 pagination；ListToolsRequest 简化 |
| DTO | `common/src/api/skill.rs` | SkillQueryRequest 加 pagination；ListSkillsRequest 简化 |
| DAO | `src/service/dao/{agent,project,task,tool,skill}/mod.rs` | Query 结构体加 pagination，query 签名改 PagedResult |
| DAO | `src/service/dao/{agent,project,task,tool,skill}/sqlite.rs` | 抽取 push_query_filters + COUNT + LIMIT/OFFSET |
| Domain | `src/service/domain/hr/mod.rs` | AgentManage::query + SkillManage::query_skills 返回 PagedResult |
| Domain | `src/service/domain/hr/agent.rs` | AgentDomainImpl::query 用 PagedResult::map |
| Domain | `src/service/domain/hr/skill.rs` | HrDomainImpl::query_skills 用 PagedResult::map |
| Domain | `src/service/domain/project/mod.rs` | ProjectManage::query + TaskManage::query 返回 PagedResult |
| Domain | `src/service/domain/project/{project,task}.rs` | 实现用 PagedResult::map |
| Domain | `src/service/domain/finance/mod.rs` | ToolProviderManage::query_tools 返回 PagedResult |
| Domain | `src/service/domain/finance/tool_provider.rs` | 实现用 PagedResult::map |
| Handler | `src/handlers/hr/agent/query_agents.rs` | 返回 PagedResult<AgentListItem> |
| Handler | `src/handlers/project/project/query_projects.rs` | 返回 PagedResult<ProjectListItem> |
| Handler | `src/handlers/project/task/query_tasks.rs` | 返回 PagedResult<TaskListItem> |
| Handler | `src/handlers/finance/tool/query_tools.rs` | 返回 PagedResult<ToolListItem> |
| Handler | `src/handlers/hr/skill/query_skills.rs` | 返回 PagedResult<SkillListItem> |
| Handler | `src/handlers/hr/agent/list_agents.rs` | 简化为只传 pagination，返回 PagedResult |
| Handler | `src/handlers/project/project/list_projects.rs` | 简化为只传 pagination，返回 PagedResult |
| Handler | `src/handlers/project/task/list_tasks.rs` | 简化为只传 pagination，返回 PagedResult |
| Handler | `src/handlers/finance/tool/list_tools.rs` | 简化为只传 pagination，返回 PagedResult |
| Handler | `src/handlers/hr/skill/list_skills.rs` | 简化为只传 pagination，返回 PagedResult |

### 前端修改

| 文件 | 职责 |
|------|------|
| `frontend/src/api/hr.rs` | 新增 query_agents/query_skills；list_agents/list_skills 简化为只接受 pagination |
| `frontend/src/api/project.rs` | 新增 query_projects/query_tasks；list_projects/list_tasks 简化为只接受 pagination |
| `frontend/src/api/finance.rs` | 新增 query_tools；list_tools 简化为只接受 pagination |
| `frontend/src/pages/hr/agent_detail.rs` | 关系图批量查询改用 query_* 接口 |
| `frontend/src/pages/project/project_detail.rs` | 关系图批量查询改用 query_* 接口 |
| `frontend/src/pages/project/task_detail.rs` | 关系图批量查询改用 query_* 接口 |
| `frontend/src/pages/project/tasks.rs` | 任务列表筛选改用 query_tasks 接口 |
| `frontend/src/pages/hr/agents.rs` | list_agents 调用适配新签名 |
| `frontend/src/pages/hr/skills.rs` | list_skills 调用适配新签名 |
| `frontend/src/pages/finance/tools.rs` | list_tools 调用适配新签名 |
| `frontend/src/pages/project/projects.rs` | list_projects 调用适配新签名 |
| `frontend/src/pages/project/artifacts.rs` | list_projects 调用适配新签名 |
| `frontend/src/pages/project/task_edit_modal.rs` | list_agents/list_projects 调用适配新签名 |
| `frontend/src/pages/message/chat.rs` | list_projects 调用适配新签名 |
| `frontend/src/hooks/use_workspace_data.rs` | list_agents/list_projects/list_tasks 调用适配新签名 |

### 参考文件（不改）

| 文件 | 用途 |
|------|------|
| `common/src/api/mod.rs:55-83` | PaginationParams + PagedResult 定义 |
| `src/service/dao/mcp_server/sqlite.rs:102-138, 172-203` | COUNT + LIMIT/OFFSET + push_query_filters 参考实现 |
| `common/src/api/mcp_server.rs:61-80, 106-113` | ListMcpServersRequest/Response 参考模式 |

---

### Task 1: 5 个 QueryRequest DTO 加 pagination 字段

**Files:**
- Modify: `common/src/api/agent.rs` (AgentQueryRequest)
- Modify: `common/src/api/project.rs` (ProjectQueryRequest)
- Modify: `common/src/api/task.rs` (TaskQueryRequest)
- Modify: `common/src/api/tool.rs` (ToolQueryRequest)
- Modify: `common/src/api/skill.rs` (SkillQueryRequest)

**设计要点**：
- POST body 的 QueryRequest 用 `#[serde(flatten)]` 嵌入 pagination
- 移除现有裸 `limit: Option<usize>` 字段
- ToolQueryRequest 额外移除裸 `offset: Option<usize>` 字段
- 新增 `#[serde(flatten)] pub pagination: PaginationParams`

- [ ] **Step 1: 修改 AgentQueryRequest**

文件 `common/src/api/agent.rs`，将 `pub limit: Option<usize>,` 字段替换为：
```rust
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
```
在文件顶部添加 `use crate::api::PaginationParams;`（如未有）。

- [ ] **Step 2: 修改 ProjectQueryRequest**

文件 `common/src/api/project.rs`，将 `pub limit: Option<usize>,` 字段替换为：
```rust
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
```
确认文件顶部有 `use crate::api::PaginationParams;`。

- [ ] **Step 3: 修改 TaskQueryRequest**

文件 `common/src/api/task.rs`，将 `pub limit: Option<usize>,` 字段替换为：
```rust
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
```
确认文件顶部有 `use crate::api::PaginationParams;`。

- [ ] **Step 4: 修改 ToolQueryRequest（移除 limit + offset，加 pagination）**

文件 `common/src/api/tool.rs`，将：
```rust
    pub limit: Option<usize>,
    pub offset: Option<usize>,
```
两个字段替换为：
```rust
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
```
确认文件顶部有 `use crate::api::PaginationParams;`。

- [ ] **Step 5: 修改 SkillQueryRequest**

文件 `common/src/api/skill.rs`，将 `pub limit: Option<usize>,` 字段替换为：
```rust
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
```
确认文件顶部有 `use crate::api::PaginationParams;`。

- [ ] **Step 6: Commit**

```bash
git add common/src/api/agent.rs common/src/api/project.rs common/src/api/task.rs common/src/api/tool.rs common/src/api/skill.rs
git commit -m "refactor(query): 5 个 QueryRequest DTO 移除裸 limit/offset，统一改为 pagination: PaginationParams"
```

---

### Task 2: 5 个 DAO Query 结构体加 pagination 字段

**Files:**
- Modify: `src/service/dao/agent/mod.rs` (AgentQuery)
- Modify: `src/service/dao/project/mod.rs` (ProjectQuery)
- Modify: `src/service/dao/task/mod.rs` (TaskQuery)
- Modify: `src/service/dao/tool/mod.rs` (ToolQuery)
- Modify: `src/service/dao/skill/mod.rs` (SkillQuery)

- [ ] **Step 1-5: 5 个 Query 结构体**

对每个 Query 结构体，移除 `pub limit: Option<usize>,`（ToolQuery 还移除 `pub offset: Option<usize>,`），替换为：
```rust
    pub pagination: common::api::PaginationParams,
```

- [ ] **Step 6: Commit**

```bash
git add src/service/dao/agent/mod.rs src/service/dao/project/mod.rs src/service/dao/task/mod.rs src/service/dao/tool/mod.rs src/service/dao/skill/mod.rs
git commit -m "refactor(dao): 5 个 Query 结构体移除裸 limit/offset，统一改为 pagination: PaginationParams"
```

---

### Task 3-7: 5 个 DAO SQL 改 PagedResult（抽取 push_query_filters + COUNT）

**Files:**
- Modify: `src/service/dao/agent/mod.rs` + `sqlite.rs`
- Modify: `src/service/dao/project/mod.rs` + `sqlite.rs`
- Modify: `src/service/dao/task/mod.rs` + `sqlite.rs`
- Modify: `src/service/dao/tool/mod.rs` + `sqlite.rs`
- Modify: `src/service/dao/skill/mod.rs` + `sqlite.rs`

**参考**：`src/service/dao/mcp_server/sqlite.rs:102-138, 172-203`

每个 DAO 的改造模式一致，下面以 Agent 为完整模板，其余 4 个列出关键差异。

- [ ] **Step 1 (Agent): 修改 trait 签名**

文件 `src/service/dao/agent/mod.rs`，将 `query` 方法返回类型从 `Result<Vec<AgentPo>>` 改为 `Result<common::api::PagedResult<AgentPo>>`。

- [ ] **Step 2 (Agent): 抽取 push_query_filters + 重写 query**

文件 `src/service/dao/agent/sqlite.rs`，在 impl 块外新增：
```rust
/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &AgentQuery,
) {
    if let Some(ids) = &query.ids {
        if !ids.is_empty() {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids { separated.push_bind(id); }
            separated.push_unseparated(")");
        }
    }
    if let Some(status) = &query.status {
        builder.push(" AND status = ").push_bind(*status as i32);
    }
    if let Some(exclude_status) = &query.exclude_status {
        builder.push(" AND status != ").push_bind(*exclude_status as i32);
    }
    if let Some(created_by) = &query.created_by {
        builder.push(" AND created_by = ").push_bind(created_by);
    }
    if let Some(model_provider_id) = &query.model_provider_id {
        builder.push(" AND model_provider_id = ").push_bind(model_provider_id);
    }
    if let Some(roles) = &query.roles {
        if !roles.is_empty() {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(agents.role) WHERE json_each.value IN (");
            let mut separated = builder.separated(", ");
            for role in roles { separated.push_bind(role); }
            separated.push_unseparated("))");
        }
    }
    if let Some(keyword) = &query.keyword {
        if !keyword.is_empty() {
            log_warn!("keyword in agent query is deprecated, use search_agents for FTS5; keyword ignored");
        }
    }
}
```

将 `query` 方法替换为：
```rust
    async fn query(&self, ctx: RequestContext, query: AgentQuery)
    -> Result<common::api::PagedResult<AgentPo>> {
        let pool = ctx.db_pool();
        let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM agents WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, role, description, soul, capabilities, runtime_config, model_provider_id, status, kind, created_by, modified_by, created_at, updated_at FROM agents WHERE 1=1"#,
        );
        push_query_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let items = list_builder.build_query_as().fetch_all(pool).await?;
        Ok(common::api::PagedResult { items, total: total as usize })
    }
```

- [ ] **Step 3 (Project): 同模式改造**

文件 `src/service/dao/project/mod.rs`：trait 签名改 `Result<common::api::PagedResult<ProjectPo>>`。

文件 `src/service/dao/project/sqlite.rs` 的 `push_query_filters`：
```rust
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &ProjectQuery,
) {
    // 默认软删除过滤
    builder.push(" AND \"status\" != 0");
    if let Some(ids) = &query.ids {
        if !ids.is_empty() {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids { separated.push_bind(id); }
            drop(separated);
            builder.push(")");
        }
    }
    if let Some(root_user_id) = &query.root_user_id {
        builder.push(" AND root_user_id = ").push_bind(root_user_id);
    }
    if let Some(status_list) = &query.status_in {
        if !status_list.is_empty() {
            builder.push(" AND \"status\" IN (");
            let mut separated = builder.separated(", ");
            for s in status_list { separated.push_bind(*s as i32); }
            drop(separated);
            builder.push(")");
        }
    }
}
```
query 方法同 Agent 模式，COUNT 表名 `projects`，LIST 字段列表保持原样，排序 `ORDER BY priority DESC, created_at DESC`。

- [ ] **Step 4 (Task): 同模式改造**

文件 `src/service/dao/task/mod.rs`：trait 签名改 `Result<common::api::PagedResult<TaskPo>>`。

文件 `src/service/dao/task/sqlite.rs` 的 `push_query_filters`：
```rust
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &TaskQuery,
) {
    builder.push(r#" AND "status" != 0"#);
    if let Some(ids) = &query.ids {
        if !ids.is_empty() {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids { separated.push_bind(id); }
            drop(separated);
            builder.push(")");
        }
    }
    if let Some(assignee_type) = &query.assignee_type {
        builder.push(r#" AND "assignee_type" = "#).push_bind(*assignee_type as i32);
    }
    if let Some(assignee_id) = &query.assignee_id {
        builder.push(" AND assignee_id = ").push_bind(assignee_id);
    }
    if let Some(project_id) = &query.project_id {
        builder.push(" AND project_id = ").push_bind(project_id);
    }
    if let Some(status_list) = &query.status_in {
        if !status_list.is_empty() {
            builder.push(r#" AND "status" IN ("#);
            let mut separated = builder.separated(", ");
            for s in status_list { separated.push_bind(*s as i32); }
            drop(separated);
            builder.push(")");
        }
    }
}
```
query 方法同模式，COUNT 表名 `tasks`，排序 `ORDER BY priority DESC, created_at DESC`。

- [ ] **Step 5 (Tool): 统一 WHERE 1=1 模式 + JOIN 支持**

文件 `src/service/dao/tool/mod.rs`：trait 签名改 `Result<common::api::PagedResult<ToolPo>>`。

文件 `src/service/dao/tool/sqlite.rs` 的 `push_query_filters`（注意 Tool 用表别名 `t.`）：
```rust
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &ToolQuery,
) {
    if let Some(agent_id) = &query.agent_id {
        builder.push(" AND at.agent_id = ").push_bind(agent_id);
    }
    if let Some(ids) = &query.ids {
        if !ids.is_empty() {
            builder.push(" AND t.id IN (");
            let mut separated = builder.separated(", ");
            for id in ids { separated.push_bind(id.clone()); }
            separated.push_unseparated(")");
        }
    }
    if let Some(keyword) = &query.keyword {
        if !keyword.is_empty() {
            log_warn!("keyword in ToolDao::query is deprecated, use search_tools; keyword ignored");
        }
    }
    if let Some(tags) = &query.tags {
        if !tags.is_empty() {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(t.tags) WHERE json_each.value IN (");
            let mut separated = builder.separated(", ");
            for tag in tags { separated.push_bind(tag); }
            separated.push_unseparated("))");
        }
    }
    if let Some(protocol) = query.protocol {
        builder.push(" AND t.protocol = ").push_bind(protocol as i32);
    }
    if let Some(status) = query.status {
        builder.push(" AND t.status = ").push_bind(status as i32);
    }
    if let Some(exclude_status) = query.exclude_status {
        builder.push(" AND t.status != ").push_bind(exclude_status as i32);
    }
    if let Some(server_id) = &query.mcp_server_id {
        builder.push(" AND json_extract(t.config, '$.server_id') = ").push_bind(server_id.clone());
    }
    if let Some(enabled_only) = query.enabled_only {
        if enabled_only { builder.push(" AND t.status = 1"); }
    }
}
```
query 方法（COUNT 需含 JOIN）：
```rust
    async fn query(&self, ctx: RequestContext, query: ToolQuery) -> Result<common::api::PagedResult<ToolPo>> {
        let pool = ctx.db_pool();
        let has_agent_filter = query.agent_id.is_some();
        let join_clause = if has_agent_filter { " INNER JOIN agent_tools at ON t.id = at.tool_id" } else { "" };

        let count_sql = format!("SELECT COUNT(*) FROM tools t{}", join_clause);
        let mut count_builder = sqlx::QueryBuilder::new(&count_sql);
        count_builder.push(" WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let list_sql = format!("SELECT t.* FROM tools t{}", join_clause);
        let mut list_builder = sqlx::QueryBuilder::new(&list_sql);
        list_builder.push(" WHERE 1=1");
        push_query_filters(&mut list_builder, &query);
        if has_agent_filter {
            list_builder.push(" ORDER BY at.created_at ASC");
        } else {
            list_builder.push(" ORDER BY t.created_at DESC");
        }
        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }
        let items = list_builder.build_query_as().fetch_all(pool).await?;
        Ok(common::api::PagedResult { items, total: total as usize })
    }
```

- [ ] **Step 6 (Skill): 同模式改造**

文件 `src/service/dao/skill/mod.rs`：trait 签名改 `Result<common::api::PagedResult<SkillPo>>`。

文件 `src/service/dao/skill/sqlite.rs` 的 `push_query_filters`：
```rust
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &SkillQuery,
) {
    if let Some(ids) = &query.ids {
        if !ids.is_empty() {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids { separated.push_bind(id); }
            separated.push_unseparated(")");
        }
    }
    if let Some(status) = &query.status {
        builder.push(" AND status = ").push_bind(*status as i32);
    }
    if let Some(exclude_status) = &query.exclude_status {
        builder.push(" AND status != ").push_bind(*exclude_status as i32);
    }
    if let Some(category) = &query.category {
        builder.push(" AND category = ").push_bind(category);
    }
    if let Some(author_id) = &query.author_id {
        builder.push(" AND author_id = ").push_bind(author_id);
    }
    if let Some(parent_skill_id) = &query.parent_skill_id {
        builder.push(" AND parent_skill_id = ").push_bind(parent_skill_id);
    }
    if let Some(tags) = &query.tags {
        if !tags.is_empty() {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (");
            let mut separated = builder.separated(", ");
            for tag in tags { separated.push_bind(tag); }
            separated.push_unseparated("))");
        }
    }
    if let Some(keyword) = &query.keyword {
        if !keyword.is_empty() {
            log_warn!("keyword in skill query is deprecated, use search_skills; keyword ignored");
        }
    }
}
```
query 方法同模式，COUNT 表名 `skills`，排序 `ORDER BY updated_at DESC`，使用 `build_query_as::<SkillPo>()`。

- [ ] **Step 7: Commit**

```bash
git add src/service/dao/agent/ src/service/dao/project/ src/service/dao/task/ src/service/dao/tool/ src/service/dao/skill/
git commit -m "feat(dao): 5 个 query 方法改返回 PagedResult，抽取 push_query_filters 复用"
```

---

### Task 8: 5 个 Domain query 方法改返回 PagedResult

**Files:**
- Modify: `src/service/domain/hr/mod.rs` (AgentManage::query + SkillManage::query_skills)
- Modify: `src/service/domain/hr/agent.rs` (AgentDomainImpl::query 实现)
- Modify: `src/service/domain/hr/skill.rs` (HrDomainImpl::query_skills 实现)
- Modify: `src/service/domain/project/mod.rs` (ProjectManage::query + TaskManage::query)
- Modify: `src/service/domain/project/project.rs` (ProjectDomainImpl::query 实现)
- Modify: `src/service/domain/project/task.rs` (ProjectDomainImpl::query 实现)
- Modify: `src/service/domain/finance/mod.rs` (ToolProviderManage::query_tools)
- Modify: `src/service/domain/finance/tool_provider.rs` (ToolProviderDomainImpl::query_tools 实现)

**设计要点**：用 `PagedResult::map` 把 `PagedResult<Po>` 转为 `PagedResult<业务实体>`，保留 total。

- [ ] **Step 1-5: 修改 5 个 trait 签名 + 实现**

对每个 Domain query 方法：
1. trait 签名返回类型从 `Result<Vec<Xxx>>` 改为 `Result<common::api::PagedResult<Xxx>>`
2. 实现改为：
```rust
async fn query(&self, ctx: RequestContext, query: XxxQuery) -> Result<common::api::PagedResult<Xxx>> {
    let page = self.xxx_dal.query(ctx, query).await?;
    Ok(page.map(Xxx::from_po))
}
```

**注意**：需确认各实体的 Po→业务实体转换方法名（可能是 `from_po`、`from` 或其他）。执行时先 grep 确认：
```bash
grep -rn "fn from_po\|fn from(" src/models/ | grep -E "Agent|Project|Task|Tool|Skill"
```

- [ ] **Step 6: Commit**

```bash
git add src/service/domain/
git commit -m "refactor(domain): 5 个 query 方法改返回 PagedResult，用 map 保留 total"
```

---

### Task 9: 5 个 query handler 改返回 PagedResult

**Files:**
- Modify: `src/handlers/hr/agent/query_agents.rs`
- Modify: `src/handlers/project/project/query_projects.rs`
- Modify: `src/handlers/project/task/query_tasks.rs`
- Modify: `src/handlers/finance/tool/query_tools.rs`
- Modify: `src/handlers/hr/skill/query_skills.rs`

**设计要点**：query handler 透传 `params.pagination` 到 Query 结构体，返回 `PagedResult<T>`。

- [ ] **Step 1-5: 修改 5 个 query handler**

对每个 query handler：
1. 返回类型改为 `Result<common::api::PagedResult<XxxListItem>>`
2. 构造 Query 时用 `pagination: params.pagination` 替换原来的 `limit: params.limit`（ToolQueryRequest 还需移除 `offset: params.offset`）
3. 用 `page.map(|item| { ... mapping ... })` 替换 `items.iter().map(...).collect()`

例如 query_agents：
```rust
pub async fn query_agents(
    ctx: RequestContext,
    params: AgentQueryRequest,
) -> Result<common::api::PagedResult<AgentListItem>> {
    let page = domain().agent_manage().query(ctx, AgentQuery {
        ids: params.ids,
        keyword: params.keyword,
        status: params.status,
        exclude_status: Some(AgentStatus::Deleted),
        created_by: params.created_by,
        model_provider_id: params.model_provider_id,
        roles: params.roles,
        pagination: params.pagination,
        ..Default::default()
    }).await?;

    Ok(page.map(|agent| {
        let runtime_state = match &agent.runtime_info {
            Some(info) => info.state as i32,
            None => AgentRuntimeState::Idle as i32,
        };
        AgentListItem {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            roles: agent.po.get_roles(),
            description: if agent.po.description.is_empty() { None } else { Some(agent.po.description.clone()) },
            kind: agent.po.kind.to_string(),
            model_provider_id: agent.po.model_provider_id.clone(),
            status: agent.po.status as i32,
            created_at: agent.po.created_at,
            runtime_state,
        }
    }))
}
```

其他 4 个同理：query_projects 用 `page.map(response::to_list_item)`，query_tasks 同理，query_tools 用 `page.map(to_list_item)`，query_skills 同理。

- [ ] **Step 6: Commit**

```bash
git add src/handlers/hr/agent/query_agents.rs src/handlers/project/project/query_projects.rs src/handlers/project/task/query_tasks.rs src/handlers/finance/tool/query_tools.rs src/handlers/hr/skill/query_skills.rs
git commit -m "refactor(handler): 5 个 query handler 改返回 PagedResult，透传 pagination"
```

---

### Task 10: 5 个 ListXxxRequest DTO 简化（移除查询字段，只保留 pagination）

**Files:**
- Modify: `common/src/api/agent.rs` (ListAgentsRequest)
- Modify: `common/src/api/project.rs` (ListProjectsRequest)
- Modify: `common/src/api/task.rs` (ListTasksRequest)
- Modify: `common/src/api/tool.rs` (ListToolsRequest)
- Modify: `common/src/api/skill.rs` (ListSkillsRequest)

**设计要点**：
- list 是语法糖：只接受分页参数，不接受任何查询功能
- GET query param 用 `#[serde(flatten)]` + `#[param(source = "query")]` 嵌入 PaginationParams
- 移除所有查询字段（ids/status/keyword/agent_id/project_id/assignee_id/assignee_type/category/author_id/only_enabled 等）

- [ ] **Step 1: 简化 ListAgentsRequest**

文件 `common/src/api/agent.rs`，将：
```rust
pub struct ListAgentsRequest {
    #[param(source = "query")]
    pub status: Option<AgentStatus>,
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```
改为：
```rust
/// Agent 列表请求（语法糖：只接受分页参数，内部固定排除 Deleted + created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}
```

- [ ] **Step 2: 简化 ListProjectsRequest**

文件 `common/src/api/project.rs`，将：
```rust
pub struct ListProjectsRequest {
    #[param(source = "query")]
    pub root_user_id: Option<String>,
    #[param(source = "query")]
    pub status: Option<ProjectStatus>,
    #[param(source = "query")]
    pub limit: Option<usize>,
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```
改为：
```rust
/// Project 列表请求（语法糖：只接受分页参数，内部固定排除 status=0 + priority DESC, created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListProjectsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}
```

- [ ] **Step 3: 简化 ListTasksRequest**

文件 `common/src/api/task.rs`，将：
```rust
pub struct ListTasksRequest {
    #[param(source = "query")]
    pub project_id: Option<String>,
    #[param(source = "query")]
    pub status: Option<i32>,
    #[param(source = "query")]
    pub assignee_id: Option<String>,
    #[param(source = "query")]
    pub assignee_type: Option<i32>,
    #[param(source = "query")]
    pub limit: Option<usize>,
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```
改为：
```rust
/// Task 列表请求（语法糖：只接受分页参数，内部固定排除 status=0 + priority DESC, created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListTasksRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}
```

- [ ] **Step 4: 简化 ListToolsRequest**

文件 `common/src/api/tool.rs`，将：
```rust
pub struct ListToolsRequest {
    #[param(source = "query")]
    pub agent_id: Option<String>,
    #[param(source = "query")]
    pub keyword: Option<String>,
    #[param(source = "query")]
    pub only_enabled: Option<bool>,
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```
改为：
```rust
/// Tool 列表请求（语法糖：只接受分页参数，内部固定 created_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListToolsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}
```

- [ ] **Step 5: 简化 ListSkillsRequest**

文件 `common/src/api/skill.rs`，将：
```rust
pub struct ListSkillsRequest {
    #[param(source = "query")]
    pub status: Option<SkillStatus>,
    #[param(source = "query")]
    pub category: Option<String>,
    #[param(source = "query")]
    pub author_id: Option<String>,
    #[param(source = "query")]
    pub keyword: Option<String>,
    #[param(source = "query")]
    pub limit: Option<usize>,
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```
改为：
```rust
/// Skill 列表请求（语法糖：只接受分页参数，内部固定排除 Expired + updated_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSkillsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}
```

- [ ] **Step 6: Commit**

```bash
git add common/src/api/agent.rs common/src/api/project.rs common/src/api/task.rs common/src/api/tool.rs common/src/api/skill.rs
git commit -m "refactor(list): 5 个 ListXxxRequest 简化为只接受 pagination，移除查询字段"
```

---

### Task 11: 5 个 list handler 改造（返回 PagedResult，内部固定默认过滤）

**Files:**
- Modify: `src/handlers/hr/agent/list_agents.rs`
- Modify: `src/handlers/project/project/list_projects.rs`
- Modify: `src/handlers/project/task/list_tasks.rs`
- Modify: `src/handlers/finance/tool/list_tools.rs`
- Modify: `src/handlers/hr/skill/list_skills.rs`

**设计要点**：
- list handler 只接受 pagination（从简化后的 ListXxxRequest）
- 内部构造 Query 时固定默认过滤（如排除 Deleted/Expired）
- 返回 `PagedResult<XxxListItem>`（与 query handler 统一）

- [ ] **Step 1: 改造 list_agents**

文件 `src/handlers/hr/agent/list_agents.rs`，将整个 handler 改为：
```rust
pub async fn list_agents(
    ctx: RequestContext,
    params: ListAgentsRequest,
) -> Result<common::api::PagedResult<AgentListItem>> {
    // list 是语法糖：只接受分页，内部固定排除 Deleted
    let page = domain()
        .agent_manage()
        .query(
            ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(|agent| {
        let runtime_state = match &agent.runtime_info {
            Some(info) => info.state as i32,
            None => AgentRuntimeState::Idle as i32,
        };
        AgentListItem {
            id: agent.id().to_string(),
            name: agent.name().to_string(),
            roles: agent.po.get_roles(),
            description: if agent.po.description.is_empty() { None } else { Some(agent.po.description.clone()) },
            kind: agent.po.kind.to_string(),
            model_provider_id: agent.po.model_provider_id.clone(),
            status: agent.po.status as i32,
            created_at: agent.po.created_at,
            runtime_state,
        }
    }))
}
```

- [ ] **Step 2: 改造 list_projects**

文件 `src/handlers/project/project/list_projects.rs`，将整个 handler 改为：
```rust
pub async fn list_projects(
    ctx: RequestContext,
    params: ListProjectsRequest,
) -> Result<common::api::PagedResult<ProjectListItem>> {
    // list 是语法糖：只接受分页，内部固定 root_user_id=ctx.uid() + 排除 status=0
    let root_user_id = ctx.uid();
    if root_user_id.is_empty() {
        bail_error!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let page = domain()
        .project_manage()
        .query(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id),
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(response::to_list_item))
}
```
**注意**：原 list_projects 有 `root_user_id` 参数，简化后从 ctx.uid() 获取（list 是语法糖，默认查当前用户的 projects）。

- [ ] **Step 3: 改造 list_tasks**

文件 `src/handlers/project/task/list_tasks.rs`，将整个 handler 改为：
```rust
pub async fn list_tasks(
    ctx: RequestContext,
    params: ListTasksRequest,
) -> Result<common::api::PagedResult<TaskListItem>> {
    // list 是语法糖：只接受分页，内部固定排除 status=0
    let page = domain()
        .task_manage()
        .query(
            ctx,
            TaskQuery {
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(response::to_list_item))
}
```

- [ ] **Step 4: 改造 list_tools**

文件 `src/handlers/finance/tool/list_tools.rs`，将整个 handler 改为：
```rust
pub async fn list_tools(
    ctx: RequestContext,
    params: ListToolsRequest,
) -> Result<common::api::PagedResult<ToolListItem>> {
    // list 是语法糖：只接受分页
    let page = domain()
        .tool_provider_manage()
        .query_tools(
            ctx,
            ToolQuery {
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(to_list_item))
}
```

- [ ] **Step 5: 改造 list_skills**

文件 `src/handlers/hr/skill/list_skills.rs`，将整个 handler 改为：
```rust
pub async fn list_skills(
    ctx: RequestContext,
    params: ListSkillsRequest,
) -> Result<common::api::PagedResult<SkillListItem>> {
    // list 是语法糖：只接受分页，内部固定排除 Expired
    let page = domain()
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                exclude_status: Some(SkillStatus::Expired),
                pagination: params.pagination,
                ..Default::default()
            },
        )
        .await?;

    Ok(page.map(to_list_item))
}
```

- [ ] **Step 6: 编译检查**

Run: `cargo check 2>&1 | grep "error\[" | head -20`
Expected: 0 个后端错误。如果报错，检查是否有遗漏的调用方或测试文件。

- [ ] **Step 7: Commit**

```bash
git add src/handlers/hr/agent/list_agents.rs src/handlers/project/project/list_projects.rs src/handlers/project/task/list_tasks.rs src/handlers/finance/tool/list_tools.rs src/handlers/hr/skill/list_skills.rs
git commit -m "refactor(handler): 5 个 list handler 简化为只接受 pagination，返回 PagedResult"
```

---

### Task 12: 前端 API 层适配（新增 query_* 函数 + list_* 简化）

**Files:**
- Modify: `frontend/src/api/hr.rs`
- Modify: `frontend/src/api/project.rs`
- Modify: `frontend/src/api/finance.rs`

**设计要点**：
- list_* 函数简化为只接受 `pagination: Option<(usize, usize)>` 或 `PaginationParams`
- 新增 query_* 函数（POST body，接受完整查询条件 + pagination）
- 两者都返回 `PagedResult<T>`

- [ ] **Step 1: 修改 frontend/src/api/hr.rs**

将 `list_agents` 签名改为只接受 pagination：
```rust
pub async fn list_agents(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<AgentListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit { params.push(format!("limit={}", l)); }
    if let Some(o) = offset { params.push(format!("offset={}", o)); }
    let url = if params.is_empty() { "/api/v1/hr/agents".to_string() }
              else { format!("/api/v1/hr/agents?{}", params.join("&")) };
    api_get_or_default(&url).await
}
```

新增 `query_agents`：
```rust
pub async fn query_agents(req: &AgentQueryRequest) -> Result<PagedResult<AgentListItem>, ApiError> {
    api_post("/api/v1/hr/agents/query", req).await
}
```

将 `list_skills` 签名改为只接受 pagination：
```rust
pub async fn list_skills(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<SkillListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit { params.push(format!("limit={}", l)); }
    if let Some(o) = offset { params.push(format!("offset={}", o)); }
    let url = if params.is_empty() { "/api/v1/hr/skills".to_string() }
              else { format!("/api/v1/hr/skills?{}", params.join("&")) };
    api_get_or_default(&url).await
}
```

新增 `query_skills`：
```rust
pub async fn query_skills(req: &SkillQueryRequest) -> Result<PagedResult<SkillListItem>, ApiError> {
    api_post("/api/v1/hr/skills/query", req).await
}
```

- [ ] **Step 2: 修改 frontend/src/api/project.rs**

将 `list_projects` 签名改为只接受 pagination：
```rust
pub async fn list_projects(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<ProjectListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit { params.push(format!("limit={}", l)); }
    if let Some(o) = offset { params.push(format!("offset={}", o)); }
    let url = if params.is_empty() { "/api/v1/projects".to_string() }
              else { format!("/api/v1/projects?{}", params.join("&")) };
    api_get_or_default(&url).await
}
```

将 `list_tasks` 签名改为只接受 pagination：
```rust
pub async fn list_tasks(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<TaskListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit { params.push(format!("limit={}", l)); }
    if let Some(o) = offset { params.push(format!("offset={}", o)); }
    let url = if params.is_empty() { "/api/v1/tasks".to_string() }
              else { format!("/api/v1/tasks?{}", params.join("&")) };
    api_get_or_default(&url).await
}
```

新增 `query_projects` 和 `query_tasks`：
```rust
pub async fn query_projects(req: &ProjectQueryRequest) -> Result<PagedResult<ProjectListItem>, ApiError> {
    api_post("/api/v1/projects/query", req).await
}

pub async fn query_tasks(req: &TaskQueryRequest) -> Result<PagedResult<TaskListItem>, ApiError> {
    api_post("/api/v1/tasks/query", req).await
}
```

- [ ] **Step 3: 修改 frontend/src/api/finance.rs**

将 `list_tools` 签名改为只接受 pagination：
```rust
pub async fn list_tools(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<ToolListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit { params.push(format!("limit={}", l)); }
    if let Some(o) = offset { params.push(format!("offset={}", o)); }
    let url = if params.is_empty() { "/api/v1/finance/tools".to_string() }
              else { format!("/api/v1/finance/tools?{}", params.join("&")) };
    api_get_or_default(&url).await
}
```

新增 `query_tools`：
```rust
pub async fn query_tools(req: &ToolQueryRequest) -> Result<PagedResult<ToolListItem>, ApiError> {
    api_post("/api/v1/finance/tools/query", req).await
}
```

- [ ] **Step 4: 在 frontend/src/api/common.rs 或 mod.rs 中确认 PagedResult 已导入**

确认 `common::api::PagedResult` 已在前端可用。前端 Cargo.toml 已依赖 common crate。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/api/project.rs frontend/src/api/finance.rs
git commit -m "refactor(frontend-api): list_* 简化为只接受 pagination，新增 query_* 函数"
```

---

### Task 13: 前端页面适配（6 个非 None 调用点改用 query_* + 所有 list_* 调用适配）

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs` (关系图批量查询改用 query_*)
- Modify: `frontend/src/pages/project/project_detail.rs` (关系图批量查询改用 query_*)
- Modify: `frontend/src/pages/project/task_detail.rs` (关系图批量查询改用 query_*)
- Modify: `frontend/src/pages/project/tasks.rs` (任务列表筛选改用 query_tasks)
- Modify: `frontend/src/pages/hr/agents.rs` (list_agents 调用适配)
- Modify: `frontend/src/pages/hr/skills.rs` (list_skills 调用适配)
- Modify: `frontend/src/pages/finance/tools.rs` (list_tools 调用适配)
- Modify: `frontend/src/pages/project/projects.rs` (list_projects 调用适配)
- Modify: `frontend/src/pages/project/artifacts.rs` (list_projects 调用适配)
- Modify: `frontend/src/pages/project/task_edit_modal.rs` (list_agents/list_projects 调用适配)
- Modify: `frontend/src/pages/message/chat.rs` (list_projects 调用适配)
- Modify: `frontend/src/hooks/use_workspace_data.rs` (list_* 调用适配)

**设计要点**：
- 所有 `list_*(None)` 调用改为 `list_*(None, None)`（只传 pagination）
- 响应从 `resp.agents`/`resp.projects` 等改为 `resp.items`
- 6 个非 None 调用点改用 query_* 接口

- [ ] **Step 1: agent_detail.rs 改造**

文件 `frontend/src/pages/hr/agent_detail.rs`：

1. `list_tools(None)` → `list_tools(None, None)`，响应 `.tools` → `.items`
2. `list_tasks(None, None, Some(&aid), Some(1), None)` → 改用 `query_tasks`：
```rust
let req = TaskQueryRequest {
    assignee_id: Some(aid.clone()),
    assignee_type: Some(AssigneeType::Agent),
    pagination: PaginationParams::default(),
    ..Default::default()
};
match query_tasks(&req).await {
    Ok(page) => {
        let tasks = page.items;
        // ... 从 tasks 收集 project_ids ...
        let project_ids: Vec<String> = tasks.iter()
            .filter_map(|t| t.project_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        graph_tasks.set(tasks);

        if project_ids.is_empty() {
            graph_projects.set(Vec::new());
        } else {
            let req = ProjectQueryRequest {
                ids: Some(project_ids),
                pagination: PaginationParams::default(),
                ..Default::default()
            };
            match query_projects(&req).await {
                Ok(page) => graph_projects.set(page.items),
                Err(e) => toast.error(&format!("批量获取项目失败: {}", e)),
            }
        }
    }
    Err(e) => toast.error(&format!("获取任务列表失败: {}", e)),
}
```

- [ ] **Step 2: project_detail.rs 改造**

文件 `frontend/src/pages/project/project_detail.rs`：

list_tasks 通过 `list_project_tasks` 调用（专用接口，不是 list_tasks，保持不变）。
但 `list_agents(Some(&assignee_ids))` → 改用 `query_agents`：
```rust
let req = AgentQueryRequest {
    ids: Some(assignee_ids),
    pagination: PaginationParams::default(),
    ..Default::default()
};
match query_agents(&req).await {
    Ok(page) => graph_agents.set(page.items),
    Err(e) => toast.error(&format!("批量获取 Agent 失败: {}", e)),
}
```

- [ ] **Step 3: task_detail.rs 改造**

文件 `frontend/src/pages/project/task_detail.rs`：

1. `list_agents(Some(&[id]))` → 改用 `query_agents`
2. `list_projects(Some(&[id]))` → 改用 `query_projects`

- [ ] **Step 4: tasks.rs 改造（任务列表筛选改用 query_tasks）**

文件 `frontend/src/pages/project/tasks.rs`：

将 `list_tasks(project_id, status, None, at, None)` → 改用 `query_tasks`：
```rust
let req = TaskQueryRequest {
    project_id: if pid.is_empty() { None } else { Some(pid.clone()) },
    status_in: status.map(|s| vec![TaskStatus::from_i32(s)]),
    assignee_type: at.map(AssigneeType::from_i32),
    pagination: PaginationParams::default(),
    ..Default::default()
};
match query_tasks(&req).await {
    Ok(page) => { /* ... 用 page.items ... */ }
    Err(e) => toast.error(&e),
}
```

- [ ] **Step 5: 其余 list_* 调用适配**

对以下文件，将 `list_*(None)` 改为 `list_*(None, None)`，响应从 `resp.xxx` 改为 `resp.items`：
- `frontend/src/pages/hr/agents.rs` — `list_agents(None)` → `list_agents(None, None)`，`.agents` → `.items`
- `frontend/src/pages/hr/skills.rs` — `list_skills(None)` → `list_skills(None, None)`，`.skills` → `.items`
- `frontend/src/pages/finance/tools.rs` — `list_tools(None)` → `list_tools(None, None)`，`.tools` → `.items`
- `frontend/src/pages/project/projects.rs` — `list_projects(None)` → `list_projects(None, None)`，`.projects` → `.items`
- `frontend/src/pages/project/artifacts.rs` — `list_projects(None)` → `list_projects(None, None)`，`.projects` → `.items`
- `frontend/src/pages/project/task_edit_modal.rs` — `list_agents(None)` → `list_agents(None, None)`，`list_projects(None)` → `list_projects(None, None)`
- `frontend/src/pages/message/chat.rs` — `list_projects(None)` → `list_projects(None, None)`
- `frontend/src/hooks/use_workspace_data.rs` — `list_agents(None)` → `list_agents(None, None)`，`list_projects(None)` → `list_projects(None, None)`，`list_tasks(None, None, None, None, None)` → `list_tasks(None, None)`

- [ ] **Step 6: 编译检查**

Run: `cd frontend && cargo check 2>&1 | tail -10`
Expected: 0 个错误。如有错误，检查是否有遗漏的调用点或字段访问。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/ frontend/src/hooks/
git commit -m "refactor(frontend): 6 个查询场景改用 query_* 接口，list_* 调用适配新签名"
```

---

### Task 14: 最终验证 + 推送

- [ ] **Step 1: 后端测试**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 全部测试通过。如有测试因 Query 结构体改动而失败，需更新测试文件中的 `limit:` → `pagination:`。

- [ ] **Step 2: 检查测试文件**

Run: `grep -rn "limit:" src/service/dao/*_test.rs src/service/dao/*_sqlite_test.rs 2>/dev/null | grep -v "pagination" | head -20`
Expected: 0 行。如有，改为 `pagination: PaginationParams { limit: Some(...), offset: None }`。

- [ ] **Step 3: 前端测试**

Run: `cd frontend && cargo test 2>&1 | tail -5`
Expected: 全部测试通过。

- [ ] **Step 4: 前端 release build**

Run: `cd frontend && cargo build --release 2>&1 | tail -5`
Expected: 编译成功。

- [ ] **Step 5: 推送**

```bash
git push
```

---

## Self-Review

### 1. Spec 覆盖
- ✅ 5 个 QueryRequest DTO 加 pagination（Task 1）
- ✅ 5 个 DAO Query 结构体加 pagination（Task 2）
- ✅ 5 个 DAO SQL 改 PagedResult（Task 3-7）
- ✅ 5 个 Domain query 方法改 PagedResult（Task 8）
- ✅ 5 个 query handler 改 PagedResult（Task 9）
- ✅ 5 个 ListXxxRequest 简化为只接受 pagination（Task 10）
- ✅ 5 个 list handler 改造为返回 PagedResult（Task 11）
- ✅ 前端 API 层新增 query_* + list_* 简化（Task 12）
- ✅ 前端页面适配（Task 13）
- ✅ 验证 + 推送（Task 14）

### 2. 设计对齐
- ✅ list 是语法糖：只接受 pagination，内部固定默认过滤和排序
- ✅ query 是核心：完整查询条件 + pagination
- ✅ 统一返回 PagedResult<T>：list 和 query 结构一致
- ✅ 查询操作统一走 query 接口

### 3. 潜在风险点
- **list_projects 移除 root_user_id 参数**：原 list_projects 接受 root_user_id 查询参数，简化后从 ctx.uid() 获取。前端需确认 list_projects 调用不传 root_user_id（当前前端都不传，由后端从 ctx 获取）。
- **tasks.rs 筛选改用 query_tasks**：任务列表页的筛选功能从 list_tasks 改为 query_tasks，需确保 query_tasks 支持 project_id/status/assignee_type 过滤（TaskQuery 已有这些字段）。
- **serde flatten 兼容性**：`#[serde(flatten)]` 在 GET query param 和 POST body 中的行为需验证。
- **from_po 方法名**：执行前需确认各实体的 Po→业务实体转换方法名。
