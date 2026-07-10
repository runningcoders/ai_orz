//! Tool DAO trait

use common::error::{err, bail_err, Error, Result};
use common::models::{CallSummary, ToolStats, StatsFetchOptions};
use crate::models::tool::ToolPo;
use crate::models::vector::VectorIndexParams;
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatEvent, Stats};
use async_trait::async_trait;
use common::enums::{ToolProtocol, ToolStatus};
use serde_json::Value as JsonValue;
use std::sync::Arc;

pub mod sqlite;
pub mod vector;
pub mod stats_duckdb;

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod stats_duckdb_test;

/// Get global Tool DAO (alias for get, consistent with other DAOs)
pub fn dao() -> Arc<dyn ToolDao> {
    sqlite::dao()
}

/// Initialize global Tool DAO
pub fn init() {
    sqlite::init();
    vector::init();
    stats_duckdb::stats_init();
}

/// Tool 查询参数
#[derive(Debug, Clone, Default)]
pub struct ToolQuery {
    pub agent_id: Option<String>,
    pub ids: Option<Vec<String>>, // 按 ID 批量查询
    pub keyword: Option<String>,  // 关键词搜索
    pub protocol: Option<ToolProtocol>,
    pub status: Option<ToolStatus>,
    pub exclude_status: Option<ToolStatus>,
    pub mcp_server_id: Option<String>,
    pub enabled_only: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Tool 搜索参数（向量 + 关键词混合搜索）
#[derive(Debug, Clone, Default)]
pub struct ToolSearch {
    pub keyword: Option<String>,
    pub limit: usize,
    pub agent_id: Option<String>,
    pub enabled_only: bool,
}

/// 工具统计查询参数
#[derive(Debug, Clone, Default)]
pub struct ToolStatsQuery {
    /// 工具 ID（必填）
    pub tool_id: String,
    /// Agent ID（可选过滤）
    pub agent_id: Option<String>,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 聚合函数（内部使用）
    pub aggregations: Vec<StatAggregation>,
}

/// 工具统计 DAO 接口
///
/// 数据来源：tool_call_events 表
#[async_trait::async_trait]
pub trait ToolStatsDao: Send + Sync {
    /// 工具调用事件类型
    type ToolCallEvent: StatEvent + 'static + Send + Sync;

    /// 获取事件表名
    fn table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ToolCallEvent>()
    }

    /// 底层通用查询（内部使用）
    async fn query_tool_calls(&self, ctx: RequestContext, query: ToolStatsQuery) -> Result<Vec<JsonValue>>;

    /// 工具总调用次数
    async fn sum_calls(&self, ctx: RequestContext, mut query: ToolStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_tool_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 工具失败调用次数
    async fn sum_failed_calls(&self, ctx: RequestContext, mut query: ToolStatsQuery) -> Result<u64> {
        query.filters.push(StatFilter::Equals {
            key: "status".to_string(),
            value: JsonValue::String("failed".to_string()),
        });
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_tool_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 获取工具统计数据
    async fn get_stats(&self, ctx: RequestContext, query: ToolStatsQuery, options: StatsFetchOptions) -> Result<ToolStats> {
        let mut stats = ToolStats::default();

        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = ToolStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = ToolStatsQuery {
                    time_range: Some((start, end)),
                    ..query.clone()
                };
                let range_calls = self.sum_calls(ctx.clone(), range_query).await?;
                let duration_secs = (end - start) as f64 / 1000.0;
                if duration_secs > 0.0 { Some(range_calls as f64 / duration_secs) } else { None }
            } else {
                None
            };

            stats.call_summary = Some(CallSummary {
                total_calls,
                avg_qps,
                instant_qps,
            });

            let failed = self.sum_failed_calls(ctx, query).await?;
            stats.failed_count = Some(failed);
        }

        Ok(stats)
    }
}

/// Tool DAO trait
#[async_trait]
pub trait ToolDao: Send + Sync {
    /// Create a new tool
    async fn create_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()>;

    /// Update an existing tool
    async fn update_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<()>;

    /// Delete a tool
    async fn delete_tool(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// Get tool by ID
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<ToolPo>>;

    /// Get tool by name
    async fn get_by_name(&self, ctx: RequestContext, name: &str) -> Result<Option<ToolPo>>;

    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: ToolQuery) -> Result<Vec<ToolPo>>;

    /// List all enabled tools
    async fn list_enabled(&self, ctx: RequestContext) -> Result<Vec<ToolPo>>;

    /// Add tool to agent
    async fn add_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<()>;

    /// Remove tool from agent
    async fn remove_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()>;

    /// List all tools for an agent
    async fn list_tools_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<ToolPo>>;

    /// Sync all registered built-in tools to database
    /// If a tool already exists (by ID), skip it to avoid duplicates
    /// Returns number of newly inserted tools
    async fn sync_builtin_tools_to_db(&self, ctx: RequestContext) -> Result<usize>;

    /// 关键词搜索工具（使用 query 方法的关键词查询实现）
    async fn search(&self, ctx: RequestContext, params: ToolSearch) -> Result<Vec<ToolPo>>;
}

// ==================== ToolVectorDao Trait ====================

/// Tool Vector DAO trait - 仅负责工具向量索引的 CRUD，与基础工具数据解耦
#[async_trait]
pub trait ToolVectorDao: Send + Sync {
    /// 插入或更新工具的向量索引
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 纯向量语义搜索，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<crate::models::vector::VectorSearchHit>>;

    /// 获取指定工具的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        tool_id: &str,
    ) -> Result<Option<crate::models::vector::VectorRow>>;
}

// ==================== 统一导出 ====================

// 子模块构造函数别名（用于 DAL 层组合）
pub use sqlite::{dao as base_dao, init as init_base, new as new_tool_dao};
pub use vector::{dao as vector_dao, init as init_vector, new as new_tool_vector_dao};
pub use stats_duckdb::{stats_dao, stats_init, stats_new};
