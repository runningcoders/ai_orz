//! Project Stats DAO DuckDB 单元测试

use crate::pkg::stats::*;
use crate::pkg::request_context_test_support;
use crate::service::dao::project::{ProjectStatsDao, ProjectStatsQuery};
use crate::service::dao::project::stats_duckdb::stats_new;
use common::error::Result;
use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::tempdir;

async fn setup_test_env(
    project_id: &str,
    event_count: usize,
) -> Result<(crate::pkg::RequestContext, std::sync::Arc<dyn ProjectStatsDao>)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let mut stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp();
    for i in 0..event_count {
        let event = DefaultStatEvent::new(now + i as i64 * 1000)
            .with_tags(json!({
                "project_id": project_id,
                "agent_id": "agent-test",
            }))
            .with_metrics(json!({
                "tokens_input": 100 + i * 10,
                "tokens_output": 50 + i * 5,
            }));
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();
    Ok((ctx, dao))
}

#[tokio::test]
async fn test_sum_tokens_basic() -> Result<()> {
    let project_id = "project-sum-test";
    let (ctx, dao) = setup_test_env(project_id, 5).await?;

    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_tokens(ctx, query).await?;

    assert_eq!(result.total_calls, 5);
    assert_eq!(result.total_tokens_input, 100 + 110 + 120 + 130 + 140);
    assert_eq!(result.total_tokens_output, 50 + 55 + 60 + 65 + 70);

    Ok(())
}

#[tokio::test]
async fn test_query_time_series() -> Result<()> {
    let project_id = "project-ts-test";
    let (ctx, dao) = setup_test_env(project_id, 3).await?;

    let now = Utc::now().timestamp();
    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        time_range: Some((now - 10000, now + 10000)),
        interval: Some(StatsInterval::Hourly),
        ..Default::default()
    };

    let points = dao.query_time_series(ctx, query).await?;

    assert!(!points.is_empty());
    let total_calls: u64 = points.iter().map(|p| p.call_count).sum();
    assert_eq!(total_calls, 3);

    Ok(())
}

#[tokio::test]
async fn test_query_aggregation_with_group_by() -> Result<()> {
    let project_id = "project-agg-test";
    let (ctx, dao) = setup_test_env(project_id, 4).await?;

    let query = ProjectStatsQuery {
        project_id: project_id.to_string(),
        group_by: vec!["agent_id".to_string()],
        aggregations: vec![
            StatAggregation::Count,
            StatAggregation::Sum("tokens_input".to_string()),
        ],
        ..Default::default()
    };

    let rows = dao.query_aggregation(ctx, query).await?;

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.groups.get("agent_id"), Some(&json!("agent-test")));
    assert_eq!(row.aggregations.get("count"), Some(&4.0));
    assert_eq!(row.aggregations.get("tokens_input"), Some(&(100.0 + 110.0 + 120.0 + 130.0)));

    Ok(())
}

#[tokio::test]
async fn test_filter_by_different_project() -> Result<()> {
    let project_a = "project-a";
    let project_b = "project-b";

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let mut stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp();

    for i in 0..3 {
        let event = DefaultStatEvent::new(now + i as i64 * 1000)
            .with_tags(json!({
                "project_id": project_a,
                "agent_id": "agent-test",
            }))
            .with_metrics(json!({
                "tokens_input": 100 + i * 10,
                "tokens_output": 50 + i * 5,
            }));
        stats.record(tmp_ctx.clone(), event).await?;
    }

    for i in 0..2 {
        let event = DefaultStatEvent::new(now + i as i64 * 1000)
            .with_tags(json!({
                "project_id": project_b,
                "agent_id": "agent-test",
            }))
            .with_metrics(json!({
                "tokens_input": 200 + i * 10,
                "tokens_output": 100 + i * 5,
            }));
        stats.record(tmp_ctx.clone(), event).await?;
    }

    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let query_a = ProjectStatsQuery {
        project_id: project_a.to_string(),
        ..Default::default()
    };
    let result_a = dao.sum_tokens(ctx.clone(), query_a).await?;
    assert_eq!(result_a.total_calls, 3);

    let query_b = ProjectStatsQuery {
        project_id: project_b.to_string(),
        ..Default::default()
    };
    let result_b = dao.sum_tokens(ctx, query_b).await?;
    assert_eq!(result_b.total_calls, 2);
    assert_eq!(result_b.total_tokens_input, 200 + 210);

    Ok(())
}
