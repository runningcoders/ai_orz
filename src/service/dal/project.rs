//! Project DAL 模块
//!
//! 职责：Project 领域的数据访问层，封装 ProjectDao 提供统一的查询接口

use common::enums::ProjectStatus;
use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::service::dao::project::{ProjectDao, ProjectQuery};
use std::sync::{Arc, OnceLock};
use crate::service::dao::project;

// ==================== 单例管理 ====================

static PROJECT_DAL: OnceLock<Arc<dyn ProjectDal + Send + Sync>> = OnceLock::new();

/// 获取 Project DAL 单例
pub fn dal() -> Arc<dyn ProjectDal + Send + Sync> {
    PROJECT_DAL.get().cloned().unwrap()
}

/// 初始化 Project DAL
pub fn init() {
    let _ = PROJECT_DAL.set(new(project::dao()));
}

/// 创建 Project DAL（返回 trait 对象）
pub fn new(project_dao: Arc<dyn ProjectDao + Send + Sync>) -> Arc<dyn ProjectDal + Send + Sync> {
    Arc::new(ProjectDalImpl { project_dao })
}

// ==================== DAL 接口 ====================

/// Project DAL 接口
#[async_trait::async_trait]
pub trait ProjectDal: Send + Sync {
    /// 创建项目
    async fn create(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;

    /// 根据 ID 获取项目
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ProjectPo>, AppError>;

    /// 获取根用户下的所有项目
    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError>;

    /// 获取根用户下指定状态的项目
    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError>;

    /// 通用综合查询
    async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<ProjectPo>, AppError>;

    /// 更新项目信息
    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError>;

    /// 更新项目状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<(), AppError>;

    /// 归档项目（软删除）
    async fn archive(&self, ctx: RequestContext, id: &str, modified_by: &str) -> Result<(), AppError>;

    /// 统计根用户的项目总数
    async fn count_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64, AppError>;

    /// 统计根用户指定状态的项目数
    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64, AppError>;
}

// ==================== DAL 实现 ====================

/// Project DAL 实现
struct ProjectDalImpl {
    project_dao: Arc<dyn ProjectDao + Send + Sync>,
}

#[async_trait::async_trait]
impl ProjectDal for ProjectDalImpl {
    async fn create(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError> {
        self.project_dao.insert(ctx, project).await
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ProjectPo>, AppError> {
        self.project_dao.find_by_id(ctx, id).await
    }

    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError> {
        self.project_dao.list_by_root_user(ctx, root_user_id, limit).await
    }

    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError> {
        self.project_dao.list_by_root_user_and_status(ctx, root_user_id, status, limit).await
    }

    async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<ProjectPo>, AppError> {
        self.project_dao.query(ctx, query).await
    }

    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError> {
        self.project_dao.update(ctx, project).await
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<(), AppError> {
        self.project_dao.update_status(ctx, id, status, modified_by).await
    }

    async fn archive(&self, ctx: RequestContext, id: &str, modified_by: &str) -> Result<(), AppError> {
        self.project_dao.update_status(ctx, id, ProjectStatus::Archived, modified_by).await
    }

    async fn count_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64, AppError> {
        self.project_dao.count_by_root_user(ctx, root_user_id).await
    }

    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64, AppError> {
        self.project_dao.count_by_root_user_and_status(ctx, root_user_id, status).await
    }
}
