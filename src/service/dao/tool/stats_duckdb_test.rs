//! Tool Stats DAO DuckDB 单元测试

use crate::pkg::request_context_test_support;
use crate::pkg::stats::*;
use crate::service::dao::tool::stats_duckdb::stats_new;
use crate::service::dao::tool::{ToolStatsDao, ToolStatsQuery};
use chrono::Utc;
use common::error::Result;
use common::models::StatsFetchOptions;
use sqlx::SqlitePool;
use tempfile::tempdir;

async fn setup_test_env(
    tool_id: &str,
    agent_id: &str,
    success_count: usize,
    failed_count: usize,
) -> Result<(
    crate::pkg::RequestContext,
    std::sync::Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>,
)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.register_table(ToolCallStatTable)?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();
    for i in 0..success_count {
        let event = ToolCallEvent::new(now + i as i64 * 1000)
            .with_tool_id(tool_id.to_string())
            .with_tool_name(format!("tool_{}", tool_id))
            .with_agent_id(Some(agent_id.to_string()))
            .with_status("success".to_string())
            .with_duration_ms((100 + i * 10) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    for i in 0..failed_count {
        let event = ToolCallEvent::new(now + (success_count + i) as i64 * 1000)
            .with_tool_id(tool_id.to_string())
            .with_tool_name(format!("tool_{}", tool_id))
            .with_agent_id(Some(agent_id.to_string()))
            .with_status("failed".to_string())
            .with_duration_ms((50 + i * 10) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();
    Ok((ctx, dao))
}

#[tokio::test]
async fn test_sum_calls_basic() -> Result<()> {
    let tool_id = "tool-sum-test";
    let agent_id = "agent-1";
    let (ctx, dao) = setup_test_env(tool_id, agent_id, 5, 2).await?;

    let query = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 7);

    Ok(())
}

#[tokio::test]
async fn test_sum_calls_zero() -> Result<()> {
    let tool_id = "tool-zero-test";
    let agent_id = "agent-1";
    let (ctx, dao) = setup_test_env(tool_id, agent_id, 0, 0).await?;

    let query = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 0);

    Ok(())
}

#[tokio::test]
async fn test_sum_failed_calls() -> Result<()> {
    let tool_id = "tool-failed-test";
    let agent_id = "agent-1";
    let (ctx, dao) = setup_test_env(tool_id, agent_id, 3, 2).await?;

    let query = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_failed_calls(ctx, query).await?;

    assert_eq!(result, 2);

    Ok(())
}

#[tokio::test]
async fn test_get_stats_with_call_summary() -> Result<()> {
    let tool_id = "tool-get-stats-test";
    let agent_id = "agent-1";
    let (ctx, dao) = setup_test_env(tool_id, agent_id, 10, 3).await?;

    let query = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        ..Default::default()
    };

    let options = StatsFetchOptions {
        with_call_summary: true,
        with_token_summary: false,
        with_time_series: false,
        time_range: None,
        interval: None,
    };

    let stats = dao.get_stats(ctx, query, options).await?;

    assert!(stats.call_summary.is_some());
    let call_summary = stats.call_summary.unwrap();
    assert_eq!(call_summary.total_calls, 13);
    assert!(call_summary.instant_qps >= 0.0);
    assert!(call_summary.avg_qps.is_none());
    assert_eq!(stats.failed_count, Some(3));

    Ok(())
}

#[tokio::test]
async fn test_get_stats_without_call_summary() -> Result<()> {
    let tool_id = "tool-no-summary-test";
    let agent_id = "agent-1";
    let (ctx, dao) = setup_test_env(tool_id, agent_id, 5, 2).await?;

    let query = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        ..Default::default()
    };

    let options = StatsFetchOptions {
        with_call_summary: false,
        with_token_summary: false,
        with_time_series: false,
        time_range: None,
        interval: None,
    };

    let stats = dao.get_stats(ctx, query, options).await?;

    assert!(stats.call_summary.is_none());
    assert!(stats.failed_count.is_none());

    Ok(())
}

#[tokio::test]
async fn test_filter_by_agent_id() -> Result<()> {
    let tool_id = "tool-agent-filter";
    let agent_a = "agent-a";
    let agent_b = "agent-b";

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.register_table(ToolCallStatTable)?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();

    for i in 0..5 {
        let event = ToolCallEvent::new(now + i as i64 * 1000)
            .with_tool_id(tool_id.to_string())
            .with_tool_name(format!("tool_{}", tool_id))
            .with_agent_id(Some(agent_a.to_string()))
            .with_status("success".to_string())
            .with_duration_ms(100);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    for i in 0..3 {
        let event = ToolCallEvent::new(now + i as i64 * 1000)
            .with_tool_id(tool_id.to_string())
            .with_tool_name(format!("tool_{}", tool_id))
            .with_agent_id(Some(agent_b.to_string()))
            .with_status("success".to_string())
            .with_duration_ms(200);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let query_a = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        agent_id: Some(agent_a.to_string()),
        ..Default::default()
    };
    let result_a = dao.sum_calls(ctx.clone(), query_a).await?;
    assert_eq!(result_a, 5);

    let query_b = ToolStatsQuery {
        tool_id: tool_id.to_string(),
        agent_id: Some(agent_b.to_string()),
        ..Default::default()
    };
    let result_b = dao.sum_calls(ctx, query_b).await?;
    assert_eq!(result_b, 3);

    Ok(())
}
