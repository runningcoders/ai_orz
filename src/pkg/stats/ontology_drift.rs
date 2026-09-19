//! 本体漂移统计事件
//!
//! `OntologyDriftEvent` 绑定 `ontology_drift_events` 表，
//! 用于记录记忆沉淀时出现的词元原文（只记原文，不解析结论——
//! 解析结果会随本体进化过期，原文是不可变事实）。

use super::*;
use ai_orz_macros::StatsEvent;
use common::error::Result;
use duckdb::{Connection, ToSql};
use uuid::Uuid;

#[derive(Debug, Clone, Default, StatsEvent)]
#[event_type = "ontology_drift"]
pub struct OntologyDriftEvent {
    #[timestamp]
    pub timestamp: i64,
    /// 谁沉淀的
    #[tag]
    pub agent_id: String,
    /// 词元类别："relation" | "class"
    #[tag]
    pub kind: String,
    /// 词元原文（trim 后），不解析、不翻译
    #[tag]
    pub raw_term: String,
    /// 联邦调用方组织（审计维度）：由 Stats::record 从 ctx 自动注入
    #[tag]
    pub caller_organization_id: Option<String>,
}

impl OntologyDriftEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            agent_id: String::new(),
            kind: String::new(),
            raw_term: String::new(),
            caller_organization_id: None,
        }
    }

    pub fn with_agent_id(mut self, v: String) -> Self {
        self.agent_id = v;
        self
    }

    pub fn with_kind(mut self, v: String) -> Self {
        self.kind = v;
        self
    }

    pub fn with_raw_term(mut self, v: String) -> Self {
        self.raw_term = v;
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OntologyDriftStatTable;

impl StatTable<OntologyDriftEvent> for OntologyDriftStatTable {
    fn table_name(&self) -> &str {
        "ontology_drift_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS ontology_drift_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                agent_id VARCHAR,
                kind VARCHAR,
                raw_term VARCHAR,
                caller_organization_id VARCHAR
            );
        "#;
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to create ontology_drift_events table: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &OntologyDriftEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO ontology_drift_events (
                id, timestamp, agent_id, kind, raw_term, caller_organization_id
            ) VALUES (?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(
            sql,
            [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.agent_id as &dyn ToSql,
                &event.kind as &dyn ToSql,
                &event.raw_term as &dyn ToSql,
                &event.caller_organization_id as &dyn ToSql,
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert ontology drift event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(
        &self,
        conn: &mut Connection,
        events: &[OntologyDriftEvent],
    ) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let sql = r#"
                INSERT INTO ontology_drift_events (
                    id, timestamp, agent_id, kind, raw_term, caller_organization_id
                ) VALUES (?, ?, ?, ?, ?, ?);
            "#;
            conn.execute(
                sql,
                [
                    &id.to_string() as &dyn ToSql,
                    &event.timestamp as &dyn ToSql,
                    &event.agent_id as &dyn ToSql,
                    &event.kind as &dyn ToSql,
                    &event.raw_term as &dyn ToSql,
                    &event.caller_organization_id as &dyn ToSql,
                ],
            )
            .map_err(|e| {
                common::error::Error::internal(format!(
                    "Failed to bulk insert ontology drift event: {}",
                    e
                ))
            })?;
        }
        Ok(())
    }
}
