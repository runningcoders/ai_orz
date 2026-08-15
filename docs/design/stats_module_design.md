# 统计模块设计 — 双层实现：DuckDB 持久化 + 内存实时

> 🎯 **本文档定位**：嵌入式多维统计数据收集存储框架设计——DuckDB 持久化版（跨重启/复杂 SQL）+ 内存实时版（重启重置/零 DB 依赖）双层互补
> 状态：v1.0（2026-08-15 整理）
> 查阅场景：新增统计打点类型、排查批量写入/连接管理、理解持久化与内存版数据一致性语义时打开；具体框架实现看 pkg/stats/
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构（Stats 是基础设施层模块）
> - [stats_query_design.md](./stats_query_design.md) — 统计查询模块（为前端提供数据查询的 Domain 层封装）

## 定位与目标

统一的**嵌入式多维统计数据收集存储模块**，为项目中各种打点统计提供通用框架。

`pkg/stats/` 提供**双层互补**实现：

| 层级 | 位置 | 适用场景 | 生命周期 | 查询能力 |
|------|------|----------|----------|----------|
| **持久化版** | `pkg/stats/` 顶层 | 业务事件统计（Agent/Project/Task/ModelProvider/Tool） | 跨重启保留 | 复杂 SQL 聚合、时序查询 |
| **内存版** | `pkg/stats/runtime/` 子模块 | 运行时能力统计（AOP/SSE/Channel） | 重启重置 | 快照式查询，零 DB 依赖 |

> **核心设计思想：**
> 1. 框架提供通用能力：连接管理、批量写入、扩展接口
> 2. 用户可以完全自定义：Event 结构、Table 结构，满足不同场景
> 3. 默认提供开箱即用：`DefaultStatEvent` + `DefaultStatTable`
> 4. 支持四种组合场景：默认/自定义 × 默认/自定义
> 5. **双层互补**：业务事件用持久化版，运行时能力用内存版

**支持的使用场景：**
- Agent 按天统计调用次数、Token 消耗（持久化版）
- ModelProvider 全局累计统计（持久化版）
- 工具调用成功失败计数（持久化版）
- Task 总轮次 Token 统计（持久化版）
- 任意自定义打点监控（持久化版）
- AOP 事件队列运行时统计（内存版，已接入）
- SSE/WS 连接数监控（内存版，待接入）
- Channel 推送指标（内存版，待接入）

## 核心 trait 设计

### StatEvent - 统计事件 trait

所有统计事件都需要实现这个 trait：

```rust
pub trait StatEvent: Send + Sync + Debug {
    /// 获取时间戳（毫秒），必须实现
    fn timestamp(&self) -> i64;

    /// 获取事件类型名称，默认用 type_name
    fn event_type(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// 获取标签引用（可选）
    fn tags(&self) -> Option<&serde_json::Value> {
        None
    }

    /// 获取标签 JSON（如果自定义组装，默认使用 tags()）
    fn tags_json(&self) -> Option<serde_json::Value> {
        self.tags().cloned()
    }

    /// 获取指标引用（可选）
    fn metrics(&self) -> Option<&serde_json::Value> {
        None
    }

    /// 获取指标 JSON（如果自定义组装，默认使用 metrics()）
    fn metrics_json(&self) -> Option<serde_json::Value> {
        self.metrics().cloned()
    }
}
```

**默认方法**让用户只需要实现必须的部分。

### `#[derive(StatsEvent)]` 宏

推荐使用 `ai_orz_macros::StatsEvent` derive 宏自动实现 `StatEvent` trait，通过字段注解声明标签和指标，零样板代码。

**使用示例**：
```rust
use ai_orz_macros::StatsEvent;

#[derive(Debug, Clone, StatsEvent)]
#[event_type = "tool_call"]
pub struct ToolCallEvent {
    #[timestamp]
    timestamp: i64,
    #[tag]
    tool_id: String,
    #[tag]
    agent_id: Option<String>,
    #[metric]
    duration_ms: u64,
    #[metric]
    status: String,
}
```

**结构体级注解**：
| 注解 | 说明 |
|------|------|
| `#[event_type = "xxx"]` | 自定义事件类型名称，不写则用 type_name |

**字段级注解**：
| 注解 | 说明 | 支持类型 |
|------|------|----------|
| `#[timestamp]` | 时间戳字段（必须且只能一个） | `i64` |
| `#[tag]` | 标签维度字段（可多个） | `String` / `Option<String>` |
| `#[metric]` | 指标字段（可多个） | 数值 / `String` |

**类型处理**：
- `Option<String>` tag：为 `None` 时自动跳过，不插入空值
- `String` tag/metric：直接转换为 JSON String
- 数值 metric：直接转换为 JSON Number

**已使用的事件**：
- `ModelCallEvent` — 模型调用统计
- `ToolCallEvent` — 工具调用统计

### StatTable - 统计表 trait

每个统计表实现这个 trait，对应 DuckDB 中的一张表：

```rust
pub trait StatTable<E: StatEvent>: Send + Sync + Debug {
    /// 表名
    fn table_name(&self) -> &str;

    /// 创建表（如果不存在），初始化 schema
    fn create_table(&self, conn: &mut duckdb::Connection) -> Result<()>;

    /// 插入单个事件
    fn insert_event(&self, conn: &mut duckdb::Connection, event: &E) -> Result<()>;

    /// 批量插入事件
    fn bulk_insert_events(&self, conn: &mut duckdb::Connection, events: &[E]) -> Result<()>;

    /// 是否是专用表结构（有独立字段，而非 tags/metrics JSON）
    fn is_dedicated_table(&self) -> bool { false }

    /// 获取标签/维度列的 SQL 引用方式
    /// 默认表：json_extract_string(tags, '$.column')
    /// 专用表：直接字段名
    fn column_sql(&self, column: &str) -> String { ... }

    /// 获取指标列的 SQL 引用方式
    /// 默认表：json_extract(metrics, '$.metric')
    /// 专用表：直接字段名
    fn metric_sql(&self, metric: &str) -> String { ... }

    /// 获取过滤条件（等于匹配）的 SQL 列引用方式
    fn filter_equals_sql(&self, column: &str) -> String { ... }

    /// 获取过滤条件（范围匹配）的 SQL 列引用方式
    fn filter_range_sql(&self, column: &str) -> String { ... }
}
```

**表自描述设计：** 查询构建方法（`build_aggregation_query`、`query_time_series`、`append_filters`）不再硬编码判断表结构，而是通过 `StatTable` 的元数据方法（`column_sql`、`metric_sql`、`filter_equals_sql`、`filter_range_sql`）获取 SQL 生成策略，让每种表实现与自身表结构紧密绑定。新增专用表时只需覆盖 `is_dedicated_table()` 返回 `true`，默认方法自动切换为直接字段引用。

## 支持四种组合场景

完全灵活，覆盖所有使用场景：

| 场景 | Event | Table | 适用情况 |
|------|-------|-------|----------|
| 1️⃣ **默认开箱即用** | `DefaultStatEvent` | `DefaultStatTable` | 快速打点，灵活扩展 |
| 2️⃣ **自定义 Event + 默认表** | 用户自定义 `MyEvent` | `DefaultStatTable` | 强类型 Event，不想单独建表 |
| 3️⃣ **默认 Event + 自定义表** | `DefaultStatEvent` | 用户自定义 `MyTable` | 默认 Event 结构，需要单独表隔离 |
| 4️⃣ **自定义 Event + 自定义表** | 用户自定义 `MyEvent` | 用户自定义 `MyTable` | 高性能需求，完全自定义 |

## 默认开箱即用实现

### DefaultStatEvent

通用事件结构，标签和指标都是 JSON：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DefaultStatEvent {
    pub timestamp: i64,
    pub tags: serde_json::Value,
    pub metrics: serde_json::Value,
}

impl StatEvent for DefaultStatEvent {
    fn timestamp(&self) -> i64 { self.timestamp }
    fn tags(&self) -> Option<&serde_json::Value> { Some(&self.tags) }
    fn metrics(&self) -> Option<&serde_json::Value> { Some(&self.metrics) }
}
```

### DefaultStatTable

默认通用表，存储所有没有自定义表的事件：

```sql
CREATE TABLE IF NOT EXISTS default_events (
    id UUID PRIMARY KEY,
    timestamp BIGINT,
    event_type VARCHAR,
    tags JSON,
    metrics JSON
);
```

**支持任何实现了 `StatEvent` 的类型存入默认表**，所以"自定义 Event + 默认表"场景开箱即用。

## 专用统计事件

针对确定的业务场景，项目内置了两个专用事件类型，各自绑定独立表，与默认表分离：

| 事件类型 | 表名 | 文件 | 用途 |
|----------|------|------|------|
| `ModelCallEvent` | `model_call_events` | `pkg/stats/model_call.rs` | LLM 模型调用统计（token 用量、调用次数） |
| `ToolCallEvent` | `tool_call_events` | `pkg/stats/tool_call.rs` | 工具调用统计（调用次数、参数/结果大小） |

**设计原则：**
- 默认表（`default_events`）保留给灵活场景使用，默认 event 仍走默认表处理
- 专用事件数据只写入各自的专用表，互不干扰
- 每个专用事件独立一个文件，方便后续查阅和扩展
- 专用表使用独立字段（VARCHAR/BIGINT）而非 JSON tags/metrics，查询性能更优

**专用表结构（以 `model_call_events` 为例）：**

```sql
CREATE TABLE IF NOT EXISTS model_call_events (
    timestamp BIGINT,
    agent_id VARCHAR,
    project_id VARCHAR,
    task_id VARCHAR,
    model_provider_id VARCHAR,
    model_name VARCHAR,
    organization_id VARCHAR,
    user_id VARCHAR,
    tokens_input BIGINT,
    tokens_output BIGINT,
    total_tokens BIGINT
);
```

**专用表的 `StatTable` 实现：**

覆盖 `is_dedicated_table()` 返回 `true`，继承的 `column_sql()`、`metric_sql()`、`filter_equals_sql()`、`filter_range_sql()` 自动切换为直接字段引用模式，无需额外代码。

`initialize_default()` 同时注册三张表：

```rust
pub fn initialize_default(&self) -> Result<()> {
    self.register_table(DefaultStatTable)?;
    self.register_table(ModelCallStatTable)?;
    self.register_table(ToolCallStatTable)?;
    Ok(())
}
```

### rig hook 自动采集

`RuntimeMonitoringHook`（`pkg/monitoring/rig_hook.rs`）在 rig 运行时回调中自动发送专用事件：

- `on_completion_response` → 发送 `ModelCallEvent`，使用专用字段 API（`with_agent_id`、`with_tokens_input` 等）

### 工具调用统一采集

`ToolCallLoggingDecorator`（`pkg/tool_tracing/tool_call_logger.rs`）包装所有工具调用（manual + auto 模式），在调用完成后发送 `ToolCallEvent`，使用专用字段 API。使用 `stats_opt()` 安全获取 Stats，未初始化时优雅跳过。

## 顶层 Stats 结构

```rust
pub struct Stats {
    conn: Mutex<Connection>,
    tables: Mutex<HashMap<TypeId, (String, Arc<dyn ErasedStatTable>, ErasedBuffer)>>,
    tables_by_name: Mutex<HashMap<String, Arc<dyn ErasedStatTable>>>,
    batch_size: usize,
}

impl Stats {
    /// 打开数据库并初始化
    pub async fn open(path: &str, batch_size: usize) -> Result<Self>;

    /// 初始化默认表（DefaultStatTable + ModelCallStatTable + ToolCallStatTable）
    pub fn initialize_default(&self) -> Result<()>;

    /// 注册自定义表，自动按事件类型绑定
    pub fn register_table<E: StatEvent + 'static + Send + Sync, T: StatTable<E> + 'static>(
        &self,
        table: T,
    ) -> Result<()>;

    /// 按事件类型获取表名
    pub fn get_table_name<E>(&self) -> Option<String>;

    /// 按表名获取 ErasedStatTable 元数据（用于查询构建）
    pub fn get_table_by_name(&self, name: &str) -> Option<Arc<dyn ErasedStatTable>>;

    /// 记录事件（自动按事件类型找到注册的表）
    pub async fn record<E: StatEvent + 'static + Send + Sync>(
        &self,
        ctx: RequestContext,
        event: E,
    ) -> Result<()>;

    /// 强制刷新所有缓冲
    pub async fn flush_all(&self, ctx: RequestContext) -> Result<()>;

    /// 通用聚合查询（通过 ErasedStatTable 元数据构建 SQL）
    pub async fn query_aggregation(
        &self, ctx: RequestContext, table_name: Option<&str>,
        filters: &[StatFilter], group_by: &[&str],
        aggregations: &[StatAggregation], time_range: Option<(i64, i64)>,
    ) -> Result<Vec<AggregationRow>>;

    /// 时序查询（通过 ErasedStatTable 元数据构建 SQL）
    pub async fn query_time_series(
        &self, ctx: RequestContext, table_name: Option<&str>,
        filters: &[StatFilter], interval: StatsInterval, time_range: (i64, i64),
    ) -> Result<Vec<TimeSeriesPoint>>;

    /// 执行查询 SQL，返回 JSON 结果
    pub async fn query(
        &self, ctx: RequestContext, sql: &str, params: &[StatParam],
    ) -> Result<Vec<serde_json::Value>>;
}
```

**核心设计变化：**
- **表自描述**：查询构建方法不再硬编码判断表结构，而是通过 `get_table_by_name()` 获取 `ErasedStatTable`，调用其 `column_sql()`/`metric_sql()`/`filter_equals_sql()`/`filter_range_sql()` 生成 SQL
- **反向索引**：`tables_by_name` 支持按表名查找表元数据，`register_table` 时同步写入
- **每个事件类型自动绑定唯一一张表**，注册之后用户不需要每次调用都指定表，直接 `record(event)` 就行

**遵循项目统一约定：**
- 所有操作方法第一个参数必须是 `ctx: RequestContext`
- 连接生命周期由 Storage 统一管理，不单独持有
- 完全可测试，上下文可以串联日志追踪

## 批量写入设计

- 每个表有独立的缓冲队列
- 调用 `record()` 只写入内存缓冲
- 缓冲大小达到 `batch_size` 自动刷盘
- 用户可以手动调用 `flush_all()` / `flush_table()` 强制刷盘
- 批量写入提高 DuckDB 写入性能

## 宏扩展：简化自定义 Event 实现

为了降低用户自定义 Event 的写入难度，可以提供宏自动实现 `StatEvent` trait：

```rust
#[derive(Debug, Clone, StatsEvent)]
#[stats(table = "model_calls")]
pub struct ModelCallEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub model_provider_id: String,
    #[tag]
    pub agent_id: Option<String>,
    #[metric]
    pub tokens_input: i64,
    #[metric]
    pub tokens_output: i64,
}
```

宏自动生成：
```rust
impl StatEvent for ModelCallEvent {
    fn timestamp(&self) -> i64 {
        self.timestamp
    }

    fn event_type(&self) -> &str {
        "ModelCallEvent"
    }

    fn tags_json(&self) -> Option<serde_json::Value> {
        Some(json!({
            "model_provider_id": &self.model_provider_id,
            "agent_id": &self.agent_id,
        }))
    }

    fn metrics_json(&self) -> Option<serde_json::Value> {
        Some(json!({
            "tokens_input": self.tokens_input,
            "tokens_output": self.tokens_output,
        }))
    }
}
```

**用户只需要定义结构体，加上属性宏，就完成了！** 不需要手动写 boilerplate。

## 使用示例

### 示例 1：默认开箱即用

```rust
let mut stats = Stats::open("./stats.duckdb", 100).await?;
stats.initialize_default(); // 自动注册 DefaultStatEvent → DefaultStatTable

let event = DefaultStatEvent {
    timestamp: std::time::SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64,
    tags: json!({
        "agent_id": "agent-123",
        "model_provider_id": "provider-456",
    }),
    metrics: json!({
        "tokens_input": 1024,
        "tokens_output": 256,
        "tool_calls": 2,
    }),
};

// 最简！自动按事件类型找表，不用指定
stats.record(ctx, event).await?;
```

### 示例 1b：宏最简写法

```rust
// 自动填充 timestamp，自动找表，一句话搞定
record_event!(ctx, DefaultStatEvent {
    tags: json!({ "agent_id": "agent-123", "model_provider_id": "provider-456" }),
    metrics: json!({ "tokens_input": 1024, "tokens_output": 256 }),
}).await?;
```

### 示例 2：自定义 Event + 默认表（宏写法）

```rust
#[derive(Debug, Clone, StatsEvent)]
pub struct ApiCallEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub endpoint: String,
    #[tag]
    pub method: String,
    #[metric]
    pub latency_ms: f64,
    #[metric]
    pub status_code: u16,
}

// 注册：用户自定义 Event，自动绑定到默认表？
// 不，用户自定义 Event 需要自己注册自定义表，或者我们默认：如果没有注册，自动用默认表？
// 当前设计：必须注册，你自定义 Event 必须注册对应表，默认表就是 DefaultStatTable
stats.register_table(DefaultStatTable);

// 使用：最简，不用指定表
record_event!(ctx, ApiCallEvent {
    endpoint: "/api/v1/agent".to_string(),
    method: "POST".to_string(),
    latency_ms: 123.4,
    status_code: 200,
}).await?;
```

### 示例 3：自定义 Event + 自定义表

```rust
// 1. 定义 Event，宏自动实现 StatEvent
#[derive(Debug, Clone, StatsEvent)]
pub struct ModelCallEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub model_provider_id: String,
    #[tag]
    pub agent_id: Option<String>,
    #[metric]
    pub tokens_input: i64,
    #[metric]
    pub tokens_output: i64,
}

// 2. 定义 Table
pub struct ModelCallTable;

impl StatTable<ModelCallEvent> for ModelCallTable {
    fn table_name(&self) -> &str { "model_calls" }

    fn create_table(&self, conn: &mut duckdb::Connection) -> Result<()> {
        conn.execute("
            CREATE TABLE IF NOT EXISTS model_calls (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                event_type VARCHAR,
                tags JSON,
                metrics JSON,
                model_provider_id VARCHAR,
                agent_id VARCHAR,
                tokens_input BIGINT,
                tokens_output BIGINT
            );
        ", []).map_err(Into::into)
    }

    fn insert_event(
        &self,
        conn: &mut duckdb::Connection,
        event: &ModelCallEvent,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO model_calls VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                &Uuid::new_v4().to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &"ModelCallEvent".to_string() as &dyn ToSql,
                &event.tags_json() as &dyn ToSql,
                &event.metrics_json() as &dyn ToSql,
                &event.model_provider_id as &dyn ToSql,
                &event.agent_id as &dyn ToSql,
                &event.tokens_input as &dyn ToSql,
                &event.tokens_output as &dyn ToSql,
            ],
        ).map_err(Into::into)
    }

    fn bulk_insert_events(
        &self,
        conn: &mut duckdb::Connection,
        events: &[ModelCallEvent],
    ) -> Result<()> {
        for event in events {
            self.insert_event(conn, event)?;
        }
        Ok(())
    }
}

// 3. 注册并使用 → 注册之后自动绑定 ModelCallEvent → ModelCallTable
stats.register_table(ModelCallTable);

// 最简使用！自动找到对应的表，不用指定
record_event!(ctx, ModelCallEvent {
    model_provider_id: "openai".to_string(),
    agent_id: Some("agent-123".to_string()),
    tokens_input: 1024,
    tokens_output: 256,
}).await?;
```
            ),
        ).map_err(Into::into)
    }
}

// 3. 注册并使用
stats.register_table(ModelCallTable);
stats.record(&ModelCallTable, ModelCallEvent { ... }).await?;
```

## 集成到项目

### 配置

在 `common/src/config.rs` 增加配置：

```rust
/// Stats 数据库配置
#[derive(Debug, Clone, Deserialize)]
pub struct StatsConfig {
    /// Stats DuckDB 文件路径（相对于 BASE_DATA_PATH）
    pub db_file_name: String,
    /// 批量写入缓冲大小
    pub batch_size: usize,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            db_file_name: "stats.duckdb".to_string(),
            batch_size: 100,
        }
    }
}
```

### 初始化

在 `src/pkg/storage/mod.rs` 增加 Stats 初始化：

```rust
pub struct Storage {
    // ... 已有字段
    stats: once_cell::sync::OnceCell<Stats>,
}

impl Storage {
    pub async fn init(&mut self, config: &Config) -> Result<()> {
        // ... 已有初始化

        // 初始化 Stats
        let stats_path = format!("{}/{}", self.base_data_path, config.stats.db_file_name);
        let mut stats = Stats::open(&stats_path, config.stats.batch_size).await?;
        stats.register_table(DefaultStatTable);
        self.stats.set(stats).map_err(|_| ...)?;

        Ok(())
    }

    /// 获取 Stats 引用
    pub fn stats(&self) -> &Stats {
        self.stats.get().unwrap()
    }
}
```

### 使用（从任何地方）

```rust
use crate::pkg::stats::{Stats, DefaultStatEvent, record_event};
use serde_json::json;

// 从 RequestContext 获取 Stats 可变引用
let stats = ctx.stats_mut();

// 最简宏写法，自动填充 timestamp
record_event!(ctx, DefaultStatEvent {
    tags: json!({
        "agent_id": "agent-123",
        "model_provider_id": "provider-456",
    }),
    metrics: json!({
        "tokens_input": 1024,
        "tokens_output": 256,
        "tool_calls": 2,
    }),
});
```

**现在 API 更简洁：**
- `stats.record(ctx, event)` → 自动按事件类型找到注册的表
- `record_event!` 宏自动帮你构造事件，不需要写全 timestamp
- 完全不用每次指定表

## 数据表位置

- DuckDB 文件放在：`{BASE_DATA_PATH}/stats.duckdb`
- 和主业务 SQLite 数据库分离，互不干扰

## 设计优势

1. **极致扩展性**：支持任意自定义 Event 和 Table，框架不限制
2. **自动绑定**：每个事件类型自动绑定唯一一张表，注册之后不用每次指定，使用更简洁
3. **默认开箱即用**：简单打点直接用 `DefaultStatEvent` + `DefaultStatTable`，调用只需要一句话
4. **交叉场景支持**：自定义 Event 可以用默认表，满足大部分场景
5. **性能优化**：批量写入，自定义强类型表可以获得更好性能
6. **宏简化**：`#[derive(StatsEvent)]` 自动生成 trait 实现，零样板代码
7. **符合项目架构**：放在 `pkg/stats`，旁路监控不入侵核心业务 DAO/DAL 分层
8. **多维聚合**：DuckDB 原生支持任意维度 SQL 聚合查询，满足统计分析需求
9. **线程安全**：内部用 `Mutex` 保护连接和缓冲，支持多线程并发写入

## 统计查询 API

在通用 `Stats` 上提供高层查询方法，底层使用 DuckDB SQL，上层返回结构化结果：

### `get_table_name<E>` — 通过事件类型获取表名

```rust
pub fn get_table_name<E>(&self) -> Option<&str>
where
    E: StatEvent + 'static + Send + Sync,
```

从 Stats 注册表中获取指定事件类型对应的表名。DAO 层通过关联类型绑定事件类型后，调用此方法获取表名，保证写入和查询使用相同的表。

### `query_aggregation` — 通用聚合查询

```rust
pub async fn query_aggregation(
    &self,
    ctx: RequestContext,
    table_name: Option<&str>,  // 新增：None 默认为 "default_events"
    filters: &[StatFilter],
    group_by: &[&str],
    aggregations: &[StatAggregation],
    time_range: Option<(i64, i64)>,
) -> Result<Vec<AggregationRow>>
```

**实现细节：**
- `group_by` 字段使用 `json_extract_string(tags, '$.field')` 提取，避免字符串值带引号
- 聚合函数支持 `Count`、`Sum(metric)`、`Avg(metric)`
- 返回 `AggregationRow { groups, aggregations }`，其中 groups 按 group_by 字段名组织

### `query_time_series` — 时序查询

```rust
pub async fn query_time_series(
    &self,
    ctx: RequestContext,
    table_name: Option<&str>,  // 新增：None 默认为 "default_events"
    filters: &[StatFilter],
    interval: StatsInterval,
    time_range: (i64, i64),
) -> Result<Vec<TimeSeriesPoint>>
```

**实现细节：**
- 时间戳按 interval 截断：`timestamp / interval_ms * interval_ms`
- 自动聚合 `tokens_input`、`tokens_output`、`call_count`
- 返回结构化 `TimeSeriesPoint` 数组

### `StatParam` — 类型安全的 SQL 参数

```rust
pub enum StatParam {
    Int(i64),
    Double(f64),
    Str(String),
}
```

替代 `dyn ToSql`，解决 `Send + Sync` 问题，在 `query()` 内部转换为 `&dyn ToSql`。

## 内存版运行时统计（runtime 子模块）

`pkg/stats/runtime/` 子模块提供**纯内存**统计收集能力，作为持久化版（DuckDB）的轻量补充。

### 设计动机

某些统计场景**没有持久化价值**：
- AOP 事件队列运行时状态：重启后事件本身就丢失了
- SSE/WS 连接数：实时变化，重启即归零
- Channel 推送指标：瞬时指标才有意义

这些场景用 DuckDB 持久化：
1. 浪费磁盘 IO（每秒写入无意义）
2. 数据增长率失控（无淘汰策略）
3. 查询需求简单（只需最近 60 分钟时序 + 当前分布）

因此引入纯内存实现，**重启即重置**，与运行时能力本身的生命周期一致。

### 核心类型

```rust
// src/pkg/stats/runtime/mod.rs

pub struct RuntimeStatsCollector<K> {
    inner: Arc<RwLock<Inner<K>>>,
}

impl<K: Clone + Eq + Hash + Send + Sync + Debug + 'static> RuntimeStatsCollector<K> {
    pub fn new() -> Self;
    pub async fn record(&self, key: K, duration: Option<u64>);
    pub async fn snapshot(&self) -> RuntimeStatsSnapshot<K>;
    pub async fn uptime_secs(&self) -> u64;
}

pub struct RuntimeStatsSnapshot<K> {
    pub total_counts: HashMap<K, u64>,
    pub buckets: Vec<TimeBucketSnapshot<K>>,
    pub total_duration_ms: u64,
    pub total_completed: u64,
    pub started_at: i64,
}
```

### 关键设计

**1. 泛型维度键 K**：业务层自由选择键类型（`String` / 元组 / struct），约束 `Clone + Eq + Hash + Send + Sync + Debug + 'static`。

**2. `record` 的 `duration: Option<u64>`**：
- `None` — 只计数，不累计耗时（如 "published" 状态）
- `Some(ms)` — 计数 + 累计耗时（如 "success"/"failed" 状态）

业务层决定哪些事件计时，避免在框架层硬编码"哪些状态是终止状态"。

**3. `snapshot()` 返回深拷贝**：调用方释放读锁后安全处理，避免锁竞争。业务层在快照基础上实现专属聚合（按 status 分类、按维度 group by 等）。

**4. 滑动窗口 60 分钟**：按分钟桶，自动淘汰过期数据。内存占用估算：60 桶 × 每桶 ~20 个维度组合 × 32 字节 ≈ 38KB。

**5. 总计数器全生命周期**：`total_counts` / `total_duration_ms` / `total_completed` 不受滑动窗口限制，进程重启才重置。

### 接入示例

以 AOP 接入为例：

```rust
// 1. 定义维度键（业务层 wrap 类型）
type AopDimKey = (String, String, String); // (event_kind, consumer_name, status)

// 2. wrap RuntimeStatsCollector，实现专属聚合
pub struct AopStatsCollector {
    inner: RuntimeStatsCollector<AopDimKey>,
}

impl AopStatsCollector {
    pub async fn record(&self, kind: &str, consumer: &str, status: &str, duration_ms: u64) {
        let key = (kind.to_string(), consumer.to_string(), status.to_string());
        let duration = if status == "success" || status == "failed" {
            Some(duration_ms)
        } else {
            None
        };
        self.inner.record(key, duration).await;
    }

    pub async fn overview(&self) -> AopOverview {
        let snap = self.inner.snapshot().await;
        // 在 snapshot 基础上做 AOP 专属聚合（按 status 分类）
        // ...
    }
}
```

### 双访问路径

`pkg/stats/mod.rs` 同时提供：
- 完整路径 `crate::pkg::stats::runtime::RuntimeStatsCollector` — 明确区分子模块（推荐）
- 短路径 `crate::pkg::stats::RuntimeStatsCollector` — 通过 `pub use` re-export（便于简化导入）

### 适用场景判断

| 场景特征 | 选择 |
|----------|------|
| 需要跨重启保留的历史数据 | 持久化版 |
| 需要 SQL 聚合、复杂过滤 | 持久化版 |
| 数据量可控、有淘汰策略需求 | 内存版（60 分钟窗口） |
| 实时监控、前端轮询渲染 | 内存版 |
| 重启即重置是可接受的 | 内存版 |

## 当前已实现

- [x] duckdb-rs 1.4 升级适配，解决所有 API 不兼容问题
- [x] 核心设计：按事件类型自动绑定表，每个事件类型对应唯一表
- [x] `Stats` 自动缓冲，批量写入，每个事件类型独立缓冲
- [x] `record_event!` 宏简化调用，自动推断表，自动填充 timestamp
- [x] `query_aggregation` 通用聚合查询（支持过滤、分组、聚合）
- [x] `query_time_series` 时序查询（支持 Hourly/Daily）
- [x] `StatParam` 类型安全参数枚举（解决 `dyn ToSql` 的 `Send` 问题）
- [x] 统计结果模型迁移到 `common/src/models/stats.rs`（`StatsInterval`、`TimeSeriesPoint`、`TokenSumResult`）
- [x] 专用事件 `ModelCallEvent` / `ToolCallEvent` 独立文件，各自绑定专用表
- [x] 专用表使用独立字段结构（VARCHAR/BIGINT），查询性能更优
- [x] `StatTable` trait 表自描述：`is_dedicated_table()`、`column_sql()`、`metric_sql()`、`filter_equals_sql()`、`filter_range_sql()`
- [x] 查询构建逻辑下放到表实现，消除硬编码表结构判断
- [x] `Stats` 反向索引 `tables_by_name`，支持按表名查找 `ErasedStatTable` 元数据
- [x] rig hook 自动采集模型调用统计（`ModelCallEvent`）
- [x] `ToolCallLoggingDecorator` 统一采集工具调用统计（`ToolCallEvent`，覆盖 manual + auto）
- [x] `RequestContext::stats_opt()` 安全获取 Stats
- [x] 所有单元测试通过 ✅

### 内存版 runtime 子模块（pkg/stats/runtime/）

- [x] `RuntimeStatsCollector<K>` 泛型内存收集器（滑动窗口 60 分钟 + 总计数器全生命周期）
- [x] `record(key, duration: Option<u64>)` 灵活计时（None 不累计，Some 累计）
- [x] `snapshot()` 返回深拷贝，释放读锁后业务层安全聚合
- [x] `uptime_secs()` 收集器运行时长
- [x] `pkg/stats/mod.rs` 双访问路径（完整 `runtime::RuntimeStatsCollector` + 短路径 re-export）
- [x] 8 个泛型核心单元测试通过 ✅
- [x] AOP 已接入（`AopStatsCollector` wrap `RuntimeStatsCollector<(String, String, String)>`）
- [x] AOP 7 个 wrap 层 + hook 集成测试通过 ✅

## 开放性问题

1. **过期数据清理**：可以后续增加定时清理 `default_events` 旧数据的功能，第一版本不做要求。
2. **异步运行**：DuckDB 本身是同步 API，当前实现直接在 async 上下文运行，因为批量写入频率不高，不会阻塞太久。如果后续发现瓶颈，可以用 `tokio::task::spawn_blocking` 优化。
