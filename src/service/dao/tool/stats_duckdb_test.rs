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
        tool_id: Some(tool_id.to_string()),
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
        tool_id: Some(tool_id.to_string()),
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
        tool_id: Some(tool_id.to_string()),
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
        tool_id: Some(tool_id.to_string()),
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
        tool_id: Some(tool_id.to_string()),
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
    assert!(stats.avg_duration_ms.is_none());

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
        tool_id: Some(tool_id.to_string()),
        agent_id: Some(agent_a.to_string()),
        ..Default::default()
    };
    let result_a = dao.sum_calls(ctx.clone(), query_a).await?;
    assert_eq!(result_a, 5);

    let query_b = ToolStatsQuery {
        tool_id: Some(tool_id.to_string()),
        agent_id: Some(agent_b.to_string()),
        ..Default::default()
    };
    let result_b = dao.sum_calls(ctx, query_b).await?;
    assert_eq!(result_b, 3);

    Ok(())
}

#[tokio::test]
async fn test_avg_duration_ms() -> Result<()> {
    let tool_id = "tool-avg-duration";
    let agent_id = "agent-1";
    // 5 success: 100/110/120/130/140；2 failed: 50/60
    let (ctx, dao) = setup_test_env(tool_id, agent_id, 5, 2).await?;

    let query = ToolStatsQuery {
        tool_id: Some(tool_id.to_string()),
        ..Default::default()
    };

    let avg = dao.avg_duration_ms(ctx.clone(), query.clone()).await?;
    let expected = (100.0 + 110.0 + 120.0 + 130.0 + 140.0 + 50.0 + 60.0) / 7.0;
    assert!(
        (avg - expected).abs() < 1e-6,
        "avg_duration_ms = {avg}, expected {expected}"
    );

    // get_stats 应把平均值一并填充（与 failed_count 同一分支）
    let options = StatsFetchOptions {
        with_call_summary: true,
        ..Default::default()
    };
    let stats = dao.get_stats(ctx, query, options).await?;
    assert_eq!(stats.avg_duration_ms, Some(expected));

    Ok(())
}

/// `tool_id = None` 时不按工具收窄（组织级汇总读数口径）
#[tokio::test]
async fn test_org_level_aggregation_without_tool_id() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.register_table(ToolCallStatTable)?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();
    // tool-a: 4 次（1 次失败，耗时 100）
    for i in 0..4 {
        let status = if i == 0 { "failed" } else { "success" };
        let event = ToolCallEvent::new(now + i as i64 * 1000)
            .with_tool_id("tool-a".to_string())
            .with_tool_name("tool_a".to_string())
            .with_status(status.to_string())
            .with_duration_ms(100);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    // tool-b: 3 次（全成功，耗时 200）
    for i in 0..3 {
        let event = ToolCallEvent::new(now + (10 + i) as i64 * 1000)
            .with_tool_id("tool-b".to_string())
            .with_tool_name("tool_b".to_string())
            .with_status("success".to_string())
            .with_duration_ms(200);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let all = ToolStatsQuery::default();
    assert_eq!(dao.sum_calls(ctx.clone(), all.clone()).await?, 7);
    assert_eq!(dao.sum_failed_calls(ctx.clone(), all.clone()).await?, 1);
    // (4 * 100 + 3 * 200) / 7 = 1000 / 7
    let avg = dao.avg_duration_ms(ctx, all).await?;
    assert!((avg - 1000.0 / 7.0).abs() < 1e-6, "avg = {avg}");

    Ok(())
}

/// 窗口内无调用时 `avg_duration_ms` 留 None（不能给 0ms，否则会被读成「瞬时返回」）
#[tokio::test]
async fn test_get_stats_avg_duration_none_when_no_calls() -> Result<()> {
    let (ctx, dao) = setup_test_env("tool-no-calls", "agent-1", 0, 0).await?;

    let options = StatsFetchOptions {
        with_call_summary: true,
        ..Default::default()
    };
    let stats = dao
        .get_stats(ctx, ToolStatsQuery::default(), options)
        .await?;

    assert_eq!(stats.call_summary.map(|c| c.total_calls), Some(0));
    assert_eq!(stats.failed_count, Some(0));
    assert!(stats.avg_duration_ms.is_none());

    Ok(())
}
