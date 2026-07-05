//! Agent DAO 模块

use common::error::Result;
use common::models::{AgentStats, CallSummary, StatsFetchOptions};
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use common::enums::AgentStatus;
use crate::pkg::stats::{StatFilter, StatAggregation, StatEvent, Stats};
use serde_json::Value as JsonValue;

/// Agent 查询参数
#[derive(Debug, Clone, Default)]
pub struct AgentQuery {
    pub name: Option<String>,
    pub status: Option<AgentStatus>,
    pub exclude_status: Option<AgentStatus>,
    pub created_by: Option<String>,
    pub model_provider_id: Option<String>,
    pub limit: Option<usize>,
}

/// Agent 统计查询参数
#[derive(Debug, Clone, Default)]
pub struct AgentStatsQuery {
    /// Agent ID（必填）
    pub agent_id: String,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 聚合函数（内部使用）
    pub aggregations: Vec<StatAggregation>,
}

/// Agent DAO 接口
#[async_trait::async_trait]
pub trait AgentDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<()>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AgentPo>>;
    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: AgentQuery)
    -> Result<Vec<AgentPo>>;
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<AgentPo>>;
    async fn update(&self, ctx: RequestContext, agent: &AgentPo) -> Result<()>;
    async fn delete(&self, ctx: RequestContext, agent: &AgentPo) -> Result<()>;
}

/// Agent 统计 DAO 接口
///
/// 只负责 Agent 自身维度的统计（目前只有调用次数汇总）。
/// 模型调用相关的统计（token、时序等）由 ModelProviderStatsDao 负责。
#[async_trait::async_trait]
pub trait AgentStatsDao: Send + Sync {
    /// 模型调用事件类型
    type ModelCallEvent: StatEvent + 'static + Send + Sync;

    /// 获取模型调用表名（从 Stats 注册表中查询）
    fn model_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ModelCallEvent>()
    }

    /// 底层通用查询（内部使用，不对外暴露业务语义）
    async fn query_model_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<JsonValue>>;

    /// 模型调用总次数
    async fn sum_calls(&self, ctx: RequestContext, mut query: AgentStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_model_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 获取 Agent 自身统计数据
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
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = AgentStatsQuery {
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
                avg_qps,
                instant_qps,
            });
        }

        Ok(stats)
    }
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod stats_duckdb_test;