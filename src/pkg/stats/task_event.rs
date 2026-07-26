//! Task 业务统计事件
//!
//! `TaskEvent` 绑定 `task_events` 表，
//! 用于记录任务生命周期中的关键业务动作：创建、开始、完成、取消、状态流转等。

use super::*;
use ai_orz_macros::StatsEvent;
use common::error::Result;
use duckdb::{Connection, ToSql};
use uuid::Uuid;

#[derive(Debug, Clone, StatsEvent)]
#[event_type = "task"]
pub struct TaskEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub task_id: String,
    #[tag]
    pub project_id: Option<String>,
    #[tag]
    pub event_type: String,
    #[tag]
    pub organization_id: Option<String>,
    #[tag]
    pub operator_type: Option<String>,
    #[tag]
    pub operator_id: Option<String>,
    #[tag]
    pub root_user_id: Option<String>,
    #[tag]
    pub assignee_type: Option<String>,
    #[tag]
    pub assignee_id: Option<String>,
    #[tag]
    pub from_assignee_id: Option<String>,
    #[tag]
    pub from_status: Option<String>,
    #[tag]
    pub to_status: Option<String>,
    #[metric]
    pub duration_ms: Option<u64>,
    #[metric]
    pub priority: i32,
}

impl TaskEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            task_id: String::new(),
            project_id: None,
            event_type: String::new(),
            organization_id: None,
            operator_type: None,
            operator_id: None,
            root_user_id: None,
            assignee_type: None,
            assignee_id: None,
            from_assignee_id: None,
            from_status: None,
            to_status: None,
            duration_ms: None,
            priority: 0,
        }
    }

    pub fn with_task_id(mut self, v: String) -> Self {
        self.task_id = v;
        self
    }

    pub fn with_project_id(mut self, v: Option<String>) -> Self {
        self.project_id = v;
        self
    }

    pub fn with_event_type(mut self, v: String) -> Self {
        self.event_type = v;
        self
    }

    pub fn with_organization_id(mut self, v: Option<String>) -> Self {
        self.organization_id = v;
        self
    }

    pub fn with_operator_type(mut self, v: Option<String>) -> Self {
        self.operator_type = v;
        self
    }

    pub fn with_operator_id(mut self, v: Option<String>) -> Self {
        self.operator_id = v;
        self
    }

    pub fn with_root_user_id(mut self, v: Option<String>) -> Self {
        self.root_user_id = v;
        self
    }

    pub fn with_assignee_type(mut self, v: Option<String>) -> Self {
        self.assignee_type = v;
        self
    }

    pub fn with_assignee_id(mut self, v: Option<String>) -> Self {
        self.assignee_id = v;
        self
    }

    pub fn with_from_assignee_id(mut self, v: Option<String>) -> Self {
        self.from_assignee_id = v;
        self
    }

    pub fn with_from_status(mut self, v: Option<String>) -> Self {
        self.from_status = v;
        self
    }

    pub fn with_to_status(mut self, v: Option<String>) -> Self {
        self.to_status = v;
        self
    }

    pub fn with_duration_ms(mut self, v: Option<u64>) -> Self {
        self.duration_ms = v;
        self
    }

    pub fn with_priority(mut self, v: i32) -> Self {
        self.priority = v;
        self
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TaskStatTable;

impl StatTable<TaskEvent> for TaskStatTable {
    fn table_name(&self) -> &str {
        "task_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS task_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                task_id VARCHAR,
                project_id VARCHAR,
                event_type VARCHAR,
                organization_id VARCHAR,
                operator_type VARCHAR,
                operator_id VARCHAR,
                root_user_id VARCHAR,
                assignee_type VARCHAR,
                assignee_id VARCHAR,
                from_assignee_id VARCHAR,
                from_status VARCHAR,
                to_status VARCHAR,
                duration_ms BIGINT,
                priority INTEGER
            );
        "#;
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!("Failed to create task_events table: {}", e))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &TaskEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO task_events (
                id, timestamp, task_id, project_id, event_type,
                organization_id, operator_type, operator_id,
                root_user_id, assignee_type, assignee_id,
                from_assignee_id, from_status, to_status,
                duration_ms, priority
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(
            sql,
            [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.task_id as &dyn ToSql,
                &event.project_id as &dyn ToSql,
                &event.event_type as &dyn ToSql,
                &event.organization_id as &dyn ToSql,
                &event.operator_type as &dyn ToSql,
                &event.operator_id as &dyn ToSql,
                &event.root_user_id as &dyn ToSql,
                &event.assignee_type as &dyn ToSql,
                &event.assignee_id as &dyn ToSql,
                &event.from_assignee_id as &dyn ToSql,
                &event.from_status as &dyn ToSql,
                &event.to_status as &dyn ToSql,
                &event.duration_ms as &dyn ToSql,
                &event.priority as &dyn ToSql,
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert task event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[TaskEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let sql = r#"
                INSERT INTO task_events (
                    id, timestamp, task_id, project_id, event_type,
                    organization_id, operator_type, operator_id,
                    root_user_id, assignee_type, assignee_id,
                    from_assignee_id, from_status, to_status,
                    duration_ms, priority
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
            "#;
            conn.execute(
                sql,
                [
                    &id.to_string() as &dyn ToSql,
                    &event.timestamp as &dyn ToSql,
                    &event.task_id as &dyn ToSql,
                    &event.project_id as &dyn ToSql,
                    &event.event_type as &dyn ToSql,
                    &event.organization_id as &dyn ToSql,
                    &event.operator_type as &dyn ToSql,
                    &event.operator_id as &dyn ToSql,
                    &event.root_user_id as &dyn ToSql,
                    &event.assignee_type as &dyn ToSql,
                    &event.assignee_id as &dyn ToSql,
                    &event.from_assignee_id as &dyn ToSql,
                    &event.from_status as &dyn ToSql,
                    &event.to_status as &dyn ToSql,
                    &event.duration_ms as &dyn ToSql,
                    &event.priority as &dyn ToSql,
                ],
            )
            .map_err(|e| {
                common::error::Error::internal(format!("Failed to bulk insert task event: {}", e))
            })?;
        }
        Ok(())
    }
}
