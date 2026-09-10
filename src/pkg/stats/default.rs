//! 默认统计事件实现

use super::*;
use common::error::Result;
use duckdb::{Connection, ToSql};
use serde_json::Value;
use uuid::Uuid;

/// 默认统计事件
#[derive(Debug, Clone)]
pub struct DefaultStatEvent {
    timestamp: i64,
    tags: Option<Value>,
    metrics: Option<Value>,
}

impl DefaultStatEvent {
    /// Create a new default stat event
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            tags: None,
            metrics: None,
        }
    }

    /// Add tags to the event
    pub fn with_tags(mut self, tags: Value) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Add metrics to the event
    pub fn with_metrics(mut self, metrics: Value) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

impl StatEvent for DefaultStatEvent {
    fn timestamp(&self) -> i64 {
        self.timestamp
    }

    fn event_type(&self) -> &str {
        "default"
    }

    fn tags_json(&self) -> Option<Value> {
        self.tags.clone()
    }

    fn metrics_json(&self) -> Option<Value> {
        self.metrics.clone()
    }
}

/// Default table for default events
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultStatTable;

impl StatTable<DefaultStatEvent> for DefaultStatTable {
    fn table_name(&self) -> &str {
        "default_events"
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS default_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                event_type VARCHAR,
                tags JSON,
                metrics JSON
            );
        "#;
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!("Failed to create default_events table: {}", e))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &DefaultStatEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let timestamp = event.timestamp();
        let event_type = event.event_type().to_string();
        // 字段缺失兼容：tags/metrics 未设置时绑 NULL，而不是空串。
        // 空串不是合法 JSON 文档，DuckDB 的 JSON 列会直接拒绝（Malformed JSON）→
        // 一次写入失败会拖垮整批；缺失字段本身就该表达为 NULL，且查询侧
        // `json_extract(NULL, '$.x')` 安全返回 NULL，不会报错。
        let tags_str = event.tags_json().map(|v| v.to_string());
        let metrics_str = event.metrics_json().map(|v| v.to_string());

        let sql = r#"
            INSERT INTO default_events (id, timestamp, event_type, tags, metrics) VALUES (?, ?, ?, ?, ?);
        "#;
        conn.execute(
            sql,
            [
                &id.to_string() as &dyn ToSql,
                &timestamp as &dyn ToSql,
                &event_type as &dyn ToSql,
                &tags_str as &dyn ToSql,
                &metrics_str as &dyn ToSql,
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert default event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[DefaultStatEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let timestamp = event.timestamp();
            let event_type = event.event_type().to_string();
            // 同 insert_event：缺失字段绑 NULL，避免空串触发 JSON 解析失败拖垮整批
            let tags_str = event.tags_json().map(|v| v.to_string());
            let metrics_str = event.metrics_json().map(|v| v.to_string());

            let sql = r#"
                INSERT INTO default_events (id, timestamp, event_type, tags, metrics) VALUES (?, ?, ?, ?, ?);
            "#;
            conn.execute(
                sql,
                [
                    &id.to_string() as &dyn ToSql,
                    &timestamp as &dyn ToSql,
                    &event_type as &dyn ToSql,
                    &tags_str as &dyn ToSql,
                    &metrics_str as &dyn ToSql,
                ],
            )
            .map_err(|e| {
                common::error::Error::internal(format!(
                    "Failed to bulk insert default event: {}",
                    e
                ))
            })?;
        }
        Ok(())
    }
}
