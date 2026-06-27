# 统计模块设计 — Stats 基于 DuckDB

## 定位与目标

统一的**嵌入式多维统计数据收集存储模块**，为项目中各种打点统计提供通用框架：

> **核心设计思想：**
> 1. 框架提供通用能力：连接管理、批量写入、扩展接口
> 2. 用户可以完全自定义：Event 结构、Table 结构，满足不同场景
> 3. 默认提供开箱即用：`DefaultStatEvent` + `DefaultStatTable`
> 4. 支持四种组合场景：默认/自定义 × 默认/自定义

**支持的使用场景：**
- Agent 按天统计调用次数、Token 消耗
- ModelProvider 全局累计统计
- 工具调用成功失败计数
- Task 总轮次 Token 统计
- 任意自定义打点监控

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

### StatTable - 统计表 trait

每个统计表实现这个 trait，对应 DuckDB 中的一张表：

```rust
pub trait StatTable<E: StatEvent>: Send + Sync + Debug {
    /// 表名
    fn table_name(&self) -> &str;

    /// 创建表（如果不存在），初始化 schema
    async fn create_table(&self, conn: &mut duckdb::Connection) -> Result<()>;

    /// 插入单个事件
    async fn insert_event(
        &self,
        conn: &mut duckdb::Connection,
        event: &E,
    ) -> Result<()>;

    /// 批量插入事件
    async fn bulk_insert_events(
        &self,
        conn: &mut duckdb::Connection,
        events: &[E],
    ) -> Result<()>;
}
```

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

## 顶层 Stats 结构

```rust
pub struct Stats {
    conn: Mutex<Connection>,
    tables: HashMap<TypeId, (String, Box<dyn ErasedStatTable>, ErasedBuffer)>,
    batch_size: usize,
}

impl Stats {
    /// 打开数据库并初始化
    pub async fn open(path: &str, batch_size: usize) -> Result<Self>;

    /// 初始化默认表（for DefaultStatEvent）
    pub fn initialize_default(&mut self) -> Result<()>;

    /// 注册自定义表，自动按事件类型绑定
    pub fn register_table<E: StatEvent + 'static + Send + Sync, T: StatTable<E> + 'static>(
        &mut self,
        table: T,
    ) -> Result<()>;

    /// 记录事件（自动按事件类型找到注册的表）
    /// 遵循项目约定：第一个参数就是 ctx
    pub async fn record<E: StatEvent + 'static + Send + Sync>(
        &mut self,
        ctx: RequestContext,
        event: E,
    ) -> Result<()>;

    /// 强制刷新所有缓冲
    pub async fn flush_all(&mut self, ctx: RequestContext) -> Result<()>;

    /// 执行查询 SQL，返回 JSON 结果
    pub async fn query(
        &self,
        ctx: RequestContext,
        sql: &str,
        params: &[&dyn duckdb::ToSql],
    ) -> Result<Vec<serde_json::Value>>;

    /// 获取指定事件类型待缓冲长度
    pub fn pending_buffer_len<E: StatEvent + 'static>(&self) -> usize;

    /// 获取注册的表数量
    pub fn registered_table_count(&self) -> usize;
}
```

**核心设计变化：每个事件类型自动绑定唯一一张表**，注册之后用户不需要每次调用都指定表，直接 `record(event)` 就行，更简洁。

**内部用类型擦除支持不同表不同事件类型，对用户透明。**

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

## 当前已实现

- [x] duckdb-rs 1.4 升级适配，解决所有 API 不兼容问题
- [x] 核心设计：按事件类型自动绑定表，每个事件类型对应唯一表
- [x] `Stats` 自动缓冲，批量写入，每个事件类型独立缓冲
- [x] `record_event!` 宏简化调用，自动推断表，自动填充 timestamp
- [x] `#[derive(StatsEvent)]` 过程宏自动实现 trait
- [x] 所有单元测试通过 ✅

## 开放性问题

1. **过期数据清理**：可以后续增加定时清理 `default_events` 旧数据的功能，第一版本不做要求。
2. **异步运行**：DuckDB 本身是同步 API，当前实现直接在 async 上下文运行，因为批量写入频率不高，不会阻塞太久。如果后续发现瓶颈，可以用 `tokio::task::spawn_blocking` 优化。
