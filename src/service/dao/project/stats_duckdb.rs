//! ProjectStatsDao DuckDB 实现

use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatsInterval};
use crate::service::dao::project::{ProjectStatsDao, ProjectStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static PROJECT_STATS_DAO: OnceLock<Arc<dyn ProjectStatsDao>> = OnceLock::new();

/// 创建一个全新的 Project Stats DAO 实例（用于测试）
pub fn stats_new() -> Arc<dyn ProjectStatsDao> {
    Arc::new(ProjectStatsDaoDuckDbImpl)
}

/// 获取 ProjectStatsDao 单例
pub fn stats_dao() -> Arc<dyn ProjectStatsDao> {
    PROJECT_STATS_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn stats_init() {
    let _ = PROJECT_STATS_DAO.set(stats_new());
}

// ==================== 实现 ====================

/// Project 统计 DAO DuckDB 实现（空结构体，每次从 ctx.stats() 获取连接）
struct ProjectStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl ProjectStatsDao for ProjectStatsDaoDuckDbImpl {
    /// 通用查询方法：根据 query 参数自动选择查询模式
    async fn query(&self, ctx: RequestContext, mut query: ProjectStatsQuery) -> Result<Vec<JsonValue>> {
        // 自动注入 project_id 过滤条件
        let project_filter = StatFilter::Equals {
            key: "project_id".to_string(),
            value: JsonValue::String(query.project_id.clone()),
        };
        query.filters.insert(0, project_filter);

        let stats = ctx.stats();

        if query.interval.is_some() {
            let interval = query.interval.unwrap_or(StatsInterval::Daily);
            let time_range = query.time_range.ok_or_else(|| {
                Error::bad_request("time_range is required for time series query")
            })?;

            let points = stats.query_time_series(
                ctx.clone(),
                &query.filters,
                interval,
                time_range,
            ).await?;

            Ok(points.iter().map(|p| {
                serde_json::to_value(p).unwrap_or(JsonValue::Null)
            }).collect())
        } else if !query.aggregations.is_empty() || !query.group_by.is_empty() {
            let rows = stats.query_aggregation(
                ctx.clone(),
                &query.filters,
                &query.group_by.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &query.aggregations,
                query.time_range,
            ).await?;

            Ok(rows.iter().map(|r| {
                let mut obj = serde_json::Map::new();
                for (k, v) in &r.groups {
                    obj.insert(k.clone(), v.clone());
                }
                for (k, v) in &r.aggregations {
                    obj.insert(k.clone(), serde_json::Value::from(*v));
                }
                JsonValue::Object(obj)
            }).collect())
        } else {
            let default_aggregations = vec![
                StatAggregation::Sum("tokens_input".to_string()),
                StatAggregation::Sum("tokens_output".to_string()),
                StatAggregation::Count,
            ];

            let rows = stats.query_aggregation(
                ctx.clone(),
                &query.filters,
                &[],
                &default_aggregations,
                query.time_range,
            ).await?;

            Ok(rows.iter().map(|r| {
                let mut obj = serde_json::Map::new();
                for (k, v) in &r.groups {
                    obj.insert(k.clone(), v.clone());
                }
                for (k, v) in &r.aggregations {
                    obj.insert(k.clone(), serde_json::Value::from(*v));
                }
                JsonValue::Object(obj)
            }).collect())
        }
    }
}
