//! Task DAO 模块

use crate::error::AppError;
use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use common::enums::TaskStatus;

/// Task DAO 接口
#[async_trait::async_trait]
pub trait TaskDaoTrait: Send + Sync + std::fmt::Debug {
    /// 插入新任务
    async fn insert(&self, ctx: RequestContext, task: &TaskPo) -> Result<(), AppError>;
    /// 根据 ID 查询任务
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>, AppError>;
    /// 根据分配对象查询任务列表
    async fn list_by_assignee(&self, ctx: RequestContext, assignee_type: Option<common::enums::AssigneeType>, assignee_id: &str, limit: Option<usize>) -> Result<Vec<TaskPo>, AppError>;
    /// 根据状态查询任务列表
    async fn list_by_status(&self, ctx: RequestContext, assignee_type: Option<common::enums::AssigneeType>, assignee_id: &str, status: Vec<TaskStatus>, limit: Option<usize>) -> Result<Vec<TaskPo>, AppError>;
    /// 更新任务
    async fn update(&self, ctx: RequestContext, task: &TaskPo) -> Result<(), AppError>;
    /// 更新任务状态
    async fn update_status(&self, ctx: RequestContext, id: &str, status: TaskStatus, modified_by: &str) -> Result<(), AppError>;
    /// 统计分配对象的任务总数
    async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64, AppError>;
    /// 统计分配对象指定状态的任务数
    async fn count_by_assignee_and_status(&self, ctx: RequestContext, assignee_id: &str, status: TaskStatus) -> Result<u64, AppError>;
}

mod sqlite;
pub use self::sqlite::{dao, init, get_dao};

#[cfg(test)]
mod sqlite_test;
