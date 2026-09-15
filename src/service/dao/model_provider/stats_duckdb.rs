//! ModelProviderStatsDao DuckDB 实现

use crate::pkg::RequestContext;
use crate::pkg::stats::{ModelCallEvent, StatAggregation, StatFilter, StatsInterval};
use crate::service::dao::model_provider::{ModelProviderStatsDao, ModelProviderStatsQuery};
use common::error::Result;
use common::models::clamp_interval_to_span;
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

static MODEL_PROVIDER_STATS_DAO: OnceLock<
    Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
> = OnceLock::new();

pub fn stats_new() -> Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>> {
    Arc::new(ModelProviderStatsDaoDuckDbImpl)
}

pub fn stats_dao() -> Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>> {
    MODEL_PROVIDER_STATS_DAO.get().cloned().unwrap()
}

pub fn stats_init() {
    let _ = MODEL_PROVIDER_STATS_DAO.set(stats_new());
}

struct ModelProviderStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl ModelProviderStatsDao for ModelProviderStatsDaoDuckDbImpl {
    type ModelCallEvent = ModelCallEvent;

    async fn query_model_calls(
        &self,
        ctx: RequestContext,
        mut query: ModelProviderStatsQuery,
    ) -> Result<Vec<JsonValue>> {
        if let Some(ref provider_id) = query.model_provider_id {
            let provider_filter = StatFilter::Equals {
                key: "model_provider_id".to_string(),
                value: JsonValue::String(provider_id.clone()),
            };
            query.filters.insert(0, provider_filter);
        }

        if let Some(ref agent_id) = query.agent_id {
            let agent_filter = StatFilter::Equals {
                key: "agent_id".to_string(),
                value: JsonValue::String(agent_id.clone()),
            };
            query.filters.insert(0, agent_filter);
        }

        if let Some(ref project_id) = query.project_id {
            let project_filter = StatFilter::Equals {
                key: "project_id".to_string(),
                value: JsonValue::String(project_id.clone()),
            };
            query.filters.insert(0, project_filter);
        }

        if let Some(ref task_id) = query.task_id {
            let task_filter = StatFilter::Equals {
                key: "task_id".to_string(),
                value: JsonValue::String(task_id.clone()),
            };
            query.filters.insert(0, task_filter);
        }

        if let Some(ref user_id) = query.user_id {
            let user_filter = StatFilter::Equals {
                key: "user_id".to_string(),
                value: JsonValue::String(user_id.clone()),
            };
            query.filters.insert(0, user_filter);
        }

        let stats = ctx.stats();
        let table_name = self.model_call_table_name(stats);

        self.do_query(ctx, query, table_name).await
    }
}

impl ModelProviderStatsDaoDuckDbImpl {
    async fn do_query(
        &self,
        ctx: RequestContext,
        query: ModelProviderStatsQuery,
        table_name: Option<String>,
    ) -> Result<Vec<JsonValue>> {
        if query.interval.is_some() {
            let requested_interval = query.interval.unwrap_or(StatsInterval::Daily);
            // time_range=None 兜底为「最近 7 天」（与 get-agent handler 的默认窗口一致）：
            // 绝大多数看板只关心近期数据，全历史扫描既慢也无必要；需要更大范围的
            // 调用方由前端显式指定时间区间。此前此处直接 bad_request，导致用户页统计整体失败。
            let now = chrono::Utc::now().timestamp_millis();
            let time_range = query.time_range.unwrap_or((now - 7 * 86_400_000, now));

            // 策略护栏：粒度与窗口跨度由请求侧自由组合，若不加约束，`minutely` 配 30 天窗口
            // 会产出 43200 行。收敛点刻意放在**这里**而不是 handler 白名单：只有此处同时
            // 拿得到「最终生效的粒度」与「兜底后的窗口」——尤其是 time_range=None 被兜底成
            // 7 天的情况，handler 侧根本看不到窗口，无从判断代价。
            // 本分支是全项目时序查询的唯一入口（见 ModelProviderStatsDao 的调用方），
            // 因此这一道护栏无法被绕过。
            let span_ms = time_range.1 - time_range.0;
            let interval = clamp_interval_to_span(requested_interval, span_ms);
            if interval != requested_interval {
                log_debug!(
                    &ctx,
                    "query_model_calls",
                    requested = ?requested_interval,
                    effective = ?interval,
                    span_ms,
                    "时序粒度超出桶数上限，已自动收敛"
                );
            }

            let points = ctx
                .stats()
                .query_time_series(
                    ctx.clone(),
                    table_name.as_deref(),
                    &query.filters,
                    interval,
                    time_range,
                )
                .await?;

            Ok(points
                .iter()
                .map(|p| serde_json::to_value(p).unwrap_or(JsonValue::Null))
                .collect())
        } else if !query.aggregations.is_empty() || !query.group_by.is_empty() {
            let rows = ctx
                .stats()
                .query_aggregation(
                    ctx.clone(),
                    table_name.as_deref(),
                    &query.filters,
                    &query
                        .group_by
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                    &query.aggregations,
                    query.time_range,
                )
                .await?;

            Ok(rows
                .iter()
                .map(|r| {
                    let mut obj = serde_json::Map::new();
                    for (k, v) in &r.groups {
                        obj.insert(k.clone(), v.clone());
                    }
                    for (k, v) in &r.aggregations {
                        obj.insert(k.clone(), serde_json::Value::from(*v));
                    }
                    JsonValue::Object(obj)
                })
                .collect())
        } else {
            let default_aggregations = vec![
                StatAggregation::Sum("tokens_input".to_string()),
                StatAggregation::Sum("tokens_output".to_string()),
                StatAggregation::Count,
            ];

            let rows = ctx
                .stats()
                .query_aggregation(
                    ctx.clone(),
                    table_name.as_deref(),
                    &query.filters,
                    &[],
                    &default_aggregations,
                    query.time_range,
                )
                .await?;

            Ok(rows
                .iter()
                .map(|r| {
                    let mut obj = serde_json::Map::new();
                    for (k, v) in &r.groups {
                        obj.insert(k.clone(), v.clone());
                    }
                    for (k, v) in &r.aggregations {
                        obj.insert(k.clone(), serde_json::Value::from(*v));
                    }
                    JsonValue::Object(obj)
                })
                .collect())
        }
    }
}
