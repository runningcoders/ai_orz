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

mod artifact;
mod project;
mod task;

#[cfg(test)]
mod project_test;

pub use artifact::{ArtifactDomain, ListArtifactsParams};
pub use project::ProjectDomain;
pub use task::TaskDomain;

// ==================== 单例 ====================

static PROJECT_DOMAIN: OnceLock<Arc<dyn ProjectDomainProvider>> = OnceLock::new();

/// 获取 Project Domain 单例
pub fn domain() -> Arc<dyn ProjectDomainProvider> {
    PROJECT_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Project Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(
    project_dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>,
    task_dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>,
    artifact_dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>,
) -> Arc<dyn ProjectDomainProvider> {
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

/// Project Domain 总接口
///
/// 统一对外暴露项目领域的所有能力
pub trait ProjectDomainProvider: Send + Sync {
    /// 获取项目业务
    fn project(&self) -> &ProjectDomain;

    /// 获取任务业务
    fn task(&self) -> &TaskDomain;

    /// 获取产物业务
    fn artifact(&self) -> &ArtifactDomain;
}

// ==================== 实现 ====================

/// Project Domain 实现
///
/// 聚合所有子领域实现
struct ProjectDomainImpl {
    project: ProjectDomain,
    task: TaskDomain,
    artifact: ArtifactDomain,
}

impl ProjectDomainImpl {
    /// 创建 Domain 实例
    fn new(
        project_dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>,
        task_dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>,
        artifact_dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>,
    ) -> Self {
        let project = ProjectDomain::new(project_dal.clone());
        let task = TaskDomain::new(task_dal.clone());
        let artifact = ArtifactDomain::new(project_dal, task_dal, artifact_dal);
        Self {
            project,
            task,
            artifact,
        }
    }
}

impl ProjectDomainProvider for ProjectDomainImpl {
    fn project(&self) -> &ProjectDomain {
        &self.project
    }

    fn task(&self) -> &TaskDomain {
        &self.task
    }

    fn artifact(&self) -> &ArtifactDomain {
        &self.artifact
    }
}
