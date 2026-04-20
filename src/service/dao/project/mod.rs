//! Project DAO 模块

use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use common::enums::ProjectStatus;

/// Project DAO 接口
#[async_trait::async_trait]
pub trait ProjectDaoTrait: Send + Sync + std::fmt::Debug {
    /// 插入新项目
    async fn insert(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;
    /// 根据 ID 查询项目
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>, AppError>;
    /// 根据根用户查询项目列表
    async fn list_by_root_user(&self, ctx: RequestContext, root_user_id: &str, limit: Option<usize>) -> Result<Vec<ProjectPo>, AppError>;
    /// 根据根用户和状态查询项目列表
    async fn list_by_root_user_and_status(&self, ctx: RequestContext, root_user_id: &str, status: Vec<ProjectStatus>, limit: Option<usize>) -> Result<Vec<ProjectPo>, AppError>;
    /// 更新项目
    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;
    /// 更新项目状态
    async fn update_status(&self, ctx: RequestContext, id: &str, status: ProjectStatus, modified_by: &str) -> Result<(), AppError>;
    /// 统计根用户的项目总数
    async fn count_by_root_user(&self, ctx: RequestContext, root_user_id: &str) -> Result<u64, AppError>;
    /// 统计根用户指定状态的项目数
    async fn count_by_root_user_and_status(&self, ctx: RequestContext, root_user_id: &str, status: ProjectStatus) -> Result<u64, AppError>;
}

mod sqlite;
pub use self::sqlite::{dao, init};

#[cfg(test)]
mod sqlite_test;
