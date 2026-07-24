# 批量查询与通用 query 接口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 5 个实体（Agent/Project/Task/Tool/Skill）增强查询能力：list 接口支持 ids 批量查询（满足当前需求），同时补齐通用 query 接口（面向未来复杂查询），并修复 Agent handler 内存过滤和 Tool DTO 注解缺失两个现有 bug。

**Architecture:** 明确分层职责——DAO 层 `query(XxxQuery)` 是核心查询能力（已就绪），Domain 层 `query` 是核心方法的暴露（Agent/Skill/Tool 已有，Project/Task 需补齐），`list(...)` 是列表场景的语法糖（保留不动）。Handler 层双通道：`list_*` handler 增强 ids 支持并通过 query 实现（语法糖调用核心），新增 `query_*` handler 暴露完整 Query 能力（POST + body，支持复杂查询）。前端 3 个详情页改用 list + ids 消除 N+1。

**Tech Stack:** Rust (axum + sqlx + ai_orz_macros::Params + #[generate_http_handler]), Dioxus 前端

---

## 设计原则

1. **query 是核心，list 是语法糖**：Domain 层 `query(ctx, XxxQuery)` 是通用查询核心方法，`list(...)` 是列表场景的便捷封装
2. **list 接口增强**：DTO 加 ids 字段，handler 内部统一走 `query(ctx, XxxQuery { ids, status, ... })`，无分支
3. **query 接口补齐**：新增 `POST /api/v1/{entity}/query` 路由，接收完整 QueryRequest DTO（body），暴露全部 Query 能力
4. **Agent handler 内存过滤 bug 必须修复**：所有路径统一走 query SQL 层过滤
5. **现有 list 方法签名不动**：list 作为语法糖保留原签名，其他调用方不受影响

---

## File Structure

### 后端
- **list DTO 层（5 文件）**：`common/src/api/{agent,project,task,tool,skill}.rs` — ListXxxRequest 加 ids
- **query DTO 层（5 文件）**：同上文件 — 新增 XxxQueryRequest 结构体
- **Domain 层（1 文件）**：`src/service/domain/project/mod.rs` — ProjectManage/TaskManage 补 query 方法
- **list Handler 层（5 文件）**：`src/handlers/.../list_*.rs` — 改走 query
- **query Handler 层（5 新文件）**：`src/handlers/.../query_*.rs` — 新增 query handler
- **路由（1 文件）**：`src/router.rs` — 注册 5 个新路由
- **DTO 测试**：`common/src/api/*_test.rs` — 补字段

### 前端
- **API 层（4 文件）**：`frontend/src/api/{hr,project,tool,skill}.rs` — list_* 加 ids 参数
- **页面层（3 文件）**：`frontend/src/pages/...` — 改用批量查询

---

## Task 1: 修复 ListToolsRequest 注解 + 5 个 list DTO 加 ids 字段

**Files:**
- Modify: `common/src/api/tool.rs` (ListToolsRequest 修复注解 + 加 ids)
- Modify: `common/src/api/agent.rs` (ListAgentsRequest 加 ids)
- Modify: `common/src/api/project.rs` (ListProjectsRequest 加 ids)
- Modify: `common/src/api/task.rs` (ListTasksRequest 加 ids)
- Modify: `common/src/api/skill.rs` (ListSkillsRequest 加 ids)

### Step 1: 修复 ListToolsRequest 注解 + 加 ids

当前 `ListToolsRequest` 3 个字段均漏标 `#[param(source = "query")]`（bug），同时追加 ids：

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListToolsRequest {
    /// Filter by bound agent ID
    #[param(source = "query")]
    pub agent_id: Option<String>,
    /// Search by keyword in name/description
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// Filter by enabled status
    #[param(source = "query")]
    pub only_enabled: Option<bool>,
    /// 按 ID 批量查询
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```

### Step 2: ListAgentsRequest 加 ids

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentsRequest {
    /// 可选状态筛选
    #[param(source = "query")]
    pub status: Option<AgentStatus>,
    /// 按 ID 批量查询
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```

### Step 3: ListProjectsRequest 加 ids

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListProjectsRequest {
    #[param(source = "query")]
    pub root_user_id: Option<String>,
    #[param(source = "query")]
    pub status: Option<ProjectStatus>,
    #[param(source = "query")]
    pub limit: Option<usize>,
    /// 按 ID 批量查询
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```

### Step 4: ListTasksRequest 加 ids

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
    /// 按 ID 批量查询
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```

### Step 5: ListSkillsRequest 加 ids

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
    /// 按 ID 批量查询
    #[param(source = "query")]
    pub ids: Option<Vec<String>>,
}
```

- [ ] **Step 6: 修复受影响的 DTO 测试**

检查 `common/src/api/*_test.rs` 中构造 `ListXxxRequest` 的位置，补齐 `ids` 字段。

Run: `cargo test -p common --lib 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add common/src/api/agent.rs common/src/api/project.rs common/src/api/task.rs common/src/api/tool.rs common/src/api/skill.rs common/src/api/*_test.rs
git commit -m "feat(dto): 5 个 ListXxxRequest 新增 ids + 修复 ListToolsRequest 注解"
```

---

## Task 2: Domain 层为 ProjectManage 和 TaskManage 补充通用 query 方法

**Files:**
- Modify: `src/service/domain/project/mod.rs`

### 设计说明

AgentManage 已有 `query(ctx, AgentQuery) -> Result<Vec<Agent>>`（`hr/mod.rs:162`），SkillManage 有 `query_skills`，ToolProviderManage 有 `query_tools`。ProjectManage 和 TaskManage 只有 `list(...)` 语法糖，**缺少通用 query 核心方法**。本任务补齐，明确 query 是核心、list 是语法糖。

### Step 1: Read 现有代码确认转换模式

Read `src/service/domain/project/mod.rs`，确认：
- `ProjectManage::list` impl 中 `Vec<ProjectPo>` → `Vec<Project>` 的转换方式（`po.into()` / `Project::from_po(po)` / `Project::new(po)`）
- `TaskManage::list` impl 中 `Vec<TaskPo>` → `Vec<Task>` 的转换方式
- `self.project_dao()` / `self.task_dao()` 访问方法名
- AgentManage 的 `query` 方法是 trait 默认方法（带 body）还是抽象方法

### Step 2: ProjectManage trait 新增 query

在 `src/service/domain/project/mod.rs` 的 `ProjectManage` trait 中添加（与 AgentManage 的 query 模式对齐）：

```rust
/// 通用查询（核心方法，支持 ids/keyword/status 等组合过滤）
///
/// 注：`list(...)` 是列表场景的语法糖，内部可调用此方法；
/// 需要更复杂组合过滤时，handler 应直接调用 `query`。
async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<Project>> {
    self.project_dao().query(ctx, query).await?
        .into_iter()
        .map(|po| po.into())  // 用 Step 1 确认的转换方式
        .collect()
}
```

注意：
- 如果 AgentManage 的 query 是 trait 默认方法（带 body），这里也用默认方法
- 如果是抽象方法，在 `impl ProjectManage for DomainImpl` 中实现
- import `ProjectQuery`：`use crate::service::dao::project::ProjectQuery;`

### Step 3: TaskManage trait 新增 query

同一个文件的 `TaskManage` trait 中添加：

```rust
/// 通用查询（核心方法，支持 ids/assignee/project/status 等组合过滤）
///
/// 注：`list(...)` 是列表场景的语法糖；复杂组合过滤应直接调用 `query`。
async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<Task>> {
    self.task_dao().query(ctx, query).await?
        .into_iter()
        .map(|po| po.into())  // 用 Step 1 确认的转换方式
        .collect()
}
```

注意：import `TaskQuery`：`use crate::service::dao::task::TaskQuery;`

### Step 4: 验证编译

Run: `cargo check 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/project/mod.rs
git commit -m "feat(domain): 为 ProjectManage/TaskManage 补充通用 query 核心方法"
```

---

## Task 3: 5 个 list handler 改走 query（修复 Agent 内存过滤）

**Files:**
- Modify: `src/handlers/hr/agent/list_agents.rs`
- Modify: `src/handlers/project/project/list_projects.rs`
- Modify: `src/handlers/project/task/list_tasks.rs`
- Modify: `src/handlers/finance/tool/list_tools.rs`
- Modify: `src/handlers/hr/skill/list_skills.rs`

### 设计说明

list handler 内部统一走 `domain().xxx_manage().query(ctx, XxxQuery { ... })`，根据 params 构造 Query。无分支——一个 query 调用处理所有过滤条件。原 `list(...)` 方法保留不动，handler 不再调用它。

### Step 1: Agent list_agents handler（修复内存过滤 bug）

Read `src/handlers/hr/agent/list_agents.rs`，确认现有转换方式（`a.into()` 或 `AgentListItem::from(&a)`）。

替换 handler 逻辑（移除内存 `.filter()`，统一走 query）：

```rust
pub async fn list_agents(ctx: RequestContext, params: ListAgentsRequest) -> Result<ListAgentsResponse> {
    // 统一走通用 query SQL 层过滤（修复原内存过滤 bug）
    // query 是核心查询方法，list_agents 是列表场景的语法糖
    let agents = domain().agent_manage().query(ctx, AgentQuery {
        status: params.status,
        exclude_status: Some(AgentStatus::Deleted),
        ids: params.ids,
        ..Default::default()
    }).await?;

    let items: Vec<AgentListItem> = agents.iter().map(|a| a.into()).collect();
    Ok(ListAgentsResponse { agents: items })
}
```

注意：import `AgentQuery`、`AgentStatus`（如未 import）。`a.into()` 用现有转换方式。

### Step 2: Project list_projects handler

Read `src/handlers/project/project/list_projects.rs`，确认 root_user_id 获取方式和转换方式。

替换为走 query：

```rust
pub async fn list_projects(ctx: RequestContext, params: ListProjectsRequest) -> Result<Vec<ProjectListItem>> {
    // 走通用 query（list 是语法糖，handler 内部统一用 query）
    let root_user_id = /* 原有获取逻辑 */;
    let projects = domain().project_manage().query(ctx, ProjectQuery {
        root_user_id: Some(root_user_id),
        status_in: params.status.map(|s| vec![s]),
        ids: params.ids,
        limit: params.limit,
        ..Default::default()
    }).await?;

    Ok(projects.iter().map(|p| p.into()).collect())
}
```

注意：import `ProjectQuery`。`p.into()` 用现有转换。

### Step 3: Task list_tasks handler

Read `src/handlers/project/task/list_tasks.rs`，确认现有参数转换（AssigneeType::from_i32 等）和 `to_list_item` 函数。

替换为走 query：

```rust
pub async fn list_tasks(ctx: RequestContext, params: ListTasksRequest) -> Result<Vec<TaskListItem>> {
    // 走通用 query
    let assignee_type = params.assignee_type.and_then(AssigneeType::from_i32);
    let status_in = params.status.and_then(TaskStatus::from_i32).map(|s| vec![s]);
    let tasks = domain().task_manage().query(ctx, TaskQuery {
        project_id: params.project_id,
        assignee_type,
        assignee_id: params.assignee_id,
        status_in,
        ids: params.ids,
        limit: params.limit,
        ..Default::default()
    }).await?;

    Ok(tasks.iter().map(|t| to_list_item(t)).collect())
}
```

注意：import `TaskQuery`。`to_list_item` 复用现有函数。

### Step 4: Tool list_tools handler

Read `src/handlers/finance/tool/list_tools.rs`。

Tool handler 已用 `query_tools(ctx, ToolQuery)` 模式，只需在 Query 构造补 `ids`：

```rust
pub async fn list_tools(ctx: RequestContext, params: ListToolsRequest) -> Result<ListToolsResponse> {
    let query = ToolQuery {
        agent_id: params.agent_id,
        keyword: params.keyword,
        enabled_only: params.only_enabled,
        ids: params.ids,
        exclude_status: Some(ToolStatus::Deleted),
        ..Default::default()
    };
    let tools = domain().tool_provider_manage().query_tools(ctx, query).await?;
    // ... 原有响应转换 ...
}
```

### Step 5: Skill list_skills handler

Read `src/handlers/hr/skill/list_skills.rs`。

Skill handler 已用 `query_skills(ctx, SkillQuery)` 模式，补 `ids`：

```rust
pub async fn list_skills(ctx: RequestContext, params: ListSkillsRequest) -> Result<ListSkillsResponse> {
    let query = SkillQuery {
        status: params.status,
        exclude_status: params.status.is_none().then_some(SkillStatus::Expired),
        category: params.category,
        author_id: params.author_id,
        keyword: params.keyword,
        limit: params.limit,
        ids: params.ids,
        ..Default::default()
    };
    let skills = domain().skill_manage().query_skills(ctx, query).await?;
    // ... 原有响应转换 ...
}
```

### Step 6: 验证编译

Run: `cargo check 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/handlers/hr/agent/list_agents.rs src/handlers/project/project/list_projects.rs src/handlers/project/task/list_tasks.rs src/handlers/finance/tool/list_tools.rs src/handlers/hr/skill/list_skills.rs
git commit -m "refactor(handler): 5 个 list handler 统一走 query 核心方法 + 修复 Agent 内存过滤"
```

---

## Task 4: 新增 5 个 QueryRequest DTO

**Files:**
- Modify: `common/src/api/agent.rs` (新增 AgentQueryRequest)
- Modify: `common/src/api/project.rs` (新增 ProjectQueryRequest)
- Modify: `common/src/api/task.rs` (新增 TaskQueryRequest)
- Modify: `common/src/api/tool.rs` (新增 ToolQueryRequest)
- Modify: `common/src/api/skill.rs` (新增 SkillQueryRequest)

### 设计说明

为 query 接口设计的 DTO，暴露完整 Query 能力。与 ListXxxRequest 的区别：ListXxxRequest 是列表场景的精简参数（GET query param），XxxQueryRequest 是完整查询能力（POST body）。

### Step 1: AgentQueryRequest

在 `common/src/api/agent.rs` 中新增（放在 ListAgentsResponse 之后）：

```rust
/// Agent 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct AgentQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索（名称/描述）
    pub keyword: Option<String>,
    /// 状态筛选
    pub status: Option<AgentStatus>,
    /// 创建者 ID
    pub created_by: Option<String>,
    /// 模型供应商 ID
    pub model_provider_id: Option<String>,
    /// 角色列表
    pub roles: Option<Vec<String>>,
    /// 返回数量限制
    pub limit: Option<usize>,
}
```

注意：无 `#[param(source = "query")]` 注解的字段默认为 body 字段（`#[generate_http_handler]` 宏行为）。

### Step 2: ProjectQueryRequest

在 `common/src/api/project.rs` 中新增：

```rust
/// Project 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ProjectQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 根用户 ID
    pub root_user_id: Option<String>,
    /// 状态列表（OR 语义）
    pub status_in: Option<Vec<ProjectStatus>>,
    /// 返回数量限制
    pub limit: Option<usize>,
}
```

### Step 3: TaskQueryRequest

在 `common/src/api/task.rs` 中新增：

```rust
/// Task 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct TaskQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 所属项目 ID
    pub project_id: Option<String>,
    /// 分配对象类型
    pub assignee_type: Option<AssigneeType>,
    /// 分配对象 ID
    pub assignee_id: Option<String>,
    /// 状态列表（OR 语义）
    pub status_in: Option<Vec<TaskStatus>>,
    /// 返回数量限制
    pub limit: Option<usize>,
}
```

### Step 4: ToolQueryRequest

在 `common/src/api/tool.rs` 中新增：

```rust
/// Tool 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ToolQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 绑定的 Agent ID
    pub agent_id: Option<String>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 协议类型
    pub protocol: Option<ToolProtocol>,
    /// 状态
    pub status: Option<ToolStatus>,
    /// MCP 服务器 ID
    pub mcp_server_id: Option<String>,
    /// 仅启用
    pub enabled_only: Option<bool>,
    /// 返回数量限制
    pub limit: Option<usize>,
    /// 偏移量
    pub offset: Option<usize>,
}
```

注意：需要确认 `ToolProtocol` 类型已 import。

### Step 5: SkillQueryRequest

在 `common/src/api/skill.rs` 中新增：

```rust
/// Skill 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SkillQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 状态
    pub status: Option<SkillStatus>,
    /// 分类
    pub category: Option<String>,
    /// 作者 ID
    pub author_id: Option<String>,
    /// 父技能 ID
    pub parent_skill_id: Option<String>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 返回数量限制
    pub limit: Option<usize>,
}
```

- [ ] **Step 6: 验证 common crate 编译**

Run: `cargo check -p common 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add common/src/api/agent.rs common/src/api/project.rs common/src/api/task.rs common/src/api/tool.rs common/src/api/skill.rs
git commit -m "feat(dto): 新增 5 个 QueryRequest DTO（通用查询接口入参）"
```

---

## Task 5: 新增 5 个 query handler + 路由注册

**Files:**
- Create: `src/handlers/hr/agent/query_agents.rs`
- Create: `src/handlers/project/project/query_projects.rs`
- Create: `src/handlers/project/task/query_tasks.rs`
- Create: `src/handlers/finance/tool/query_tools.rs`
- Create: `src/handlers/hr/skill/query_skills.rs`
- Modify: `src/handlers/hr/agent/mod.rs` (pub mod query_agents)
- Modify: `src/handlers/project/project/mod.rs` (pub mod query_projects)
- Modify: `src/handlers/project/task/mod.rs` (pub mod query_tasks)
- Modify: `src/handlers/finance/tool/mod.rs` (pub mod query_tools)
- Modify: `src/handlers/hr/skill/mod.rs` (pub mod query_skills)
- Modify: `src/router.rs` (注册 5 个新路由)

### Step 1: Read 现有 list handler 和 router.rs

Read 一个现有 list handler（如 `src/handlers/hr/agent/list_agents.rs`）和 `src/router.rs`，确认：
- `#[generate_http_handler]` 宏的使用方式
- handler 函数签名模式
- router.rs 路由注册模式（`.route("/agents", get(...))`）
- 是否有 POST 路由先例（`.route("/xxx", post(...))`）

### Step 2: query_agents handler

创建 `src/handlers/hr/agent/query_agents.rs`：

```rust
//! Agent 通用查询接口
//!
//! 与 list_agents 的区别：list 是列表场景语法糖（GET + query param），
//! query 是完整查询能力（POST + body），支持复杂组合过滤。

use axum::extract::Json;
use common::api::{AgentQueryRequest, AgentListItem, ListAgentsResponse, ApiError};
use crate::service::dao::agent::AgentQuery;
use crate::service::enums::AgentStatus;
// 其他 import 参考 list_agents.rs

#[generate_http_handler]
pub async fn query_agents(ctx: RequestContext, params: AgentQueryRequest) -> Result<ListAgentsResponse> {
    // 走通用 query 核心方法
    let agents = domain().agent_manage().query(ctx, AgentQuery {
        ids: params.ids,
        keyword: params.keyword,
        status: params.status,
        exclude_status: Some(AgentStatus::Deleted),
        created_by: params.created_by,
        model_provider_id: params.model_provider_id,
        roles: params.roles,
        limit: params.limit,
        ..Default::default()
    }).await?;

    let items: Vec<AgentListItem> = agents.iter().map(|a| a.into()).collect();
    Ok(ListAgentsResponse { agents: items })
}
```

注意：
- 参考现有 list_agents.rs 的 import 和宏使用方式
- `#[generate_http_handler]` 宏会根据 AgentQueryRequest 的字段注解（无 source 注解 = body）自动生成 `Json<AgentQueryRequest>` 提取器
- 确认 `domain()`、`RequestContext`、`Result` 等 import

### Step 3: query_projects handler

创建 `src/handlers/project/project/query_projects.rs`：

```rust
//! Project 通用查询接口

use common::api::{ProjectQueryRequest, ProjectListItem, ApiError};
use crate::service::dao::project::ProjectQuery;
// 其他 import 参考 list_projects.rs

#[generate_http_handler]
pub async fn query_projects(ctx: RequestContext, params: ProjectQueryRequest) -> Result<Vec<ProjectListItem>> {
    let projects = domain().project_manage().query(ctx, ProjectQuery {
        ids: params.ids,
        keyword: params.keyword,
        root_user_id: params.root_user_id,
        status_in: params.status_in,
        limit: params.limit,
        ..Default::default()
    }).await?;

    Ok(projects.iter().map(|p| p.into()).collect())
}
```

### Step 4: query_tasks handler

创建 `src/handlers/project/task/query_tasks.rs`：

```rust
//! Task 通用查询接口

use common::api::{TaskQueryRequest, TaskListItem, ApiError};
use crate::service::dao::task::TaskQuery;
// 其他 import 参考 list_tasks.rs

#[generate_http_handler]
pub async fn query_tasks(ctx: RequestContext, params: TaskQueryRequest) -> Result<Vec<TaskListItem>> {
    let tasks = domain().task_manage().query(ctx, TaskQuery {
        ids: params.ids,
        keyword: params.keyword,
        project_id: params.project_id,
        assignee_type: params.assignee_type,
        assignee_id: params.assignee_id,
        status_in: params.status_in,
        limit: params.limit,
        ..Default::default()
    }).await?;

    Ok(tasks.iter().map(|t| to_list_item(t)).collect())
}
```

注意：`to_list_item` 函数在 `src/handlers/project/task/response.rs`，确认 import。

### Step 5: query_tools handler

创建 `src/handlers/finance/tool/query_tools.rs`：

```rust
//! Tool 通用查询接口

use common::api::{ToolQueryRequest, /* 响应类型 */ ApiError};
use crate::service::dao::tool::ToolQuery;
use crate::service::enums::ToolStatus;
// 其他 import 参考 list_tools.rs

#[generate_http_handler]
pub async fn query_tools(ctx: RequestContext, params: ToolQueryRequest) -> Result<ListToolsResponse> {
    let query = ToolQuery {
        ids: params.ids,
        keyword: params.keyword,
        agent_id: params.agent_id,
        tags: params.tags,
        protocol: params.protocol,
        status: params.status,
        exclude_status: Some(ToolStatus::Deleted),
        mcp_server_id: params.mcp_server_id,
        enabled_only: params.enabled_only,
        limit: params.limit,
        offset: params.offset,
        ..Default::default()
    };
    let tools = domain().tool_provider_manage().query_tools(ctx, query).await?;
    // ... 响应转换参考 list_tools.rs ...
}
```

注意：参考 list_tools.rs 的响应类型和转换方式。

### Step 6: query_skills handler

创建 `src/handlers/hr/skill/query_skills.rs`：

```rust
//! Skill 通用查询接口

use common::api::{SkillQueryRequest, /* 响应类型 */ ApiError};
use crate::service::dao::skill::SkillQuery;
use crate::service::enums::SkillStatus;
// 其他 import 参考 list_skills.rs

#[generate_http_handler]
pub async fn query_skills(ctx: RequestContext, params: SkillQueryRequest) -> Result<ListSkillsResponse> {
    let query = SkillQuery {
        ids: params.ids,
        keyword: params.keyword,
        status: params.status,
        exclude_status: params.status.is_none().then_some(SkillStatus::Expired),
        category: params.category,
        author_id: params.author_id,
        parent_skill_id: params.parent_skill_id,
        tags: params.tags,
        limit: params.limit,
        ..Default::default()
    };
    let skills = domain().skill_manage().query_skills(ctx, query).await?;
    // ... 响应转换参考 list_skills.rs ...
}
```

### Step 7: 注册 mod 和路由

在 5 个 `mod.rs` 中添加 `pub mod query_xxx;`（参考现有 `pub mod list_xxx;`）。

在 `src/router.rs` 中注册 5 个新路由：

```rust
// Agent query
.route("/agents/query", post(handlers::hr::agent::query_agents::query_agents_handler))
// Project query
.route("/projects/query", post(handlers::project::project::query_projects::query_projects_handler))
// Task query
.route("/tasks/query", post(handlers::project::task::query_tasks::query_tasks_handler))
// Tool query
.route("/tools/query", post(handlers::finance::tool::query_tools::query_tools_handler))
// Skill query
.route("/skills/query", post(handlers::hr::skill::query_skills::query_skills_handler))
```

注意：
- 确认 router.rs 顶部已 `use axum::routing::{get, post, ...}`（如 post 未 import）
- handler 函数名由 `#[generate_http_handler]` 宏生成，通常是 `query_agents_handler`（函数名 + `_handler`）

### Step 8: 验证编译

Run: `cargo check 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add src/handlers/hr/agent/query_agents.rs src/handlers/hr/agent/mod.rs src/handlers/project/project/query_projects.rs src/handlers/project/project/mod.rs src/handlers/project/task/query_tasks.rs src/handlers/project/task/mod.rs src/handlers/finance/tool/query_tools.rs src/handlers/finance/tool/mod.rs src/handlers/hr/skill/query_skills.rs src/handlers/hr/skill/mod.rs src/router.rs
git commit -m "feat(handler): 新增 5 个 query handler + POST /{entity}/query 路由"
```

---

## Task 6: 后端测试验证

- [ ] **Step 1: 运行后端全量测试**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 所有测试 PASS（746+ tests）

- [ ] **Step 2: 如有测试因新字段失败，修复构造**

搜索测试中构造 `ListXxxRequest` 的位置，补齐 `ids: None` 字段。

- [ ] **Step 3: Commit（如有修复）**

```bash
git add -A
git commit -m "test: 修复因新增 ids 字段导致的测试构造"
```

---

## Task 7: 前端 API 函数加 ids 参数

**Files:**
- Modify: `frontend/src/api/hr.rs` (list_agents)
- Modify: `frontend/src/api/project.rs` (list_projects, list_tasks)
- Modify: `frontend/src/api/tool.rs` (list_tools，如有)
- Modify: `frontend/src/api/skill.rs` (list_skills，如有)

### Step 1: Read 现有前端 API 文件

Read 这 4 个文件，确认：
- 现有 `list_agents` / `list_projects` / `list_tasks` / `list_tools` / `list_skills` 函数签名
- HTTP 客户端类型和 query param 构造方式

### Step 2: list_agents 新增 ids 参数

根据现有实现模式，新增 `ids: Option<&[String]>` 参数，构造重复 query param：

```rust
pub async fn list_agents(ids: Option<&[String]>) -> Result<ListAgentsResponse, ApiError> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(ids) = ids {
        for id in ids {
            params.push(("ids", id.clone()));
        }
    }
    // 用 params 构造 query string 发送 GET /api/v1/agents
    // ... 原有解析逻辑 ...
}
```

### Step 3: list_projects 新增 ids 参数

```rust
pub async fn list_projects(ids: Option<&[String]>) -> Result<Vec<ProjectListItem>, ApiError> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(ids) = ids {
        for id in ids { params.push(("ids", id.clone())); }
    }
    // ... 原有请求逻辑 ...
}
```

### Step 4: list_tasks 新增 ids 参数

`list_tasks` 已有 4 个参数，追加 ids：

```rust
pub async fn list_tasks(
    project_id: Option<&str>,
    status: Option<i32>,
    assignee_id: Option<&str>,
    assignee_type: Option<i32>,
    ids: Option<&[String]>,
) -> Result<ListTasksResponse, ApiError> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(v) = project_id { params.push(("project_id", v.to_string())); }
    if let Some(v) = status { params.push(("status", v.to_string())); }
    if let Some(v) = assignee_id { params.push(("assignee_id", v.to_string())); }
    if let Some(v) = assignee_type { params.push(("assignee_type", v.to_string())); }
    if let Some(ids) = ids {
        for id in ids { params.push(("ids", id.clone())); }
    }
    // ... 原有请求逻辑 ...
}
```

### Step 5: list_tools 和 list_skills（如有前端调用）

检查 `frontend/src/api/tool.rs` 和 `frontend/src/api/skill.rs` 是否存在 `list_tools` / `list_skills`。如存在，按相同模式添加 `ids: Option<&[String]>` 参数。如不存在，跳过。

### Step 6: 更新现有调用方

搜索 `list_agents(` 和 `list_tasks(` 的所有调用点，补充 `None` 或具体 ids 参数。特别注意：
- `frontend/src/pages/workspace.rs`（如有调用）
- 其他页面如有调用

### Step 7: 验证前端编译**

Run: `cd frontend && cargo check 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/api/project.rs frontend/src/api/tool.rs frontend/src/api/skill.rs frontend/src/pages/
git commit -m "feat(frontend-api): list_* 函数新增 ids 批量查询参数"
```

---

## Task 8: 前端 3 个详情页改用批量查询

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`
- Modify: `frontend/src/pages/project/project_detail.rs`
- Modify: `frontend/src/pages/project/task_detail.rs`

### Step 1: Agent 详情页

将 `agent_detail.rs` 中逐个 `get_project` 循环替换为单次 `list_projects(Some(&pid_vec))` 批量调用：

```rust
// 新代码（批量查询）:
let pid_vec: Vec<String> = project_ids.into_iter().collect();
if pid_vec.is_empty() {
    graph_projects.set(Vec::new());
} else {
    match list_projects(Some(&pid_vec)).await {
        Ok(projects) => graph_projects.set(projects),
        Err(e) => toast.error(&format!("批量获取项目失败: {}", e)),
    }
}
```

注意：`list_projects` 返回 `Vec<ProjectListItem>`，不需要 `From` 转换。删除不再需要的 `get_project` import。

### Step 2: Project 详情页

将 `project_detail.rs` 中逐个 `get_agent` 循环替换为单次 `list_agents(Some(&aid_vec))` 批量调用：

```rust
// 新代码（批量查询）:
let aid_vec: Vec<String> = assignee_ids.into_iter().collect();
if aid_vec.is_empty() {
    graph_agents.set(Vec::new());
} else {
    match list_agents(Some(&aid_vec)).await {
        Ok(resp) => graph_agents.set(resp.agents),
        Err(e) => toast.error(&format!("批量获取 Agent 失败: {}", e)),
    }
}
```

注意：`list_agents` 返回 `ListAgentsResponse { agents }`，需要 `.agents`。`graph_projects` 保持不变（从 project_data 构造单个）。删除不再需要的 `get_agent` import。

### Step 3: Task 详情页

将 `task_detail.rs` 中单个 `get_agent` 和 `get_project` 替换为批量调用（虽只有 1 个元素，但统一接口）：

```rust
if assignee_type_for_graph == 1 {
    let ids = vec![assignee_id_for_graph.clone()];
    match list_agents(Some(&ids)).await {
        Ok(resp) => {
            if let Some(a) = resp.agents.into_iter().next() {
                graph_agents.set(vec![a]);
            }
        }
        Err(e) => toast.error(&format!("获取 Agent 失败: {}", e)),
    }
}

if let Some(pid) = &pid_for_graph {
    let ids = vec![pid.clone()];
    match list_projects(Some(&ids)).await {
        Ok(projects) => {
            if let Some(p) = projects.into_iter().next() {
                graph_projects.set(vec![p]);
            }
        }
        Err(e) => toast.error(&format!("获取 Project 失败: {}", e)),
    }
}
```

### Step 4: 清理 import

3 个文件中：
- 删除 `get_agent`、`get_project` 的 import（如果不再使用）
- 添加 `list_agents`/`list_projects` 的 import
- 保留 `AgentListItem::from` / `ProjectListItem::from`（Project/Agent 详情页从自身数据构造 graph_* 时仍需要）

### Step 5: 验证前端编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/hr/agent_detail.rs frontend/src/pages/project/project_detail.rs frontend/src/pages/project/task_detail.rs
git commit -m "perf(frontend): 3 个详情页改用批量查询消除 N+1 请求"
```

---

## Task 9: 最终验证 + 推送

- [ ] **Step 1: 后端全量测试**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: 所有测试 PASS

- [ ] **Step 2: 前端全量测试**

Run: `cd frontend && cargo test 2>&1 | tail -5`
Expected: 所有测试 PASS

- [ ] **Step 3: 前端 release build**

Run: `cd frontend && cargo build --release 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 4: 推送到远程**

```bash
git push
```

---

## Self-Review

### Spec coverage
- ✅ 5 个实体的 list 接口支持 ids 批量查询 — Task 1 (DTO) + Task 3 (handler 走 query)
- ✅ 5 个实体的 query 接口补齐 — Task 4 (DTO) + Task 5 (handler + 路由)
- ✅ Agent handler 内存过滤修复 — Task 3 Step 1（统一走 query SQL 层）
- ✅ Tool DTO 注解缺失修复 — Task 1 Step 1
- ✅ Domain 层 query 核心方法补齐 — Task 2（Project/Task）
- ✅ 前端 N+1 消除 — Task 8
- ✅ 测试验证 — Task 6 + Task 9

### 设计原则对齐
- ✅ **query 是核心，list 是语法糖**：Domain 层 query 是核心方法（注释明确），list 保留不动
- ✅ **list handler 内部走 query**：list 是语法糖，调用核心 query 方法
- ✅ **query 接口面向未来**：POST + body，暴露完整 Query 能力，当前无前端调用方
- ✅ **现有 list 方法签名不动**：作为语法糖保留，其他调用方不受影响

### Type consistency
- Domain: `query(ctx, ProjectQuery) -> Result<Vec<Project>>` 与 AgentManage 模式一致
- DTO: `ListXxxRequest.ids: Option<Vec<String>>` (GET query) vs `XxxQueryRequest.ids: Option<Vec<String>>` (POST body)
- 前端: `list_xxx(ids: Option<&[String]>)` 签名一致
- 路由: `GET /api/v1/{entity}` (list) + `POST /api/v1/{entity}/query` (query)
