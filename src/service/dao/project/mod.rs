//! Project DAO 模块

use common::error::{Error, Result};
use common::models::{StatsInterval, TimeSeriesPoint, TokenSumResult};
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, AggregationRow, StatEvent, Stats};
use common::enums::ProjectStatus;
use serde_json::Value as JsonValue;

/// Project 查询参数
#[derive(Debug, Clone, Default)]
pub struct ProjectQuery {
    pub root_user_id: Option<String>,
    pub status_in: Option<Vec<ProjectStatus>>,
    pub limit: Option<usize>,
}

/// Project DAO 接口
#[async_trait::async_trait]
pub trait ProjectDao: Send + Sync + std::fmt::Debug {
    /// 插入新项目
    async fn insert(&self, ctx: RequestContext, project: &ProjectPo) -> Result<()>;
    /// 根据 ID 查询项目
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ProjectPo>>;
    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<Vec<ProjectPo>>;
    /// 根据根用户查询项目列表
    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>>;
    /// 根据根用户和状态查询项目列表
    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>>;
    /// 更新项目
    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<()>;
    /// 更新项目状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()>;
    /// 统计根用户的项目总数
    async fn count_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64>;
    /// 统计根用户指定状态的项目数
    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64>;
}

/// Project 统计查询参数（统一结构体，覆盖所有查询场景）
#[derive(Debug, Clone, Default)]
pub struct ProjectStatsQuery {
    /// Project ID（必填）
    pub project_id: String,
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

/// Project 统计 DAO 接口
#[async_trait::async_trait]
pub trait ProjectStatsDao: Send + Sync {
    type ModelCallEvent: StatEvent + 'static + Send + Sync;
    type ToolCallEvent: StatEvent + 'static + Send + Sync;

    fn model_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ModelCallEvent>()
    }

    fn tool_call_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ToolCallEvent>()
    }

    async fn query_model_calls(&self, ctx: RequestContext, query: ProjectStatsQuery) -> Result<Vec<JsonValue>>;

    async fn query_tool_calls(&self, ctx: RequestContext, query: ProjectStatsQuery) -> Result<Vec<JsonValue>>;

    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: ProjectStatsQuery) -> Result<Vec<AggregationRow>> {
        let group_by = query.group_by.clone();
        let rows = self.query_model_calls(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_aggregation_row(&row, &group_by));
        }
        Ok(result)
    }

    async fn query_model_call_time_series(&self, ctx: RequestContext, mut query: ProjectStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
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

    async fn sum_tokens(&self, ctx: RequestContext, mut query: ProjectStatsQuery) -> Result<TokenSumResult> {
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

    async fn sum_tool_calls(&self, ctx: RequestContext, query: ProjectStatsQuery) -> Result<u64> {
        let mut query = query;
        query.group_by = vec![];
        query.aggregations = vec![StatAggregation::Count];
        query.interval = None;
        let rows = self.query_tool_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    async fn query_tool_call_time_series(&self, ctx: RequestContext, mut query: ProjectStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
        if query.interval.is_none() {
            query.interval = Some(StatsInterval::Daily);
        }
        let rows = self.query_tool_calls(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_time_series_point(&row));
        }
        Ok(result)
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

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod stats_duckdb_test;
