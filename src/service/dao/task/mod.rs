//! Task DAO 模块

use common::error::{Error, Result};
use common::models::{StatsInterval, TimeSeriesPoint, TokenSumResult};
use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, AggregationRow, StatEvent, Stats};
use common::enums::AssigneeType;
use common::enums::TaskStatus;
use serde_json::Value as JsonValue;

/// Task 查询参数
#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    pub assignee_type: Option<AssigneeType>,
    pub assignee_id: Option<String>,
    pub project_id: Option<String>,
    pub status_in: Option<Vec<TaskStatus>>,
    pub limit: Option<usize>,
}

/// Task DAO 接口
#[async_trait::async_trait]
pub trait TaskDao: Send + Sync + std::fmt::Debug {
    /// 插入新任务
    async fn insert(&self, ctx: RequestContext, task: &TaskPo) -> Result<()>;
    /// 根据 ID 查询任务
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>>;
    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<TaskPo>>;
    /// 根据分配对象查询任务列表
    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;
    /// 根据状态查询任务列表
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;
    /// 更新任务
    async fn update(&self, ctx: RequestContext, task: &TaskPo) -> Result<()>;
    /// 更新任务状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()>;
    /// 统计分配对象的任务总数
    async fn count_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
    ) -> Result<u64>;
    /// 统计分配对象指定状态的任务数
    async fn count_by_assignee_and_status(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
        status: TaskStatus,
    ) -> Result<u64>;
}

/// Task 统计查询参数（统一结构体，覆盖所有查询场景）
#[derive(Debug, Clone, Default)]
pub struct TaskStatsQuery {
    /// Task ID（必填）
    pub task_id: String,
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

/// Task 统计 DAO 接口
#[async_trait::async_trait]
pub trait TaskStatsDao: Send + Sync {
    /// 绑定的事件类型，用于从 Stats 注册表获取表名
    type Event: StatEvent + 'static + Send + Sync;

    /// 获取绑定的表名（从 Stats 注册表中查询）
    fn table_name<'a>(&self, stats: &'a Stats) -> Option<&'a str> {
        stats.get_table_name::<Self::Event>()
    }

    /// 通用查询方法
    async fn query(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<JsonValue>>;

    /// 语法糖：聚合查询（返回结构化 AggregationRow）
    async fn query_aggregation(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<AggregationRow>> {
        let group_by = query.group_by.clone();
        let rows = self.query(ctx, query).await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(parse_aggregation_row(&row, &group_by));
        }
        Ok(result)
    }

    /// 语法糖：时序查询（返回结构化 TimeSeriesPoint）
    async fn query_time_series(&self, ctx: RequestContext, mut query: TaskStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
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
    async fn sum_tokens(&self, ctx: RequestContext, mut query: TaskStatsQuery) -> Result<TokenSumResult> {
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

pub mod sqlite;
pub use self::sqlite::{dao, get_dao, init, new};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod stats_duckdb_test;
