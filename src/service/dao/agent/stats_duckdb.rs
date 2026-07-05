//! AgentStatsDao DuckDB 实现

use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatsInterval, ModelCallEvent};
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

        self.do_query(ctx, query, table_name).await
    }
}

impl AgentStatsDaoDuckDbImpl {
    async fn do_query(&self, ctx: RequestContext, mut query: AgentStatsQuery, table_name: Option<String>) -> Result<Vec<JsonValue>> {
        if query.interval.is_some() {
            let interval = query.interval.unwrap_or(StatsInterval::Daily);
            let time_range = query.time_range.ok_or_else(|| {
                Error::bad_request("time_range is required for time series query")
            })?;
            
            let points = ctx.stats().query_time_series(
                ctx.clone(),
                table_name.as_deref(),
                &query.filters,
                interval,
                time_range,
            ).await?;
            
            Ok(points.iter().map(|p| {
                serde_json::to_value(p).unwrap_or(JsonValue::Null)
            }).collect())
        } else if !query.aggregations.is_empty() || !query.group_by.is_empty() {
            let rows = ctx.stats().query_aggregation(
                ctx.clone(),
                table_name.as_deref(),
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
            
            let rows = ctx.stats().query_aggregation(
                ctx.clone(),
                table_name.as_deref(),
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
