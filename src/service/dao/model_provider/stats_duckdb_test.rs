//! ModelProvider Stats DAO DuckDB 单元测试

use crate::pkg::request_context_test_support;
use crate::pkg::stats::*;
use crate::service::dao::model_provider::stats_duckdb::stats_new;
use crate::service::dao::model_provider::{ModelProviderStatsDao, ModelProviderStatsQuery};
use chrono::Utc;
use common::error::Result;
use common::models::{StatsFetchOptions, TimeSeriesPoint};
use serde_json::json;
use sqlx::SqlitePool;
use tempfile::tempdir;

async fn setup_test_env(
    model_provider_id: &str,
    event_count: usize,
) -> Result<(
    crate::pkg::RequestContext,
    std::sync::Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let db_path_str = db_path.to_str().unwrap();

    let stats = Stats::open(db_path_str, 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();
    for i in 0..event_count {
        // 全部落在过去（最近 1~N 秒前）：时序查询的默认窗口是「最近 7 天」，
        // 未来时间戳的事件不在窗口内，会产生误导性的断言结果。
        let event = ModelCallEvent::new(now - (event_count as i64 - i as i64) * 1000)
            .with_model_provider_id(Some(model_provider_id.to_string()))
            .with_agent_id(Some("agent-test".to_string()))
            .with_tokens_input((100 + i * 10) as u64)
            .with_tokens_output((50 + i * 5) as u64)
            .with_total_tokens((150 + i * 15) as u64)
            // token 明细列（缓存命中 / 思考）：随主 token 一并落盘，供 sum_tokens 断言
            .with_cached_tokens((10 + i * 2) as u64)
            .with_reasoning_tokens((5 + i) as u64);
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
        model_provider_id: Some(model_provider_id.to_string()),
        ..Default::default()
    };

    let result = dao.sum_tokens(ctx, query).await?;

    assert_eq!(result.total_calls, 5);
    assert_eq!(result.total_tokens_input, 100 + 110 + 120 + 130 + 140);
    assert_eq!(result.total_tokens_output, 50 + 55 + 60 + 65 + 70);
    // 明细列：10/12/14/16/18 与 5/6/7/8/9；断言非 0 才能证明列真的被读出来
    // （列缺失时 SUM 走 COALESCE=0，与「未上报」同形，0 断言形同虚设）
    assert_eq!(result.total_cached_tokens, 10 + 12 + 14 + 16 + 18);
    assert_eq!(result.total_reasoning_tokens, 5 + 6 + 7 + 8 + 9);

    Ok(())
}

/// 默认统计口径只包含对话调用；向量化调用仅在显式指定 `call_kind=embedding`
/// 时可查（向量模型详情页使用）。
#[tokio::test]
async fn test_call_kind_excludes_embedding_by_default() -> Result<()> {
    const PROVIDER_ID: &str = "provider-call-kind-test";

    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let stats = Stats::open(db_path.to_str().unwrap(), 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());
    let now = Utc::now().timestamp_millis();

    // 2 条对话调用：每次 100 输入 token
    for i in 0..2 {
        let event = ModelCallEvent::new(now - (2 - i) * 1000)
            .with_model_provider_id(Some(PROVIDER_ID.to_string()))
            .with_tokens_input(100)
            .with_total_tokens(100);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    // 3 条向量化调用：每次 10 输入 token
    for i in 0..3 {
        let event = ModelCallEvent::new(now - (3 - i) * 1000)
            .with_call_kind(Some("embedding".to_string()))
            .with_model_provider_id(Some(PROVIDER_ID.to_string()))
            .with_tokens_input(10)
            .with_total_tokens(10);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    let dao = stats_new();

    let chat = dao
        .sum_tokens(
            ctx.clone(),
            ModelProviderStatsQuery {
                model_provider_id: Some(PROVIDER_ID.to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(chat.total_calls, 2, "默认口径不应计入向量化调用");
    assert_eq!(chat.total_tokens_input, 200);

    let embedding = dao
        .sum_tokens(
            ctx.clone(),
            ModelProviderStatsQuery {
                model_provider_id: Some(PROVIDER_ID.to_string()),
                call_kind: Some("embedding".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(embedding.total_calls, 3, "sum_tokens 按事件条数计数");
    assert_eq!(embedding.total_tokens_input, 30);
    assert_eq!(embedding.total_tokens_output, 0);

    // 一次 HTTP 请求一条事件：multimodal 逐条请求时调用次数与事件条数一致
    let points = dao
        .query_model_call_time_series(
            ctx,
            ModelProviderStatsQuery {
                model_provider_id: Some(PROVIDER_ID.to_string()),
                call_kind: Some("embedding".to_string()),
                time_range: Some((now - 10_000_000, now + 10_000_000)),
                interval: Some(StatsInterval::Minutely),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(points.iter().map(|p| p.call_count).sum::<u64>(), 3);

    Ok(())
}

#[tokio::test]
async fn test_query_model_call_time_series() -> Result<()> {
    let model_provider_id = "provider-ts-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 3).await?;

    let now = Utc::now().timestamp_millis();
    let query = ModelProviderStatsQuery {
        model_provider_id: Some(model_provider_id.to_string()),
        time_range: Some((now - 10000000, now + 10000000)),
        interval: Some(StatsInterval::Hourly),
        ..Default::default()
    };

    let points = dao.query_model_call_time_series(ctx, query).await?;

    assert!(!points.is_empty());
    let total_calls: u64 = points.iter().map(|p| p.call_count).sum();
    assert_eq!(total_calls, 3);
    // 缓存/思考明细随时序一并聚合（3 条事件：cached = 10+12+14，reasoning = 5+6+7）
    let total_cached: u64 = points.iter().map(|p| p.tokens_cached).sum();
    let total_reasoning: u64 = points.iter().map(|p| p.tokens_reasoning).sum();
    assert_eq!(total_cached, 36);
    assert_eq!(total_reasoning, 18);

    Ok(())
}

/// 分钟级时序：验证 `StatsInterval::Minutely` 的 truncate 表达式在 DuckDB 下
/// 确实对齐到 60000ms 边界。
///
/// ⚠️ 断言必须包含 `interval_start > 0`：曾经 truncate 用 `timestamp / 60000`
/// （DuckDB 浮点除 → DOUBLE），parse_time_series_point 的 `as_i64()` 解析失败
/// 落到 unwrap_or(0)，而 `0 % 60000 == 0` 让对齐断言恒真——假绿灯。
#[tokio::test]
async fn test_query_model_call_time_series_minutely() -> Result<()> {
    let model_provider_id = "provider-ts-minute-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 3).await?;

    let now = Utc::now().timestamp_millis();
    let query = ModelProviderStatsQuery {
        model_provider_id: Some(model_provider_id.to_string()),
        time_range: Some((now - 10_000_000, now + 10_000_000)),
        interval: Some(StatsInterval::Minutely),
        ..Default::default()
    };

    let points = dao.query_model_call_time_series(ctx, query).await?;

    assert!(!points.is_empty());
    for p in &points {
        assert!(
            p.interval_start > 0,
            "interval_start 落 0（浮点除法回归标志）: {}",
            p.interval_start
        );
        assert_eq!(
            p.interval_start % 60_000,
            0,
            "interval_start 未按分钟对齐: {}",
            p.interval_start
        );
    }
    let total_calls: u64 = points.iter().map(|p| p.call_count).sum();
    assert_eq!(total_calls, 3);

    Ok(())
}

/// 时序查询不传 time_range 时兜底为「最近 7 天」（与 get-agent 默认窗口一致）；
/// 此前此处 bad_request 导致用户页统计整体失败。
#[tokio::test]
async fn test_query_model_call_time_series_without_time_range() -> Result<()> {
    let model_provider_id = "provider-ts-no-range-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 2).await?;

    let query = ModelProviderStatsQuery {
        model_provider_id: Some(model_provider_id.to_string()),
        time_range: None,
        interval: Some(StatsInterval::Daily),
        ..Default::default()
    };

    let points = dao.query_model_call_time_series(ctx, query).await?;

    assert!(!points.is_empty(), "默认 7 天窗口的时序查询不应返回空");
    let total_calls: u64 = points.iter().map(|p| p.call_count).sum();
    assert_eq!(total_calls, 2);

    Ok(())
}

/// 策略护栏接线验证：宽窗口 + 细粒度必须在 DAO 内被收敛到粗档。
///
/// 断言用「桶起始是否按天对齐」判断**实际生效的粒度**，而不是数桶个数 ——
/// 测试数据都挤在几秒内，`minutely` 与 `daily` 都只会产出一个桶，数个数区分不出来。
/// 反向（未被收敛时保持分钟对齐）由 `common::models::stats_test` 的纯函数用例覆盖，
/// 且 `interval_start % 60000 == 0` 对天桶同样成立，在这里做反向断言没有鉴别力。
#[tokio::test]
async fn test_time_series_interval_is_clamped_for_wide_window() -> Result<()> {
    let model_provider_id = "provider-ts-clamp-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 3).await?;

    let now = Utc::now().timestamp_millis();
    let query = ModelProviderStatsQuery {
        model_provider_id: Some(model_provider_id.to_string()),
        // 30 天窗口配分钟桶 = 43200 桶，远超 STATS_MAX_BUCKETS(512) → 直落天桶
        // （小时桶 720 桶仍超限）
        time_range: Some((now - 30 * 86_400_000, now)),
        interval: Some(StatsInterval::Minutely),
        ..Default::default()
    };

    let points = dao.query_model_call_time_series(ctx, query).await?;

    assert!(!points.is_empty());
    for p in &points {
        assert_eq!(
            p.interval_start % 86_400_000,
            0,
            "宽窗口的 minutely 未被收敛到 daily（桶起始未按天对齐）: {}",
            p.interval_start
        );
    }
    let total_calls: u64 = points.iter().map(|p| p.call_count).sum();
    assert_eq!(total_calls, 3, "收敛粒度不应丢数据");

    Ok(())
}

#[tokio::test]
async fn test_query_model_call_aggregation_with_group_by() -> Result<()> {
    let model_provider_id = "provider-agg-test";
    let (ctx, dao) = setup_test_env(model_provider_id, 4).await?;

    let query = ModelProviderStatsQuery {
        model_provider_id: Some(model_provider_id.to_string()),
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
    assert_eq!(row.groups.get("agent_id"), Some(&json!("agent-test")));
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
        model_provider_id: Some(provider_a.to_string()),
        ..Default::default()
    };
    let result_a = dao.sum_tokens(ctx.clone(), query_a).await?;
    assert_eq!(result_a.total_calls, 3);

    let query_b = ModelProviderStatsQuery {
        model_provider_id: Some(provider_b.to_string()),
        ..Default::default()
    };
    let result_b = dao.sum_tokens(ctx, query_b).await?;
    assert_eq!(result_b.total_calls, 2);
    assert_eq!(result_b.total_tokens_input, 200 + 210);

    Ok(())
}

// ==================== 平均耗时（avg_duration_ms）口径 ====================
//
// 这组断言的靶心是「0 是数据、NULL 是没有数据」这一条不可混淆的口径：
// - 无耗时样本 ⇒ `None` + `duration_sample_count == 0`（**不是** `Some(0.0)`）；
// - 真实 0ms 调用 ⇒ 参与平均（它是一次「瞬时返回」，仍是有信息的观测）；
// - 全窗口汇总必须按**样本数加权**，不能对桶均值再求简单平均。

/// 带 `call_kind` / `duration_ms` 的事件夹具
///
/// 与 `setup_test_env` 同构，区别是这两列由调用方逐条指定——它们正是本次扩展的取数来源，
/// 用默认值铺出来的数据无法证明任何事。
async fn setup_duration_env(
    model_provider_id: &str,
    specs: &[(Option<u64>, &str)],
) -> Result<(
    crate::pkg::RequestContext,
    std::sync::Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("stats.db");
    let stats = Stats::open(db_path.to_str().unwrap(), 100).await?;
    stats.initialize_default()?;

    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let tmp_ctx = request_context_test_support::new_test_ctx("tmp-user", pool.clone());

    let now = Utc::now().timestamp_millis();
    for (i, (duration, call_kind)) in specs.iter().enumerate() {
        let event = ModelCallEvent::new(now - (specs.len() as i64 - i as i64) * 1000)
            .with_model_provider_id(Some(model_provider_id.to_string()))
            .with_call_kind(Some((*call_kind).to_string()))
            .with_duration_ms(*duration);
        stats.record(tmp_ctx.clone(), event).await?;
    }
    stats.flush_all(tmp_ctx).await?;

    let ctx = request_context_test_support::new_test_ctx_with_stats("test-user", pool, stats);
    Ok((ctx, stats_new()))
}

fn duration_stats_options() -> StatsFetchOptions {
    let now = Utc::now().timestamp_millis();
    StatsFetchOptions {
        with_call_summary: true,
        with_token_summary: false,
        with_time_series: true,
        time_range: Some((now - 7 * 86_400_000, now + 1_000)),
        interval: Some(StatsInterval::Daily),
    }
}

/// 反事实之一：窗口内**没有任何**带耗时的调用 ⇒ 汇总为 `None`，不是 `Some(0.0)`
///
/// 同时断言时序序列**确实被查出来了**（非空、且每个桶都是 None）：否则「None」可能只是
/// 「压根没查时序」的同形结果，就这样断言等于自欺。
#[tokio::test]
async fn test_avg_duration_none_when_no_duration_samples() -> Result<()> {
    const PROVIDER_ID: &str = "provider-avg-none";
    // 三条都用 chat：DAO 的缺省口径是 `call_kind="chat"`（decision 走自己的桶），
    // 混入 decision 会让「调用次数零回归」这条断言算错，从而掩盖真正要验的东西。
    let (ctx, dao) = setup_duration_env(
        PROVIDER_ID,
        &[(None, "chat"), (None, "chat"), (None, "chat")],
    )
    .await?;

    let query = ModelProviderStatsQuery {
        model_provider_id: Some(PROVIDER_ID.to_string()),
        ..Default::default()
    };

    let stats = dao.get_stats(ctx, query, duration_stats_options()).await?;

    assert_eq!(
        stats.avg_duration_ms, None,
        "没有耗时样本时必须是 None；写成 Some(0.0) 会被展示成「调用瞬时返回」"
    );
    assert_eq!(stats.duration_sample_count, 0);

    let series = stats
        .model_call_time_series
        .expect("时序必须被查出（否则 None 无从归因）");
    assert!(!series.is_empty(), "三次调用应至少产生一个桶");
    assert!(series.iter().all(|p| p.avg_duration_ms.is_none()));
    assert!(series.iter().all(|p| p.duration_sample_count == 0));
    // 调用次数口径不受影响（既有列零回归的最小证据）
    assert_eq!(series.iter().map(|p| p.call_count).sum::<u64>(), 3);

    Ok(())
}

/// 有耗时样本 ⇒ 平均值正确，且**真实 0ms 也参与平均**
///
/// 0ms 是有效观测（一次瞬时返回），如果把它当「没数据」跳过，均值会虚高——
/// 这条断言把「0 参与平均」与「NULL 不参与平均」在同一张表里钉死。
#[tokio::test]
async fn test_avg_duration_averages_real_zero_ms_and_skips_null() -> Result<()> {
    const PROVIDER_ID: &str = "provider-avg-basic";
    // 100 / 200 / 300 各一次 + 一次真实 0ms + 两次 NULL
    let (ctx, dao) = setup_duration_env(
        PROVIDER_ID,
        &[
            (Some(100), "chat"),
            (Some(200), "chat"),
            (Some(300), "chat"),
            (Some(0), "chat"),
            (None, "chat"),
            (None, "chat"),
        ],
    )
    .await?;

    let query = ModelProviderStatsQuery {
        model_provider_id: Some(PROVIDER_ID.to_string()),
        ..Default::default()
    };

    let stats = dao.get_stats(ctx, query, duration_stats_options()).await?;

    // (100 + 200 + 300 + 0) / 4 = 150；NULL 两行不参与，也不拉低
    assert_eq!(stats.avg_duration_ms, Some(150.0));
    assert_eq!(
        stats.duration_sample_count, 4,
        "样本数只数非空 duration_ms（4 条），不是调用次数（6 条）"
    );

    Ok(())
}

/// 不同 `call_kind` 的耗时互不混计（chat 与 decision 各自独立）
#[tokio::test]
async fn test_avg_duration_isolated_between_call_kinds() -> Result<()> {
    const PROVIDER_ID: &str = "provider-avg-kind";
    let (ctx, dao) = setup_duration_env(
        PROVIDER_ID,
        &[
            (Some(50), "chat"),
            (Some(50), "chat"),
            (Some(300), "decision"),
            (Some(300), "decision"),
        ],
    )
    .await?;

    // ⚠️ `call_kind` 是**独立字段**而非普通 filter：DAO 在 None 时缺省填 "chat"，
    // 若改走 `filters` 会与缺省值 AND 成 `call_kind='chat' AND call_kind='decision'`（恒空）。
    let kind_query = |kind: Option<&str>| ModelProviderStatsQuery {
        model_provider_id: Some(PROVIDER_ID.to_string()),
        call_kind: kind.map(|k| k.to_string()),
        ..Default::default()
    };

    let chat = dao
        .get_stats(
            ctx.clone(),
            kind_query(Some("chat")),
            duration_stats_options(),
        )
        .await?;
    let decision = dao
        .get_stats(
            ctx.clone(),
            kind_query(Some("decision")),
            duration_stats_options(),
        )
        .await?;

    assert_eq!(chat.avg_duration_ms, Some(50.0));
    assert_eq!(chat.duration_sample_count, 2);
    assert_eq!(decision.avg_duration_ms, Some(300.0));
    assert_eq!(decision.duration_sample_count, 2);

    // 缺省（call_kind=None）落回 "chat" 桶：decision 的 300ms **不得**混进来。
    // 这是「不混计」最直接的证据——若缺省口径退化成「全量」，此处会变成 175。
    // 生产侧同源：`dal::model_provider::call_kind_for_capability` 把
    // Decision capability 的 provider 固定映射到 "decision" 桶。
    let default_bucket = dao
        .get_stats(ctx, kind_query(None), duration_stats_options())
        .await?;
    assert_eq!(default_bucket.avg_duration_ms, Some(50.0));
    assert_eq!(default_bucket.duration_sample_count, 2);

    Ok(())
}

/// 纯函数口径：全窗口平均必须按**样本数加权**，不能对桶均值求简单平均
///
/// 两个桶：1000ms×100 次、1ms×1 次。
/// - 加权（正确）：100001/101 ≈ 990.11
/// - 简单平均（错误）：500.5 —— 差距巨大，故这条断言能真正抓住口径退化。
#[test]
fn test_weighted_avg_duration_is_sample_weighted_not_plain_mean() {
    let point = |avg: Option<f64>, n: u64| TimeSeriesPoint {
        interval_start: 0,
        tokens_input: 0,
        tokens_output: 0,
        tokens_cached: 0,
        tokens_reasoning: 0,
        call_count: n,
        avg_duration_ms: avg,
        duration_sample_count: n,
    };

    let points = vec![point(Some(1000.0), 100), point(Some(1.0), 1)];
    let (avg, samples) = super::weighted_avg_duration_ms(&points);
    assert_eq!(samples, 101);
    let got = avg.expect("有样本时必须给出 Some");
    assert!(
        (got - 100_001.0 / 101.0).abs() < 1e-9,
        "必须按样本数加权，实际 {got}"
    );
    assert!(
        (got - 500.5).abs() > 1.0,
        "Σ(avg×n)/Σn 若退化成桶均值的简单平均，此处会得到 500.5"
    );

    // 全空桶 ⇒ (None, 0)，不是 (Some(0.0), 0)
    let empty = vec![point(None, 0), point(None, 0)];
    assert_eq!(super::weighted_avg_duration_ms(&empty), (None, 0));
}
