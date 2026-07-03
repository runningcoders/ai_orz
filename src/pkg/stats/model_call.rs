//! 模型调用统计事件
//!
//! `ModelCallEvent` 绑定 `model_call_events` 表，
//! 用于记录 LLM 模型调用的 token 用量、调用次数等指标。

use super::*;
use uuid::Uuid;
use duckdb::{Connection, ToSql};
use common::error::Result;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ModelCallEvent {
    timestamp: i64,
    agent_id: Option<String>,
    project_id: Option<String>,
    task_id: Option<String>,
    model_provider_id: Option<String>,
    model_name: Option<String>,
    organization_id: Option<String>,
    user_id: Option<String>,
    tokens_input: u64,
    tokens_output: u64,
    total_tokens: u64,
}

impl ModelCallEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            agent_id: None,
            project_id: None,
            task_id: None,
            model_provider_id: None,
            model_name: None,
            organization_id: None,
            user_id: None,
            tokens_input: 0,
            tokens_output: 0,
            total_tokens: 0,
        }
    }

    pub fn with_agent_id(mut self, v: Option<String>) -> Self {
        self.agent_id = v;
        self
    }

    pub fn with_project_id(mut self, v: Option<String>) -> Self {
        self.project_id = v;
        self
    }

    pub fn with_task_id(mut self, v: Option<String>) -> Self {
        self.task_id = v;
        self
    }

    pub fn with_model_provider_id(mut self, v: Option<String>) -> Self {
        self.model_provider_id = v;
        self
    }

    pub fn with_model_name(mut self, v: Option<String>) -> Self {
        self.model_name = v;
        self
    }

    pub fn with_organization_id(mut self, v: Option<String>) -> Self {
        self.organization_id = v;
        self
    }

    pub fn with_user_id(mut self, v: Option<String>) -> Self {
        self.user_id = v;
        self
    }

    pub fn with_tokens_input(mut self, v: u64) -> Self {
        self.tokens_input = v;
        self
    }

    pub fn with_tokens_output(mut self, v: u64) -> Self {
        self.tokens_output = v;
        self
    }

    pub fn with_total_tokens(mut self, v: u64) -> Self {
        self.total_tokens = v;
        self
    }
}

impl StatEvent for ModelCallEvent {
    fn timestamp(&self) -> i64 {
        self.timestamp
    }

    fn event_type(&self) -> &str {
        "model_call"
    }

    fn tags_json(&self) -> Option<Value> {
        let mut map = serde_json::Map::new();
        if let Some(v) = &self.agent_id {
            map.insert("agent_id".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.project_id {
            map.insert("project_id".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.task_id {
            map.insert("task_id".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.model_provider_id {
            map.insert("model_provider_id".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.model_name {
            map.insert("model_name".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.organization_id {
            map.insert("organization_id".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.user_id {
            map.insert("user_id".into(), Value::String(v.clone()));
        }
        if !map.is_empty() {
            Some(Value::Object(map))
        } else {
            None
        }
    }

    fn metrics_json(&self) -> Option<Value> {
        Some(json!({
            "call_count": 1,
            "tokens_input": self.tokens_input,
            "tokens_output": self.tokens_output,
            "total_tokens": self.total_tokens,
        }))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelCallStatTable;

impl StatTable<ModelCallEvent> for ModelCallStatTable {
    fn table_name(&self) -> &str {
        "model_call_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS model_call_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                agent_id VARCHAR,
                project_id VARCHAR,
                task_id VARCHAR,
                model_provider_id VARCHAR,
                model_name VARCHAR,
                organization_id VARCHAR,
                user_id VARCHAR,
                tokens_input BIGINT,
                tokens_output BIGINT,
                total_tokens BIGINT
            );
        "#;
        conn.execute(sql, [])
            .map_err(|e| common::error::Error::internal(format!("Failed to create model_call_events table: {}", e)))?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &ModelCallEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO model_call_events (
                id, timestamp, agent_id, project_id, task_id,
                model_provider_id, model_name, organization_id, user_id,
                tokens_input, tokens_output, total_tokens
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(sql, [
            &id.to_string() as &dyn ToSql,
            &event.timestamp as &dyn ToSql,
            &event.agent_id as &dyn ToSql,
            &event.project_id as &dyn ToSql,
            &event.task_id as &dyn ToSql,
            &event.model_provider_id as &dyn ToSql,
            &event.model_name as &dyn ToSql,
            &event.organization_id as &dyn ToSql,
            &event.user_id as &dyn ToSql,
            &event.tokens_input as &dyn ToSql,
            &event.tokens_output as &dyn ToSql,
            &event.total_tokens as &dyn ToSql,
        ])
            .map_err(|e| common::error::Error::internal(format!("Failed to insert model call event: {}", e)))?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[ModelCallEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let sql = r#"
                INSERT INTO model_call_events (
                    id, timestamp, agent_id, project_id, task_id,
                    model_provider_id, model_name, organization_id, user_id,
                    tokens_input, tokens_output, total_tokens
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
            "#;
            conn.execute(sql, [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.agent_id as &dyn ToSql,
                &event.project_id as &dyn ToSql,
                &event.task_id as &dyn ToSql,
                &event.model_provider_id as &dyn ToSql,
                &event.model_name as &dyn ToSql,
                &event.organization_id as &dyn ToSql,
                &event.user_id as &dyn ToSql,
                &event.tokens_input as &dyn ToSql,
                &event.tokens_output as &dyn ToSql,
                &event.total_tokens as &dyn ToSql,
            ])
                .map_err(|e| common::error::Error::internal(format!("Failed to bulk insert model call event: {}", e)))?;
        }
        Ok(())
    }
}
