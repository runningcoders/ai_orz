//! 工具调用统计事件
//!
//! `ToolCallEvent` 绑定 `tool_call_events` 表，
//! 用于记录工具调用的次数、耗时、参数/结果大小等指标。

use super::*;
use ai_orz_macros::StatsEvent;
use common::error::Result;
use duckdb::{Connection, ToSql};
use uuid::Uuid;

#[derive(Debug, Clone, Default, StatsEvent)]
#[event_type = "tool_call"]
pub struct ToolCallEvent {
    #[timestamp]
    timestamp: i64,
    #[tag]
    tool_id: String,
    #[tag]
    tool_name: String,
    #[tag]
    agent_id: Option<String>,
    #[tag]
    project_id: Option<String>,
    #[tag]
    task_id: Option<String>,
    #[tag]
    organization_id: Option<String>,
    /// 联邦调用方组织（审计维度）：由 Stats::record 从 ctx 自动注入
    #[tag]
    caller_organization_id: Option<String>,
    #[tag]
    user_id: Option<String>,
    #[metric]
    call_count: u64,
    #[metric]
    args_len: u64,
    #[metric]
    result_len: u64,
    #[metric]
    duration_ms: u64,
    #[metric]
    status: String,
}

impl ToolCallEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            tool_id: String::new(),
            tool_name: String::new(),
            agent_id: None,
            project_id: None,
            task_id: None,
            organization_id: None,
            caller_organization_id: None,
            user_id: None,
            call_count: 1,
            args_len: 0,
            result_len: 0,
            duration_ms: 0,
            status: "success".to_string(),
        }
    }

    pub fn with_tool_id(mut self, v: String) -> Self {
        self.tool_id = v;
        self
    }

    pub fn with_tool_name(mut self, v: String) -> Self {
        self.tool_name = v;
        self
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

    pub fn with_organization_id(mut self, v: Option<String>) -> Self {
        self.organization_id = v;
        self
    }

    pub fn with_user_id(mut self, v: Option<String>) -> Self {
        self.user_id = v;
        self
    }

    pub fn with_args_len(mut self, v: u64) -> Self {
        self.args_len = v;
        self
    }

    pub fn with_result_len(mut self, v: u64) -> Self {
        self.result_len = v;
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
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolCallStatTable;

impl StatTable<ToolCallEvent> for ToolCallStatTable {
    fn table_name(&self) -> &str {
        "tool_call_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS tool_call_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                tool_id VARCHAR,
                tool_name VARCHAR,
                agent_id VARCHAR,
                project_id VARCHAR,
                task_id VARCHAR,
                organization_id VARCHAR,
                caller_organization_id VARCHAR,
                user_id VARCHAR,
                args_len BIGINT,
                result_len BIGINT,
                duration_ms BIGINT,
                status VARCHAR
            );
        "#;
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to create tool_call_events table: {}",
                e
            ))
        })?;
        // 兼容已有 DuckDB 文件：补加审计维度列（幂等）
        conn.execute(
            "ALTER TABLE tool_call_events ADD COLUMN IF NOT EXISTS caller_organization_id VARCHAR",
            [],
        )
        .map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to migrate tool_call_events table: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &ToolCallEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO tool_call_events (
                id, timestamp, tool_id, tool_name, agent_id,
                project_id, task_id, organization_id, caller_organization_id, user_id,
                args_len, result_len, duration_ms, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(
            sql,
            [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.tool_id as &dyn ToSql,
                &event.tool_name as &dyn ToSql,
                &event.agent_id as &dyn ToSql,
                &event.project_id as &dyn ToSql,
                &event.task_id as &dyn ToSql,
                &event.organization_id as &dyn ToSql,
                &event.caller_organization_id as &dyn ToSql,
                &event.user_id as &dyn ToSql,
                &event.args_len as &dyn ToSql,
                &event.result_len as &dyn ToSql,
                &event.duration_ms as &dyn ToSql,
                &event.status as &dyn ToSql,
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert tool call event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[ToolCallEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let sql = r#"
                INSERT INTO tool_call_events (
                    id, timestamp, tool_id, tool_name, agent_id,
                    project_id, task_id, organization_id, caller_organization_id, user_id,
                    args_len, result_len, duration_ms, status
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
            conn.execute(
                sql,
                [
                    &id.to_string() as &dyn ToSql,
                    &event.timestamp as &dyn ToSql,
                    &event.tool_id as &dyn ToSql,
                    &event.tool_name as &dyn ToSql,
                    &event.agent_id as &dyn ToSql,
                    &event.project_id as &dyn ToSql,
                    &event.task_id as &dyn ToSql,
                    &event.organization_id as &dyn ToSql,
                    &event.caller_organization_id as &dyn ToSql,
                    &event.user_id as &dyn ToSql,
                    &event.args_len as &dyn ToSql,
                    &event.result_len as &dyn ToSql,
                    &event.duration_ms as &dyn ToSql,
                    &event.status as &dyn ToSql,
                ],
            )
            .map_err(|e| {
                common::error::Error::internal(format!(
                    "Failed to bulk insert tool call event: {}",
                    e
                ))
            })?;
        }
        Ok(())
    }
}
