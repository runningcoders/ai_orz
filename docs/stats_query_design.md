# 统计查询模块设计方案

## 需求背景

需要为前端页面提供统计数据查询能力：
- Agent 页面：查看 Agent 相关的 token 消耗、调用次数、QPS 时序
- Project 页面：查看该项目下的统计
- Task 页面：查看该任务下的统计

## 架构设计

遵循项目现有分层规范，将统计查询按实体拆分到各自 DAO，融入现有架构不新增独立模块。

### 分层结构

| 层级 | 位置 | 职责 |
|------|------|------|
| **公共类型 & 底层能力** | `pkg/stats` | 定义 `StatFilter`, `StatAggregation`, `AggregationRow`, `StatsInterval`, `TimeSeriesPoint`, `TokenSumResult` 公共类型；提供 `query_aggregation`, `query_time_series` 通用 DuckDB 查询能力 |
| **DAO - Agent** | `service/dao/agent/stats.rs` | `AgentStatsDao` trait + DuckDB 实现，自动添加 `agent_id` 过滤 |
| **DAO - Project** | `service/dao/project/stats.rs` | `ProjectStatsDao` trait + DuckDB 实现，自动添加 `project_id` 过滤 |
| **DAO - Task** | `service/dao/task/stats.rs` | `TaskStatsDao` trait + DuckDB 实现，自动添加 `task_id` 过滤 |
| **DAL** | 对应实体 DAL | 新增统计查询方法，依赖 stats DAO |
| **Domain** | 现有 Domain | 新增统计查询接口，调用 DAL 获取结果 |
| **Handler** | `handlers/...` | 独立 handler 文件，HTTP 接口返回 JSON |

### 设计原则

1. **每个实体独立 DAO**：便于独立拓展，修改一个不影响其他
2. **通用查询基础上提供语法糖**：底层通用，上层封装常见场景
3. **参数结构化**：每个查询方法使用结构体组织参数，方便未来拓展
4. **直接从 RequestContext 获取 Stats 实例**：统一管理，DAO 不需要持有
5. **对齐现有初始化模式**：使用 `OnceLock` 单例

---

## 数据结构定义

### pkg/stats 公共类型（已完成）

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

pub enum StatsInterval {
    Hourly,
    Daily,
}

pub struct TimeSeriesPoint {
    pub interval_start: i64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub call_count: u64,
}

pub struct TokenSumResult {
    pub total_tokens_input: u64,
    pub total_tokens_output: u64,
    pub total_calls: u64,
}
```

### DAO 层接口示例（Agent）

```rust
pub struct AgentStatsQuery {
    pub agent_id: String,
    pub filters: Vec<StatFilter>,
    pub group_by: Vec<String>,
    pub aggregations: Vec<StatAggregation>,
    pub time_range: Option<(i64, i64)>,
}

pub struct AgentStatsTimeSeriesQuery {
    pub agent_id: String,
    pub filters: Vec<StatFilter>,
    pub interval: StatsInterval,
    pub time_range: (i64, i64),
}

pub struct AgentStatsTokenSumQuery {
    pub agent_id: String,
    pub filters: Vec<StatFilter>,
    pub time_range: Option<(i64, i64)>,
}

pub trait AgentStatsDao: Send + Sync {
    async fn query_aggregation(
        &self,
        ctx: RequestContext,
        query: AgentStatsQuery,
    ) -> Result<Vec<AggregationRow>>;

    async fn query_time_series(
        &self,
        ctx: RequestContext,
        query: AgentStatsTimeSeriesQuery,
    ) -> Result<Vec<TimeSeriesPoint>>;

    async fn sum_tokens(
        &self,
        ctx: RequestContext,
        query: AgentStatsTokenSumQuery,
    ) -> Result<TokenSumResult>;
}
```

**实现要点：**
- `AgentStatsDaoDuckDbImpl` 是空结构体，不需要存储状态
- 每次查询从 `ctx.stats()` 获取 `Stats` 实例
- 自动将 `agent_id` 添加到过滤条件中，调用者不需要重复添加
- 调用 `pkg/stats` 提供的通用查询方法

Project 和 Task 完全相同结构，只是过滤维度不同 (`project_id` / `task_id`)。

---

## 初始化流程

和其他 DAO 保持一致：

```rust
static AGENT_STATS_DAO: OnceLock<Arc<dyn AgentStatsDao>> = OnceLock::new();

pub fn dao() -> Arc<dyn AgentStatsDao> {
    AGENT_STATS_DAO.get().cloned().unwrap()
}

pub fn init() {
    let _ = AGENT_STATS_DAO.set(Arc::new(AgentStatsDaoDuckDbImpl::new()));
}
```

在全局 `init_all_dao` 中调用各自的 `init()`。

---

## DAL 层集成示例

在现有 `AgentDalImpl` 中添加依赖：

```rust
pub struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao>,
}

impl AgentDal {
    pub async fn query_stats_aggregation(
        &self,
        ctx: RequestContext,
        query: AgentStatsQuery,
    ) -> Result<Vec<AggregationRow>> {
        self.agent_stats_dao.query_aggregation(ctx, query).await
    }

    pub async fn get_agent_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        time_range: Option<(i64, i64)>,
        interval: Option<StatsInterval>,
    ) -> Result<(TokenSumResult, Option<Vec<TimeSeriesPoint>>)> {
        let sum_query = AgentStatsTokenSumQuery {
            agent_id: agent_id.to_string(),
            filters: vec![],
            time_range,
        };
        let sum = self.agent_stats_dao.sum_tokens(ctx, sum_query).await?;

        let time_series = if let Some(interval) = interval {
            let ts_query = AgentStatsTimeSeriesQuery {
                agent_id: agent_id.to_string(),
                filters: vec![],
                interval,
                time_range: time_range.unwrap(),
            };
            Some(self.agent_stats_dao.query_time_series(ctx, ts_query).await?)
        } else {
            None
        };

        Ok((sum, time_series))
    }
}
```

---

## 扩展性

### 新增实体统计

只需要新增 `service/dao/{entity}/stats.rs`，按相同模式实现即可，不需要修改其他地方代码。

### 新增查询维度

在现有 DAO 新增方法即可，不影响其他代码。

### 自定义复杂查询

可以直接调用通用 `query_aggregation` 方法，灵活组合过滤、分组、聚合。

---

## 记录事件规范

在 `brain_dal.think()` 中记录模型调用事件时，自动从 `RequestContext` 提取标签：

```rust
// 自动提取
let agent_id = ctx.agent_id();
let task_id = ctx.task_id();
let project_id = ctx.project_id();
let model_provider_id = brain.cortex.model_provider.po.id;

let tags = json!({
    "agent_id": agent_id,
    "task_id": task_id,
    "project_id": project_id,
    "model_provider_id": model_provider_id,
});

let metrics = json!({
    "tokens_input": tokens_input,
    "tokens_output": tokens_output,
    "latency_ms": latency_ms,
});

record_event!(ctx, ModelCallEvent {
    tags,
    metrics,
});
```

这样所有维度都能正确记录，支持任意维度组合查询。

## 版本信息

| 版本 | 日期 | 作者 | 变更 |
|------|------|------|------|
| v1.0 | 2026-06-28 | 讨论确定 | 初始设计 |
