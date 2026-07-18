//! ProjectStatsDao DuckDB 实现

use common::error::Result;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, ProjectEvent};
use crate::service::dao::project::{ProjectStatsDao, ProjectStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static PROJECT_STATS_DAO: OnceLock<Arc<dyn ProjectStatsDao<ProjectEvent = ProjectEvent>>> = OnceLock::new();

pub fn stats_new() -> Arc<dyn ProjectStatsDao<ProjectEvent = ProjectEvent>> {
    Arc::new(ProjectStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn ProjectStatsDao<ProjectEvent = ProjectEvent>> {
    PROJECT_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = PROJECT_STATS_DAO.set(stats_new());
}

struct ProjectStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl ProjectStatsDao for ProjectStatsDaoDuckDbImpl {
    type ProjectEvent = ProjectEvent;

    async fn query_project_calls(&self, ctx: RequestContext, mut query: ProjectStatsQuery) -> Result<Vec<JsonValue>> {
        let project_filter = StatFilter::Equals {
            key: "project_id".to_string(),
            value: JsonValue::String(query.project_id.clone()),
        };
        query.filters.insert(0, project_filter);

        let stats = ctx.stats();
        let table_name = self.project_table_name(stats);

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