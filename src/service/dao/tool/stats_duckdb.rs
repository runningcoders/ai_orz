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
