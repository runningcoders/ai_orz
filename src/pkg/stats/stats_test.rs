//! 统计模块单元测试

#![cfg(test)]

use crate::pkg::RequestContext;
use crate::pkg::stats::*;
use chrono::Utc;
use common::error::Result;
use duckdb::{Connection, ToSql};
use serde_json::{Value, json};
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn test_open_and_create_default_table() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    assert_eq!(stats.registered_table_count(), 6);

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
    let result = stats
        .query(
            ctx.clone(),
            "SELECT COUNT(*) as count FROM default_events",
            &[],
        )
        .await?;
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
    let result = stats
        .query(
            ctx.clone(),
            "SELECT COUNT(*) as count FROM default_events",
            &[],
        )
        .await?;
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
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!("Failed to create agent_execution table: {}", e))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &AgentExecutionEvent) -> Result<()> {
        let sql = "INSERT INTO agent_execution 
                   (id, timestamp, event_type, tags, metrics, agent_id, task_id, start_timestamp, end_timestamp, total_tokens, success, error_message)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let tags_json = event.tags.as_ref().map(|v| v.to_string());
        let metrics_json = event.metrics.as_ref().map(|v| v.to_string());
        conn.execute(
            sql,
            [
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
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert agent execution event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(
        &self,
        conn: &mut Connection,
        events: &[AgentExecutionEvent],
    ) -> Result<()> {
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

    assert_eq!(stats.registered_table_count(), 7);

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
    let result = stats
        .query(
            ctx.clone(),
            "SELECT COUNT(*) as count FROM agent_execution",
            &[],
        )
        .await?;
    assert_eq!(result.len(), 1);
    let count: i64 = result[0].get("count").unwrap().as_i64().unwrap();
    assert_eq!(count, 3);

    Ok(())
}

// ==================== AgentAwakeEvent 测试 ====================

#[tokio::test]
async fn test_agent_awake_event_exit_reason_write_and_read() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let ctx = RequestContext::new(None, None);
    let now = Utc::now().timestamp();

    // 记录 3 个不同 exit_reason 的事件
    let reasons = ["final", "maxroundsexceeded", "cancelled"];
    for reason in &reasons {
        let event = AgentAwakeEvent::new(now)
            .with_agent_id("agent_test_001".to_string())
            .with_status("success".to_string())
            .with_exit_reason(reason.to_string());
        stats.record(ctx.clone(), event).await?;
    }

    assert_eq!(stats.pending_buffer_len::<AgentAwakeEvent>(), 3);
    stats.flush_all(ctx.clone()).await?;
    assert_eq!(stats.pending_buffer_len::<AgentAwakeEvent>(), 0);

    // 查询验证 exit_reason 字段正确写入
    let result = stats
        .query(
            ctx.clone(),
            "SELECT exit_reason FROM agent_awake_events WHERE agent_id = 'agent_test_001' ORDER BY exit_reason",
            &[],
        )
        .await?;

    assert_eq!(result.len(), 3);
    let got: Vec<String> = result
        .iter()
        .map(|row| {
            row.get("exit_reason")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(got, vec!["cancelled", "final", "maxroundsexceeded"]);

    Ok(())
}

#[tokio::test]
async fn test_agent_awake_event_default_exit_reason_is_empty() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let ctx = RequestContext::new(None, None);
    let now = Utc::now().timestamp();

    // 不设置 exit_reason，验证默认为空字符串
    let event = AgentAwakeEvent::new(now)
        .with_agent_id("agent_test_002".to_string())
        .with_status("success".to_string());
    stats.record(ctx.clone(), event).await?;
    stats.flush_all(ctx.clone()).await?;

    let result = stats
        .query(
            ctx.clone(),
            "SELECT exit_reason FROM agent_awake_events WHERE agent_id = 'agent_test_002'",
            &[],
        )
        .await?;

    assert_eq!(result.len(), 1);
    let exit_reason = result[0].get("exit_reason").unwrap().as_str().unwrap();
    assert_eq!(exit_reason, "");

    Ok(())
}

/// 审计维度（联邦调用方组织）：ctx 带 caller_organization_id 时，
/// record 自动注入到事件列；本地路径恒 None。
/// 见 docs/plan/跨组织业务调用方案.md §八
#[tokio::test]
async fn test_caller_organization_id_auto_injection() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let now = Utc::now().timestamp();
    let peer_org = "org_peer_001".to_string();

    // 联邦路径：ctx 带 caller_organization_id
    let fed_ctx = RequestContext::builder()
        .try_caller_organization_id(Some(peer_org.clone()))
        .build();
    let fed_event = ProjectEvent::new(now)
        .with_project_id("proj_fed_001".to_string())
        .with_event_type("created".to_string());
    stats.record(fed_ctx.clone(), fed_event).await?;

    // 本地路径：ctx 无 caller_organization_id
    let local_ctx = RequestContext::new(None, None);
    let local_event = ProjectEvent::new(now)
        .with_project_id("proj_local_001".to_string())
        .with_event_type("created".to_string());
    stats.record(local_ctx.clone(), local_event).await?;

    stats.flush_all(fed_ctx.clone()).await?;

    // 联邦事件：caller_organization_id = 对端 org
    let rows = stats
        .query(
            fed_ctx.clone(),
            "SELECT caller_organization_id FROM project_events WHERE project_id = 'proj_fed_001'",
            &[],
        )
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("caller_organization_id").unwrap().as_str(),
        Some(peer_org.as_str())
    );

    // 本地事件：恒 None
    let rows = stats
        .query(
            fed_ctx.clone(),
            "SELECT caller_organization_id FROM project_events WHERE project_id = 'proj_local_001'",
            &[],
        )
        .await?;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("caller_organization_id").unwrap().is_null());

    Ok(())
}

/// 字段缺失兼容：统计事件的 tags / metrics 都是可选的，
/// 缺失时绑 NULL（而非空串，空串不是合法 JSON，DuckDB 的 JSON 列会拒绝并拖垮整批）。
#[tokio::test]
async fn test_missing_optional_json_fields_are_compatible() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let stats = Stats::open(db_path.to_str().unwrap(), 100).await?;
    stats.initialize_default()?;

    let ctx = RequestContext::new(None, None);
    let now = Utc::now().timestamp();

    // 三种缺失组合：全缺 / 只有 tags / 只有 metrics
    let events = vec![
        DefaultStatEvent::new(now),
        DefaultStatEvent::new(now).with_tags(json!({ "only": "tags" })),
        DefaultStatEvent::new(now).with_metrics(json!({ "only": "metrics" })),
    ];
    for event in events {
        stats.record(ctx.clone(), event).await?;
    }

    // 整批落盘不应因任一事件缺字段而失败
    stats.flush_all(ctx.clone()).await?;
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 0);

    let rows = stats
        .query(
            ctx.clone(),
            "SELECT COUNT(*) AS count, COUNT(tags) AS with_tags, COUNT(metrics) AS with_metrics FROM default_events",
            &[],
        )
        .await?;
    let row = rows[0].as_object().unwrap();
    assert_eq!(row.get("count").unwrap().as_i64(), Some(3));
    assert_eq!(row.get("with_tags").unwrap().as_i64(), Some(1));
    assert_eq!(row.get("with_metrics").unwrap().as_i64(), Some(1));

    // 缺失字段上做 JSON 提取不应报错，只是取不到值
    let rows = stats
        .query(
            ctx.clone(),
            "SELECT json_extract_string(tags, '$.only') AS v FROM default_events ORDER BY v",
            &[],
        )
        .await?;
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .filter(|r| !r.get("v").unwrap().is_null())
            .count(),
        1
    );

    Ok(())
}

/// 周期落盘：`open_and_init` 按 `flush_interval_secs` 主动 flush，
/// 低频实例（长期攒不满 batch_size）也能让近期数据对查询可见。
#[tokio::test]
async fn test_periodic_flush_via_open_and_init() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let config = common::config::StatsConfig {
        // 远大于本次写入量：确保不是「缓冲攒满」触发的落盘
        batch_size: 100,
        flush_interval_secs: 1,
        ..Default::default()
    };

    let stats = Stats::open_and_init(db_path.to_str().unwrap(), &config).await?;

    let ctx = RequestContext::new(None, None);
    let event = DefaultStatEvent::new(Utc::now().timestamp())
        .with_tags(json!({ "kind": "periodic" }))
        .with_metrics(json!({ "value": 1 }));
    stats.record(ctx.clone(), event).await?;
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 1);

    // 不做任何手动 flush，等一个周期后缓冲应由周期任务落盘
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 0);

    let rows = stats
        .query(
            ctx.clone(),
            "SELECT COUNT(*) AS count FROM default_events",
            &[],
        )
        .await?;
    assert_eq!(rows[0].get("count").unwrap().as_i64(), Some(1));

    Ok(())
}

/// `flush_interval_secs = 0` 关闭周期落盘：缓冲保持不落盘，
/// 退化为「仅靠 batch_size 与退出时落盘」。
#[tokio::test]
async fn test_periodic_flush_disabled_when_interval_zero() -> Result<()> {
    crate::pkg::storage::test_support::init_for_test().await;

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let config = common::config::StatsConfig {
        batch_size: 100,
        flush_interval_secs: 0,
        ..Default::default()
    };

    let stats = Stats::open_and_init(db_path.to_str().unwrap(), &config).await?;

    let ctx = RequestContext::new(None, None);
    let event = DefaultStatEvent::new(Utc::now().timestamp());
    stats.record(ctx.clone(), event).await?;

    // 若误启动了周期任务，1s 后缓冲会被清空
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    assert_eq!(stats.pending_buffer_len::<DefaultStatEvent>(), 1);

    Ok(())
}
