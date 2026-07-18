//! Model Provider DAO 模块

use common::error::Result;
use common::models::{StatsInterval, TimeSeriesPoint, TokenSumResult, ModelCallStats, CallSummary, StatsFetchOptions};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, AggregationRow, StatEvent, Stats};
use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};
use serde_json::Value as JsonValue;

/// ModelProvider 查询参数
#[derive(Debug, Clone, Default)]
pub struct ModelProviderQuery {
    pub provider_type: Option<ProviderType>,
    pub capability: Option<ModelCapability>,
    pub status: Option<ModelProviderStatus>,
    pub exclude_status: Option<ModelProviderStatus>,
    pub limit: Option<usize>,
}

/// Model Provider DAO 接口
#[async_trait::async_trait]
pub trait ModelProviderDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, provider: &ModelProviderPo)
    -> Result<()>;
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProviderPo>>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ModelProviderQuery,
    ) -> Result<Vec<ModelProviderPo>>;

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>>;
    async fn update(&self, ctx: RequestContext, provider: &ModelProviderPo)
    -> Result<()>;
    async fn delete(&self, ctx: RequestContext, provider: &ModelProviderPo)
    -> Result<()>;

    /// 获取默认的 Embedding Provider（第一个可用的）
    async fn get_default_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>>;

    /// 获取当前启用的 Embedding Provider（用于唯一性校验）
    async fn find_enabled_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>>;
}

/// ModelProvider 统计查询参数（统一结构体，覆盖所有查询场景）
///
/// 支持按多个维度过滤，所有维度都是可选的，可以组合使用。
#[derive(Debug, Clone, Default)]
pub struct ModelProviderStatsQuery {
    /// ModelProvider ID（可选）
    pub model_provider_id: Option<String>,
    /// Agent ID（可选，按 Agent 维度过滤）
    pub agent_id: Option<String>,
    /// Project ID（可选，按 Project 维度过滤）
    pub project_id: Option<String>,
    /// Task ID（可选，按 Task 维度过滤）
    pub task_id: Option<String>,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 分组字段（聚合查询专用）
    pub group_by: Vec<String>,
    /// 聚合函数（聚合查询专用）
    pub aggregations: Vec<StatAggregation>,
    /// 时间间隔（时序查询专用）
    pub interval: Option<StatsInterval>,
}

/// ModelProvider 统计 DAO 接口
#[async_trait::async_trait]
pub trait ModelProviderStatsDao: Send + Sync {
    type ModelCallEvent: StatEvent + 'static + Send + Sync;

    fn model_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ModelCallEvent>()
    }

    async fn query_model_calls(&self, ctx: RequestContext, query: ModelProviderStatsQuery) -> Result<Vec<JsonValue>>;

    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: ModelProviderStatsQuery) -> Result<Vec<AggregationRow>> {
        let group_by = query.group_by.clone();
        let rows = self.query_model_calls(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_aggregation_row(&row, &group_by));
        }
        Ok(result)
    }

    async fn query_model_call_time_series(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
        if query.interval.is_none() {
            query.interval = Some(StatsInterval::Daily);
        }
        let rows = self.query_model_calls(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_time_series_point(&row));
        }
        Ok(result)
    }

    async fn sum_tokens(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<TokenSumResult> {
        query.group_by = vec![];
        query.aggregations = vec![
            StatAggregation::Sum("tokens_input".into()),
            StatAggregation::Sum("tokens_output".into()),
            StatAggregation::Count,
        ];
        query.interval = None;
        let rows = self.query_model_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(TokenSumResult {
                total_tokens_input: 0,
                total_tokens_output: 0,
                total_calls: 0,
            });
        }
        let row = &rows[0];
        Ok(TokenSumResult {
            total_tokens_input: row.get("tokens_input").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64,
            total_tokens_output: row.get("tokens_output").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64,
            total_calls: row.get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64,
        })
    }

    async fn get_stats(&self, ctx: RequestContext, query: ModelProviderStatsQuery, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let mut stats = ModelCallStats {
            call_summary: None,
            token_summary: None,
            model_call_time_series: None,
        };

        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = ModelProviderStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = ModelProviderStatsQuery {
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

        if options.with_token_summary {
            stats.token_summary = Some(self.sum_tokens(ctx.clone(), query.clone()).await?);
        }

        if options.with_time_series {
            let mut ts_query = query;
            ts_query.time_range = options.time_range;
            ts_query.interval = options.interval.or(Some(StatsInterval::Daily));
            stats.model_call_time_series = Some(self.query_model_call_time_series(ctx, ts_query).await?);
        }

        Ok(stats)
    }

    async fn sum_calls(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<u64> {
        query.group_by = vec![];
        query.aggregations = vec![StatAggregation::Count];
        query.interval = None;
        let rows = self.query_model_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }
}

/// 解析 JSON 为 AggregationRow
fn parse_aggregation_row(row: &JsonValue, group_by: &[String]) -> AggregationRow {
    use std::collections::HashMap;
    let obj = match row {
        JsonValue::Object(o) => o,
        _ => return AggregationRow { groups: HashMap::new(), aggregations: HashMap::new() },
    };

    let mut groups = HashMap::new();
    let mut aggregations = HashMap::new();

    for (key, value) in obj {
        if group_by.contains(key) {
            groups.insert(key.clone(), value.clone());
        } else {
            let f = match value {
                JsonValue::Number(n) => n.as_f64().unwrap_or(0.0),
                _ => 0.0,
            };
            aggregations.insert(key.clone(), f);
        }
    }

    AggregationRow { groups, aggregations }
}

/// 解析 JSON 为 TimeSeriesPoint
fn parse_time_series_point(row: &JsonValue) -> TimeSeriesPoint {
    let obj = match row.as_object() {
        Some(o) => o,
        _ => return TimeSeriesPoint { interval_start: 0, tokens_input: 0, tokens_output: 0, call_count: 0 },
    };

    TimeSeriesPoint {
        interval_start: obj.get("interval_start").and_then(|v| v.as_i64()).unwrap_or(0),
        tokens_input: obj.get("tokens_input").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64,
        tokens_output: obj.get("tokens_output").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64,
        call_count: obj.get("call_count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64,
    }
}

mod sqlite;
pub use self::sqlite::{dao, init};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod stats_duckdb_test;
