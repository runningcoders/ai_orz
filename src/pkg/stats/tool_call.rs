//! 工具调用统计事件
//!
//! `ToolCallEvent` 绑定 `tool_call_events` 表，
//! 用于记录工具调用的次数、耗时、参数/结果大小等指标。

use super::*;
use uuid::Uuid;
use duckdb::{Connection, ToSql};
use common::error::Result;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolCallEvent {
    timestamp: i64,
    tags: Option<Value>,
    metrics: Option<Value>,
}

impl ToolCallEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            tags: None,
            metrics: None,
        }
    }

    pub fn with_tags(mut self, tags: Value) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_metrics(mut self, metrics: Value) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

impl StatEvent for ToolCallEvent {
    fn timestamp(&self) -> i64 {
        self.timestamp
    }

    fn event_type(&self) -> &str {
        "tool_call"
    }

    fn tags_json(&self) -> Option<Value> {
        self.tags.clone()
    }

    fn metrics_json(&self) -> Option<Value> {
        self.metrics.clone()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolCallStatTable;

impl StatTable<ToolCallEvent> for ToolCallStatTable {
    fn table_name(&self) -> &str {
        "tool_call_events"
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS tool_call_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                event_type VARCHAR,
                tags JSON,
                metrics JSON
            );
        "#;
        conn.execute(sql, [])
            .map_err(|e| common::error::Error::internal(format!("Failed to create tool_call_events table: {}", e)))?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &ToolCallEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let timestamp = event.timestamp();
        let event_type = event.event_type().to_string();
        let tags_str = event.tags_json().map(|v| v.to_string()).unwrap_or_default();
        let metrics_str = event.metrics_json().map(|v| v.to_string()).unwrap_or_default();

        let sql = r#"
            INSERT INTO tool_call_events (id, timestamp, event_type, tags, metrics) VALUES (?, ?, ?, ?, ?);
        "#;
        conn.execute(sql, [
            &id.to_string() as &dyn ToSql,
            &timestamp as &dyn ToSql,
            &event_type as &dyn ToSql,
            &tags_str as &dyn ToSql,
            &metrics_str as &dyn ToSql,
        ])
            .map_err(|e| common::error::Error::internal(format!("Failed to insert tool call event: {}", e)))?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[ToolCallEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let timestamp = event.timestamp();
            let event_type = event.event_type().to_string();
            let tags_str = event.tags_json().map(|v| v.to_string()).unwrap_or_default();
            let metrics_str = event.metrics_json().map(|v| v.to_string()).unwrap_or_default();

            let sql = r#"
                INSERT INTO tool_call_events (id, timestamp, event_type, tags, metrics) VALUES (?, ?, ?, ?, ?);
            "#;
            conn.execute(sql, [
                &id.to_string() as &dyn ToSql,
                &timestamp as &dyn ToSql,
                &event_type as &dyn ToSql,
                &tags_str as &dyn ToSql,
                &metrics_str as &dyn ToSql,
            ])
                .map_err(|e| common::error::Error::internal(format!("Failed to bulk insert tool call event: {}", e)))?;
        }
        Ok(())
    }
}
