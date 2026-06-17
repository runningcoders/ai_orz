//! Artifact 核心业务
//!
//! 负责产物的创建、查询、管理

use crate::error::AppError;
use crate::models::artifact::Artifact;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::dal::artifact::ArtifactQuery;
use common::enums::{ArtifactSourceType, FileType};

use super::ProjectDomainImpl;

/// Artifact 列表查询参数。
#[derive(Debug, Clone)]
pub struct ListArtifactsParams {
    pub project_id: String,
    pub task_id: Option<String>,
    pub file_type: Option<FileType>,
    pub source_type: Option<ArtifactSourceType>,
    pub limit: Option<usize>,
}

#[async_trait::async_trait]
impl super::ArtifactManage for ProjectDomainImpl {
    /// 创建 Attachment 引用型产物。
    #[allow(clippy::too_many_arguments)]
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
    ) -> Result<Artifact, AppError> {
        self.validate_project_and_task(ctx.clone(), &project_id, task_id.as_deref())
            .await?;

        let mut artifact = if let Some(task_id) = task_id {
            Artifact::new_task_with_source_type(
                project_id,
                task_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::Attachment,
                created_by.clone(),
            )
        } else {
            Artifact::new_project_with_source_type(
                project_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::Attachment,
                created_by.clone(),
            )
        };
        artifact.po.set_tags(tags, created_by);
        self.artifact_dal.create(ctx, &artifact).await?;
        Ok(artifact)
    }

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
    ) -> Result<Artifact, AppError> {
        self.validate_project_access(ctx.clone(), &project_id)
            .await?;
        let artifact = Artifact::new_project(
            project_id,
            name,
            description,
            file_type,
            file_meta,
            created_by,
        );
        self.artifact_dal.create(ctx.clone(), &artifact).await?;
        Ok(artifact)
    }

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
    ) -> Result<Artifact, AppError> {
        self.validate_project_and_task(ctx.clone(), &project_id, Some(&task_id))
            .await?;
        let artifact = Artifact::new_task(
            project_id,
            task_id,
            name,
            description,
            file_type,
            file_meta,
            created_by,
        );
        self.artifact_dal.create(ctx.clone(), &artifact).await?;
        Ok(artifact)
    }

    /// 根据 ID 获取产物
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Artifact>, AppError> {
        let Some(artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            return Ok(None);
        };
        self.validate_project_access(ctx, &artifact.po.project_id)
            .await?;
        Ok(Some(artifact))
    }

    /// 获取项目下的所有产物
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Artifact>, AppError> {
        self.validate_project_access(ctx.clone(), project_id)
            .await?;
        self.artifact_dal.list_by_project(ctx, project_id).await
    }

    /// 获取任务下的所有产物
    async fn list_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Artifact>, AppError> {
        let Some(task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            return Err(AppError::NotFound(format!("Task not found: {}", task_id)));
        };
        let Some(project_id) = task.po.project_id.as_deref() else {
            return Err(AppError::BadRequest(format!(
                "Task {} does not belong to a project",
                task_id
            )));
        };
        self.validate_project_access(ctx.clone(), project_id)
            .await?;
        self.artifact_dal.list_by_task(ctx, task_id).await
    }

    /// 按项目范围查询产物，支持 task/file/source/limit 过滤。
    async fn list(
        &self,
        ctx: RequestContext,
        params: ListArtifactsParams,
    ) -> Result<Vec<Artifact>, AppError> {
        if params.project_id.trim().is_empty() {
            return Err(AppError::BadRequest("project_id 不能为空".to_string()));
        }

        self.validate_project_and_task(ctx.clone(), &params.project_id, params.task_id.as_deref())
            .await?;

        self.artifact_dal
            .query(
                ctx,
                ArtifactQuery {
                    project_id: Some(params.project_id),
                    task_id: params.task_id,
                    file_type: params.file_type,
                    source_type: params.source_type,
                    limit: params.limit,
                },
            )
            .await
    }

    /// 删除产物
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let Some(artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            return Err(AppError::NotFound(format!("Artifact not found: {}", id)));
        };
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;
        self.artifact_dal.delete(ctx, id).await
    }
}

impl ProjectDomainImpl {
    async fn validate_project_access(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError> {
        let Some(project) = self.project_dal.find_by_id(ctx.clone(), project_id).await? else {
            return Err(AppError::NotFound(format!(
                "Project not found: {}",
                project_id
            )));
        };

        let current_user_id = ctx.uid();
        if current_user_id.is_empty() {
            return Err(AppError::BadRequest("当前用户不能为空".to_string()));
        }
        if project.po.root_user_id != current_user_id {
            return Err(AppError::BadRequest(format!(
                "无权访问 Project: {}",
                project_id
            )));
        }

        Ok(())
    }

    async fn validate_project_and_task(
        &self,
        ctx: RequestContext,
        project_id: &str,
        task_id: Option<&str>,
    ) -> Result<(), AppError> {
        self.validate_project_access(ctx.clone(), project_id)
            .await?;

        if let Some(task_id) = task_id {
            let Some(task) = self.task_dal.find_by_id(ctx, task_id).await? else {
                return Err(AppError::NotFound(format!("Task not found: {}", task_id)));
            };
            if task.po.project_id.as_deref() != Some(project_id) {
                return Err(AppError::BadRequest(format!(
                    "Task {} does not belong to project {}",
                    task_id, project_id
                )));
            }
        }

        Ok(())
    }
}
