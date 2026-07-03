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
| **跨层共享模型** | `common/src/models/stats.rs` | `StatsInterval`、`TimeSeriesPoint`、`TokenSumResult` 公共结果结构体 |
| **公共类型 & 底层能力** | `pkg/stats` | 定义 `StatFilter`、`StatAggregation`、`AggregationRow`、`StatParam`；提供 `query_aggregation`、`query_time_series` 通用 DuckDB 查询能力 |
| **DAO - Agent** | `service/dao/agent/stats_duckdb.rs` | `AgentStatsDao` trait + DuckDB 实现，自动添加 `agent_id` 过滤 |
| **DAO - Project** | `service/dao/project/stats_duckdb.rs` | `ProjectStatsDao` trait + DuckDB 实现，自动添加 `project_id` 过滤 |
| **DAO - Task** | `service/dao/task/stats_duckdb.rs` | `TaskStatsDao` trait + DuckDB 实现，自动添加 `task_id` 过滤 |
| **DAO - ModelProvider** | `service/dao/model_provider/stats_duckdb.rs` | `ModelProviderStatsDao` trait + DuckDB 实现，自动添加 `model_provider_id` 过滤 |
| **DAL** | 对应实体 DAL | 新增统计查询方法，依赖 stats DAO |
| **Domain** | 现有 Domain | 新增统计查询接口，调用 DAL 获取结果 |
| **Handler** | `handlers/...` | 独立 handler 文件，HTTP 接口返回 JSON |

### 设计原则

1. **每个实体独立 DAO**：便于独立拓展，修改一个不影响其他
2. **通用查询基础上提供语法糖**：底层通用 `query()`，上层封装 `sum_tokens()`、`query_time_series()` 等常见场景
3. **参数结构化**：使用 `AgentStatsQuery` 统一结构体组织参数，通过 `interval`、`aggregations` 等字段区分查询模式
4. **直接从 RequestContext 获取 Stats 实例**：统一管理，DAO 不需要持有
5. **对齐现有初始化模式**：使用 `OnceLock` 单例
6. **统计结果模型跨层共享**：`TokenSumResult`、`TimeSeriesPoint`、`StatsInterval` 放在 `common/src/models`，DAO/DAL/Domain/Handler 共用

---

## 数据结构定义

### 跨层共享模型（common/src/models/stats.rs）

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

/// 类型安全的 SQL 参数（解决 dyn ToSql 的 Send 问题）
pub enum StatParam {
    Int(i64),
    Double(f64),
    Str(String),
}
```

### DAO 层接口（Agent，实际实现）

```rust
/// Agent 统计查询参数（统一结构体，覆盖所有查询场景）
pub struct AgentStatsQuery {
    /// Agent ID（必填）
    pub agent_id: String,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 分组字段（聚合查询专用）
    pub group_by: Vec<String>,
    /// 聚合函数（聚合查询专用）
    pub aggregations: Vec<StatAggregation>,
    /// 时间间隔（时序查询专用，填了此字段走时序查询）
    pub interval: Option<StatsInterval>,
}

pub trait AgentStatsDao: Send + Sync {
    /// 模型调用事件类型，用于从 Stats 注册表获取表名
    type ModelCallEvent: StatEvent + 'static + Send + Sync;
    /// 工具调用事件类型，用于从 Stats 注册表获取表名
    type ToolCallEvent: StatEvent + 'static + Send + Sync;

    /// 获取模型调用表名（从 Stats 注册表中查询，默认实现）
    fn model_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ModelCallEvent>()
    }

    /// 获取工具调用表名（从 Stats 注册表中查询，默认实现）
    fn tool_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ToolCallEvent>()
    }

    /// 模型调用通用查询（聚合 / 过滤 / 分组 / 时序）
    async fn query_model_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<JsonValue>>;
    /// 工具调用通用查询（聚合 / 过滤 / 分组 / 时序）
    async fn query_tool_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<JsonValue>>;

    /// 语法糖：模型调用时序查询（返回结构化 TimeSeriesPoint）
    async fn query_model_call_time_series(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<TimeSeriesPoint>>;
    /// 语法糖：模型调用聚合查询（返回 AggregationRow）
    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<AggregationRow>>;

    /// 语法糖：Token 汇总（从模型调用表查询，返回 TokenSumResult）
    async fn sum_tokens(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<TokenSumResult>;
    /// 语法糖：工具调用次数汇总（从工具调用表查询，返回总次数）
    async fn sum_tool_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<u64>;
}
```

**设计要点：**
- 使用统一 `AgentStatsQuery` 结构体，通过 `interval` 字段区分时序查询 vs 聚合查询
- `sum_tokens` 内部自动设置 `aggregations = [Sum("tokens_input"), Sum("tokens_output"), Count]`
- **多关联类型**：`type ModelCallEvent` 和 `type ToolCallEvent` 分别绑定模型调用和工具调用事件类型，trait 提供默认 `model_call_table_name()` / `tool_call_table_name()` 方法从 Stats 注册表获取表名
- **事件与表名映射**：写入时通过 `record_event!(ctx, Event)` 自动路由到对应表，查询时通过关联类型获取相同表名，保证一致性
- **查询分离**：`query_model_calls` 查询 `model_call_events` 表，`query_tool_calls` 查询 `tool_call_events` 表
- `query_model_call_time_series` 内部要求 `interval` 字段，自动调用 `Stats::query_time_series`
- 所有方法自动将 `agent_id` 添加到过滤条件，调用者不需要重复添加

---

## 初始化流程

和其他 DAO 保持一致：

```rust
static AGENT_STATS_DAO: OnceLock<Arc<dyn AgentStatsDao>> = OnceLock::new();

/// 创建一个全新的 Agent Stats DAO 实例（用于测试）
pub fn stats_new() -> Arc<dyn AgentStatsDao> {
    Arc::new(AgentStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn AgentStatsDao> {
    AGENT_STATS_DAO.get().cloned().unwrap()
}
```

---

## 实现细节与踩坑记录

### 1. 聚合查询 JSON 返回格式

**问题：** `AggregationRow` 序列化为嵌套 JSON `{"groups": {...}, "aggregations": {...}}`，而 DAO 层的 `sum_tokens` 等语法糖方法期望扁平结构。

**解决：** 在 DAO 实现中手动展平：
```rust
let mut obj = serde_json::Map::new();
for (k, v) in &r.groups { obj.insert(k.clone(), v.clone()); }
for (k, v) in &r.aggregations { obj.insert(k.clone(), serde_json::Value::from(*v)); }
```

### 2. json_extract 字符串值带引号

**问题：** DuckDB 的 `json_extract` 返回 JSON 类型，字符串值被序列化为 `"\"provider-test\""`，导致 group by 比较失败。

**解决：** 使用 `json_extract_string(tags, '$.field')` 替代 `json_extract`，返回纯字符串标量值。

### 3. dyn ToSql 的 Send 问题

**问题：** `dyn ToSql` 不是 `Send + Sync`，无法直接在 async 函数中作为参数传递。

**解决：** 引入 `StatParam` 枚举替代 `dyn ToSql`，在 `query()` 内部转换为 `&dyn ToSql`。

### 4. Stats 不可变问题

**问题：** `RequestContext::stats()` 返回 `&Stats`，但 `record` 方法需要 `&mut self`。

**解决：** 测试中使用独立的 `&mut Stats` 写入数据，然后注入到 `RequestContext`。

---

## DAL 层集成示例

在现有 `AgentDalImpl` 中添加依赖：

```rust
pub struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao>,
}

impl AgentDal {
    pub async fn get_agent_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        time_range: Option<(i64, i64)>,
        interval: Option<StatsInterval>,
    ) -> Result<(TokenSumResult, Option<Vec<TimeSeriesPoint>>)> {
        let sum_query = AgentStatsQuery {
            agent_id: agent_id.to_string(),
            filters: vec![],
            time_range,
            group_by: vec![],
            aggregations: vec![],
            interval: None,
        };
        let sum = self.agent_stats_dao.sum_tokens(ctx.clone(), sum_query).await?;

        let time_series = if let Some(interval) = interval {
            let ts_query = AgentStatsQuery {
                agent_id: agent_id.to_string(),
                filters: vec![],
                time_range,
                group_by: vec![],
                aggregations: vec![],
                interval: Some(interval),
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

只需要新增 `service/dao/{entity}/stats_duckdb.rs`，按相同模式实现即可，不需要修改其他地方代码。

### 新增查询维度

在现有 DAO 新增方法即可，不影响其他代码。

### 自定义复杂查询

可以直接调用通用 `query()` 方法，灵活组合过滤、分组、聚合，返回原始 JSON。

---

## 记录事件规范

#### 模型调用统计

在 rig hook `on_completion_response` 中自动发送 `ModelCallEvent`，使用专用字段 API：

```rust
let event = ModelCallEvent::new(timestamp)
    .with_agent_id(agent_id)
    .with_project_id(project_id)
    .with_model_provider_id(model_provider_id)
    .with_tokens_input(usage.input_tokens)
    .with_tokens_output(usage.output_tokens)
    .with_total_tokens(usage.total_tokens);

ctx.stats().record(ctx.clone(), event).await?;
```

#### 工具调用统计

在 `ToolCallLoggingDecorator` 中统一发送 `ToolCallEvent`，覆盖 manual + auto 模式：

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

#### 默认事件（灵活场景）

对于未确定专用表的灵活打点场景，仍可使用 `DefaultStatEvent` 写入 `default_events` 表：

```rust
record_event!(ctx, DefaultStatEvent {
    tags: json!({ "event": "custom_metric" }),
    metrics: json!({ "value": 42 }),
}).await?;
```

---

## 版本信息

| 版本 | 日期 | 作者 | 变更 |
|------|------|------|------|
| v1.0 | 2026-06-28 | 讨论确定 | 初始设计 |
| v1.1 | 2026-07-02 | 实现迭代 | Agent Stats DAO 实现完成；统计模型迁移到 common/src/models；接口从三个独立结构体改为统一 AgentStatsQuery；补充实现踩坑记录 |
| v1.2 | 2026-07-02 | 全实体覆盖 | 新增 Project/Task/ModelProvider 三个 Stats DAO；每个 DAO 4 个单元测试（sum_tokens/time_series/aggregation/filter_isolation）；共 16 个 stats 测试 |
| v1.3 | 2026-07-02 | 事件关联优化 | Stats 新增 get_table_name<E> 方法；查询方法新增 table_name 参数；DAO trait 使用关联类型绑定事件类型，提供默认 table_name 方法；实现写入和查询的事件→表名一致性 |
| v1.4 | 2026-07-03 | 专用事件拆分 | DAO trait 拆分为 ModelCallEvent / ToolCallEvent 双关联类型；query_model_calls / query_tool_calls 分别查询专用表；专用事件独立文件（model_call.rs / tool_call.rs）；rig hook 自动采集模型调用和工具调用统计 |
| v1.5 | 2026-07-03 | 表自描述重构 | StatTable trait 新增 is_dedicated_table / column_sql / metric_sql / filter_equals_sql / filter_range_sql；专用表改用独立字段结构（VARCHAR/BIGINT）；查询构建逻辑下放到表实现，消除硬编码表结构判断；Stats 新增 tables_by_name 反向索引；ToolCallLoggingDecorator 统一采集工具调用统计；RequestContext::stats_opt() 安全获取 |
