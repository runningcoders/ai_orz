//! Artifact 核心业务
//!
//! 负责产物的创建、查询、管理

use crate::error::AppError;
use crate::models::artifact::Artifact;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use common::enums::FileType;
use std::sync::Arc;

/// Artifact 业务领域
#[derive(Clone)]
pub struct ArtifactDomain {
    dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>,
}

impl ArtifactDomain {
    /// 创建 ArtifactDomain 实例
    pub fn new(dal: Arc<dyn crate::service::dal::artifact::ArtifactDal + Send + Sync>) -> Self {
        Self { dal }
    }

    /// 创建项目级产物
    pub async fn create_project_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Result<Artifact, AppError> {
        let artifact = Artifact::new_project(
            project_id,
            name,
            description,
            file_type,
            file_meta,
            created_by,
        );
        self.dal.create(ctx.clone(), &artifact).await?;
        Ok(artifact)
    }

    /// 创建任务级产物
    pub async fn create_task_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Result<Artifact, AppError> {
        let artifact = Artifact::new_task(
            project_id,
            task_id,
            name,
            description,
            file_type,
            file_meta,
            created_by,
        );
        self.dal.create(ctx.clone(), &artifact).await?;
        Ok(artifact)
    }

    /// 根据 ID 获取产物
    pub async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Artifact>, AppError> {
        self.dal.find_by_id(ctx, id).await
    }

    /// 获取项目下的所有产物
    pub async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Artifact>, AppError> {
        self.dal.list_by_project(ctx, project_id).await
    }

    /// 获取任务下的所有产物
    pub async fn list_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Artifact>, AppError> {
        self.dal.list_by_task(ctx, task_id).await
    }

    /// 删除产物
    pub async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        self.dal.delete(ctx, id).await
    }
}
