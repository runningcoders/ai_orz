//! Task DAO 模块

use common::error::{Error, Result};
use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use common::enums::AssigneeType;
use common::enums::TaskStatus;
use common::bail_err;

/// Task 查询参数
#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    pub assignee_type: Option<AssigneeType>,
    pub assignee_id: Option<String>,
    pub project_id: Option<String>,
    pub status_in: Option<Vec<TaskStatus>>,
    pub limit: Option<usize>,
}

/// Task DAO 接口
#[async_trait::async_trait]
pub trait TaskDao: Send + Sync + std::fmt::Debug {
    /// 插入新任务
    async fn insert(&self, ctx: RequestContext, task: &TaskPo) -> Result<()>;
    /// 根据 ID 查询任务
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>>;
    /// 通用查询
    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<TaskPo>>;
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

pub mod sqlite;
pub use self::sqlite::{dao, get_dao, init, new};

#[cfg(test)]
mod sqlite_test;
