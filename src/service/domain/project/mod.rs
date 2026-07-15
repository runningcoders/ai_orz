//! Project Domain 层
//!
//! 遵循严格分层架构：
//! - Domain 层只组合 DAL，不调用 DAO
//! - Domain 层返回业务实体（Project/Task/Artifact），不返回 PO
//! - 按业务对象分模块，不是按操作分
//!
//! 模块结构：
//! - project.rs - 项目核心业务
//! - task.rs - 任务核心业务
//! - artifact.rs - 产物核心业务

use async_trait::async_trait;
use std::sync::{Arc, OnceLock};

use common::error::Result;
use crate::models::artifact::Artifact;
use crate::models::file::FileMeta;
use crate::models::project::Project;
use crate::models::task::Task;
use crate::pkg::RequestContext;
use common::enums::{AssigneeType, FileType, ProjectStatus, TaskStatus};

mod artifact;
mod project;
mod task;

#[cfg(test)]
mod project_test;

pub use artifact::ListArtifactsParams;

// ==================== 单例 ====================

static PROJECT_DOMAIN: OnceLock<Arc<dyn ProjectDomain>> = OnceLock::new();

/// 获取 Project Domain 单例
pub fn domain() -> Arc<dyn ProjectDomain> {
    PROJECT_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Project Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(
    project_dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>,
    task_dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>,
    artifact_dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>,
) -> Arc<dyn ProjectDomain> {
    Arc::new(ProjectDomainImpl::new(project_dal, task_dal, artifact_dal))
}

/// 初始化 Project Domain（使用全局单例 DAL）
pub fn init() {
    let project_domain = ProjectDomainImpl::new(
        crate::service::dal::project::dal(),
        crate::service::dal::task::dal(),
        crate::service::dal::artifact::dal(),
    );
    let _ = PROJECT_DOMAIN.set(Arc::new(project_domain));
}

// ==================== trait 定义 ====================

/// Project Domain 总 trait
///
/// 聚合项目领域所有子功能 trait
pub trait ProjectDomain: Send + Sync {
    /// Project 管理能力
    fn project_manage(&self) -> &dyn ProjectManage;

    /// Task 管理能力
    fn task_manage(&self) -> &dyn TaskManage;

    /// Artifact 管理能力
    fn artifact_manage(&self) -> &dyn ArtifactManage;
}

/// Project 管理 trait
///
/// 定义项目相关的业务接口
#[async_trait]
pub trait ProjectManage: Send + Sync {
    /// 创建新项目
    async fn create(
        &self,
        ctx: RequestContext,
        name: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        root_user_id: String,
        created_by: String,
    ) -> Result<Project>;

    /// 根据 ID 获取项目
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>>;

    /// 根据 ID 获取项目（带附带信息选项）
    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::project::ProjectFetchOptions,
    ) -> Result<Option<Project>>;

    /// 获取用户的所有项目
    async fn list_by_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<Vec<Project>>;

    /// 查询用户项目列表
    async fn list(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Option<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Project>>;

    /// 启动项目
    async fn start(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<()>;

    /// 完成项目
    async fn complete(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<()>;

    /// 归档项目
    async fn archive(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<()>;

    /// 更新项目基本信息
    async fn update_basic(
        &self,
        ctx: RequestContext,
        project_id: &str,
        name: Option<String>,
        description: Option<String>,
        priority: Option<i32>,
        tags: Option<Vec<String>>,
        modified_by: String,
    ) -> Result<Project>;

    /// 统一项目状态流转
    async fn transition_status(
        &self,
        ctx: RequestContext,
        project: &mut Project,
        target_status: ProjectStatus,
    ) -> Result<()>;
}

/// Task 管理 trait
///
/// 定义任务相关的业务接口
#[async_trait]
pub trait TaskManage: Send + Sync {
    /// 创建新任务
    async fn create(
        &self,
        ctx: RequestContext,
        title: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        root_user_id: String,
        assignee_type: AssigneeType,
        assignee_id: String,
        project_id: Option<String>,
        created_by: String,
    ) -> Result<Task>;

    /// 创建新任务（支持管理面完整可选字段）
    async fn create_with_options(
        &self,
        ctx: RequestContext,
        title: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        root_user_id: String,
        assignee_type: AssigneeType,
        assignee_id: String,
        project_id: Option<String>,
        due_at: Option<i64>,
        dependencies: Vec<String>,
        created_by: String,
    ) -> Result<Task>;

    /// 根据 ID 获取任务
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>>;

    /// 根据 ID 获取任务（带附带信息选项）
    async fn get_task(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::task::TaskFetchOptions,
    ) -> Result<Option<Task>>;

    /// 获取项目下的所有任务
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Task>>;

    /// 获取分配给 Agent 的所有任务
    async fn list_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Task>>;

    /// 查询任务列表
    async fn list(
        &self,
        ctx: RequestContext,
        project_id: Option<&str>,
        assignee_type: Option<AssigneeType>,
        assignee_id: Option<&str>,
        status: Option<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Task>>;

    /// 更新任务基本信息
    async fn update_basic(
        &self,
        ctx: RequestContext,
        task_id: &str,
        title: Option<String>,
        description: Option<String>,
        priority: Option<i32>,
        tags: Option<Vec<String>>,
        due_at: Option<i64>,
        dependencies: Option<Vec<String>>,
    ) -> Result<Task>;

    /// 开始任务
    async fn start(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<()>;

    /// 完成任务
    async fn complete(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<()>;

    /// 取消任务
    async fn cancel(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<()>;

    /// 统一任务状态流转
    async fn transition_status(
        &self,
        ctx: RequestContext,
        task: &mut Task,
        target_status: TaskStatus,
    ) -> Result<()>;

    /// 更新任务进度（0-100）
    async fn update_progress(
        &self,
        ctx: RequestContext,
        task_id: &str,
        progress: i32,
    ) -> Result<Task>;
}

/// Artifact 管理 trait
///
/// 定义产物相关的业务接口
#[async_trait]
pub trait ArtifactManage: Send + Sync {
    /// 创建 Attachment 引用型产物。
    async fn create_attachment_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact>;

    /// 创建项目级产物
    async fn create_project_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Result<Artifact>;

    /// 创建任务级产物
    async fn create_task_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Result<Artifact>;

    /// 根据 ID 获取产物
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Artifact>>;

    /// 获取项目下的所有产物
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Artifact>>;

    /// 获取任务下的所有产物
    async fn list_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Artifact>>;

    /// 按项目范围查询产物，支持 task/file/source/limit 过滤。
    async fn list(
        &self,
        ctx: RequestContext,
        params: ListArtifactsParams,
    ) -> Result<Vec<Artifact>>;

    /// 删除产物
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// Get artifact content (only for generated-content artifacts).
    /// Returns the raw bytes if exists, None otherwise.
    async fn get_artifact_content(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> std::result::Result<Option<Artifact>, common::error::Error>;

    /// Read artifact content bytes from storage (only for generated-content artifacts).
    async fn read_content(
        &self,
        ctx: RequestContext,
        artifact: &Artifact,
    ) -> Result<Vec<u8>>;

    /// Update artifact content (full replace, only for generated-content artifacts).
    /// Returns the updated artifact.
    async fn update_artifact_content(
        &self,
        ctx: RequestContext,
        id: &str,
        content: Vec<u8>,
        expected_updated_at: Option<i64>,
    ) -> Result<Artifact>;
}

// ==================== 实现 ====================

/// Project Domain 实现
///
/// 聚合所有项目子功能实现
struct ProjectDomainImpl {
    project_dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>,
    task_dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>,
    artifact_dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>,
}

impl ProjectDomainImpl {
    /// 创建 Domain 实例
    fn new(
        project_dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>,
        task_dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>,
        artifact_dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>,
    ) -> Self {
        Self {
            project_dal,
            task_dal,
            artifact_dal,
        }
    }
}

impl ProjectDomain for ProjectDomainImpl {
    fn project_manage(&self) -> &dyn ProjectManage {
        self
    }

    fn task_manage(&self) -> &dyn TaskManage {
        self
    }

    fn artifact_manage(&self) -> &dyn ArtifactManage {
        self
    }
}
