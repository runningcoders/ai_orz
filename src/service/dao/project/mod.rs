//! Project DAO 模块

use common::error::Result;
use common::models::{ProjectStats, CallSummary, StatsFetchOptions};
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatEvent, Stats};
use common::enums::ProjectStatus;
use serde_json::Value as JsonValue;

/// Project 查询参数
#[derive(Debug, Clone, Default)]
pub struct ProjectQuery {
    pub root_user_id: Option<String>,
    pub status_in: Option<Vec<ProjectStatus>>,
    pub limit: Option<usize>,
}

/// Project DAO 接口
#[async_trait::async_trait]
pub trait ProjectDao: Send + Sync + std::fmt::Debug {
    /// 插入新项目
    async fn insert(&self, ctx: RequestContext, project: &ProjectPo) -> Result<()>;
    /// 根据 ID 查询项目
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ProjectPo>>;
    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<Vec<ProjectPo>>;
    /// 根据根用户查询项目列表
    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>>;
    /// 根据根用户和状态查询项目列表
    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>>;
    /// 更新项目
    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<()>;
    /// 更新项目状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()>;
    /// 统计根用户的项目总数
    async fn count_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64>;
    /// 统计根用户指定状态的项目数
    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64>;
}

/// Project 统计查询参数
#[derive(Debug, Clone, Default)]
pub struct ProjectStatsQuery {
    /// Project ID（必填）
    pub project_id: String,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 聚合函数（内部使用）
    pub aggregations: Vec<StatAggregation>,
}

/// Project 统计 DAO 接口
///
/// 只负责 Project 自身维度的统计（目前只有业务事件次数汇总）。
/// 数据来源：project_events 表
/// 模型调用相关的统计（token、时序等）由 ModelProviderStatsDao 负责。
#[async_trait::async_trait]
pub trait ProjectStatsDao: Send + Sync {
    /// Project 业务事件类型
    type ProjectEvent: StatEvent + 'static + Send + Sync;

    /// 获取 Project 业务事件表名（从 Stats 注册表中查询）
    fn project_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::ProjectEvent>()
    }

    /// 底层通用查询（内部使用，不对外暴露业务语义）
    async fn query_project_calls(&self, ctx: RequestContext, query: ProjectStatsQuery) -> Result<Vec<JsonValue>>;

    /// Project 业务事件总次数
    async fn sum_calls(&self, ctx: RequestContext, mut query: ProjectStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_project_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 获取 Project 自身统计数据
    async fn get_stats(&self, ctx: RequestContext, query: ProjectStatsQuery, options: StatsFetchOptions) -> Result<ProjectStats> {
        let mut stats = ProjectStats::default();

        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = ProjectStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = ProjectStatsQuery {
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
        }

        Ok(stats)
    }
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod stats_duckdb_test;
