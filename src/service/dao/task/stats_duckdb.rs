//! TaskStatsDao DuckDB 实现

use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatsInterval, DefaultStatEvent};
use crate::service::dao::task::{TaskStatsDao, TaskStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static TASK_STATS_DAO: OnceLock<Arc<dyn TaskStatsDao<Event = DefaultStatEvent>>> = OnceLock::new();

pub fn stats_new() -> Arc<dyn TaskStatsDao<Event = DefaultStatEvent>> {
    Arc::new(TaskStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn TaskStatsDao<Event = DefaultStatEvent>> {
    TASK_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = TASK_STATS_DAO.set(stats_new());
}

struct TaskStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl TaskStatsDao for TaskStatsDaoDuckDbImpl {
    type Event = DefaultStatEvent;

    async fn query(&self, ctx: RequestContext, mut query: TaskStatsQuery) -> Result<Vec<JsonValue>> {
        let task_filter = StatFilter::Equals {
            key: "task_id".to_string(),
            value: JsonValue::String(query.task_id.clone()),
        };
        query.filters.insert(0, task_filter);

        let stats = ctx.stats();
        let table_name = self.table_name(stats);

        if query.interval.is_some() {
            let interval = query.interval.unwrap_or(StatsInterval::Daily);
            let time_range = query.time_range.ok_or_else(|| {
                Error::bad_request("time_range is required for time series query")
            })?;

            let points = stats.query_time_series(
                ctx.clone(),
                table_name,
                &query.filters,
                interval,
                time_range,
            ).await?;

            Ok(points.iter().map(|p| {
                serde_json::to_value(p).unwrap_or(JsonValue::Null)
            }).collect())
        } else if !query.aggregations.is_empty() || !query.group_by.is_empty() {
            let rows = stats.query_aggregation(
                ctx.clone(),
                table_name,
                &query.filters,
                &query.group_by.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &query.aggregations,
                query.time_range,
            ).await?;

            Ok(rows.iter().map(|r| {
                let mut obj = serde_json::Map::new();
                for (k, v) in &r.groups {
                    obj.insert(k.clone(), v.clone());
                }
                for (k, v) in &r.aggregations {
                    obj.insert(k.clone(), serde_json::Value::from(*v));
                }
                JsonValue::Object(obj)
            }).collect())
        } else {
            let default_aggregations = vec![
                StatAggregation::Sum("tokens_input".to_string()),
                StatAggregation::Sum("tokens_output".to_string()),
                StatAggregation::Count,
            ];

            let rows = stats.query_aggregation(
                ctx.clone(),
                table_name,
                &query.filters,
                &[],
                &default_aggregations,
                query.time_range,
            ).await?;

            Ok(rows.iter().map(|r| {
                let mut obj = serde_json::Map::new();
                for (k, v) in &r.groups {
                    obj.insert(k.clone(), v.clone());
                }
                for (k, v) in &r.aggregations {
                    obj.insert(k.clone(), serde_json::Value::from(*v));
                }
                JsonValue::Object(obj)
            }).collect())
        }
    }
}
