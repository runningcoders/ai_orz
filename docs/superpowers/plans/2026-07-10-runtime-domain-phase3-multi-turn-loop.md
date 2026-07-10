# Phase 3: Multi-Turn Loop Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement multi-turn conversation loop control for agents, including turn limits, task completion detection, prompt context differentiation, and tool failure tracking.

**Architecture:** Turn limiting and completion detection live in the consumer layer (above Runtime Domain). Stats are queried through DAL layer via "fetch options" pattern — callers can either query stats directly or get them as side data when fetching an Agent entity. ToolStatsDao follows the same pattern as AgentStatsDao.

**Tech Stack:** Rust, Axum, SQLx, DuckDB stats module

---

## File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `src/service/dao/tool/mod.rs` | Add `ToolStatsDao` trait and `ToolStatsQuery` struct | Modify |
| `src/service/dao/tool/stats_duckdb.rs` | DuckDB implementation of ToolStatsDao | Create |
| `src/service/dao/tool/stats_duckdb_test.rs` | Tests for ToolStatsDao | Create |
| `src/service/dal/tool.rs` | Add `get_stats` method to ToolDal, inject ToolStatsDao | Modify |
| `src/models/agent.rs` | Add `stats: AgentStats` field to Agent entity | Modify |
| `src/service/dal/agent.rs` | Add `AgentFetchOptions` struct, extend find_by_id/query with options, inject stats | Modify |
| `src/service/domain/hr/agent_manage.rs` | Extend `get_agent` with `AgentFetchOptions` parameter | Modify |
| `src/consumer/message.rs` | Add turn limit check + task completion check in handle_agent_message | Modify |
| `src/service/domain/runtime/context_assembly.rs` | Differentiate current_message by MessageType | Modify |
| `src/service/domain/runtime/awakening.rs` | Inject tool failure count into prompt; record failed awake events | Modify |
| `common/src/models/stats.rs` | Add `ToolStats` struct if needed | Modify |

---

## Task 1: ToolStatsDao trait + DuckDB implementation

**Files:**
- Modify: `src/service/dao/tool/mod.rs`
- Create: `src/service/dao/tool/stats_duckdb.rs`
- Create: `src/service/dao/tool/stats_duckdb_test.rs`

Follows the exact same pattern as [AgentStatsDao](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/agent/mod.rs#L47-L120) / [AgentStatsDaoDuckDbImpl](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/agent/stats_duckdb.rs).

Data source: `tool_call_events` table (already created by `ToolCallStatTable` in [tool_call.rs](file:///Users/aman/Technology/rust/ai_orz/src/pkg/stats/tool_call.rs#L118-L210)).

### ToolStatsQuery fields:
- `tool_id: String` (required, primary filter)
- `agent_id: Option<String>` (optional secondary filter)
- `filters: Vec<StatFilter>` (extra filters)
- `time_range: Option<(i64, i64)>`
- `aggregations: Vec<StatAggregation>` (internal use)

### ToolStatsDao trait methods:
- `type ToolCallEvent: StatEvent`
- `fn table_name(&self, stats: &Stats) -> Option<String>`
- `async fn query_tool_calls(&self, ctx: RequestContext, query: ToolStatsQuery) -> Result<Vec<JsonValue>>`
- `async fn sum_calls(&self, ctx: RequestContext, query: ToolStatsQuery) -> Result<u64>` — total calls
- `async fn sum_failed_calls(&self, ctx: RequestContext, query: ToolStatsQuery) -> Result<u64>` — calls where status="failed"
- `async fn get_stats(&self, ctx: RequestContext, query: ToolStatsQuery, options: StatsFetchOptions) -> Result<ToolStats>`

### ToolStats struct (add to common/src/models/stats.rs):
```rust
#[derive(Debug, Clone, Default)]
pub struct ToolStats {
    pub call_summary: Option<CallSummary>,
    pub failed_count: Option<u64>,
}
```

- [ ] **Step 1: Add ToolStats to common models**

Add to `common/src/models/stats.rs`:

```rust
/// 工具统计结果
#[derive(Debug, Clone, Default)]
pub struct ToolStats {
    /// 调用汇总
    pub call_summary: Option<CallSummary>,
    /// 失败次数
    pub failed_count: Option<u64>,
}
```

Run: `cargo check -p common`
Expected: PASS (just adding a struct)

- [ ] **Step 2: Add ToolStatsQuery and ToolStatsDao trait to tool DAO module**

Add to `src/service/dao/tool/mod.rs` (before the ToolDao trait section):

```rust
use common::models::{ToolStats, StatsFetchOptions};
use crate::pkg::stats::{StatAggregation, StatEvent, Stats};

/// 工具统计查询参数
#[derive(Debug, Clone, Default)]
pub struct ToolStatsQuery {
    /// 工具 ID（必填）
    pub tool_id: String,
    /// Agent ID（可选过滤）
    pub agent_id: Option<String>,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 聚合函数（内部使用）
    pub aggregations: Vec<StatAggregation>,
}

/// 工具统计 DAO 接口
///
/// 数据来源：tool_call_events 表
#[async_trait::async_trait]
pub trait ToolStatsDao: Send + Sync {
    /// 工具调用事件类型
    type ToolCallEvent: StatEvent + 'static + Send + Sync;

    /// 获取事件表名
    fn table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ToolCallEvent>()
    }

    /// 底层通用查询（内部使用）
    async fn query_tool_calls(&self, ctx: RequestContext, query: ToolStatsQuery) -> Result<Vec<JsonValue>>;

    /// 工具总调用次数
    async fn sum_calls(&self, ctx: RequestContext, mut query: ToolStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_tool_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 工具失败调用次数
    async fn sum_failed_calls(&self, ctx: RequestContext, mut query: ToolStatsQuery) -> Result<u64> {
        use crate::pkg::stats::StatFilter;
        query.filters.push(StatFilter::Equals {
            key: "status".to_string(),
            value: JsonValue::String("failed".to_string()),
        });
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_tool_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 获取工具统计数据
    async fn get_stats(&self, ctx: RequestContext, query: ToolStatsQuery, options: StatsFetchOptions) -> Result<ToolStats> {
        let mut stats = ToolStats::default();

        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = ToolStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = ToolStatsQuery {
                    time_range: Some((start, end)),
                    ..query.clone()
                };
                let range_calls = self.sum_calls(ctx.clone(), range_query).await?;
                let duration_secs = (end - start) as f64 / 1000.0;
                if duration_secs > 0.0 { Some(range_calls as f64 / duration_secs) } else { None }
            } else {
                None
            };

            stats.call_summary = Some(CallSummary {
                total_calls,
                instant_qps,
                avg_qps,
                peak_qps: None,
            });
        }

        if options.with_call_summary {
            // failed_count 也在 with_call_summary 下返回，保持和 call_summary 一致
            let failed = self.sum_failed_calls(ctx, query).await?;
            stats.failed_count = Some(failed);
        }

        Ok(stats)
    }
}
```

Run: `cargo check`
Expected: FAIL (ToolStatsDao trait references missing imports, also missing `CallSummary` import and `StatFilter`)

- [ ] **Step 3: Fix imports in tool/mod.rs**

Add these imports at the top of `src/service/dao/tool/mod.rs`:

```rust
use common::error::Result;
use common::models::{CallSummary, ToolStats, StatsFetchOptions};
use crate::models::tool::ToolPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatEvent, Stats};
use serde_json::Value as JsonValue;
```

Run: `cargo check`
Expected: FAIL (no DuckDB implementation yet, but trait should compile)

- [ ] **Step 4: Create DuckDB implementation**

Create `src/service/dao/tool/stats_duckdb.rs`:

```rust
//! ToolStatsDao DuckDB 实现

use common::error::Result;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, ToolCallEvent};
use crate::service::dao::tool::{ToolStatsDao, ToolStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static TOOL_STATS_DAO: OnceLock<Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>> = OnceLock::new();

pub fn stats_new() -> Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>> {
    Arc::new(ToolStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>> {
    TOOL_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = TOOL_STATS_DAO.set(stats_new());
}

struct ToolStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl ToolStatsDao for ToolStatsDaoDuckDbImpl {
    type ToolCallEvent = ToolCallEvent;

    async fn query_tool_calls(&self, ctx: RequestContext, mut query: ToolStatsQuery) -> Result<Vec<JsonValue>> {
        let tool_filter = StatFilter::Equals {
            key: "tool_id".to_string(),
            value: JsonValue::String(query.tool_id.clone()),
        };
        query.filters.insert(0, tool_filter);

        if let Some(agent_id) = &query.agent_id {
            let agent_filter = StatFilter::Equals {
                key: "agent_id".to_string(),
                value: JsonValue::String(agent_id.clone()),
            };
            query.filters.insert(1, agent_filter);
        }

        let stats = ctx.stats();
        let table_name = self.table_name(stats);

        let rows = ctx.stats().query_aggregation(
            ctx.clone(),
            table_name.as_deref(),
            &query.filters,
            &[],
            &query.aggregations,
            query.time_range,
        ).await?;

        Ok(rows.iter().map(|r| {
            let mut obj = serde_json::Map::new();
            for (k, v) in &r.aggregations {
                obj.insert(k.clone(), serde_json::Value::from(*v));
            }
            JsonValue::Object(obj)
        }).collect())
    }
}
```

Run: `cargo check`
Expected: FAIL (need to add `stats_init` call in tool module, and `ToolCallEvent` import path may need adjustment)

- [ ] **Step 5: Wire up stats_init in tool DAO module**

Add to `src/service/dao/tool/mod.rs` at the bottom (module exports):

```rust
pub mod stats_duckdb;
```

And check that the existing `init()` function calls `stats_duckdb::stats_init()`.

Run: `cargo check`
Expected: PASS

- [ ] **Step 6: Write unit tests for ToolStatsDao**

Create `src/service/dao/tool/stats_duckdb_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::pkg::stats::*;
    use crate::service::dao::tool::stats_duckdb::stats_new;
    use crate::service::dao::tool::{ToolStatsDao, ToolStatsQuery};
    use common::models::StatsFetchOptions;
    use crate::pkg::RequestContext;

    fn setup() -> (Stats, RequestContext) {
        let stats = Stats::new_in_memory().unwrap();
        stats.register_table(ToolCallStatTable).unwrap();
        let ctx = RequestContext::new(None, None).with_stats(stats.clone());
        (stats, ctx)
    }

    fn insert_events(stats: &Stats, tool_id: &str, agent_id: &str, count: u64, status: &str) {
        let mut events = Vec::new();
        for i in 0..count {
            let mut event = ToolCallEvent::new(1000000 + i as i64);
            event.tool_id = tool_id.to_string();
            event.tool_name = format!("tool_{}", tool_id);
            event.agent_id = Some(agent_id.to_string());
            event.status = status.to_string();
            event.duration_ms = 100 + i;
            events.push(event);
        }
        stats.bulk_record(&events).unwrap();
    }

    #[tokio::test]
    async fn test_sum_calls_empty() {
        let (_stats, ctx) = setup();
        let dao = stats_new();
        let query = ToolStatsQuery {
            tool_id: "nonexistent".to_string(),
            ..Default::default()
        };
        let count = dao.sum_calls(ctx, query).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_sum_calls_with_data() {
        let (stats, ctx) = setup();
        insert_events(&stats, "tool_1", "agent_1", 5, "success");
        let dao = stats_new();
        let query = ToolStatsQuery {
            tool_id: "tool_1".to_string(),
            ..Default::default()
        };
        let count = dao.sum_calls(ctx, query).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_sum_failed_calls() {
        let (stats, ctx) = setup();
        insert_events(&stats, "tool_1", "agent_1", 3, "success");
        insert_events(&stats, "tool_1", "agent_1", 2, "failed");
        let dao = stats_new();
        let query = ToolStatsQuery {
            tool_id: "tool_1".to_string(),
            ..Default::default()
        };
        let failed = dao.sum_failed_calls(ctx, query).await.unwrap();
        assert_eq!(failed, 2);
    }

    #[tokio::test]
    async fn test_get_stats_with_call_summary() {
        let (stats, ctx) = setup();
        insert_events(&stats, "tool_1", "agent_1", 10, "success");
        insert_events(&stats, "tool_1", "agent_1", 3, "failed");
        let dao = stats_new();
        let query = ToolStatsQuery {
            tool_id: "tool_1".to_string(),
            ..Default::default()
        };
        let options = StatsFetchOptions {
            with_call_summary: true,
            ..Default::default()
        };
        let stats_result = dao.get_stats(ctx, query, options).await.unwrap();
        assert!(stats_result.call_summary.is_some());
        assert_eq!(stats_result.call_summary.unwrap().total_calls, 13);
        assert_eq!(stats_result.failed_count, Some(3));
    }

    #[tokio::test]
    async fn test_filter_by_agent_id() {
        let (stats, ctx) = setup();
        insert_events(&stats, "tool_1", "agent_1", 5, "success");
        insert_events(&stats, "tool_1", "agent_2", 3, "success");
        let dao = stats_new();
        let query = ToolStatsQuery {
            tool_id: "tool_1".to_string(),
            agent_id: Some("agent_1".to_string()),
            ..Default::default()
        };
        let count = dao.sum_calls(ctx, query).await.unwrap();
        assert_eq!(count, 5);
    }
}
```

Run: `cargo test -p ai-orz tool::stats_duckdb_test -- --nocapture`
Expected: 5 tests PASS

- [ ] **Step 7: Commit**

```bash
git add common/src/models/stats.rs src/service/dao/tool/mod.rs src/service/dao/tool/stats_duckdb.rs src/service/dao/tool/stats_duckdb_test.rs
git commit -m "feat(stats): add ToolStatsDao with DuckDB implementation"
```

---

## Task 2: ToolDal get_stats method

**Files:**
- Modify: `src/service/dal/tool.rs`

Follows the same pattern as [AgentDal::get_stats](file:///Users/aman/Technology/rust/ai_orz/src/service/dal/agent.rs#L183-L190).

- [ ] **Step 1: Add ToolStatsDao to ToolDalImpl struct and constructor**

Modify `src/service/dal/tool.rs`:

Add import:
```rust
use crate::service::dao::tool::{ToolDao, ToolQuery, ToolStatsDao, ToolStatsQuery};
use common::models::{ToolStats, StatsFetchOptions};
```

Add field to ToolDalImpl:
```rust
tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>,
```

Update `new()` and `init()` to accept and inject `tool_stats_dao`.

- [ ] **Step 2: Add get_stats method to ToolDal trait**

Add to the ToolDal trait:
```rust
/// 获取工具统计数据
async fn get_stats(&self, ctx: RequestContext, tool_id: &str, options: StatsFetchOptions) -> Result<ToolStats>;
```

Implement in ToolDalImpl:
```rust
async fn get_stats(&self, ctx: RequestContext, tool_id: &str, options: StatsFetchOptions) -> Result<ToolStats> {
    let query = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        time_range: options.time_range,
        ..Default::default()
    };
    self.tool_stats_dao.get_stats(ctx, query, options).await
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/dal/tool.rs
git commit -m "feat(dal): add get_stats to ToolDal"
```

---

## Task 3: AgentFetchOptions + stats injection in AgentDal

**Files:**
- Modify: `src/models/agent.rs` — add `stats: AgentStats` field
- Modify: `src/service/dal/agent.rs` — add AgentFetchOptions, extend find_by_id/query

This is the first implementation of the "fetch options" pattern, following the spec in [LAYERED_ARCHITECTURE_PRACTICE.md](file:///Users/aman/Technology/rust/ai_orz/docs/LAYERED_ARCHITECTURE_PRACTICE.md).

- [ ] **Step 1: Add stats field to Agent entity**

Modify `src/models/agent.rs`, add to the Agent struct:
```rust
/// 统计信息（由 DAL 层按需注入）
///
/// None 表示未注入
pub stats: Option<AgentStats>,
```

Update `Agent::from_po` to set `stats: None`.

Run: `cargo check`
Expected: PASS (new field with default None, existing code should compile)

- [ ] **Step 2: Add AgentFetchOptions to DAL**

Add to `src/service/dal/agent.rs`:

```rust
/// Agent 附带信息获取选项
///
/// 控制在获取 Agent 实体时，是否同时加载和注入额外的附带信息。
/// 所有字段都是 Option<bool>，None 表示使用默认值。
#[derive(Debug, Clone, Default)]
pub struct AgentFetchOptions {
    /// 是否加载运行时状态（默认 true，保持现有行为）
    pub with_runtime_state: Option<bool>,
    /// 是否加载统计信息
    pub with_stats: Option<bool>,
    /// 统计过滤条件（with_stats=true 时生效）
    pub stats_task_id: Option<String>,
}
```

- [ ] **Step 3: Extend find_by_id with options**

Change the `find_by_id` signature in the trait:
```rust
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>>;
```
→ 
```rust
async fn find_by_id(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>>;
```

Update the implementation to conditionally inject stats:
```rust
async fn find_by_id(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>> {
    let opt = self.agent_dao.find_by_id(ctx.clone(), id).await?;
    let Some(mut agent) = opt.map(Agent::from_po) else {
        return Ok(None);
    };

    // 注入运行时状态（默认 true）
    if options.with_runtime_state.unwrap_or(true) {
        agent = Self::inject_runtime_state(agent);
    }

    // 注入统计信息（默认 false）
    if options.with_stats.unwrap_or(false) {
        let mut stats_query = AgentStatsQuery {
            agent_id: agent.po.id.clone(),
            ..Default::default()
        };
        if let Some(task_id) = &options.stats_task_id {
            stats_query.filters.push(crate::pkg::stats::StatFilter::Equals {
                key: "task_id".to_string(),
                value: serde_json::Value::String(task_id.clone()),
            });
        }
        let stats_options = StatsFetchOptions {
            with_call_summary: true,
            ..Default::default()
        };
        let stats = self.agent_stats_dao.get_stats(ctx, stats_query, stats_options).await?;
        agent.stats = Some(stats);
    }

    Ok(Some(agent))
}
```

- [ ] **Step 4: Update all callers of find_by_id**

Find all callers of `find_by_id` and add `AgentFetchOptions::default()` to keep existing behavior (runtime_state=true, stats=false).

Key callers to check:
- `HrDomain::agent_manage().get_agent()` — likely calls `agent_dal.find_by_id()`
- Test files

Run: `cargo check`
Expected: FAIL until all callers are updated

- [ ] **Step 5: Update query method similarly**

Add options parameter to `query` method, same pattern.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: All 548 tests PASS

- [ ] **Step 7: Commit**

```bash
git add src/models/agent.rs src/service/dal/agent.rs
git commit -m "feat(dal): add AgentFetchOptions with stats injection pattern"
```

---

## Task 4: AgentStatsQuery with task_id filter

Wait — we already added task_id filtering via the `filters: Vec<StatFilter>` in AgentStatsQuery. The stats_task_id in AgentFetchOptions converts to a StatFilter.

But let's verify AgentStatsQuery can filter by task_id — looking at agent_awake_events table... let's check if AgentAwakeEvent has task_id field.

**Actually, check first:** Does `AgentAwakeEvent` have a `task_id` field? Let's verify:

From the awakening.rs code, we see `AgentAwakeEvent` is used but we need to check its definition.

- [ ] **Step 1: Verify AgentAwakeEvent has task_id**

Check `src/pkg/stats/agent_awake.rs` (or wherever it's defined). If it doesn't have task_id, we need to add it.

If task_id is missing:
- Add `task_id: Option<String>` field to AgentAwakeEvent struct
- Add `#[tag]` attribute
- Update builder method
- Update create_table + insert_event in the StatTable impl
- Update the record_event! call site in awakening.rs to include task_id

Run: `cargo test`
Expected: PASS

- [ ] **Step 2: Commit**

```bash
git add src/pkg/stats/agent_awake.rs src/service/domain/runtime/awakening.rs
git commit -m "feat(stats): add task_id to AgentAwakeEvent"
```

---

## Task 5: Turn limit check in consumer

**Files:**
- Modify: `src/consumer/message.rs`
- Modify: `src/service/domain/hr/agent_manage.rs` — extend `get_agent` with options

Add turn count check in `handle_agent_message` before calling awaken.

Domain 层直接扩展现有的 `get_agent` 方法，增加 options 参数，和 DAL 层的设计保持一致。这样未来新增任何附带信息都不需要加新方法，只需要扩展 options 结构体。

- [ ] **Step 1: Extend get_agent with options in HrDomain::AgentManage**

在 AgentManage trait 中扩展 `get_agent` 方法签名，增加 options 参数：

```rust
// 旧签名
async fn get_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Option<Agent>>;

// 新签名
async fn get_agent(&self, ctx: RequestContext, agent_id: &str, options: AgentFetchOptions) -> Result<Option<Agent>>;
```

`AgentFetchOptions` 从 DAL 层导出（`crate::service::dal::agent::AgentFetchOptions`）。

实现中直接透传给 `agent_dal.find_by_id(ctx, agent_id, options)`。

- [ ] **Step 2: Update all callers of get_agent**

查找所有调用 `get_agent` 的地方，添加 `AgentFetchOptions::default()` 保持现有行为。

关键调用方：
- `src/consumer/message.rs` 中的 `handle_agent_message`
- Handler 层的各种 agent 查询接口
- 测试文件

Run: `cargo check`
Expected: FAIL until all callers updated

- [ ] **Step 3: Add turn limit check in handle_agent_message**

Modify `handle_agent_message` in `src/consumer/message.rs`:

加载 Agent 时使用带 stats 的 options：

```rust
// 加载 Agent 实体（包含统计信息，用于轮次判断）
let agent = self
    .hr_domain
    .agent_manage()
    .get_agent(
        ctx.clone(),
        agent_id,
        crate::service::dal::agent::AgentFetchOptions {
            with_stats: Some(true),
            stats_task_id: message.po.task_id.clone(),
            ..Default::default()
        },
    )
    .await?
    .ok_or_else(|| Error::not_found(format!("Agent {} not found", agent_id)))?;
```

然后在调用 awaken 之前检查轮次限制：

1. If message has task_id, extract it
2. Check agent.stats.call_summary.total_calls
3. Compare with agent.po.runtime_config.max_thinking_depth
4. If exceeded, send a warning message to user and return Ok (ack the message without awakening)

Pseudo-code:
```rust
// 检查轮次限制
if let Some(task_id) = &message.po.task_id {
    if let Some(stats) = &agent.stats {
        if let Some(call_summary) = &stats.call_summary {
            let max_depth = agent.po.runtime_config.max_thinking_depth as u64;
            if call_summary.total_calls >= max_depth {
                // 超过最大轮次，给用户发提示消息
                log_warn!(&ctx, "handle_agent_message", 
                    "Agent {} reached max thinking depth {} for task {}",
                    agent_id, max_depth, task_id);
                // 发送提示消息给用户
                self.message_domain.delivery().send_to_user(...).await?;
                return Ok(());
            }
        }
    }
}
```

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: Update consumer new_for_test if needed**

Make sure test code compiles.

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/consumer/message.rs src/service/domain/hr/agent_manage.rs
git commit -m "feat(runtime): add turn limit check in message consumer"
```

---

## Task 6: mark_done / task completion detection

**Files:**
- Modify: `src/consumer/message.rs`

If message has task_id, check task status before awakening. If task is Completed, ack the message without awakening.

- [ ] **Step 1: Add task status check in handle_agent_message**

In `handle_agent_message`, after rebuilding context and before loading agent (or after):

```rust
// 检查 task 是否已完成
if let Some(task_id) = &message.po.task_id {
    // 通过 ProjectDomain 查询 task 状态
    let task = self.project_domain.task_manage().get_task(ctx.clone(), task_id).await?;
    if let Some(task) = task {
        if task.po.status == TaskStatus::Completed {
            log_info!(&ctx, "handle_agent_message",
                "Task {} is completed, skipping awakening for agent {}",
                task_id, agent_id);
            return Ok(());
        }
    }
}
```

Wait — does the message consumer have access to ProjectDomain? Currently it has `runtime_domain`, `message_domain`, `hr_domain`. We need to add `project_domain`.

- [ ] **Step 2: Add project_domain to MessageHandlerImpl**

Add `project_domain: Arc<dyn crate::service::domain::project::ProjectDomain>` to MessageHandlerImpl.

Update `new()` and `new_for_test()` constructors.

- [ ] **Step 3: Add get_task to ProjectDomain::TaskManage**

If it doesn't already exist, add a method to fetch a task by id.

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/consumer/message.rs
git commit -m "feat(runtime): skip awakening when task is completed"
```

---

## Task 7: Prompt context differentiation by message type

**Files:**
- Modify: `src/service/domain/runtime/context_assembly.rs`

Make PromptBuilder's `current_message()` format the message differently based on MessageType:
- Text → "用户消息: {content}"
- ToolCallResult → "工具调用结果: {tool_name}\n结果: {result}"
- AgentMessage → "来自 Agent {from_id} 的消息: {content}"

- [ ] **Step 1: Modify current_message method in PromptBuilder**

Add Message parameter or message_type + content parameters. Format based on type.

- [ ] **Step 2: Update caller in awakening.rs**

Pass the full Message or relevant fields to current_message.

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/context_assembly.rs src/service/domain/runtime/awakening.rs
git commit -m "feat(runtime): differentiate prompt context by message type"
```

---

## Task 8: Tool failure count injection into prompt (P2)

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

When loading neural tools, also query failure stats for each tool. If a tool has had multiple consecutive failures, add a note in the prompt.

This is P2 — lower priority. Can be skipped for initial phase 3 delivery.

- [ ] **Step 1: Query tool failure stats in awakening**

For each neural tool and each bound tool, query failure count (filtered by agent_id + time_range like "last 24h").

- [ ] **Step 2: Add failure notes to prompt**

If any tool has > N failures, add a system note: "注意: 以下工具近期调用失败率较高，请谨慎使用: ..."

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "feat(runtime): inject tool failure warnings into prompt"
```

---

## Task 9: Failed awake event recording (P2)

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

Currently AgentAwakeEvent is only recorded on success. Record it on failure too, with status="failed".

- [ ] **Step 1: Update awakening.rs error handling**

Wrap the think call in a match, record event in both success and failure branches.

- [ ] **Step 2: Verify status field exists on AgentAwakeEvent**

If AgentAwakeEvent doesn't have status field, add it.

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/awakening.rs src/pkg/stats/agent_awake.rs
git commit -m "feat(stats): record failed awake events"
```

---

## Task 10: Final verification

- [ ] **Step 1: Run full test suite**

```bash
cargo test
```
Expected: All tests PASS (548+ new tests = ~558+)

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```
Expected: No warnings

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat(runtime): phase 3 multi-turn loop control complete"
```
