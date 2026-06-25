//! Artifact 核心业务
//!
//! 负责产物的创建、查询、管理

use common::bail_err;
use crate::models::artifact::Artifact;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::dal::artifact::ArtifactQuery;
use common::enums::{ArtifactSourceType, FileType};

use super::ProjectDomainImpl;
use common::error::Result;
use common::err;

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
    ) -> Result<Artifact> {
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
    ) -> Result<Artifact> {
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
    ) -> Result<Artifact> {
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
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Artifact>> {
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
    ) -> Result<Vec<Artifact>> {
        self.validate_project_access(ctx.clone(), project_id)
            .await?;
        self.artifact_dal.list_by_project(ctx, project_id).await
    }

    /// 获取任务下的所有产物
    async fn list_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Artifact>> {
        let Some(task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            bail_err!(NotFound, "Task not found: {}", task_id);
        };
        let Some(project_id) = task.po.project_id.as_deref() else {
            bail_err!(InvalidRequest, "Task {} does not belong to a project", task_id);
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
    ) -> Result<Vec<Artifact>> {
        if params.project_id.trim().is_empty() {
            bail_err!(InvalidRequest, "project_id 不能为空");
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
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let Some(artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            bail_err!(NotFound, "Artifact not found: {}", id);
        };
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;
        self.artifact_dal.delete(ctx, id).await
    }

    /// Get artifact content for generated-content artifacts.
    async fn get_artifact_content(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Artifact>> {
        let Some(artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            return Ok(None);
        };
        // Validate user has access to this artifact via project ownership
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;

        if artifact.po.source_type != common::enums::ArtifactSourceType::GeneratedContent {
            bail_err!(InvalidRequest, "Cannot read content directly from artifact source type {:?}, only GeneratedContent artifacts support direct content access.", artifact.po.source_type);
        }

        let content = self.artifact_dal.read_content(ctx, &artifact).await?;
        Ok(content.map(|c| (artifact, c)))
    }

    /// Update artifact content (full replacement) for generated-content artifacts.
    async fn update_artifact_content(
        &self,
        ctx: RequestContext,
        id: &str,
        content: Vec<u8>,
        expected_updated_at: Option<i64>,
    ) -> Result<Artifact> {
        let Some(mut artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            bail_err!(NotFound, "Artifact not found: {}", id);
        };
        // Validate user has access to this artifact via project ownership
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;

        if artifact.po.source_type != common::enums::ArtifactSourceType::GeneratedContent {
            bail_err!(InvalidRequest, "Cannot update content directly for artifact source type {:?}, only GeneratedContent artifacts support direct content update.", artifact.po.source_type);
        }

        // Optimistic locking check
        if let Some(expected) = expected_updated_at {
            if artifact.po.updated_at != expected {
                bail_err!(Conflict, "Conflict: expected updated_at = {}, current updated_at = {}. Please reload and try again.", expected, artifact.po.updated_at);
            }
        }

        // Write the new content to disk
        self.artifact_dal
            .write_content(ctx.clone(), &artifact, &content)
            .await?;

        // Update the artifact metadata: file size and updated timestamp
        let now = common::constants::utils::current_timestamp_ms();
        artifact.po.file_meta.0.file_size = content.len() as u64;
        artifact.po.updated_at = now;
        artifact.po.modified_by = ctx.uid();

        // Update the artifact record in database
        self.artifact_dal.update(ctx.clone(), &artifact).await?;

        Ok(artifact)
    }
}

impl ProjectDomainImpl {
    async fn validate_project_access(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<()> {
        let Some(project) = self.project_dal.find_by_id(ctx.clone(), project_id).await? else {
            bail_err!(NotFound, "Project not found: {}", project_id);
        };

        let current_user_id = ctx.uid();
        if current_user_id.is_empty() {
            bail_err!(InvalidRequest, "当前用户不能为空");
        }
        if project.po.root_user_id != current_user_id {
            bail_err!(InvalidRequest, "无权访问 Project: {}", project_id);
        }

        Ok(())
    }

    async fn validate_project_and_task(
        &self,
        ctx: RequestContext,
        project_id: &str,
        task_id: Option<&str>,
    ) -> Result<()> {
        self.validate_project_access(ctx.clone(), project_id)
            .await?;

        if let Some(task_id) = task_id {
            let Some(task) = self.task_dal.find_by_id(ctx, task_id).await? else {
                bail_err!(NotFound, "Task not found: {}", task_id);
            };
            if task.po.project_id.as_deref() != Some(project_id) {
                bail_err!(InvalidRequest, "Task {} does not belong to project {}", task_id, project_id);
            }
        }

        Ok(())
    }
}