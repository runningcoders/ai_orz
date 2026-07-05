//! Project Stats DAO DuckDB 单元测试

use crate::pkg::stats::*;
use crate::pkg::request_context_test_support;
use crate::service::dao::project::{ProjectStatsDao, ProjectStatsQuery};
use crate::service::dao::project::stats_duckdb::stats_new;
use common::error::Result;
use common::models::StatsFetchOptions;
use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

async fn setup_test_env(
    project_id: &str,
    event_count: usize,
) -> Result<(crate::pkg::RequestContext, std::sync::Arc<dyn ProjectStatsDao<ModelCallEvent = ModelCallEvent>>)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();
    for i in 0..event_count {
        let event = ModelCallEvent::new(now + i as i64 * 1000)
            .with_project_id(Some(project_id.to_string()))
            .with_agent_id(Some("agent-test".to_string()))
            .with_tokens_input((100 + i * 10) as u64)
            .with_tokens_output((50 + i * 5) as u64)
            .with_total_tokens((150 + i * 15) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();
    Ok((ctx, dao))
}

#[tokio::test]
async fn test_sum_calls_basic() -> Result<()> {
    let project_id = "project-sum-test";
    let (ctx, dao) = setup_test_env(project_id, 5).await?;

    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 5);

    Ok(())
}

#[tokio::test]
async fn test_sum_calls_zero() -> Result<()> {
    let project_id = "project-empty-test";
    let (ctx, dao) = setup_test_env(project_id, 0).await?;

    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 0);

    Ok(())
}

#[tokio::test]
async fn test_sum_calls_with_time_range() -> Result<()> {
    let project_id = "project-time-range-test";
    let (ctx, dao) = setup_test_env(project_id, 5).await?;

    let now = Utc::now().timestamp_millis();
    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        time_range: Some((now - 100000, now + 100000)),
        ..Default::default()
    };

    let result = dao.sum_calls(ctx, query).await?;

    assert_eq!(result, 5);

    Ok(())
}

#[tokio::test]
async fn test_get_stats_with_call_summary() -> Result<()> {
    let project_id = "project-stats-test";
    let (ctx, dao) = setup_test_env(project_id, 5).await?;

    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        ..Default::default()
    };

    let options = StatsFetchOptions {
        with_call_summary: true,
        ..Default::default()
    };

    let stats = dao.get_stats(ctx, query, options).await?;

    assert!(stats.call_summary.is_some());
    let call_summary = stats.call_summary.unwrap();
    assert_eq!(call_summary.total_calls, 5);
    assert!(call_summary.instant_qps >= 0.0);

    Ok(())
}

#[tokio::test]
async fn test_get_stats_without_call_summary() -> Result<()> {
    let project_id = "project-stats-no-summary-test";
    let (ctx, dao) = setup_test_env(project_id, 5).await?;

    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        ..Default::default()
    };

    let options = StatsFetchOptions::default();

    let stats = dao.get_stats(ctx, query, options).await?;

    assert!(stats.call_summary.is_none());

    Ok(())
}

#[tokio::test]
async fn test_filter_by_different_project() -> Result<()> {
    let project_a = "project-a";
    let project_b = "project-b";

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();

    for i in 0..3 {
        let event = ModelCallEvent::new(now + i as i64 * 1000)
            .with_project_id(Some(project_a.to_string()))
            .with_agent_id(Some("agent-test".to_string()))
            .with_tokens_input((100 + i * 10) as u64)
            .with_tokens_output((50 + i * 5) as u64)
            .with_total_tokens((150 + i * 15) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    for i in 0..2 {
        let event = ModelCallEvent::new(now + i as i64 * 1000)
            .with_project_id(Some(project_b.to_string()))
            .with_agent_id(Some("agent-test".to_string()))
            .with_tokens_input((200 + i * 10) as u64)
            .with_tokens_output((100 + i * 5) as u64)
            .with_total_tokens((300 + i * 15) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let query_a = ProjectStatsQuery {
        project_id: project_a.to_string(),
        ..Default::default()
    };
    let result_a = dao.sum_calls(ctx.clone(), query_a).await?;
    assert_eq!(result_a, 3);

    let query_b = ProjectStatsQuery {
        project_id: project_b.to_string(),
        ..Default::default()
    };
    let result_b = dao.sum_calls(ctx, query_b).await?;
    assert_eq!(result_b, 2);

    Ok(())
}
