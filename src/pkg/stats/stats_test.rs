//! 统计模块单元测试

#![cfg(test)]

use crate::pkg::stats::*;
use crate::pkg::RequestContext;
use common::error::Result;
use duckdb::{Connection, ToSql};
use serde_json::{json, Value};
use tempfile::tempdir;
use chrono::Utc;
use uuid::Uuid;

#[tokio::test]
async fn test_open_and_create_default_table() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    assert_eq!(stats.registered_table_count(), 3);

    Ok(())
}

#[tokio::test]
async fn test_record_single_event() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let ctx = RequestContext::new(None, None);
    let now = Utc::now().timestamp();
    let event = DefaultStatEvent::new(now)
        .with_tags(json!({ "env": "test" }))
        .with_metrics(json!({ "duration_ms": 123 }));
    stats.record(ctx, event).await?;

    // 检查缓冲计数
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 1);

    Ok(())
}

#[tokio::test]
async fn test_manual_flush() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let ctx = RequestContext::new(None, None);

    // 插入 3 个事件，手动 flush
    for i in 0..3 {
        let now = Utc::now().timestamp();
        let event = DefaultStatEvent::new(now)
            .with_tags(json!({ "index": i }))
            .with_metrics(json!({ "value": i * 10 }));
        stats.record(ctx.clone(), event).await?;
    }

    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 3);

    // 手动 flush
    stats.flush_all(ctx.clone()).await?;
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 0);

    // 查询验证
    let result = stats.query(ctx.clone(), "SELECT COUNT(*) as count FROM default_events", &[]).await?;
    assert_eq!(result.len(), 1);
    let count: i64 = result[0].get("count").unwrap().as_i64().unwrap();
    assert_eq!(count, 3);

    Ok(())
}

#[tokio::test]
async fn test_batch_flush() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    // batch size = 5
    let stats = Stats::open(db_path_str, 5).await?;
    stats.initialize_default()?;

    let ctx = RequestContext::new(None, None);

    // 记录 4 个事件，还没到 batch size，应该还在缓冲
    for i in 0..4 {
        let now = Utc::now().timestamp();
        let event = DefaultStatEvent::new(now)
            .with_tags(json!({ "index": i }))
            .with_metrics(json!({ "value": i * 10 }));
        stats.record(ctx.clone(), event).await?;
    }

    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 4);

    // 再记录一个，达到 batch size，自动 flush
    let now = Utc::now().timestamp();
    let event = DefaultStatEvent::new(now)
        .with_tags(json!({ "index": 4 }))
        .with_metrics(json!({ "value": 40 }));
    stats.record(ctx.clone(), event).await?;

    // 自动 flush 后缓冲应该为空
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 0);

    // 查询验证总共 5 条
    stats.flush_all(ctx.clone()).await?;
    let result = stats.query(ctx.clone(), "SELECT COUNT(*) as count FROM default_events", &[]).await?;
    assert_eq!(result.len(), 1);
    let count: i64 = result[0].get("count").unwrap().as_i64().unwrap();
    assert_eq!(count, 5);

    Ok(())
}

// 自定义事件测试
#[derive(Debug, Clone)]
struct AgentExecutionEvent {
    id: Uuid,
    timestamp: i64,
    agent_id: String,
    task_id: String,
    start_timestamp: i64,
    end_timestamp: Option<i64>,
    total_tokens: Option<i32>,
    success: bool,
    error_message: Option<String>,
    tags: Option<Value>,
    metrics: Option<Value>,
}

impl AgentExecutionEvent {
    fn new(agent_id: String, task_id: String, start_timestamp: i64) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: start_timestamp,
            agent_id,
            task_id,
            start_timestamp,
            end_timestamp: None,
            total_tokens: None,
            success: true,
            error_message: None,
            tags: None,
            metrics: None,
        }
    }
}

impl StatEvent for AgentExecutionEvent {
    fn timestamp(&self) -> i64 {
        self.timestamp
    }

    fn tags(&self) -> Option<&Value> {
        self.tags.as_ref()
    }

    fn tags_json(&self) -> Option<Value> {
        self.tags.clone()
    }

    fn metrics(&self) -> Option<&Value> {
        self.metrics.as_ref()
    }

    fn metrics_json(&self) -> Option<Value> {
        self.metrics.clone()
    }
}

#[derive(Debug, Clone, Copy)]
struct AgentExecutionTable;

impl StatTable<AgentExecutionEvent> for AgentExecutionTable {
    fn table_name(&self) -> &str {
        "agent_execution"
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS agent_execution (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                event_type VARCHAR,
                tags JSON,
                metrics JSON,
                agent_id VARCHAR,
                task_id VARCHAR,
                start_timestamp BIGINT,
                end_timestamp BIGINT,
                total_tokens INTEGER,
                success BOOLEAN,
                error_message VARCHAR
            );
        "#;
        conn.execute(sql, []).map_err(|e| 
            common::error::Error::internal(format!("Failed to create agent_execution table: {}", e))
        )?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &AgentExecutionEvent) -> Result<()> {
        let sql = "INSERT INTO agent_execution 
                   (id, timestamp, event_type, tags, metrics, agent_id, task_id, start_timestamp, end_timestamp, total_tokens, success, error_message)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let tags_json = event.tags.as_ref().map(|v| v.to_string());
        let metrics_json = event.metrics.as_ref().map(|v| v.to_string());
        conn.execute(sql, [
            &event.id.to_string() as &dyn ToSql,
            &event.timestamp as &dyn ToSql,
            &"agent_execution".to_string() as &dyn ToSql,
            &tags_json as &dyn ToSql,
            &metrics_json as &dyn ToSql,
            &event.agent_id as &dyn ToSql,
            &event.task_id as &dyn ToSql,
            &event.start_timestamp as &dyn ToSql,
            &event.end_timestamp as &dyn ToSql,
            &event.total_tokens as &dyn ToSql,
            &event.success as &dyn ToSql,
            &event.error_message as &dyn ToSql,
        ]).map_err(|e| 
            common::error::Error::internal(format!("Failed to insert agent execution event: {}", e))
        )?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[AgentExecutionEvent]) -> Result<()> {
        for event in events {
            self.insert_event(conn, event)?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn test_custom_event() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;
    stats.register_table(AgentExecutionTable)?;

    assert_eq!(stats.registered_table_count(), 4);

    let ctx = RequestContext::new(None, None);
    let now = Utc::now().timestamp();

    // 记录 3 个agent执行事件
    for i in 0..3 {
        let event = AgentExecutionEvent::new(
            "agent_123".to_string(),
            format!("task_{}", i),
            now + i as i64 * 1000,
        );
        stats.record(ctx.clone(), event).await?;
    }

    assert_eq!(stats.pending_buffer_len::<AgentExecutionEvent>(), 3);

    stats.flush_all(ctx.clone()).await?;
    assert_eq!(stats.pending_buffer_len::<AgentExecutionEvent>(), 0);

    // 查询验证
    let result = stats.query(ctx.clone(), "SELECT COUNT(*) as count FROM agent_execution", &[]).await?;
    assert_eq!(result.len(), 1);
    let count: i64 = result[0].get("count").unwrap().as_i64().unwrap();
    assert_eq!(count, 3);

    Ok(())
}