//! Task DAL 模块
//!
//! 职责：Task 领域的数据访问层，封装 TaskDao 提供统一的查询接口

use common::error::Result;
use common::models::{StatsFetchOptions, TaskStats, TimeSeriesPoint, TokenSumResult};
use crate::models::task::{Task, TaskPo};
use crate::pkg::RequestContext;
use crate::pkg::stats::{AggregationRow, ModelCallEvent};
use crate::service::dao::task;
use crate::service::dao::task::{TaskDao, TaskQuery, TaskStatsDao, TaskStatsQuery};
use common::enums::{AssigneeType, TaskStatus};
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;

// ==================== 单例管理 ====================

static TASK_DAL: OnceLock<Arc<dyn TaskDal + Send + Sync>> = OnceLock::new();

/// 获取 Task DAL 单例
pub fn dal() -> Arc<dyn TaskDal + Send + Sync> {
    TASK_DAL.get().cloned().unwrap()
}

/// 初始化 Task DAL
pub fn init() {
    task::stats_init();
    let _ = TASK_DAL.set(new(task::dao(), task::stats_dao()));
}

/// 创建 Task DAL（返回 trait 对象）
pub fn new(
    task_dao: Arc<dyn TaskDao + Send + Sync>,
    task_stats_dao: Arc<dyn TaskStatsDao<ModelCallEvent = ModelCallEvent>>,
) -> Arc<dyn TaskDal + Send + Sync> {
    Arc::new(TaskDalImpl { task_dao, task_stats_dao })
}

// ==================== DAL 接口 ====================

/// Task DAL 接口
#[async_trait::async_trait]
pub trait TaskDal: Send + Sync {
    /// 创建任务
    async fn create(&self, ctx: RequestContext, task: &Task) -> Result<()>;

    /// 根据 ID 获取任务
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>>;

    /// 获取分配对象下的所有任务
    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;

    /// 获取分配对象下指定状态的任务
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>>;

    /// 获取项目下的所有任务
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Task>>;

    /// 通用综合查询
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<Task>>;

    /// 更新任务信息
    async fn update(&self, ctx: RequestContext, task: &Task) -> Result<()>;

    /// 更新任务状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()>;

    /// 取消任务
    async fn cancel(
        &self,
        ctx: RequestContext,
        id: &str,
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

    // ==================== 统计查询 ====================

    /// Token 汇总
    async fn sum_tokens(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<TokenSumResult>;

    /// 模型调用次数汇总
    async fn sum_calls(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<u64>;

    /// 模型调用时序查询
    async fn query_model_call_time_series(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<TimeSeriesPoint>>;

    /// 模型调用聚合查询
    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<AggregationRow>>;

    /// 获取 Task 统计数据（按 options 控制返回哪些维度）
    async fn get_stats(&self, ctx: RequestContext, query: TaskStatsQuery, options: StatsFetchOptions) -> Result<TaskStats>;
}

// ==================== DAL 实现 ====================

/// Task DAL 实现
struct TaskDalImpl {
    task_dao: Arc<dyn TaskDao + Send + Sync>,
    task_stats_dao: Arc<dyn TaskStatsDao<ModelCallEvent = ModelCallEvent>>,
}

#[async_trait::async_trait]
impl TaskDal for TaskDalImpl {
    async fn create(&self, ctx: RequestContext, task: &Task) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, task);
        self.task_dao.insert(ctx, &task.po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>> {
        let opt = self.task_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Task::from_po))
    }

    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>> {
        self.task_dao
            .list_by_assignee(ctx, assignee_type, assignee_id, limit)
            .await
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>> {
        self.task_dao
            .list_by_status(ctx, assignee_type, assignee_id, status, limit)
            .await
    }

    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Task>> {
        let list = self
            .task_dao
            .query(
                ctx,
                TaskQuery {
                    assignee_type: None,
                    assignee_id: None,
                    project_id: Some(project_id.to_string()),
                    status_in: None,
                    limit,
                },
            )
            .await?;
        Ok(list.into_iter().map(Task::from_po).collect())
    }

    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<Task>> {
        let list = self.task_dao.query(ctx, query).await?;
        Ok(list.into_iter().map(Task::from_po).collect())
    }

    async fn update(&self, ctx: RequestContext, task: &Task) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, task);
        self.task_dao.update(ctx, &task.po).await
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().task_id(id).build();
        self.task_dao
            .update_status(ctx, id, status, modified_by)
            .await
    }

    async fn cancel(
        &self,
        ctx: RequestContext,
        id: &str,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().task_id(id).build();
        self.task_dao
            .update_status(ctx, id, TaskStatus::Cancelled, modified_by)
            .await
    }

    async fn count_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
    ) -> Result<u64> {
        self.task_dao.count_by_assignee(ctx, assignee_id).await
    }

    async fn count_by_assignee_and_status(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
        status: TaskStatus,
    ) -> Result<u64> {
        self.task_dao
            .count_by_assignee_and_status(ctx, assignee_id, status)
            .await
    }

    // ==================== 统计查询 ====================

    async fn sum_tokens(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<TokenSumResult> {
        self.task_stats_dao.sum_tokens(ctx, query).await
    }

    async fn sum_calls(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<u64> {
        self.task_stats_dao.sum_calls(ctx, query).await
    }

    async fn query_model_call_time_series(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
        self.task_stats_dao.query_model_call_time_series(ctx, query).await
    }

    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: TaskStatsQuery) -> Result<Vec<AggregationRow>> {
        self.task_stats_dao.query_model_call_aggregation(ctx, query).await
    }

    async fn get_stats(&self, ctx: RequestContext, query: TaskStatsQuery, options: StatsFetchOptions) -> Result<TaskStats> {
        self.task_stats_dao.get_stats(ctx, query, options).await
    }
}
