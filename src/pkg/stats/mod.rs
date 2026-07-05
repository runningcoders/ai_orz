//! Stats 统计模块 — 基于 DuckDB 的多维统计收集
//!
//! # Examples
//! ```ignore
//! use ai_orz::pkg::stats::{Stats, DefaultStatEvent, DefaultStatTable};
//! use serde_json::json;
//! use chrono::Utc;
//!
//! # async fn example() -> common::error::Result<()> {
//! let mut stats = Stats::open("./stats.duckdb", 100).await?;
//! stats.register_table(DefaultStatTable);
//!
//! let event = DefaultStatEvent {
//!     timestamp: Utc::now().timestamp_nanos_opt().unwrap() as i64,
//!     tags: json!({ "agent_id": "agent-123", "model_provider_id": "provider-456" }),
//!     metrics: json!({ "tokens_input": 1024, "tokens_output": 256 }),
//! };
//!
//! let ctx = RequestContext::default();
//! stats.record(ctx, &DefaultStatTable, event).await?;
//! # Ok(())
//! # }
//! ```

#![warn(clippy::all)]

mod traits;
mod erased;
mod default;
mod stats;
mod model_call;
mod tool_call;
mod agent_awake;
mod project_event;
mod task_event;

pub use common::models::{StatsInterval, TimeSeriesPoint, TokenSumResult};
pub use self::default::{DefaultStatEvent, DefaultStatTable};
pub use self::model_call::{ModelCallEvent, ModelCallStatTable};
pub use self::tool_call::{ToolCallEvent, ToolCallStatTable};
pub use self::agent_awake::{AgentAwakeEvent, AgentAwakeStatTable};
pub use self::project_event::{ProjectEvent, ProjectStatTable};
pub use self::task_event::{TaskEvent, TaskStatTable};
pub use self::stats::{
    Stats,
    StatParam,
    StatFilter,
    StatAggregation,
    AggregationRow,
};
pub use self::traits::{StatEvent, StatTable};

// Re-export the derive macro
pub use ai_orz_macros::StatsEvent;

/// 方便快捷记录统计事件宏
///
/// # Usage
///
/// 1. **自动推断表（推荐）** → 默认使用 `DefaultStatTable`，你的事件自动注册到默认表：
/// ```ignore
/// record_event!(ctx, ModelCallEvent {
///     model_provider_id: id,
///     agent_id: Some(agent_id),
///     tokens_input: input,
///     tokens_output: output,
/// });
/// ```
///
/// 2. **自动推断表 + 自定义 timestamp**:
/// ```ignore
/// record_event!(ctx, ModelCallEvent {
///     timestamp: custom_ts,
///     model_provider_id: id,
///     ...
/// });
/// ```
///
/// 3. **显式指定自定义表**:
/// ```ignore
/// record_event!(ctx, &MyCustomTable, ModelCallEvent { ... });
/// ```
#[macro_export]
macro_rules! record_event {
    // 情况 1: 只有 ctx 和 event → 自动根据事件类型找到注册的表，直接记录，最简！
    ($ctx:expr, $event_type:ident { $($tt:tt)* }) => {
        async {
            let event = $crate::pkg::stats::record_event_helper!($event_type, $($tt)*);
            if let Some(stats) = $ctx.stats_opt() {
                stats.record($ctx.clone(), event).await
            } else {
                Ok(())
            }
        }.await
    };

    // 情况 2: ctx + event 结构体表达式 → 不用构造，直接传
    ($ctx:expr, $event:expr) => {
        async {
            if let Some(stats) = $ctx.stats_opt() {
                stats.record($ctx.clone(), $event).await
            } else {
                Ok(())
            }
        }.await
    };

    // 情况 3: 显式指定表（兼容旧代码，依然支持）
    ($ctx:expr, $table:expr, $event_type:ident { $($tt:tt)* }) => {
        async {
            let event = $crate::pkg::stats::record_event_helper!($event_type, $($tt)*);
            if let Some(stats) = $ctx.stats_opt() {
                stats.record_with_table($ctx.clone(), $table, event).await
            } else {
                Ok(())
            }
        }.await
    };
}

/// 内部 helper 宏：处理结构体初始化，自动添加 timestamp 如果没有
#[macro_export]
macro_rules! record_event_helper {
    // 情况 A: 用户已经提供了 timestamp
    ($event_type:ident, timestamp: $ts:expr, $($field:ident: $value:expr),* $(,)?) => {
        $event_type {
            timestamp: $ts,
            $($field: $value),*
        }
    };

    // 情况 B: 用户没有提供 timestamp，自动填充当前时间
    ($event_type:ident, $($field:ident: $value:expr),* $(,)?) => {
        $event_type {
            timestamp: ::std::time::SystemTime::now()
                .duration_since(::std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            $($field: $value),*
        }
    };
}

pub use record_event;
pub use record_event_helper;

#[cfg(test)]
mod stats_test;
