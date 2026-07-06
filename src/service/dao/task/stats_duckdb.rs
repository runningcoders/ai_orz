//! TaskStatsDao DuckDB 实现

use common::error::Result;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, TaskEvent};
use crate::service::dao::task::{TaskStatsDao, TaskStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static TASK_STATS_DAO: OnceLock<Arc<dyn TaskStatsDao<TaskEvent = TaskEvent>>> = OnceLock::new();

pub fn stats_new() -> Arc<dyn TaskStatsDao<TaskEvent = TaskEvent>> {
    Arc::new(TaskStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn TaskStatsDao<TaskEvent = TaskEvent>> {
    TASK_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = TASK_STATS_DAO.set(stats_new());
}

struct TaskStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl TaskStatsDao for TaskStatsDaoDuckDbImpl {
    type TaskEvent = TaskEvent;

    async fn query_task_calls(&self, ctx: RequestContext, mut query: TaskStatsQuery) -> Result<Vec<JsonValue>> {
        let task_filter = StatFilter::Equals {
            key: "task_id".to_string(),
            value: JsonValue::String(query.task_id.clone()),
        };
        query.filters.insert(0, task_filter);

        let stats = ctx.stats();
        let table_name = self.task_table_name(stats);

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