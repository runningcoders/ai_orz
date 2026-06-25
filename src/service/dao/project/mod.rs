//! Project DAO 模块

use common::error::{Error, Result};
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use common::enums::ProjectStatus;
use common::bail_err;

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

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
