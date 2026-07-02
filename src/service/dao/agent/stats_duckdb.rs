//! AgentStatsDao DuckDB 实现

use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatsInterval};
use crate::service::dao::agent::{AgentStatsDao, AgentStatsQuery};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static AGENT_STATS_DAO: OnceLock<Arc<dyn AgentStatsDao>> = OnceLock::new();

/// 创建一个全新的 Agent Stats DAO 实例（用于测试）
pub fn stats_new() -> Arc<dyn AgentStatsDao> {
    Arc::new(AgentStatsDaoDuckDbImpl)
}

/// 获取 AgentStatsDao 单例
pub fn stats_dao() -> Arc<dyn AgentStatsDao> {
    AGENT_STATS_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn stats_init() {
    let _ = AGENT_STATS_DAO.set(stats_new());
}

// ==================== 实现 ====================

/// Agent 统计 DAO DuckDB 实现（空结构体，每次从 ctx.stats() 获取连接）
struct AgentStatsDaoDuckDbImpl;

#[async_trait::async_trait]
impl AgentStatsDao for AgentStatsDaoDuckDbImpl {
    /// 通用查询方法：根据 query 参数自动选择查询模式
    /// 
    /// 查询模式判断：
    /// - 填了 `interval` → 调用 `stats.query_time_series()`
    /// - 填了 `aggregations` 或 `group_by` → 调用 `stats.query_aggregation()`
    /// - 都没填 → 默认聚合（sum tokens + count）
    async fn query(&self, ctx: RequestContext, mut query: AgentStatsQuery) -> Result<Vec<JsonValue>> {
        // 自动注入 agent_id 过滤条件
        let agent_filter = StatFilter::Equals {
            key: "agent_id".to_string(),
            value: JsonValue::String(query.agent_id.clone()),
        };
        query.filters.insert(0, agent_filter);

        // 获取 Stats 实例
        let stats = ctx.stats();

        // 根据参数选择查询模式
        if query.interval.is_some() {
            // 时序查询
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
            
            // 转换为 JSON
            Ok(points.iter().map(|p| {
                serde_json::to_value(p).unwrap_or(JsonValue::Null)
            }).collect())
        } else if !query.aggregations.is_empty() || !query.group_by.is_empty() {
            // 聚合查询
            let rows = stats.query_aggregation(
                ctx.clone(),
                &query.filters,
                &query.group_by.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &query.aggregations,
                query.time_range,
            ).await?;
            
            // 展平为 JSON：groups 和 aggregations 合并到顶层
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
            // 默认聚合：sum tokens + count
            let default_aggregations = vec![
                StatAggregation::Sum("tokens_input".to_string()),
                StatAggregation::Sum("tokens_output".to_string()),
                StatAggregation::Count,
            ];
            
            let rows = stats.query_aggregation(
                ctx.clone(),
                &query.filters,
                &[],  // 不分组
                &default_aggregations,
                query.time_range,
            ).await?;
            
            // 展平为 JSON
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