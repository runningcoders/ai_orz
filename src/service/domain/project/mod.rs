//! Project Domain 模块
//!
//! 项目领域，管理：
//! - management - 项目管理（创建/更新/查询/归档）
//! - execution - 项目执行（任务分配/进度跟踪/统计）

pub mod management;
pub mod execution;

#[cfg(test)]
mod management_test;

use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use crate::service::dal::project::ProjectDal;
use async_trait::async_trait;
use common::enums::ProjectStatus;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static PROJECT_DOMAIN: OnceLock<Arc<dyn ProjectDomain>> = OnceLock::new();

/// 获取 Project Domain 单例
pub fn domain() -> Arc<dyn ProjectDomain> {
    PROJECT_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Project Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(project_dal: Arc<dyn ProjectDal>) -> Arc<dyn ProjectDomain> {
    let domain = ProjectDomainImpl::new(project_dal);
    Arc::new(domain)
}

/// 初始化 Project Domain（使用全局单例 DAL）
pub fn init() {
    let project_domain = ProjectDomainImpl::new(
        crate::service::dal::project::dal(),
    );
    let _ = PROJECT_DOMAIN.set(Arc::new(project_domain));
}

// ==================== 实现 ====================

/// Project Domain 实现
///
/// 聚合所有项目子功能实现
struct ProjectDomainImpl {
    project_dal: Arc<dyn ProjectDal>,
}

impl ProjectDomainImpl {
    /// 创建 Domain 实例
    fn new(project_dal: Arc<dyn ProjectDal>) -> Self {
        Self { project_dal }
    }
}

impl ProjectDomain for ProjectDomainImpl {
    fn management(&self) -> &dyn ProjectManagement {
        self
    }
    fn execution(&self) -> &dyn ProjectExecution {
        self
    }
}

// ==================== Command 定义 ====================

/// 创建项目命令参数
#[derive(Debug, Clone)]
pub struct CreateProjectCommand<'a> {
    /// 项目名称
    pub name: &'a str,
    /// 项目详细描述
    pub description: &'a str,
    /// 项目运作流程描述（可选）
    pub workflow: Option<&'a str>,
    /// 用户对项目的指导建议（可选）
    pub guidance: Option<&'a str>,
    /// 优先级（数值越大优先级越高）
    pub priority: i32,
    /// 标签列表
    pub tags: Vec<String>,
    /// 根用户 ID
    pub root_user_id: &'a str,
    /// 负责人 Agent ID（可选）
    pub owner_agent_id: Option<&'a str>,
    /// 开始时间戳（毫秒，可选）
    pub start_at: Option<i64>,
    /// 截止时间戳（毫秒，可选）
    pub due_at: Option<i64>,
}

/// 更新项目命令参数
#[derive(Debug, Clone)]
pub struct UpdateProjectCommand<'a> {
    /// 项目 ID
    pub project_id: &'a str,
    /// 项目名称（可选，None 表示不更新）
    pub name: Option<&'a str>,
    /// 项目详细描述（可选）
    pub description: Option<&'a str>,
    /// 项目运作流程描述（可选）
    pub workflow: Option<&'a str>,
    /// 用户对项目的指导建议（可选）
    pub guidance: Option<&'a str>,
    /// 优先级（可选）
    pub priority: Option<i32>,
    /// 标签列表（可选）
    pub tags: Option<Vec<String>>,
    /// 负责人 Agent ID（可选）
    pub owner_agent_id: Option<&'a str>,
    /// 开始时间戳（可选）
    pub start_at: Option<i64>,
    /// 截止时间戳（可选）
    pub due_at: Option<i64>,
}

// ==================== traits 定义 ====================

/// Project Domain 总 trait
///
/// 聚合项目领域所有子功能 trait
pub trait ProjectDomain: Send + Sync {
    /// 项目管理能力
    fn management(&self) -> &dyn ProjectManagement;
    /// 项目执行能力
    fn execution(&self) -> &dyn ProjectExecution;
}

/// 项目管理 trait
///
/// 定义项目管理相关的核心业务接口
#[async_trait::async_trait]
pub trait ProjectManagement: Send + Sync {
    /// 创建新项目
    async fn create_project(
        &self,
        ctx: RequestContext,
        cmd: CreateProjectCommand<'_>,
    ) -> Result<ProjectPo, AppError>;

    /// 根据 ID 获取项目
    async fn get_project_by_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Option<ProjectPo>, AppError>;

    /// 获取用户的所有项目
    async fn list_user_projects(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError>;

    /// 获取用户指定状态的项目
    async fn list_user_projects_by_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError>;

    /// 通用项目查询
    async fn query_projects(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<Vec<ProjectPo>, AppError>;

    /// 更新项目信息
    async fn update_project(
        &self,
        ctx: RequestContext,
        cmd: UpdateProjectCommand<'_>,
    ) -> Result<ProjectPo, AppError>;

    /// 更新项目状态
    async fn update_project_status(
        &self,
        ctx: RequestContext,
        project_id: &str,
        status: ProjectStatus,
    ) -> Result<(), AppError>;

    /// 归档项目（软删除）
    async fn archive_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError>;

    /// 统计用户的项目总数
    async fn count_user_projects(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64, AppError>;

    /// 统计用户指定状态的项目数
    async fn count_user_projects_by_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64, AppError>;
}

/// 项目执行 trait
///
/// 定义项目执行相关的核心业务接口
#[async_trait::async_trait]
pub trait ProjectExecution: Send + Sync {
    /// 开始项目（状态变为 InProgress，设置 start_at）
    async fn start_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError>;

    /// 完成项目（状态变为 Completed，设置 end_at）
    async fn complete_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError>;

    /// 重新激活项目（从 Completed/Archived 变回 Active）
    async fn reactivate_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError>;
}
