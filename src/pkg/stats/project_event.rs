//! Project 业务统计事件
//!
//! `ProjectEvent` 绑定 `project_events` 表，
//! 用于记录项目生命周期中的关键业务动作：创建、启动、完成、归档、状态流转等。

use super::*;
use ai_orz_macros::StatsEvent;
use uuid::Uuid;
use duckdb::{Connection, ToSql};
use common::error::Result;

#[derive(Debug, Clone, StatsEvent)]
#[event_type = "project"]
pub struct ProjectEvent {
    #[timestamp]
    pub timestamp: i64,
    #[tag]
    pub project_id: String,
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
    pub owner_type: Option<String>,
    #[tag]
    pub owner_id: Option<String>,
    #[tag]
    pub from_status: Option<String>,
    #[tag]
    pub to_status: Option<String>,
    #[metric]
    pub duration_ms: Option<u64>,
    #[metric]
    pub priority: i32,
}

impl ProjectEvent {
    pub fn new(timestamp: i64) -> Self {
        Self {
            timestamp,
            project_id: String::new(),
            event_type: String::new(),
            organization_id: None,
            operator_type: None,
            operator_id: None,
            root_user_id: None,
            owner_type: None,
            owner_id: None,
            from_status: None,
            to_status: None,
            duration_ms: None,
            priority: 0,
        }
    }

    pub fn with_project_id(mut self, v: String) -> Self {
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

    pub fn with_owner_type(mut self, v: Option<String>) -> Self {
        self.owner_type = v;
        self
    }

    pub fn with_owner_id(mut self, v: Option<String>) -> Self {
        self.owner_id = v;
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
pub struct ProjectStatTable;

impl StatTable<ProjectEvent> for ProjectStatTable {
    fn table_name(&self) -> &str {
        "project_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS project_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                project_id VARCHAR,
                event_type VARCHAR,
                organization_id VARCHAR,
                operator_type VARCHAR,
                operator_id VARCHAR,
                root_user_id VARCHAR,
                owner_type VARCHAR,
                owner_id VARCHAR,
                from_status VARCHAR,
                to_status VARCHAR,
                duration_ms BIGINT,
                priority INTEGER
            );
        "#;
        conn.execute(sql, [])
            .map_err(|e| common::error::Error::internal(format!("Failed to create project_events table: {}", e)))?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &ProjectEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO project_events (
                id, timestamp, project_id, event_type,
                organization_id, operator_type, operator_id,
                root_user_id, owner_type, owner_id,
                from_status, to_status, duration_ms, priority
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(sql, [
            &id.to_string() as &dyn ToSql,
            &event.timestamp as &dyn ToSql,
            &event.project_id as &dyn ToSql,
            &event.event_type as &dyn ToSql,
            &event.organization_id as &dyn ToSql,
            &event.operator_type as &dyn ToSql,
            &event.operator_id as &dyn ToSql,
            &event.root_user_id as &dyn ToSql,
            &event.owner_type as &dyn ToSql,
            &event.owner_id as &dyn ToSql,
            &event.from_status as &dyn ToSql,
            &event.to_status as &dyn ToSql,
            &event.duration_ms as &dyn ToSql,
            &event.priority as &dyn ToSql,
        ])
            .map_err(|e| common::error::Error::internal(format!("Failed to insert project event: {}", e)))?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[ProjectEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::now_v7();
            let sql = r#"
                INSERT INTO project_events (
                    id, timestamp, project_id, event_type,
                    organization_id, operator_type, operator_id,
                    root_user_id, owner_type, owner_id,
                    from_status, to_status, duration_ms, priority
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
            "#;
            conn.execute(sql, [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.project_id as &dyn ToSql,
                &event.event_type as &dyn ToSql,
                &event.organization_id as &dyn ToSql,
                &event.operator_type as &dyn ToSql,
                &event.operator_id as &dyn ToSql,
                &event.root_user_id as &dyn ToSql,
                &event.owner_type as &dyn ToSql,
                &event.owner_id as &dyn ToSql,
                &event.from_status as &dyn ToSql,
                &event.to_status as &dyn ToSql,
                &event.duration_ms as &dyn ToSql,
                &event.priority as &dyn ToSql,
            ])
                .map_err(|e| common::error::Error::internal(format!("Failed to bulk insert project event: {}", e)))?;
        }
        Ok(())
    }
}
