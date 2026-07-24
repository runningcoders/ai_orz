//! Task DAO 模块

use common::error::Result;
use common::models::{TaskStats, CallSummary, StatsFetchOptions};
use crate::models::task::TaskPo;
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatFilter, StatAggregation, StatEvent, Stats};
use common::enums::AssigneeType;
use common::enums::TaskStatus;
use serde_json::Value as JsonValue;

/// Task 查询参数
#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    pub assignee_type: Option<AssigneeType>,
    pub assignee_id: Option<String>,
    pub project_id: Option<String>,
    pub status_in: Option<Vec<TaskStatus>>,
    /// 按 ID 批量查询（向量搜索结果回填用）
    pub ids: Option<Vec<String>>,
    /// 关键词搜索（已在 search 方法中通过 FTS5 实现，query 方法中忽略）
    pub keyword: Option<String>,
    pub pagination: common::api::PaginationParams,
}

/// ✅ Task 搜索统一入参（关键词搜索 + 向量语义搜索共用）
#[derive(Debug, Clone, Default)]
pub struct TaskSearch {
    /// 关键词搜索查询（用于 FTS5 全文检索）
    pub keyword: Option<String>,
    /// 查询向量（用于向量语义搜索，DAL 层填充）
    pub query_vector: Option<Vec<f32>>,
    /// 返回 Top K 结果（向量搜索专用）
    pub top_k: Option<i32>,
    /// ✅ 业务过滤条件（直接复用 TaskQuery）
    pub filters: TaskQuery,
}

/// Task DAO 接口
#[async_trait::async_trait]
pub trait TaskDao: Send + Sync + std::fmt::Debug {
    /// 插入新任务
    async fn insert(&self, ctx: RequestContext, task: &TaskPo) -> Result<()>;
    /// 根据 ID 查询任务
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>>;
    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<common::api::PagedResult<TaskPo>>;

    /// 全文检索任务（FTS5 MATCH + BM25 排序）
    ///
    /// 使用 tasks_fts 虚拟表进行全文检索，返回匹配的任务及 FTS 相关性评分。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - search: 统一搜索参数（关键词 + 业务过滤）
    /// # 返回
    /// - 匹配的任务列表（按 BM25 相关性排序），每条携带 `fts_rank`（越小越相关）
    async fn search_tasks(
        &self,
        ctx: RequestContext,
        search: TaskSearch,
    ) -> Result<Vec<(TaskPo, Option<f32>)>>;
    /// 根据分配对象查询任务列表
    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;
    /// 根据状态查询任务列表
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;
    /// 更新任务
    async fn update(&self, ctx: RequestContext, task: &TaskPo) -> Result<()>;
    /// 更新任务状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()>;
    /// 统计分配对象的任务总数
    async fn count_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
    ) -> Result<u64>;
    /// 统计分配对象指定状态的任务数
    async fn count_by_assignee_and_status(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
        status: TaskStatus,
    ) -> Result<u64>;
}

// ==================== TaskVectorDao Trait ====================

/// ✅ Task Vector DAO trait - 仅负责任务向量索引的 CRUD，与基础任务数据完全解耦
#[async_trait::async_trait]
pub trait TaskVectorDao: Send + Sync {
    /// 索引任务向量（title + description 拼接）
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        task_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 语义搜索任务，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 获取指定任务的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Option<VectorRow>>;

    /// 删除任务的向量索引
    async fn delete_vector(&self, ctx: RequestContext, task_id: &str) -> Result<()>;

    /// 清空所有向量索引
    async fn clear_collection(&self, ctx: RequestContext) -> Result<()>;
}

/// Task 统计查询参数
#[derive(Debug, Clone, Default)]
pub struct TaskStatsQuery {
    /// Task ID（必填）
    pub task_id: String,
    /// 额外过滤条件
    pub filters: Vec<StatFilter>,
    /// 时间范围（毫秒，None 表示不限）
    pub time_range: Option<(i64, i64)>,
    /// 聚合函数（内部使用）
    pub aggregations: Vec<StatAggregation>,
}

/// Task 统计 DAO 接口
///
/// 只负责 Task 自身维度的统计（目前只有业务事件次数汇总）。
/// 数据来源：task_events 表
/// 模型调用相关的统计（token、时序等）由 ModelProviderStatsDao 负责。
#[async_trait::async_trait]
pub trait TaskStatsDao: Send + Sync {
    /// Task 业务事件类型
    type TaskEvent: StatEvent + 'static + Send + Sync;

    /// 获取 Task 业务事件表名（从 Stats 注册表中查询）
    fn task_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::TaskEvent>()
    }

    /// 底层通用查询（内部使用，不对外暴露业务语义）
    async fn query_task_calls(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<JsonValue>>;

    /// Task 业务事件总次数
    async fn sum_calls(&self, ctx: RequestContext, mut query: TaskStatsQuery) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_task_calls(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 获取 Task 自身统计数据
    async fn get_stats(&self, ctx: RequestContext, query: TaskStatsQuery, options: StatsFetchOptions) -> Result<TaskStats> {
        let mut stats = TaskStats::default();

        if options.with_call_summary {
            let total_calls = self.sum_calls(ctx.clone(), query.clone()).await?;
            let now = chrono::Utc::now().timestamp_millis();
            let instant_query = TaskStatsQuery {
                time_range: Some((now - 1000, now)),
                ..query.clone()
            };
            let instant_calls = self.sum_calls(ctx.clone(), instant_query).await?;
            let instant_qps = instant_calls as f64;

            let avg_qps = if let Some((start, end)) = options.time_range {
                let range_query = TaskStatsQuery {
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
pub use self::sqlite::{dao, get_dao, init, new};

pub mod vector;
pub use self::vector::{dao as vector_dao, init as init_vector, new as new_task_vector_dao};

pub mod stats_duckdb;
pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};

#[cfg(test)]
mod sqlite_test;

#[cfg(test)]
mod vector_test;

#[cfg(test)]
mod stats_duckdb_test;
