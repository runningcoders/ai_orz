//! OntologyStatsDao DuckDB 查询侧端到端测试
//!
//! 在真实 DuckDB 上覆盖 [`OntologyStatsDao`] 四个查询方法，重点：
//! recent_raw_terms 是全项目首个 raw query() 业务调用
//! （query_aggregation 无 ORDER BY / LIMIT，走逃生门），必须有端到端回归。

use crate::pkg::RequestContext;
use crate::pkg::stats::{OntologyDriftEvent, StatAggregation, Stats};
use crate::service::dao::ontology::{self, OntologyDriftQuery};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// 构造带 Stats 的测试 ctx：DuckDB 落临时文件，initialize_default 注册
/// 全部事件表（含 ontology_drift_events）。返回 TempDir 以延长临时目录
/// 生命周期到测试结束（Stats 持有文件连接）。
async fn new_stats_ctx(user_id: &str, pool: SqlitePool) -> (RequestContext, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("stats.db");
    let stats = Stats::open(db_path.to_str().unwrap(), 100).await.unwrap();
    stats.initialize_default().unwrap();
    let ctx =
        crate::pkg::request_context_test_support::new_test_ctx_with_stats(user_id, pool, stats);
    (ctx, dir)
}

fn drift_event(timestamp: i64, agent_id: &str, kind: &str, raw_term: &str) -> OntologyDriftEvent {
    OntologyDriftEvent::new(timestamp)
        .with_agent_id(agent_id.to_string())
        .with_kind(kind.to_string())
        .with_raw_term(raw_term.to_string())
}

/// 落盘一批漂移事件（record → flush_all）
async fn seed_drift_events(ctx: &RequestContext, events: Vec<OntologyDriftEvent>) {
    for event in events {
        ctx.stats().record(ctx.clone(), event).await.unwrap();
    }
    ctx.stats().flush_all(ctx.clone()).await.unwrap();
}

/// count_drift_events：总数 + agent_id / kind 维度过滤组合
#[sqlx::test]
async fn test_count_drift_events_with_dimension_filters(pool: SqlitePool) {
    let (ctx, _dir) = new_stats_ctx("test-user", pool).await;
    seed_drift_events(
        &ctx,
        vec![
            drift_event(1_000_000, "agent_a", "relation", "CONTAINS"),
            drift_event(1_000_100, "agent_a", "relation", "DEPENDS_ON"),
            drift_event(1_000_200, "agent_b", "class", "Agent"),
        ],
    )
    .await;
    ontology::stats_init();
    let dao = ontology::stats_dao();

    // 全量
    assert_eq!(
        dao.count_drift_events(ctx.clone(), OntologyDriftQuery::default())
            .await
            .unwrap(),
        3
    );
    // agent_id 维度
    let query = OntologyDriftQuery {
        agent_id: Some("agent_a".to_string()),
        ..Default::default()
    };
    assert_eq!(dao.count_drift_events(ctx.clone(), query).await.unwrap(), 2);
    // kind 维度
    let query = OntologyDriftQuery {
        kind: Some("relation".to_string()),
        ..Default::default()
    };
    assert_eq!(dao.count_drift_events(ctx.clone(), query).await.unwrap(), 2);
    // 组合维度（agent_a + class = 空集）
    let query = OntologyDriftQuery {
        agent_id: Some("agent_a".to_string()),
        kind: Some("class".to_string()),
        ..Default::default()
    };
    assert_eq!(dao.count_drift_events(ctx.clone(), query).await.unwrap(), 0);
}

/// count_by_kind / count_by_agent：group_by 维度值正确展开（DuckDB
/// GROUP BY 无序，收集后 sort 再断言）
#[sqlx::test]
async fn test_count_by_kind_and_agent(pool: SqlitePool) {
    let (ctx, _dir) = new_stats_ctx("test-user", pool).await;
    seed_drift_events(
        &ctx,
        vec![
            drift_event(1_000_000, "agent_a", "relation", "CONTAINS"),
            drift_event(1_000_100, "agent_a", "relation", "DEPENDS_ON"),
            drift_event(1_000_200, "agent_a", "class", "Agent"),
            drift_event(1_000_300, "agent_b", "relation", "OWNS"),
        ],
    )
    .await;
    ontology::stats_init();
    let dao = ontology::stats_dao();

    let mut by_kind = dao
        .count_by_kind(ctx.clone(), OntologyDriftQuery::default())
        .await
        .unwrap();
    by_kind.sort();
    assert_eq!(
        by_kind,
        vec![("class".to_string(), 1), ("relation".to_string(), 3)]
    );

    let mut by_agent = dao
        .count_by_agent(ctx.clone(), OntologyDriftQuery::default())
        .await
        .unwrap();
    by_agent.sort();
    assert_eq!(
        by_agent,
        vec![("agent_a".to_string(), 3), ("agent_b".to_string(), 1)]
    );
}

/// recent_raw_terms：raw query() 逃生门的 ORDER BY / LIMIT / agent_id 过滤
#[sqlx::test]
async fn test_recent_raw_terms_order_limit_and_filter(pool: SqlitePool) {
    let (ctx, _dir) = new_stats_ctx("test-user", pool).await;
    seed_drift_events(
        &ctx,
        vec![
            drift_event(1_000_000, "agent_a", "relation", "OLDEST"),
            drift_event(1_000_100, "agent_b", "class", "Widget"),
            drift_event(1_000_200, "agent_a", "class", "Agent"),
            drift_event(1_000_300, "agent_a", "relation", "NEWEST"),
        ],
    )
    .await;
    ontology::stats_init();
    let dao = ontology::stats_dao();

    // LIMIT 截断 + timestamp 倒序
    let terms = dao.recent_raw_terms(ctx.clone(), 2, None).await.unwrap();
    assert_eq!(
        terms
            .iter()
            .map(|t| t.raw_term.as_str())
            .collect::<Vec<_>>(),
        vec!["NEWEST", "Agent"]
    );

    // agent_id 过滤分支（带 WHERE 的 SQL 路径）
    let terms = dao
        .recent_raw_terms(ctx.clone(), 10, Some("agent_b".to_string()))
        .await
        .unwrap();
    assert_eq!(terms.len(), 1);
    assert_eq!(terms[0].raw_term, "Widget");
    assert_eq!(terms[0].agent_id, "agent_b");
    assert_eq!(terms[0].kind, "class");

    // limit = 0 → 空
    let terms = dao.recent_raw_terms(ctx.clone(), 0, None).await.unwrap();
    assert!(terms.is_empty());
}

/// query_drift_events：time_range 过滤 + group_by 维度值展开
/// （groups + aggregations 一并返回，count_by_kind / count_by_agent 的底座）
#[sqlx::test]
async fn test_query_drift_events_time_range_and_groups(pool: SqlitePool) {
    let (ctx, _dir) = new_stats_ctx("test-user", pool).await;
    seed_drift_events(
        &ctx,
        vec![
            drift_event(1_000_000, "agent_a", "relation", "CONTAINS"),
            drift_event(1_000_100, "agent_a", "class", "Agent"),
            drift_event(1_000_200, "agent_b", "relation", "OWNS"),
        ],
    )
    .await;
    ontology::stats_init();
    let dao = ontology::stats_dao();

    // time_range 闭区间命中前两条
    let query = OntologyDriftQuery {
        time_range: Some((1_000_000, 1_000_100)),
        ..Default::default()
    };
    assert_eq!(dao.count_drift_events(ctx.clone(), query).await.unwrap(), 2);

    // group_by kind：groups（维度值）+ aggregations（count）一并展开
    let query = OntologyDriftQuery {
        group_by: vec!["kind".to_string()],
        aggregations: vec![StatAggregation::Count],
        ..Default::default()
    };
    let rows = dao.query_drift_events(ctx.clone(), query).await.unwrap();
    assert_eq!(rows.len(), 2);
    let relation_row = rows
        .iter()
        .find(|r| r.get("kind").and_then(|v| v.as_str()) == Some("relation"))
        .unwrap();
    assert_eq!(
        relation_row.get("count").and_then(|v| v.as_f64()),
        Some(2.0)
    );
}
