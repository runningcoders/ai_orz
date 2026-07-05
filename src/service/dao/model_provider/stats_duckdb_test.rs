//! ModelProvider Stats DAO DuckDB 单元测试

use crate::pkg::stats::*;
use crate::pkg::request_context_test_support;
use crate::service::dao::model_provider::{ModelProviderStatsDao, ModelProviderStatsQuery};
use crate::service::dao::model_provider::stats_duckdb::stats_new;
use common::error::Result;
use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::tempdir;

async fn setup_test_env(
    model_provider_id: &str,
    event_count: usize,
) -> Result<(crate::pkg::RequestContext, std::sync::Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>)> {
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
            .with_model_provider_id(Some(model_provider_id.to_string()))
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
async fn test_sum_tokens_basic() -> Result<()> {
    let model_provider_id = "provider-sum-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 5).await?;

    let query = ModelProviderStatsQuery {
        model_provider_id: model_provider_id.to_string(),
        ..Default::default()
    };

    let result = dao.sum_tokens(ctx, query).await?;

    assert_eq!(result.total_calls, 5);
    assert_eq!(result.total_tokens_input, 100 + 110 + 120 + 130 + 140);
    assert_eq!(result.total_tokens_output, 50 + 55 + 60 + 65 + 70);

    Ok(())
}

#[tokio::test]
async fn test_query_model_call_time_series() -> Result<()> {
    let model_provider_id = "provider-ts-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 3).await?;

    let now = Utc::now().timestamp_millis();
    let query = ModelProviderStatsQuery {
        model_provider_id: model_provider_id.to_string(),
        time_range: Some((now - 10000000, now + 10000000)),
        interval: Some(StatsInterval::Hourly),
        ..Default::default()
    };

    let points = dao.query_model_call_time_series(ctx, query).await?;

    assert!(!points.is_empty());
    let total_calls: u64 = points.iter().map(|p| p.call_count).sum();
    assert_eq!(total_calls, 3);

    Ok(())
}

#[tokio::test]
async fn test_query_model_call_aggregation_with_group_by() -> Result<()> {
    let model_provider_id = "provider-agg-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 4).await?;

    let query = ModelProviderStatsQuery {
        model_provider_id: model_provider_id.to_string(),
        group_by: vec!["agent_id".to_string()],
        aggregations: vec![
            StatAggregation::Count,
            StatAggregation::Sum("tokens_input".to_string()),
        ],
        ..Default::default()
    };

    let rows = dao.query_model_call_aggregation(ctx, query).await?;

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(
        row.groups.get("agent_id"),
        Some(&json!("agent-test"))
    );
    assert_eq!(row.aggregations.get("count"), Some(&4.0));
    assert_eq!(
        row.aggregations.get("tokens_input"),
        Some(&(100.0 + 110.0 + 120.0 + 130.0))
    );

    Ok(())
}

#[tokio::test]
async fn test_filter_by_different_provider() -> Result<()> {
    let provider_a = "provider-a";
    let provider_b = "provider-b";

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
            .with_model_provider_id(Some(provider_a.to_string()))
            .with_agent_id(Some("agent-test".to_string()))
            .with_tokens_input((100 + i * 10) as u64)
            .with_tokens_output((50 + i * 5) as u64)
            .with_total_tokens((150 + i * 15) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    for i in 0..2 {
        let event = ModelCallEvent::new(now + i as i64 * 1000)
            .with_model_provider_id(Some(provider_b.to_string()))
            .with_agent_id(Some("agent-test".to_string()))
            .with_tokens_input((200 + i * 10) as u64)
            .with_tokens_output((100 + i * 5) as u64)
            .with_total_tokens((300 + i * 15) as u64);
        stats.record(tmp_ctx.clone(), event).await?;
    }

    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let query_a = ModelProviderStatsQuery {
        model_provider_id: provider_a.to_string(),
        ..Default::default()
    };
    let result_a = dao.sum_tokens(ctx.clone(), query_a).await?;
    assert_eq!(result_a.total_calls, 3);

    let query_b = ModelProviderStatsQuery {
        model_provider_id: provider_b.to_string(),
        ..Default::default()
    };
    let result_b = dao.sum_tokens(ctx, query_b).await?;
    assert_eq!(result_b.total_calls, 2);
    assert_eq!(result_b.total_tokens_input, 200 + 210);

    Ok(())
}
