# 统计查询模块设计方案

## 需求背景

需要为前端页面提供统计数据查询能力：
- Agent 页面：查看 Agent 相关的 token 消耗、调用次数、QPS 时序
- Project 页面：查看该项目下的统计
- Task 页面：查看该任务下的统计

## 架构设计

### 核心理念：领域先行，职责分离

统计查询按**领域**而非**实体**划分，每个领域专注自己的统计职责：

```
┌─────────────────────────────────────────────────────────────┐
│                    统计领域划分                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Agent自身领域          Project自身领域        Task自身领域 │
│  ┌─────────────┐       ┌─────────────┐       ┌─────────────┐│
│  │ AgentStatsDao│       │ProjectStatsDao│      │ TaskStatsDao││
│  │             │       │             │       │             ││
│  │ • call_summary    │ • call_summary    │ • call_summary    ││
│  └─────────────┘       └─────────────┘       └─────────────┘│
│         │                     │                     │        │
│         ▼                     ▼                     ▼        │
│  ┌─────────────────────────────────────────────────────┐    │
│  │           模型调用领域（ModelProviderStatsDao）      │    │
│  │  ┌─────────────────────────────────────────────┐    │    │
│  │  │ • call_summary                             │    │    │
│  │  │ • token_summary                            │    │    │
│  │  │ • model_call_time_series                   │    │    │
│  │  └─────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│  DAL 层负责组装：调用多个 DAO，返回组合结果给上层              │
└─────────────────────────────────────────────────────────────┘
```

### 设计原则

1. **领域单一职责**：每个实体的 StatsDao 只负责自身维度的统计（call_summary），模型调用领域由 ModelProviderStatsDao 统一负责
2. **领域先行**：先划分领域边界，实现细节后续演进（如 Agent 未来有了专属统计表，直接替换 DAO 实现即可）
3. **通用结构体复用**：`ModelCallStats` 作为模型调用领域的通用结果结构体，所有实体都复用
4. **DAL 层组装**：跨领域查询在 DAL 层完成，上层（Domain/Handler）看到的是完整的统计结果
5. **接口精简**：越靠上层接口越简洁，DAL 层只暴露 `get_stats(id, options)` 和 `get_model_call_stats(id, options)`

### 分层结构

| 层级 | 位置 | 职责 |
|------|------|------|
| **跨层共享模型** | `common/src/models/stats.rs` | `StatsInterval`、`TimeSeriesPoint`、`TokenSumResult`、`CallSummary`、`StatsFetchOptions`、`ModelCallStats`、各实体专属统计结构体 |
| **公共类型 & 底层能力** | `pkg/stats` | 定义 `StatFilter`、`StatAggregation`、`AggregationRow`、`StatParam`；提供 `query_aggregation`、`query_time_series` 通用 DuckDB 查询能力 |
| **DAO - Agent** | `service/dao/agent/stats_duckdb.rs` | `AgentStatsDao` trait + DuckDB 实现，只负责 Agent 自身维度的 call_summary |
| **DAO - Project** | `service/dao/project/stats_duckdb.rs` | `ProjectStatsDao` trait + DuckDB 实现，只负责 Project 自身维度的 call_summary |
| **DAO - Task** | `service/dao/task/stats_duckdb.rs` | `TaskStatsDao` trait + DuckDB 实现，只负责 Task 自身维度的 call_summary |
| **DAO - ModelProvider** | `service/dao/model_provider/stats_duckdb.rs` | `ModelProviderStatsDao` trait + DuckDB 实现，负责模型调用领域的所有统计（call_summary + token_summary + time_series），支持按 agent_id/project_id/task_id/model_provider_id 多维度过滤 |
| **DAL** | 对应实体 DAL | 组合多个 DAO，提供简洁的业务级统计接口 |
| **Domain** | 现有 Domain | 新增统计查询接口，调用 DAL 获取结果 |
| **Handler** | `handlers/...` | 独立 handler 文件，HTTP 接口返回 JSON |

---

## 数据结构定义

### 跨层共享模型（common/src/models/stats.rs）

#### 基础统计结构

```rust
/// Time series interval for grouping data
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatsInterval {
    Hourly,
    Daily,
}

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeSeriesPoint {
    pub interval_start: i64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub call_count: u64,
}

/// Total token sum result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenSumResult {
    pub total_tokens_input: u64,
    pub total_tokens_output: u64,
    pub total_calls: u64,
}

/// 调用次数汇总（最通用的统计结果）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallSummary {
    pub total_calls: u64,
    pub avg_qps: Option<f64>,      // 需要 time_range 才有值
    pub instant_qps: f64,          // 最近 1 秒调用次数
}

/// 统计数据获取选项
#[derive(Debug, Clone, Default)]
pub struct StatsFetchOptions {
    pub with_call_summary: bool,
    pub with_token_summary: bool,
    pub with_time_series: bool,
    pub time_range: Option<(i64, i64)>,
    pub interval: Option<StatsInterval>,
}
```

#### 领域统计结构体

```rust
/// Agent 自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentStats {
    pub call_summary: Option<CallSummary>,
}

/// Project 自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProjectStats {
    pub call_summary: Option<CallSummary>,
}

/// Task 自身统计数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TaskStats {
    pub call_summary: Option<CallSummary>,
}

/// 模型调用统计（通用，所有实体共用）
///
/// 由 ModelProviderStatsDao 负责计算，
/// 支持按 agent_id / project_id / task_id / model_provider_id 过滤。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelCallStats {
    pub call_summary: Option<CallSummary>,
    pub token_summary: Option<TokenSumResult>,
    pub model_call_time_series: Option<Vec<TimeSeriesPoint>>,
}
```

### pkg/stats 内部类型

```rust
pub enum StatFilter {
    Equals { key: String, value: JsonValue },
    Range { key: String, min: Option<f64>, max: Option<f64> },
}

pub enum StatAggregation {
    Count,
    Sum(String),
    Avg(String),
}

pub struct AggregationRow {
    pub groups: HashMap<String, JsonValue>,
    pub aggregations: HashMap<String, f64>,
}

pub enum StatParam {
    Int(i64),
    Double(f64),
    Str(String),
}
```

---

## DAO 层接口设计

### Agent/Project/Task StatsDao（领域：自身维度）

三个实体的 StatsDao 接口结构相同，只负责自身维度的 call_summary。

**AgentStatsDao 实现细节**：数据来自 `agent_awake_events` 表（Agent 唤醒事件），统计 Agent 被唤醒的次数和 QPS。

```rust
/// Agent 统计查询参数
pub struct AgentStatsQuery {
    pub agent_id: String,
    pub filters: Vec<StatFilter>,
    pub time_range: Option<(i64, i64)>,
    pub aggregations: Vec<StatAggregation>,
}

pub trait AgentStatsDao: Send + Sync {
    type AwakeEvent: StatEvent + 'static + Send + Sync;

    fn awake_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::AwakeEvent>()
    }

    async fn query_awake_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<JsonValue>>;

    async fn sum_calls(&self, ctx: RequestContext, mut query: AgentStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_awake_calls(ctx, query).await?;
        if rows.is_empty() { return Ok(0); }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    async fn get_stats(&self, ctx: RequestContext, query: AgentStatsQuery, options: StatsFetchOptions) -> Result<AgentStats> {
        let mut stats = AgentStats::default();
        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = AgentStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            
            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = AgentStatsQuery {
                    time_range: Some((start, end)),
                    ..query.clone()
                };
                let range_calls = self.sum_calls(ctx.clone(), range_query).await?;
                let duration_secs = (end - start) as f64 / 1000.0;
                if duration_secs > 0.0 { Some(range_calls as f64 / duration_secs) } else { None }
            } else { None };

            stats.call_summary = Some(CallSummary {
                total_calls,
                avg_qps,
                instant_qps: instant_calls as f64,
            });
        }
        Ok(stats)
    }
}
```

### ModelProviderStatsDao（领域：模型调用）

模型调用领域的统一 DAO，支持多维度过滤：

```rust
/// ModelProvider 统计查询参数（统一结构体，覆盖所有查询场景）
pub struct ModelProviderStatsQuery {
    pub model_provider_id: Option<String>,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub filters: Vec<StatFilter>,
    pub time_range: Option<(i64, i64)>,
    pub group_by: Vec<String>,
    pub aggregations: Vec<StatAggregation>,
    pub interval: Option<StatsInterval>,
}

pub trait ModelProviderStatsDao: Send + Sync {
    type ModelCallEvent: StatEvent + 'static + Send + Sync;

    fn model_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ModelCallEvent>()
    }

    async fn query_model_calls(&self, ctx: RequestContext, query: ModelProviderStatsQuery) -> Result<Vec<JsonValue>>;

    async fn query_model_call_time_series(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<Vec<TimeSeriesPoint>>;

    async fn sum_tokens(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<TokenSumResult>;

    async fn sum_calls(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<u64>;

    async fn get_stats(&self, ctx: RequestContext, query: ModelProviderStatsQuery, options: StatsFetchOptions) -> Result<ModelCallStats>;
}
```

**设计要点：**
- `model_provider_id` 改为可选，支持按其他维度过滤
- 新增 `agent_id`、`project_id`、`task_id` 可选字段，支持多维度查询
- `get_stats` 返回 `ModelCallStats`，包含 call_summary、token_summary、model_call_time_series
- 所有维度的模型调用统计都通过这个 DAO 查询

---

## DAL 层接口设计

### 统一接口风格

所有 DAL 的统计接口统一为两个方法：

```rust
/// 获取实体自身统计数据
async fn get_stats(&self, ctx: RequestContext, id: &str, options: StatsFetchOptions) -> Result<XxxStats>;

/// 获取实体维度的模型调用统计（ModelProvider DAL 没有这个方法）
async fn get_model_call_stats(&self, ctx: RequestContext, id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
```

### Agent DAL 示例

```rust
pub trait AgentDal: Send + Sync {
    // ... 基础 CRUD 方法 ...

    /// 获取 Agent 自身统计数据
    async fn get_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<AgentStats>;

    /// 获取 Agent 维度的模型调用统计
    async fn get_model_call_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
}

struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao<ModelCallEvent = ModelCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
}

impl AgentDal for AgentDalImpl {
    async fn get_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<AgentStats> {
        let query = AgentStatsQuery {
            agent_id: agent_id.to_string(),
            time_range: options.time_range,
            ..Default::default()
        };
        self.agent_stats_dao.get_stats(ctx, query, options).await
    }

    async fn get_model_call_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            agent_id: Some(agent_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao.get_stats(ctx, query, options).await
    }
}
```

### ModelProvider DAL

ModelProvider DAL 只需要一个 `get_stats` 方法，因为它自己就是模型调用领域：

```rust
pub trait ModelProviderDal: Send + Sync {
    // ... 基础 CRUD 方法 ...

    /// 获取 ModelProvider 的模型调用统计
    async fn get_stats(&self, ctx: RequestContext, model_provider_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
}
```

---

## 演进路径

### 当前阶段

- **Agent 自身统计**：数据来自 `agent_awake_events` 表（Agent 唤醒事件），统计 Agent 被唤醒的次数和 QPS
- **Project/Task 自身统计**：暂时从 `model_call_events` 表按维度过滤获取（未来会切换到各自的专属统计表）
- **模型调用统计**：统一由 `ModelProviderStatsDao` 负责，数据来自 `model_call_events` 表

### 未来阶段

当 Project/Task 有了自己的专属统计表时：

1. **替换 DAO 实现**：只需修改对应 StatsDao 的 DuckDB 实现，从新的专属表查询
2. **扩展统计结构体**：各实体 Stats 可以增加新字段（如 `task_count`、`active_user_count` 等）
3. **上层无感知**：DAL 和 Domain 层的接口完全不变，调用方不需要修改

---

## 记录事件规范

#### 模型调用统计

在 rig hook `on_completion_response` 中自动发送 `ModelCallEvent`：

```rust
let event = ModelCallEvent::new(timestamp)
    .with_agent_id(agent_id)
    .with_project_id(project_id)
    .with_task_id(task_id)
    .with_model_provider_id(model_provider_id)
    .with_model_name(model_name)
    .with_tokens_input(usage.input_tokens)
    .with_tokens_output(usage.output_tokens)
    .with_total_tokens(usage.total_tokens);

ctx.stats().record(ctx.clone(), event).await?;
```

#### 工具调用统计

在 `ToolCallLoggingDecorator` 中统一发送 `ToolCallEvent`：

```rust
let event = ToolCallEvent::new(timestamp)
    .with_tool_id(tool_id)
    .with_tool_name(tool_name)
    .with_agent_id(agent_id)
    .with_args_len(args_len)
    .with_result_len(result_len)
    .with_duration_ms(duration_ms)
    .with_status(status);

if let Some(stats) = ctx.stats_opt() {
    let _ = stats.record(ctx_clone, event);
}
```

#### Agent 唤醒统计

在 `RuntimeDomain.awaken()` 中记录 `AgentAwakeEvent`：

```rust
let event = AgentAwakeEvent::new(timestamp)
    .with_agent_id(agent_id)
    .with_project_id(project_id)
    .with_task_id(task_id)
    .with_organization_id(organization_id)
    .with_user_id(user_id)
    .with_message_id(message_id)
    .with_duration_ms(duration_ms)
    .with_status("success".to_string());

record_event!(ctx, event);
```

#### Project 业务事件

在 `ProjectDomain` 的状态变更方法中记录 `ProjectEvent`，用于追踪项目生命周期和关键业务动作。

**事件类型枚举：**

| 事件类型 | 触发时机 | 说明 |
|----------|----------|------|
| `created` | 项目创建时 | 记录项目创建人和初始状态 |
| `started` | 项目启动时 | 记录项目从非活跃状态进入进行中 |
| `completed` | 项目完成时 | 记录项目完成，可统计耗时 |
| `archived` | 项目归档时 | 记录项目归档 |
| `status_changed` | 状态流转时 | 通用状态变更记录（transition_status 调用） |

**事件字段设计：**

```rust
pub struct ProjectEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub project_id: String,
    #[tag]
    pub event_type: String,       // created / started / completed / archived / status_changed
    #[tag]
    pub organization_id: Option<String>,
    #[tag]
    pub operator_type: Option<String>,  // 操作者类型：user / agent
    #[tag]
    pub operator_id: Option<String>,    // 操作者 ID
    #[tag]
    pub root_user_id: Option<String>,
    #[tag]
    pub owner_type: Option<String>,     // 项目负责人类型：user / agent
    #[tag]
    pub owner_id: Option<String>,       // 项目负责人 ID
    #[tag]
    pub from_status: Option<String>,    // 变更前状态
    #[tag]
    pub to_status: Option<String>,      // 变更后状态
    #[metric]
    pub duration_ms: Option<u64>,       // 操作耗时
    #[metric]
    pub priority: i32,
}
```

**记录位置：**
- `ProjectManage::create()` → `created` 事件
- `ProjectManage::start()` → `started` 事件
- `ProjectManage::complete()` → `completed` 事件
- `ProjectManage::archive()` → `archived` 事件
- `ProjectManage::transition_status()` → `status_changed` 事件（内部统一调用，上面三个也会经过）

#### Task 业务事件

在 `TaskManage` 的状态变更方法中记录 `TaskEvent`，用于追踪任务生命周期和关键业务动作。

**事件类型枚举：**

| 事件类型 | 触发时机 | 说明 |
|----------|----------|------|
| `created` | 任务创建时 | 记录任务创建人和初始状态 |
| `started` | 任务开始时 | 记录任务从待办进入进行中 |
| `completed` | 任务完成时 | 记录任务完成，可统计耗时 |
| `cancelled` | 任务取消时 | 记录任务取消 |
| `assigned` | 任务分配/重新分配时 | 记录负责人变更 |
| `status_changed` | 状态流转时 | 通用状态变更记录 |

**事件字段设计：**

```rust
pub struct TaskEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub task_id: String,
    #[tag]
    pub project_id: Option<String>,   // 必须有，任务归属项目
    #[tag]
    pub event_type: String,           // created / started / completed / cancelled / assigned / status_changed
    #[tag]
    pub organization_id: Option<String>,
    #[tag]
    pub operator_type: Option<String>,  // 操作者类型：user / agent
    #[tag]
    pub operator_id: Option<String>,    // 操作者 ID
    #[tag]
    pub root_user_id: Option<String>,
    #[tag]
    pub assignee_type: Option<String>,  // 任务负责人类型：user / agent
    #[tag]
    pub assignee_id: Option<String>,    // 当前负责人 ID
    #[tag]
    pub from_assignee_id: Option<String>,  // 变更前负责人（assigned 事件用）
    #[tag]
    pub from_status: Option<String>,    // 变更前状态
    #[tag]
    pub to_status: Option<String>,      // 变更后状态
    #[metric]
    pub duration_ms: Option<u64>,       // 操作耗时
    #[metric]
    pub priority: i32,
}
```

**记录位置：**
- `TaskManage::create()` / `create_with_options()` → `created` 事件
- `TaskManage::start()` → `started` 事件
- `TaskManage::complete()` → `completed` 事件
- `TaskManage::cancel()` → `cancelled` 事件
- `TaskManage::transition_status()` → `status_changed` 事件

#### ToolCallEvent 多维度字段说明

`ToolCallEvent` 已内置 `project_id` 和 `task_id` 字段，支持按项目/任务维度统计工具调用。在调用工具时，如果上下文中包含 project_id 或 task_id，会自动填充到事件中。

---

## 事件记录原则与方法清单

### 记录原则

1. **状态变更必记录**：所有业务实体的状态流转都需要记录事件
2. **创建/删除必记录**：实体的创建和删除（归档/取消）需要记录
3. **关键动作记录**：如分配、重新分配等业务意义明确的动作
4. **只读操作不记录**：查询、列表等只读操作不记录统计事件
5. **上下文自动携带**：事件记录时自动从 `RequestContext` 提取 organization_id、user_id 等信息

### 需要记录事件的方法清单

#### Project Domain

| 方法 | 事件类型 | 说明 |
|------|----------|------|
| `ProjectManage::create()` | `created` | 项目创建 |
| `ProjectManage::start()` | `started` | 项目启动 |
| `ProjectManage::complete()` | `completed` | 项目完成 |
| `ProjectManage::archive()` | `archived` | 项目归档 |
| `ProjectManage::transition_status()` | `status_changed` | 状态流转（内部统一入口） |

> **设计说明**：`start/complete/archive` 和 `transition_status` 是并列的 API 入口，互不调用。每个入口记录对应语义的事件：
> - `create()` → `created`
> - `start()` → `started`
> - `complete()` → `completed`
> - `archive()` → `archived`
> - `transition_status()` → `status_changed`
>
> 这样设计的好处是事件类型直接对应用户的操作意图，统计时可以按不同维度聚合。

#### Task Domain

| 方法 | 事件类型 | 说明 |
|------|----------|------|
| `TaskManage::create()` | `created` | 任务创建 |
| `TaskManage::start()` | `started` | 任务开始 |
| `TaskManage::complete()` | `completed` | 任务完成 |
| `TaskManage::cancel()` | `cancelled` | 任务取消 |
| `TaskManage::transition_status()` | `status_changed` | 状态流转（内部统一入口） |

> **注意**：当前 Task Domain 中没有单独的 `assign`/`reassign` 方法，分配信息在创建时设置。未来如果增加独立的分配方法，需要补充 `assigned` 事件。

### 已实现事件清单

| 事件类型 | 结构体 | 表名 | 记录位置 | 状态 |
|----------|--------|------|----------|------|
| 模型调用 | `ModelCallEvent` | `model_call_events` | rig hook `on_completion_response` | ✅ 已实现 |
| 工具调用 | `ToolCallEvent` | `tool_call_events` | `ToolCallLoggingDecorator` | ✅ 已实现 |
| Agent 唤醒 | `AgentAwakeEvent` | `agent_awake_events` | `RuntimeDomain.awaken()` | ✅ 已实现 |
| Project 业务 | `ProjectEvent` | `project_events` | `ProjectManage` 状态方法 | ✅ 已实现 |
| Task 业务 | `TaskEvent` | `task_events` | `TaskManage` 状态方法 | ✅ 已实现 |

---

## 版本信息

| 版本 | 日期 | 作者 | 变更 |
|------|------|------|------|
| v1.0 | 2026-06-28 | 讨论确定 | 初始设计 |
| v1.1 | 2026-07-02 | 实现迭代 | Agent Stats DAO 实现完成；统计模型迁移到 common/src/models；接口从三个独立结构体改为统一 AgentStatsQuery |
| v1.2 | 2026-07-02 | 全实体覆盖 | 新增 Project/Task/ModelProvider 三个 Stats DAO；每个 DAO 4 个单元测试 |
| v1.3 | 2026-07-02 | 事件关联优化 | Stats 新增 get_table_name<E> 方法；DAO trait 使用关联类型绑定事件类型 |
| v1.4 | 2026-07-03 | 专用事件拆分 | DAO trait 拆分为 ModelCallEvent / ToolCallEvent 双关联类型；专用事件独立文件 |
| v1.5 | 2026-07-03 | 表自描述重构 | StatTable trait 新增 is_dedicated_table / column_sql / metric_sql / filter_equals_sql / filter_range_sql |
| v1.6 | 2026-07-03 | 监控链路字段补充 | ModelCallEvent 补充 model_provider_id / model_name；ToolCallEvent 新增 organization_id / user_id |
| v1.7 | 2026-07-05 | 领域拆分重构 | **核心改造**：按领域划分职责，Agent/Project/Task StatsDao 只负责自身维度的 call_summary；ModelProviderStatsDao 升级为模型调用领域 DAO；新增 ModelCallStats 通用结构体；DAL 层组装跨领域统计结果；接口精简为 get_stats(id, options) + get_model_call_stats(id, options) |
| v1.8 | 2026-07-05 | Agent 唤醒事件 + 数据源切换 | 新增 `AgentAwakeEvent` 统计事件和 `agent_awake_events` 表；在 RuntimeDomain.awaken() 中记录唤醒事件；AgentStatsDao 数据源从 model_call_events 切换到 agent_awake_events，统计内容从"模型调用次数"变为"Agent 唤醒次数" |
| v1.9 | 2026-07-06 | Project/Task 业务事件落地 | 新增 `ProjectEvent` 和 `TaskEvent` 两个业务事件结构体和表；在 ProjectDomain 和 TaskDomain 的状态变更方法中集成事件记录；`record_event!` 宏改用 `stats_opt()` 避免未初始化时 panic；544 个测试 100% 通过 |