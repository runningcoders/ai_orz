//! Agent DAO 模块

use crate::models::agent::AgentPo;
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatAggregation, StatEvent, StatFilter, Stats};
use common::enums::AgentStatus;
use common::error::Result;
use common::models::{AgentStats, CallSummary, StatsFetchOptions};
use serde_json::Value as JsonValue;

/// Agent 查询参数
#[derive(Debug, Clone, Default)]
pub struct AgentQuery {
    /// 按 ID 批量查询（向量搜索的核心过滤）
    pub ids: Option<Vec<String>>,
    /// 关键词搜索（仅用于 search 方法，query 方法中已废弃）
    pub keyword: Option<String>,
    pub status: Option<AgentStatus>,
    pub exclude_status: Option<AgentStatus>,
    pub created_by: Option<String>,
    pub model_provider_id: Option<String>,
    /// 按角色标签过滤（OR 语义，匹配任一 role 即命中）
    ///
    /// `role` 字段是 JSON 字符串数组（如 `["feishu_reception","worker"]`），
    /// DAO 层使用 `json_each` 在 SQL 层精确匹配。
    pub roles: Option<Vec<String>>,
    pub pagination: common::api::PaginationParams,
}

/// ✅ Agent 搜索统一入参（关键词搜索 + 向量语义搜索共用）
#[derive(Debug, Clone, Default)]
pub struct AgentSearch {
    /// 关键词搜索查询（用于 FTS5 全文匹配）
    pub keyword: Option<String>,
    /// 查询向量（用于向量语义搜索，DAL 层填充）
    pub query_vector: Option<Vec<f32>>,
    /// 返回 Top K 结果（向量搜索专用）
    pub top_k: Option<i32>,
    /// 向量距离阈值，超过此值的结果被过滤。None 表示使用默认值 0.8
    pub vector_distance_threshold: Option<f32>,
    /// ✅ 业务过滤条件（直接复用 AgentQuery）
    pub filters: AgentQuery,
}

/// Agent 统计查询参数
#[derive(Debug, Clone, Default)]
pub struct AgentStatsQuery {
    /// Agent ID（必填）
    pub agent_id: String,
    /// 任务 ID（可选过滤）
    pub task_id: Option<String>,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 聚合函数（内部使用）
    pub aggregations: Vec<StatAggregation>,
}

/// Agent DAO 接口
#[async_trait::async_trait]
pub trait AgentDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<()>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AgentPo>>;
    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<AgentPo>>;
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<AgentPo>>;
    async fn update(&self, ctx: RequestContext, agent: &AgentPo) -> Result<()>;
    async fn delete(&self, ctx: RequestContext, agent: &AgentPo) -> Result<()>;

    /// 统计符合条件的 Agent 数量（复用 query 的 filter 逻辑，只跑 COUNT 不跑 LIST）
    async fn count(&self, ctx: RequestContext, query: AgentQuery) -> Result<u64>;

    /// 统一搜索入口（FTS5 关键词 + 业务过滤，向量搜索由 AgentVectorDao 单独处理）
    ///
    /// 返回 `(AgentPo, fts_rank)` 元组，`fts_rank` 为 FTS5 BM25 相关性评分
    /// （越小越相关，仅关键词命中时有值，无关键词搜索时为 None）。
    async fn search_agents(
        &self,
        ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<Vec<(AgentPo, Option<f32>)>>;
}

// ==================== AgentVectorDao Trait ====================

/// Agent Vector DAO trait - 仅负责 Agent 向量索引的 CRUD，与基础 Agent 数据解耦
/// 所有方法返回完整的行级结构体，与底层 VectorStore trait 保持一致
#[async_trait::async_trait]
pub trait AgentVectorDao: Send + Sync {
    /// 插入或更新 Agent 的向量索引
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 纯向量语义搜索，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 获取指定 Agent 的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Option<VectorRow>>;

    /// 删除 Agent 的向量索引
    async fn delete_vector(&self, ctx: RequestContext, agent_id: &str) -> Result<()>;

    /// 清空所有向量索引
    async fn clear_collection(&self, ctx: RequestContext) -> Result<()>;
}

/// Agent 统计 DAO 接口
///
/// 只负责 Agent 自身维度的统计（目前只有唤醒次数汇总）。
/// 数据来源：agent_awake_events 表
/// 模型调用相关的统计（token、时序等）由 ModelProviderStatsDao 负责。
#[async_trait::async_trait]
pub trait AgentStatsDao: Send + Sync {
    /// Agent 唤醒事件类型
    type AwakeEvent: StatEvent + 'static + Send + Sync;

    /// 获取唤醒事件表名（从 Stats 注册表中查询）
    fn awake_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::AwakeEvent>()
    }

    /// 底层通用查询（内部使用，不对外暴露业务语义）
    async fn query_awake_calls(
        &self,
        ctx: RequestContext,
        query: AgentStatsQuery,
    ) -> Result<Vec<JsonValue>>;

    /// Agent 唤醒总次数
    async fn sum_calls(&self, ctx: RequestContext, mut query: AgentStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_awake_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 获取 Agent 自身统计数据
    async fn get_stats(
        &self,
        ctx: RequestContext,
        query: AgentStatsQuery,
        options: StatsFetchOptions,
    ) -> Result<AgentStats> {
        let mut stats = AgentStats::default();

        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = AgentStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = AgentStatsQuery {
                    time_range: Some((start, end)),
                    ..query.clone()
                };
                let range_calls = self.sum_calls(ctx.clone(), range_query).await?;
                let duration_secs = (end - start) as f64 / 1000.0;
                if duration_secs > 0.0 {
                    Some(range_calls as f64 / duration_secs)
                } else {
                    None
                }
            } else {
                None
            };

            stats.call_summary = Some(CallSummary {
                total_calls,
                avg_qps,
                instant_qps,
            });
        }

        Ok(stats)
    }
}

pub mod sqlite;
pub use self::sqlite::{dao as base_dao, init as init_base, new as new_agent_dao};

pub mod vector;
pub use self::vector::{dao as vector_dao, init as init_vector, new as new_agent_vector_dao};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

/// 统一初始化所有 Agent DAO 单例
pub fn init() {
    init_base();
    init_vector();
}

// ========== 向后兼容：旧代码继续使用 `agent::new()` / `agent::dao()` ==========
pub fn new() -> std::sync::Arc<dyn AgentDao> {
    new_agent_dao()
}

pub fn dao() -> std::sync::Arc<dyn AgentDao> {
    base_dao()
}

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod vector_test;

#[cfg(test)]
mod stats_duckdb_test;
