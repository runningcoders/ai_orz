//! Agent 唤醒统计事件
//!
//! `AgentAwakeEvent` 绑定 `agent_awake_events` 表，
//! 用于记录 Agent 唤醒（消息消费时唤醒）的调用次数、耗时、状态等指标。

use super::*;
use ai_orz_macros::StatsEvent;
use common::error::Result;
use duckdb::{Connection, ToSql};
use uuid::Uuid;

#[derive(Debug, Clone, StatsEvent)]
#[event_type = "agent_awake"]
pub struct AgentAwakeEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub agent_id: String,
    #[tag]
    pub project_id: Option<String>,
    #[tag]
    pub task_id: Option<String>,
    #[tag]
    pub organization_id: Option<String>,
    #[tag]
    pub user_id: Option<String>,
    #[tag]
    pub message_id: Option<String>,
    #[metric]
    pub call_count: u64,
    #[metric]
    pub duration_ms: u64,
    #[metric]
    pub status: String,
    #[metric]
    pub exit_reason: String,
}

impl AgentAwakeEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            agent_id: String::new(),
            project_id: None,
            task_id: None,
            organization_id: None,
            user_id: None,
            message_id: None,
            call_count: 1,
            duration_ms: 0,
            status: "success".to_string(),
            exit_reason: String::new(),
        }
    }

    pub fn with_agent_id(mut self, v: String) -> Self {
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

    pub fn with_organization_id(mut self, v: Option<String>) -> Self {
        self.organization_id = v;
        self
    }

    pub fn with_user_id(mut self, v: Option<String>) -> Self {
        self.user_id = v;
        self
    }

    pub fn with_message_id(mut self, v: Option<String>) -> Self {
        self.message_id = v;
        self
    }

    pub fn with_duration_ms(mut self, v: u64) -> Self {
        self.duration_ms = v;
        self
    }

    pub fn with_status(mut self, v: String) -> Self {
        self.status = v;
        self
    }

    pub fn with_exit_reason(mut self, v: String) -> Self {
        self.exit_reason = v;
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentAwakeStatTable;

impl StatTable<AgentAwakeEvent> for AgentAwakeStatTable {
    fn table_name(&self) -> &str {
        "agent_awake_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS agent_awake_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                agent_id VARCHAR,
                project_id VARCHAR,
                task_id VARCHAR,
                organization_id VARCHAR,
                user_id VARCHAR,
                message_id VARCHAR,
                call_count BIGINT,
                duration_ms BIGINT,
                status VARCHAR,
                exit_reason VARCHAR
            );
        "#;
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to create agent_awake_events table: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &AgentAwakeEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO agent_awake_events (
                id, timestamp, agent_id, project_id, task_id,
                organization_id, user_id, message_id,
                call_count, duration_ms, status, exit_reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(
            sql,
            [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.agent_id as &dyn ToSql,
                &event.project_id as &dyn ToSql,
                &event.task_id as &dyn ToSql,
                &event.organization_id as &dyn ToSql,
                &event.user_id as &dyn ToSql,
                &event.message_id as &dyn ToSql,
                &event.call_count as &dyn ToSql,
                &event.duration_ms as &dyn ToSql,
                &event.status as &dyn ToSql,
                &event.exit_reason as &dyn ToSql,
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert agent awake event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[AgentAwakeEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let sql = r#"
                INSERT INTO agent_awake_events (
                    id, timestamp, agent_id, project_id, task_id,
                    organization_id, user_id, message_id,
                    call_count, duration_ms, status, exit_reason
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
            "#;
            conn.execute(
                sql,
                [
                    &id.to_string() as &dyn ToSql,
                    &event.timestamp as &dyn ToSql,
                    &event.agent_id as &dyn ToSql,
                    &event.project_id as &dyn ToSql,
                    &event.task_id as &dyn ToSql,
                    &event.organization_id as &dyn ToSql,
                    &event.user_id as &dyn ToSql,
                    &event.message_id as &dyn ToSql,
                    &event.call_count as &dyn ToSql,
                    &event.duration_ms as &dyn ToSql,
                    &event.status as &dyn ToSql,
                    &event.exit_reason as &dyn ToSql,
                ],
            )
            .map_err(|e| {
                common::error::Error::internal(format!(
                    "Failed to bulk insert agent awake event: {}",
                    e
                ))
            })?;
        }
        Ok(())
    }
}
