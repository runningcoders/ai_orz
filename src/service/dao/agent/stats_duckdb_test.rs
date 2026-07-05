//! Agent Stats DAO DuckDB 单元测试

use crate::pkg::stats::*;
use crate::pkg::request_context_test_support;
use crate::service::dao::agent::{AgentStatsDao, AgentStatsQuery};
use crate::service::dao::agent::stats_duckdb::stats_new;
use common::error::Result;
use common::models::StatsFetchOptions;
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

async fn setup_test_env(
    agent_id: &str,
    event_count: usize,
) -> Result<(crate::pkg::RequestContext, std::sync::Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();
    for i in 0..event_count {
        let event = AgentAwakeEvent::new(now + i as i64 * 1000)
            .with_agent_id(agent_id.to_string())
            .with_duration_ms((100 + i * 10) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();
    Ok((ctx, dao))
}

#[tokio::test]
async fn test_sum_calls_basic() -> Result<()> {
    let agent_id = "agent-sum-test";
    let (ctx, dao) = setup_test_env(agent_id, 5).await?;

    let query = AgentStatsQuery {
        agent_id: agent_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 5);

    Ok(())
}

#[tokio::test]
async fn test_sum_calls_zero() -> Result<()> {
    let agent_id = "agent-zero-test";
    let (ctx, dao) = setup_test_env(agent_id, 0).await?;

    let query = AgentStatsQuery {
        agent_id: agent_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 0);

    Ok(())
}

#[tokio::test]
async fn test_sum_calls_with_time_range() -> Result<()> {
    let agent_id = "agent-time-range-test";
    let (ctx, dao) = setup_test_env(agent_id, 5).await?;

    let now = Utc::now().timestamp_millis();

    let query_all = AgentStatsQuery {
        agent_id: agent_id.to_string(),
        time_range: Some((now - 100000, now + 100000)),
        ..Default::default()
    };
    let result_all = dao.sum_calls(ctx.clone(), query_all).await?;
    assert_eq!(result_all, 5);

    let query_none = AgentStatsQuery {
        agent_id: agent_id.to_string(),
        time_range: Some((now - 1000000, now - 500000)),
        ..Default::default()
    };
    let result_none = dao.sum_calls(ctx, query_none).await?;
    assert_eq!(result_none, 0);

    Ok(())
}

#[tokio::test]
async fn test_get_stats_with_call_summary() -> Result<()> {
    let agent_id = "agent-get-stats-test";
    let (ctx, dao) = setup_test_env(agent_id, 3).await?;

    let query = AgentStatsQuery {
        agent_id: agent_id.to_string(),
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
    assert_eq!(call_summary.total_calls, 3);
    assert!(call_summary.instant_qps >= 0.0);
    assert!(call_summary.avg_qps.is_none());

    Ok(())
}

#[tokio::test]
async fn test_get_stats_without_call_summary() -> Result<()> {
    let agent_id = "agent-no-summary-test";
    let (ctx, dao) = setup_test_env(agent_id, 3).await?;

    let query = AgentStatsQuery {
        agent_id: agent_id.to_string(),
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

    Ok(())
}

#[tokio::test]
async fn test_filter_by_different_agent() -> Result<()> {
    let agent_a = "agent-a";
    let agent_b = "agent-b";

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();

    for i in 0..3 {
        let event = AgentAwakeEvent::new(now + i as i64 * 1000)
            .with_agent_id(agent_a.to_string())
            .with_duration_ms((100 + i * 10) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    for i in 0..2 {
        let event = AgentAwakeEvent::new(now + i as i64 * 1000)
            .with_agent_id(agent_b.to_string())
            .with_duration_ms((200 + i * 10) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let query_a = AgentStatsQuery {
        agent_id: agent_a.to_string(),
        ..Default::default()
    };
    let result_a = dao.sum_calls(ctx.clone(), query_a).await?;
    assert_eq!(result_a, 3);

    let query_b = AgentStatsQuery {
        agent_id: agent_b.to_string(),
        ..Default::default()
    };
    let result_b = dao.sum_calls(ctx, query_b).await?;
    assert_eq!(result_b, 2);

    Ok(())
}
