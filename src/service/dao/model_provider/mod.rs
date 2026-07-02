//! Model Provider DAO 模块

use common::error::{Error, Result};
use common::models::{StatsInterval, TimeSeriesPoint, TokenSumResult};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, AggregationRow};
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
}

/// ModelProvider 统计查询参数（统一结构体，覆盖所有查询场景）
#[derive(Debug, Clone, Default)]
pub struct ModelProviderStatsQuery {
    /// ModelProvider ID（必填）
    pub model_provider_id: String,
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
    /// 通用查询方法
    async fn query(&self, ctx: RequestContext, query: ModelProviderStatsQuery) -> Result<Vec<JsonValue>>;

    /// 语法糖：聚合查询（返回结构化 AggregationRow）
    async fn query_aggregation(&self, ctx: RequestContext, query: ModelProviderStatsQuery) -> Result<Vec<AggregationRow>> {
        let group_by = query.group_by.clone();
        let rows = self.query(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_aggregation_row(&row, &group_by));
        }
        Ok(result)
    }

    /// 语法糖：时序查询（返回结构化 TimeSeriesPoint）
    async fn query_time_series(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
        if query.interval.is_none() {
            query.interval = Some(StatsInterval::Daily);
        }
        let rows = self.query(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_time_series_point(&row));
        }
        Ok(result)
    }

    /// 语法糖：Token 汇总（返回 TokenSumResult）
    async fn sum_tokens(&self, ctx: RequestContext, mut query: ModelProviderStatsQuery) -> Result<TokenSumResult> {
        query.group_by = vec![];
        query.aggregations = vec![
            StatAggregation::Sum("tokens_input".into()),
            StatAggregation::Sum("tokens_output".into()),
            StatAggregation::Count,
        ];
        query.interval = None;
        let rows = self.query(ctx, query).await?;
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
