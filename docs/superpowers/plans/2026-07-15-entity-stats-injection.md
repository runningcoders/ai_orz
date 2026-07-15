# 实体统计数据动态注入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 5 个核心实体（Agent/Project/Task/Tool/ModelProvider）扩展 FetchOptions 机制，通过 GET 实体详情接口的 query 参数按需注入统计数据，前端打开详情页时一次请求拿到实体+统计。

**Architecture:** 复用 Agent 已有的 `AgentFetchOptions.with_stats` 模式，为 Project/Task/Tool/ModelProvider 补齐 FetchOptions + `get_xxx(ctx, id, options)` DAL 方法。Domain 层 `get` 方法签名扩展为接受 options。Handler 层在现有 GET 接口上增加 `with_stats`/`with_model_call_stats`/`stats_time_start`/`stats_time_end`/`stats_interval` query 参数，Response DTO 增加 `Option<Stats>` 字段。DAL 层内部调用已有的 `get_stats()`/`get_model_call_stats()` 组装。

**Tech Stack:** Rust, Axum, sqlx, schemars (JsonSchema), ai-orz-macros (generate_http_handler + Params derive)

---

## File Structure

### 修改文件清单

| 层 | 文件 | 改动 |
|----|------|------|
| **Common Model** | `common/src/models/stats.rs` | 所有 stats 类型加 `JsonSchema` derive |
| **Common API** | `common/src/api/agent.rs` | `GetAgentRequest` 加 query 参数，`GetAgentResponse` 加 stats 字段 |
| **Common API** | `common/src/api/project.rs` | `GetProjectRequest` 加 query 参数，`GetProjectResponse` 加 stats 字段 |
| **Common API** | `common/src/api/task.rs` | `GetTaskRequest` 加 query 参数，`GetTaskResponse` 加 stats 字段 |
| **Common API** | `common/src/api/tool.rs` | `GetToolRequest` 加 query 参数，`GetToolResponse` 加 stats 字段 |
| **Common API** | `common/src/api/model_provider.rs` | `GetModelProviderRequest` 加 query 参数，`GetModelProviderResponse` 加 stats 字段 |
| **Model** | `src/models/agent.rs` | Agent 加 `model_call_stats` 字段 |
| **Model** | `src/models/project.rs` | Project 加 `stats` + `model_call_stats` 字段 |
| **Model** | `src/models/task.rs` | Task 加 `stats` + `model_call_stats` 字段 |
| **Model** | `src/models/tool.rs` | Tool 加 `stats` 字段 |
| **Model** | `src/models/model_provider.rs` | ModelProvider 加 `stats` 字段 |
| **DAL** | `src/service/dal/agent.rs` | AgentFetchOptions 扩展，`get_agent` 实现扩展 |
| **DAL** | `src/service/dal/project.rs` | 新增 `ProjectFetchOptions` + `get_project(ctx, id, options)` |
| **DAL** | `src/service/dal/task.rs` | 新增 `TaskFetchOptions` + `get_task(ctx, id, options)` |
| **DAL** | `src/service/dal/tool.rs` | 新增 `ToolFetchOptions` + `get_tool(ctx, id, options)` |
| **DAL** | `src/service/dal/model_provider.rs` | 新增 `ModelProviderFetchOptions` + `get_model_provider(ctx, id, options)` |
| **Domain** | `src/service/domain/project/mod.rs` | `ProjectManage::get` + `TaskManage::get` 签名扩展 |
| **Domain** | `src/service/domain/project/project.rs` | 实现适配 |
| **Domain** | `src/service/domain/project/task.rs` | 实现适配 |
| **Domain** | `src/service/domain/finance/mod.rs` | `ToolProviderManage::get_tool` + `ModelProviderManage::get_model_provider` 签名扩展 |
| **Domain** | `src/service/domain/finance/tool_provider.rs` | 实现适配 |
| **Domain** | `src/service/domain/finance/model_provider.rs` | 实现适配 |
| **Handler** | `src/handlers/hr/agent/get_agent.rs` | 构建 options，映射 stats 到响应 |
| **Handler** | `src/handlers/project/project/get_project.rs` + `response.rs` | 构建 options，映射 stats |
| **Handler** | `src/handlers/project/task/get_task.rs` + `response.rs` | 构建 options，映射 stats |
| **Handler** | `src/handlers/finance/tool/get_tool.rs` + `response.rs` | 构建 options，映射 stats |
| **Handler** | `src/handlers/finance/model_provider/get_model_provider.rs` | 构建 options，映射 stats |

---

## Task 1: Foundation — Stats 类型加 JsonSchema

**Files:**
- Modify: `common/src/models/stats.rs`

**原因：** Response DTO 需要派生 `JsonSchema`，内嵌的 stats 类型也需要 `JsonSchema` 才能生成 OpenAPI schema。

- [ ] **Step 1: 修改 stats.rs，为所有类型加 JsonSchema derive**

在 `common/src/models/stats.rs` 文件顶部，将 `use serde::{Deserialize, Serialize};` 改为：

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
```

然后为以下所有结构体/枚举的 derive 列表添加 `JsonSchema`：

- `StatsInterval`（枚举，约第 8 行）
- `TimeSeriesPoint`（约第 17 行）
- `TokenSumResult`（约第 30 行）
- `CallSummary`（约第 46 行）
- `StatsFetchOptions`（约第 59 行）— 注意：此类型不需要 Serialize/Deserialize/JsonSchema，它是内部使用的，跳过
- `AgentStats`（约第 85 行）
- `ProjectStats`（约第 92 行）
- `TaskStats`（约第 98 行）
- `ToolStats`（约第 106 行）
- `ModelCallStats`（约第 118 行）

示例（以 `AgentStats` 为例）：
```rust
// 修改前
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentStats {

// 修改后
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct AgentStats {
```

对 `StatsInterval`：
```rust
// 修改前
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatsInterval {

// 修改后
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub enum StatsInterval {
```

对 `CallSummary`：
```rust
// 修改前
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallSummary {

// 修改后
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct CallSummary {
```

对 `TimeSeriesPoint`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TimeSeriesPoint {
```

对 `TokenSumResult`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TokenSumResult {
```

对 `ProjectStats`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ProjectStats {
```

对 `TaskStats`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct TaskStats {
```

对 `ToolStats`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ToolStats {
```

对 `ModelCallStats`：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema)]
pub struct ModelCallStats {
```

- [ ] **Step 2: 编译验证**

Run: `cargo check --lib`
Expected: 编译通过，无错误

- [ ] **Step 3: Commit**

```bash
git add common/src/models/stats.rs
git commit -m "refactor: add JsonSchema derive to all stats types for API DTO embedding"
```

---

## Task 2: Agent — 扩展现有模式，增加 model_call_stats

Agent 已有 `AgentFetchOptions` + `get_agent(ctx, id, options)` + `stats: Option<AgentStats>`。本任务扩展增加 `model_call_stats`。

**Files:**
- Modify: `src/models/agent.rs`
- Modify: `src/service/dal/agent.rs`
- Modify: `common/src/api/agent.rs`
- Modify: `src/handlers/hr/agent/get_agent.rs`

- [ ] **Step 1: Agent 实体加 model_call_stats 字段**

在 `src/models/agent.rs` 中：

1) 在文件顶部 import 添加：
```rust
use common::models::ModelCallStats;
```

2) 在 `Agent` 结构体（约第 133 行）的 `stats` 字段后添加：
```rust
    /// 统计数据（由 DAL 层按需注入）
    ///
    /// None 表示未查询
    pub stats: Option<AgentStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    ///
    /// None 表示未查询
    pub model_call_stats: Option<ModelCallStats>,
```

3) 在 `Debug` impl（约第 160 行）中添加 `model_call_stats` 字段：
```rust
            .field("stats", &self.stats)
            .field("model_call_stats", &self.model_call_stats)
```

4) 在 `from_po` 方法（约第 174 行）中添加：
```rust
            stats: None,
            model_call_stats: None,
```

- [ ] **Step 2: 扩展 AgentFetchOptions**

在 `src/service/dal/agent.rs` 中，扩展 `AgentFetchOptions`（约第 64 行）：

```rust
#[derive(Debug, Clone, Default)]
pub struct AgentFetchOptions {
    /// 是否加载运行时状态（默认 true）
    pub with_runtime_state: Option<bool>,
    /// 是否加载统计信息（AgentStats: 唤醒次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计过滤条件（with_stats=true 时生效，按任务 ID 过滤）
    pub stats_task_id: Option<String>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}
```

需要在文件顶部添加 import：
```rust
use common::models::StatsInterval;
```

- [ ] **Step 3: 扩展 get_agent DAL 实现**

在 `src/service/dal/agent.rs` 的 `get_agent` 实现（约第 288 行）中，在现有 `with_stats` 块之后添加 `with_model_call_stats` 块：

```rust
        if options.with_model_call_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: true,
                with_time_series: true,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let model_call_stats = self.get_model_call_stats(ctx.clone(), id, stats_options).await?;
            agent.model_call_stats = Some(model_call_stats);
        }
```

同时，更新现有 `with_stats` 块中的 `StatsFetchOptions` 构造，使其也使用 `options.stats_time_range` 和 `options.stats_interval`：

```rust
        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            // ... 其余不变
```

- [ ] **Step 4: GetAgentRequest 加 query 参数**

在 `common/src/api/agent.rs` 中，扩展 `GetAgentRequest`（约第 60 行）：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（唤醒次数汇总）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（token + 时序趋势）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}
```

- [ ] **Step 5: GetAgentResponse 加 stats 字段**

在 `common/src/api/agent.rs` 中，扩展 `GetAgentResponse`（约第 68 行），在 `tools` 字段后添加：

```rust
    /// 已绑定的工具 ID 列表
    pub tools: Vec<String>,
    /// Agent 自身统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<common::models::AgentStats>,
    /// 模型调用统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_call_stats: Option<common::models::ModelCallStats>,
```

- [ ] **Step 6: 更新 get_agent handler**

在 `src/handlers/hr/agent/get_agent.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
use crate::service::dal::agent::AgentFetchOptions;
```

2) 替换 handler 中的 `Default::default()` 为构建 options：

```rust
pub async fn get_agent(
    ctx: RequestContext,
    params: GetAgentRequest,
) -> Result<GetAgentResponse> {
    let options = AgentFetchOptions {
        with_stats: params.with_stats,
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
        ..Default::default()
    };

    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;
```

3) 在响应构造中添加 stats 字段（在 `tools,` 之后）：

```rust
        tools,
        stats: agent.stats,
        model_call_stats: agent.model_call_stats,
    })
```

- [ ] **Step 7: 编译验证**

Run: `cargo check --lib`
Expected: 编译通过

- [ ] **Step 8: 运行测试**

Run: `cargo test --lib`
Expected: 全部测试通过（现有 697 个测试不应被破坏）

- [ ] **Step 9: Commit**

```bash
git add src/models/agent.rs src/service/dal/agent.rs common/src/api/agent.rs src/handlers/hr/agent/get_agent.rs
git commit -m "feat: extend Agent with model_call_stats injection via FetchOptions"
```

---

## Task 3: Project — 新建 FetchOptions 模式（参考实现）

**Files:**
- Modify: `src/models/project.rs`
- Modify: `src/service/dal/project.rs`
- Modify: `src/service/domain/project/mod.rs`
- Modify: `src/service/domain/project/project.rs`
- Modify: `common/src/api/project.rs`
- Modify: `src/handlers/project/project/get_project.rs`
- Modify: `src/handlers/project/project/response.rs`

- [ ] **Step 1: Project 实体加 stats 字段**

在 `src/models/project.rs` 中：

1) 在文件顶部 import 添加：
```rust
use common::models::{ModelCallStats, ProjectStats};
```

2) 在 `Project` 结构体（约第 57 行）中添加字段：
```rust
pub struct Project {
    /// 底层持久化对象
    pub po: ProjectPo,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<crate::models::vector::SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<ProjectStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub model_call_stats: Option<ModelCallStats>,
}
```

3) 更新 `from_po`（约第 66 行）：
```rust
    pub fn from_po(po: ProjectPo) -> Self {
        Self { po, search_match: None, stats: None, model_call_stats: None }
    }
```

4) 更新 `new` 方法（约第 71 行）中的返回值：
```rust
        Self {
            po: ProjectPo::new(...),
            search_match: None,
            stats: None,
            model_call_stats: None,
        }
```

- [ ] **Step 2: 创建 ProjectFetchOptions + get_project DAL 方法**

在 `src/service/dal/project.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
```

2) 在 `ProjectDal` trait 定义之前（约第 69 行之前）添加：
```rust
/// Project 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ProjectFetchOptions {
    /// 是否加载统计信息（ProjectStats: 事件次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}
```

3) 在 `ProjectDal` trait 中，在 `find_by_id` 方法之后添加新方法：
```rust
    /// 根据 ID 获取项目（带附带信息选项）
    async fn get_project(&self, ctx: RequestContext, id: &str, options: ProjectFetchOptions) -> Result<Option<Project>>;
```

4) 在 `ProjectDalImpl` 的 impl 块中添加实现：
```rust
    async fn get_project(&self, ctx: RequestContext, id: &str, options: ProjectFetchOptions) -> Result<Option<Project>> {
        let opt = self.project_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut project) = opt.map(Project::from_po) else {
            return Ok(None);
        };

        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let stats = self.get_stats(ctx.clone(), id, stats_options).await?;
            project.stats = Some(stats);
        }

        if options.with_model_call_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: true,
                with_time_series: true,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let model_call_stats = self.get_model_call_stats(ctx.clone(), id, stats_options).await?;
            project.model_call_stats = Some(model_call_stats);
        }

        Ok(Some(project))
    }
```

- [ ] **Step 3: 更新 ProjectDomain trait 签名**

在 `src/service/domain/project/mod.rs` 中，修改 `ProjectManage::get` 方法签名（约第 94 行）：

```rust
    /// 根据 ID 获取项目
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>>;

    /// 根据 ID 获取项目（带附带信息选项）
    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::project::ProjectFetchOptions,
    ) -> Result<Option<Project>>;
```

保留原 `get` 方法不变（向后兼容），新增 `get_project` 方法。

- [ ] **Step 4: 实现 ProjectDomain::get_project**

在 `src/service/domain/project/project.rs` 中，在现有 `get` 方法之后添加：

```rust
    /// 根据 ID 获取项目（带附带信息选项）
    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::project::ProjectFetchOptions,
    ) -> Result<Option<Project>> {
        self.project_dal.get_project(ctx, id, options).await
    }
```

- [ ] **Step 5: GetProjectRequest 加 query 参数**

在 `common/src/api/project.rs` 中，扩展 `GetProjectRequest`（约第 25 行）：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetProjectRequest {
    /// Project ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（事件次数汇总）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（token + 时序趋势）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}
```

- [ ] **Step 6: GetProjectResponse 加 stats 字段**

在 `common/src/api/project.rs` 中，扩展 `GetProjectResponse`（约第 71 行），在 `updated_at` 字段后添加：

```rust
    /// 更新时间戳
    pub updated_at: i64,
    /// 项目统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<common::models::ProjectStats>,
    /// 模型调用统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_call_stats: Option<common::models::ModelCallStats>,
```

- [ ] **Step 7: 更新 response.rs 的 to_detail**

在 `src/handlers/project/project/response.rs` 中，更新 `to_detail` 函数：

```rust
pub(super) fn to_detail(project: &Project) -> GetProjectResponse {
    GetProjectResponse {
        id: project.po.id.clone(),
        name: project.po.name.clone(),
        description: optional_string(&project.po.description),
        workflow: project.po.workflow.clone(),
        guidance: project.po.guidance.clone(),
        status: project.po.status as i32,
        priority: project.po.priority,
        tags: project.po.get_tags(),
        root_user_id: project.po.root_user_id.clone(),
        owner_agent_id: project.po.owner_agent_id.clone(),
        start_at: project.po.start_at,
        due_at: project.po.due_at,
        end_at: project.po.end_at,
        created_at: project.po.created_at,
        updated_at: project.po.updated_at,
        stats: project.stats.clone(),
        model_call_stats: project.model_call_stats.clone(),
    }
}
```

- [ ] **Step 8: 更新 get_project handler**

在 `src/handlers/project/project/get_project.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
use crate::service::dal::project::ProjectFetchOptions;
```

2) 替换 handler 逻辑：

```rust
pub async fn get_project(
    ctx: RequestContext,
    params: GetProjectRequest,
) -> Result<GetProjectResponse> {
    let options = ProjectFetchOptions {
        with_stats: params.with_stats,
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
    };

    let project = domain()
        .project_manage()
        .get_project(ctx, &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Project {} not found", params.id)))?;

    Ok(response::to_detail(&project))
}
```

- [ ] **Step 9: 编译验证**

Run: `cargo check --lib`
Expected: 编译通过

- [ ] **Step 10: 运行测试**

Run: `cargo test --lib`
Expected: 全部测试通过

- [ ] **Step 11: Commit**

```bash
git add src/models/project.rs src/service/dal/project.rs src/service/domain/project/mod.rs src/service/domain/project/project.rs common/src/api/project.rs src/handlers/project/project/get_project.rs src/handlers/project/project/response.rs
git commit -m "feat: add ProjectFetchOptions with stats injection on get_project"
```

---

## Task 4: Task — 跟随 Project 模式

**Files:**
- Modify: `src/models/task.rs`
- Modify: `src/service/dal/task.rs`
- Modify: `src/service/domain/project/mod.rs`
- Modify: `src/service/domain/project/task.rs`
- Modify: `common/src/api/task.rs`
- Modify: `src/handlers/project/task/get_task.rs`
- Modify: `src/handlers/project/task/response.rs`

- [ ] **Step 1: Task 实体加 stats 字段**

在 `src/models/task.rs` 中：

1) 在文件顶部 import 添加：
```rust
use common::models::{ModelCallStats, TaskStats};
```

2) 在 `Task` 结构体（约第 64 行）中添加字段：
```rust
pub struct Task {
    /// 底层持久化对象
    pub po: TaskPo,
    /// 搜索匹配元信息（搜索场景下由 DAL 层填充）
    pub search_match: Option<SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<TaskStats>,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub model_call_stats: Option<ModelCallStats>,
}
```

3) 更新 `from_po`（约第 73 行）：
```rust
    pub fn from_po(po: TaskPo) -> Self {
        Self {
            po,
            search_match: None,
            stats: None,
            model_call_stats: None,
        }
    }
```

4) 更新 `new` 方法中的返回值，添加 `stats: None, model_call_stats: None`

- [ ] **Step 2: 创建 TaskFetchOptions + get_task DAL 方法**

在 `src/service/dal/task.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
```

2) 在 `TaskDal` trait 定义之前添加：
```rust
/// Task 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct TaskFetchOptions {
    /// 是否加载统计信息（TaskStats: 事件次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}
```

3) 在 `TaskDal` trait 中，`find_by_id` 之后添加：
```rust
    /// 根据 ID 获取任务（带附带信息选项）
    async fn get_task(&self, ctx: RequestContext, id: &str, options: TaskFetchOptions) -> Result<Option<Task>>;
```

4) 在 impl 块中添加实现（与 Project 的 get_project 完全同构）：
```rust
    async fn get_task(&self, ctx: RequestContext, id: &str, options: TaskFetchOptions) -> Result<Option<Task>> {
        let opt = self.task_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut task) = opt.map(Task::from_po) else {
            return Ok(None);
        };

        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let stats = self.get_stats(ctx.clone(), id, stats_options).await?;
            task.stats = Some(stats);
        }

        if options.with_model_call_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: true,
                with_time_series: true,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let model_call_stats = self.get_model_call_stats(ctx.clone(), id, stats_options).await?;
            task.model_call_stats = Some(model_call_stats);
        }

        Ok(Some(task))
    }
```

- [ ] **Step 3: 更新 TaskDomain trait 签名**

在 `src/service/domain/project/mod.rs` 中，在 `TaskManage` trait 的 `get` 方法之后添加新方法（约第 196 行之后）：

```rust
    /// 根据 ID 获取任务
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>>;

    /// 根据 ID 获取任务（带附带信息选项）
    async fn get_task(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::task::TaskFetchOptions,
    ) -> Result<Option<Task>>;
```

- [ ] **Step 4: 实现 TaskDomain::get_task**

在 `src/service/domain/project/task.rs` 中，在现有 `get` 方法之后添加：

```rust
    /// 根据 ID 获取任务（带附带信息选项）
    async fn get_task(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::task::TaskFetchOptions,
    ) -> Result<Option<Task>> {
        self.task_dal.get_task(ctx, id, options).await
    }
```

- [ ] **Step 5: GetTaskRequest 加 query 参数**

在 `common/src/api/task.rs` 中，扩展 `GetTaskRequest`（约第 37 行）：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetTaskRequest {
    /// Task ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（事件次数汇总）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（token + 时序趋势）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}
```

- [ ] **Step 6: GetTaskResponse 加 stats 字段**

在 `common/src/api/task.rs` 中，扩展 `GetTaskResponse`（约第 125 行），在 `updated_at` 字段后添加：

```rust
    /// 更新时间戳
    pub updated_at: i64,
    /// 任务统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<common::models::TaskStats>,
    /// 模型调用统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_call_stats: Option<common::models::ModelCallStats>,
```

- [ ] **Step 7: 更新 response.rs 的 to_detail**

在 `src/handlers/project/task/response.rs` 中，更新 `to_detail` 函数，在末尾 `updated_at` 之后添加：

```rust
        updated_at: task.po.updated_at,
        stats: task.stats.clone(),
        model_call_stats: task.model_call_stats.clone(),
    }
}
```

- [ ] **Step 8: 更新 get_task handler**

在 `src/handlers/project/task/get_task.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
use crate::service::dal::task::TaskFetchOptions;
```

2) 替换 handler 逻辑：

```rust
pub async fn get_task(
    ctx: RequestContext,
    params: GetTaskRequest,
) -> Result<GetTaskResponse> {
    let options = TaskFetchOptions {
        with_stats: params.with_stats,
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
    };

    let task = domain()
        .task_manage()
        .get_task(ctx, &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    Ok(response::to_detail(&task))
}
```

- [ ] **Step 9: 编译验证**

Run: `cargo check --lib`
Expected: 编译通过

- [ ] **Step 10: 运行测试**

Run: `cargo test --lib`
Expected: 全部测试通过

- [ ] **Step 11: Commit**

```bash
git add src/models/task.rs src/service/dal/task.rs src/service/domain/project/mod.rs src/service/domain/project/task.rs common/src/api/task.rs src/handlers/project/task/get_task.rs src/handlers/project/task/response.rs
git commit -m "feat: add TaskFetchOptions with stats injection on get_task"
```

---

## Task 5: Tool — 简化模式（无 model_call_stats）

Tool 没有模型调用统计，只有 `ToolStats`（call_summary + failed_count）。

**Files:**
- Modify: `src/models/tool.rs`
- Modify: `src/service/dal/tool.rs`
- Modify: `src/service/domain/finance/mod.rs`
- Modify: `src/service/domain/finance/tool_provider.rs`
- Modify: `common/src/api/tool.rs`
- Modify: `src/handlers/finance/tool/get_tool.rs`
- Modify: `src/handlers/finance/tool/response.rs`

- [ ] **Step 1: Tool 实体加 stats 字段**

在 `src/models/tool.rs` 中：

1) 在文件顶部 import 添加：
```rust
use common::models::ToolStats;
```

2) 在 `Tool` 结构体（约第 171 行）中添加字段：
```rust
pub struct Tool {
    /// Persistent metadata from DB
    pub po: ToolPo,
    /// Our core interface tool
    pub our_tool: Box<dyn CoreTool + Send + Sync>,
    /// ✅ 搜索匹配元信息（可选）
    pub search_match: Option<crate::models::vector::SearchMatchInfo>,
    /// 统计数据（由 DAL 层按需注入）
    pub stats: Option<ToolStats>,
}
```

3) 更新 `Debug` impl（约第 183 行）：
```rust
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("po", &self.po)
            .field("our_tool", &format_args!("Box<dyn CoreTool + Send + Sync>"))
            .field("stats", &self.stats)
            .finish()
    }
```

**注意：** Tool 的 `from_po` 不存在（Tool 由 DAL 的 `get_by_id` 创建，需要 `our_tool` 字段）。需要在 DAL 创建 Tool 的地方添加 `stats: None`。搜索项目中所有创建 `Tool {` 的位置，全部添加 `stats: None`。

- [ ] **Step 2: 创建 ToolFetchOptions + get_tool DAL 方法**

在 `src/service/dal/tool.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
```

2) 在 `ToolDal` trait 定义之前添加：
```rust
/// Tool 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ToolFetchOptions {
    /// 是否加载统计信息（ToolStats: 调用次数 + 失败次数）
    pub with_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}
```

3) 在 `ToolDal` trait 中，`get_by_id` 之后添加：
```rust
    /// 根据 ID 获取完整工具（带附带信息选项）
    async fn get_tool(&self, ctx: RequestContext, id: String, options: ToolFetchOptions) -> Result<Option<Tool>>;
```

4) 在 impl 块中添加实现：
```rust
    async fn get_tool(&self, ctx: RequestContext, id: String, options: ToolFetchOptions) -> Result<Option<Tool>> {
        let opt = self.get_by_id(ctx.clone(), id).await?;
        let Some(mut tool) = opt else {
            return Ok(None);
        };

        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let stats = self.get_stats(ctx.clone(), &tool.po.id, stats_options).await?;
            tool.stats = Some(stats);
        }

        Ok(Some(tool))
    }
```

- [ ] **Step 3: 更新 ToolProviderManage trait 签名**

在 `src/service/domain/finance/mod.rs` 中，在 `ToolProviderManage` trait 的 `get_tool` 方法之后添加新方法：

```rust
    /// 获取 Tool
    async fn get_tool(
        &self,
        ctx: RequestContext,
        tool_id: &str,
    ) -> Result<Option<crate::models::tool::Tool>>;

    /// 获取 Tool（带附带信息选项）
    async fn get_tool_with_options(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: crate::service::dal::tool::ToolFetchOptions,
    ) -> Result<Option<crate::models::tool::Tool>>;
```

- [ ] **Step 4: 实现 ToolProviderManage::get_tool_with_options**

在 `src/service/domain/finance/tool_provider.rs` 中，在现有 `get_tool` 之后添加：

```rust
    /// 获取 Tool（带附带信息选项）
    async fn get_tool_with_options(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: crate::service::dal::tool::ToolFetchOptions,
    ) -> Result<Option<Tool>> {
        self.tool_dal
            .get_tool(ctx.clone(), tool_id.to_string(), options)
            .await
    }
```

- [ ] **Step 5: GetToolRequest 加 query 参数**

在 `common/src/api/tool.rs` 中，扩展 `GetToolRequest`（约第 45 行）：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载统计信息（调用次数 + 失败次数）
    #[param(source = "query")]
    pub with_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}
```

- [ ] **Step 6: GetToolResponse 加 stats 字段**

在 `common/src/api/tool.rs` 中，扩展 `GetToolResponse`（约第 55 行，注意 `ToolDetail = GetToolResponse` 别名），在 `updated_at` 字段后添加：

```rust
    /// Updated timestamp
    pub updated_at: i64,
    /// 工具统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<common::models::ToolStats>,
```

- [ ] **Step 7: 更新 response.rs 的 to_detail**

在 `src/handlers/finance/tool/response.rs` 中，更新 `to_detail` 函数，在末尾 `updated_at` 之后添加：

```rust
        updated_at: tool.po.updated_at,
        stats: tool.stats.clone(),
    }
}
```

- [ ] **Step 8: 更新 get_tool handler**

在 `src/handlers/finance/tool/get_tool.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
use crate::service::dal::tool::ToolFetchOptions;
```

2) 替换 handler 逻辑：

```rust
pub async fn get_tool(
    ctx: RequestContext,
    params: GetToolRequest,
) -> Result<GetToolResponse> {
    let options = ToolFetchOptions {
        with_stats: params.with_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
    };

    let tool = domain()
        .tool_provider_manage()
        .get_tool_with_options(ctx, &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Tool {} not found", params.id)))?;

    Ok(to_detail(&tool))
}
```

- [ ] **Step 9: 编译验证**

Run: `cargo check --lib`
Expected: 编译通过

- [ ] **Step 10: 运行测试**

Run: `cargo test --lib`
Expected: 全部测试通过

- [ ] **Step 11: Commit**

```bash
git add src/models/tool.rs src/service/dal/tool.rs src/service/domain/finance/mod.rs src/service/domain/finance/tool_provider.rs common/src/api/tool.rs src/handlers/finance/tool/get_tool.rs src/handlers/finance/tool/response.rs
git commit -m "feat: add ToolFetchOptions with stats injection on get_tool"
```

---

## Task 6: ModelProvider — 仅 model_call_stats

ModelProvider 只有模型调用统计（`ModelCallStats`），没有独立的实体统计。

**Files:**
- Modify: `src/models/model_provider.rs`
- Modify: `src/service/dal/model_provider.rs`
- Modify: `src/service/domain/finance/mod.rs`
- Modify: `src/service/domain/finance/model_provider.rs`
- Modify: `common/src/api/model_provider.rs`
- Modify: `src/handlers/finance/model_provider/get_model_provider.rs`

- [ ] **Step 1: ModelProvider 实体加 stats 字段**

在 `src/models/model_provider.rs` 中：

1) 在文件顶部 import 添加：
```rust
use common::models::ModelCallStats;
```

2) 在 `ModelProvider` 结构体（约第 57 行）中添加字段：
```rust
#[derive(Clone)]
pub struct ModelProvider {
    pub po: ModelProviderPo,
    /// 模型调用统计数据（由 DAL 层按需注入）
    pub stats: Option<ModelCallStats>,
}
```

3) 更新 `Debug` impl（约第 61 行）：
```rust
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelProvider")
            .field("po", &self.po)
            .field("stats", &self.stats)
            .finish()
    }
```

4) 更新 `new` 方法（约第 71 行），在返回值中添加 `stats: None`。

5) 如果存在 `from_po` 方法，也添加 `stats: None`。如果没有，搜索项目中所有创建 `ModelProvider {` 的位置，全部添加 `stats: None`。

- [ ] **Step 2: 创建 ModelProviderFetchOptions + get_model_provider DAL 方法**

在 `src/service/dal/model_provider.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
```

2) 在 `ModelProviderDal` trait 定义之前添加：
```rust
/// ModelProvider 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct ModelProviderFetchOptions {
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}
```

3) 在 `ModelProviderDal` trait 中，`find_by_id` 之后添加：
```rust
    /// 根据 ID 获取 ModelProvider（带附带信息选项）
    async fn get_model_provider(&self, ctx: RequestContext, id: &str, options: ModelProviderFetchOptions) -> Result<Option<ModelProvider>>;
```

4) 在 impl 块中添加实现：
```rust
    async fn get_model_provider(&self, ctx: RequestContext, id: &str, options: ModelProviderFetchOptions) -> Result<Option<ModelProvider>> {
        let opt = self.model_provider_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut provider) = opt else {
            return Ok(None);
        };

        if options.with_model_call_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: true,
                with_time_series: true,
                time_range: options.stats_time_range,
                interval: options.stats_interval,
            };
            let stats = self.get_stats(ctx.clone(), id, stats_options).await?;
            provider.stats = Some(stats);
        }

        Ok(Some(provider))
    }
```

**注意：** 需要确认 `find_by_id` 返回的是 `ModelProvider` 还是 `ModelProviderPo`。如果返回 `ModelProviderPo`，需要用 `ModelProvider { po, stats: None }` 包装。

- [ ] **Step 3: 更新 ModelProviderManage trait 签名**

在 `src/service/domain/finance/mod.rs` 中，在 `ModelProviderManage` trait 的 `get_model_provider` 方法之后添加新方法：

```rust
    /// 获取 Model Provider
    async fn get_model_provider(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>>;

    /// 获取 Model Provider（带附带信息选项）
    async fn get_model_provider_with_options(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::model_provider::ModelProviderFetchOptions,
    ) -> Result<Option<ModelProvider>>;
```

- [ ] **Step 4: 实现 ModelProviderManage::get_model_provider_with_options**

在 `src/service/domain/finance/model_provider.rs` 中，在现有 `get_model_provider` 之后添加：

```rust
    async fn get_model_provider_with_options(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::model_provider::ModelProviderFetchOptions,
    ) -> Result<Option<ModelProvider>> {
        self.model_provider_dal.get_model_provider(ctx, id, options).await
    }
```

- [ ] **Step 5: GetModelProviderRequest 加 query 参数**

在 `common/src/api/model_provider.rs` 中，扩展 `GetModelProviderRequest`（约第 62 行）：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
    /// 是否加载模型调用统计（token + 时序趋势）
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_start: Option<i64>,
    /// 统计时间范围结束（毫秒时间戳）
    #[param(source = "query")]
    pub stats_time_end: Option<i64>,
    /// 时序查询粒度：hourly / daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}
```

- [ ] **Step 6: GetModelProviderResponse 加 stats 字段**

在 `common/src/api/model_provider.rs` 中，扩展 `GetModelProviderResponse`（约第 69 行），在 `updated_at` 字段后添加：

```rust
    /// Updated timestamp
    pub updated_at: i64,
    /// 模型调用统计数据（按需返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<common::models::ModelCallStats>,
```

- [ ] **Step 7: 更新 get_model_provider handler**

在 `src/handlers/finance/model_provider/get_model_provider.rs` 中：

1) 在文件顶部添加 import：
```rust
use common::models::StatsInterval;
use crate::service::dal::model_provider::ModelProviderFetchOptions;
```

2) 替换 handler 逻辑：

```rust
pub async fn get_model_provider(
    ctx: RequestContext,
    params: GetModelProviderRequest,
) -> Result<GetModelProviderResponse> {
    let options = ModelProviderFetchOptions {
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
    };

    let provider = domain()
        .model_provider_manage()
        .get_model_provider_with_options(ctx, &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    Ok(GetModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type,
        model_name: provider.po.model_name.clone(),
        base_url: if provider.po.base_url.as_ref().map_or(true, |d| d.is_empty()) {
            None
        } else {
            provider.po.base_url.clone()
        },
        description: if provider
            .po
            .description
            .as_ref()
            .map_or(true, |d| d.is_empty())
        {
            None
        } else {
            provider.po.description.clone()
        },
        created_at: provider.po.created_at,
        updated_at: provider.po.updated_at,
        stats: provider.stats,
    })
}
```

- [ ] **Step 8: 编译验证**

Run: `cargo check --lib`
Expected: 编译通过

- [ ] **Step 9: 运行测试**

Run: `cargo test --lib`
Expected: 全部测试通过

- [ ] **Step 10: Commit**

```bash
git add src/models/model_provider.rs src/service/dal/model_provider.rs src/service/domain/finance/mod.rs src/service/domain/finance/model_provider.rs common/src/api/model_provider.rs src/handlers/finance/model_provider/get_model_provider.rs
git commit -m "feat: add ModelProviderFetchOptions with stats injection on get_model_provider"
```

---

## Task 7: 全量验证 + 前端编译检查

- [ ] **Step 1: 后端全量编译**

Run: `cargo check --lib`
Expected: 0 errors

- [ ] **Step 2: 后端全量测试**

Run: `cargo test --lib`
Expected: 全部测试通过（697+ 个测试）

- [ ] **Step 3: 前端编译检查**

Run: `cd frontend && cargo check --verbose`
Expected: 0 errors

- [ ] **Step 4: 检查 API 路由注册**

确认所有 handler 的路由没有冲突。由于我们只修改了现有 handler 的参数（增加了 query 参数），路由路径不变，不应有冲突。

Run: `cargo check --lib`
Expected: 编译通过即说明路由注册正常

- [ ] **Step 5: Final commit（如有未提交的改动）**

```bash
git status
# 如果有未提交的改动
git add -A
git commit -m "feat: complete entity stats injection for all 5 entities"
```

---

## 设计决策说明

### 1. 为什么保留原 `get` 方法而不是改签名？

Project/Task/Tool/ModelProvider 的 `get(ctx, id)` 方法被多处调用（其他 handler、consumer 等）。直接改签名会导致所有调用方都需要修改。新增 `get_xxx_with_options` 或 `get_xxx(ctx, id, options)` 方法，保留原方法不变，实现零破坏性。

Agent 是例外，因为它已经有 `get_agent(ctx, id, options)` 方法，直接扩展 options 即可。

### 2. 为什么用 `skip_serializing_if = "Option::is_none"`？

统计数据是按需注入的，不请求时为 `None`。使用 `skip_serializing_if` 可以在 JSON 响应中省略 `None` 字段，保持响应体简洁。前端可以通过字段是否存在判断是否包含统计数据。

### 3. StatsFetchOptions 映射逻辑

| Query 参数 | 映射到 StatsFetchOptions |
|------------|------------------------|
| `with_stats=true` | `with_call_summary: true`（实体自身统计） |
| `with_model_call_stats=true` | `with_call_summary: true, with_token_summary: true, with_time_series: true`（模型调用全量） |
| `stats_time_start` + `stats_time_end` | `time_range: Some((start, end))` |
| `stats_interval=hourly` | `interval: Some(StatsInterval::Hourly)` |

### 4. 实体统计 vs 模型调用统计

- **实体自身统计**（`AgentStats`/`ProjectStats`/`TaskStats`/`ToolStats`）：来自各自的事件表（agent_awake_events/project_events/task_events/tool_call_events），只包含调用次数汇总
- **模型调用统计**（`ModelCallStats`）：来自 model_call_events 表，包含 token 汇总 + 时序趋势，可按 agent_id/project_id/task_id/model_provider_id 过滤

Tool 没有 `model_call_stats`（工具不调用模型），ModelProvider 只有 `model_call_stats`（模型提供商就是模型本身）。
