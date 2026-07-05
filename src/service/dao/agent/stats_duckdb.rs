//! AgentStatsDao DuckDB 实现

use common::error::Result;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, ModelCallEvent};
use crate::service::dao::agent::{AgentStatsDao, AgentStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static AGENT_STATS_DAO: OnceLock<Arc<dyn AgentStatsDao<ModelCallEvent = ModelCallEvent>>> = OnceLock::new();

pub fn stats_new() -> Arc<dyn AgentStatsDao<ModelCallEvent = ModelCallEvent>> {
    Arc::new(AgentStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn AgentStatsDao<ModelCallEvent = ModelCallEvent>> {
    AGENT_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = AGENT_STATS_DAO.set(stats_new());
}

struct AgentStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl AgentStatsDao for AgentStatsDaoDuckDbImpl {
    type ModelCallEvent = ModelCallEvent;

    async fn query_model_calls(&self, ctx: RequestContext, mut query: AgentStatsQuery) -> Result<Vec<JsonValue>> {
        let agent_filter = StatFilter::Equals {
            key: "agent_id".to_string(),
            value: JsonValue::String(query.agent_id.clone()),
        };
        query.filters.insert(0, agent_filter);

        let stats = ctx.stats();
        let table_name = self.model_call_table_name(stats);

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
