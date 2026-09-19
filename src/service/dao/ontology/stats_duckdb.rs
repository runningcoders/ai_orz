//! OntologyStatsDao DuckDB 实现
//!
//! 查询 `ontology_drift_events`（DuckDB 事件流时间线视角），与
//! [`crate::service::dal::ontology`] 的漂移看板（SQLite「现在时」视角）互补：
//! 看板回答「现在哪些词在漂移」，本模块回答「漂移如何随时间发生」。

use crate::pkg::RequestContext;
use crate::pkg::stats::{OntologyDriftEvent, StatFilter, StatParam};
use crate::service::dao::ontology::{OntologyDriftQuery, OntologyDriftTermRow, OntologyStatsDao};
use common::error::{Result, err};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static ONTOLOGY_STATS_DAO: OnceLock<Arc<dyn OntologyStatsDao<DriftEvent = OntologyDriftEvent>>> =
    OnceLock::new();

pub fn stats_new() -> Arc<dyn OntologyStatsDao<DriftEvent = OntologyDriftEvent>> {
    Arc::new(OntologyStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn OntologyStatsDao<DriftEvent = OntologyDriftEvent>> {
    ONTOLOGY_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = ONTOLOGY_STATS_DAO.set(stats_new());
}

struct OntologyStatsDaoDuckDbImpl;

/// 业务维度过滤（agent_id / kind）追加到通用 filters 之后
fn push_dimension_filters(query: &mut OntologyDriftQuery) {
    if let Some(agent_id) = &query.agent_id {
        query.filters.push(StatFilter::Equals {
            key: "agent_id".to_string(),
            value: JsonValue::String(agent_id.clone()),
        });
    }
    if let Some(kind) = &query.kind {
        query.filters.push(StatFilter::Equals {
            key: "kind".to_string(),
            value: JsonValue::String(kind.clone()),
        });
    }
}

#[async_trait::async_trait]
impl OntologyStatsDao for OntologyStatsDaoDuckDbImpl {
    type DriftEvent = OntologyDriftEvent;

    async fn query_drift_events(
        &self,
        ctx: RequestContext,
        mut query: OntologyDriftQuery,
    ) -> Result<Vec<JsonValue>> {
        push_dimension_filters(&mut query);

        let stats = ctx.stats();
        // 表未注册时显式报错而非假空：底层 query_aggregation 会把 None 表名
        // fallback 到 default_events（查出空结果），看板将「静默假空」误导排障；
        // 与 recent_raw_terms 的报错口径对齐
        let table_name = self
            .drift_table_name(stats)
            .ok_or_else(|| err!(Internal, "ontology drift 事件表未注册"))?;
        let group_by: Vec<&str> = query.group_by.iter().map(String::as_str).collect();

        let rows = ctx
            .stats()
            .query_aggregation(
                ctx.clone(),
                Some(table_name.as_str()),
                &query.filters,
                &group_by,
                &query.aggregations,
                query.time_range,
            )
            .await?;

        // groups（分组维度值）+ aggregations（聚合结果）一并展开；
        // Agent 先例只展开 aggregations（其用例恒为无条件聚合），本表
        // count_by_kind / count_by_agent 依赖 group_by，需保留维度值
        Ok(rows
            .iter()
            .map(|r| {
                let mut obj = serde_json::Map::new();
                for (k, v) in &r.groups {
                    obj.insert(k.clone(), v.clone());
                }
                for (k, v) in &r.aggregations {
                    obj.insert(k.clone(), JsonValue::from(*v));
                }
                JsonValue::Object(obj)
            })
            .collect())
    }

    async fn recent_raw_terms(
        &self,
        ctx: RequestContext,
        limit: i64,
        agent_id: Option<String>,
    ) -> Result<Vec<OntologyDriftTermRow>> {
        let limit = limit.max(0);
        let stats = ctx.stats();
        let table_name = self
            .drift_table_name(stats)
            .ok_or_else(|| err!(Internal, "ontology drift 事件表未注册"))?;

        // query_aggregation 不支持 ORDER BY / LIMIT，走 raw SQL 逃生门；
        // 表名来自 Stats 注册表常量（非用户输入）允许拼接，过滤值一律参数绑定
        let sql = if agent_id.is_some() {
            format!(
                "SELECT timestamp, agent_id, kind, raw_term FROM {table_name} \
                 WHERE agent_id = ? ORDER BY timestamp DESC LIMIT ?"
            )
        } else {
            format!(
                "SELECT timestamp, agent_id, kind, raw_term FROM {table_name} \
                 ORDER BY timestamp DESC LIMIT ?"
            )
        };

        let mut params: Vec<StatParam> = Vec::new();
        if let Some(agent_id) = agent_id {
            params.push(StatParam::Str(agent_id));
        }
        params.push(StatParam::Int(limit));

        let rows = ctx.stats().query(ctx.clone(), &sql, &params).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(OntologyDriftTermRow {
                    timestamp: row.get("timestamp")?.as_i64()?,
                    agent_id: row.get("agent_id")?.as_str()?.to_string(),
                    kind: row.get("kind")?.as_str()?.to_string(),
                    raw_term: row.get("raw_term")?.as_str()?.to_string(),
                })
            })
            .collect())
    }
}
