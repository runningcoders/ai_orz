//! Artifact 核心业务
//!
//! 负责产物的创建、查询、管理

use crate::models::artifact::Artifact;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::dal::artifact::ArtifactQuery;
use common::enums::{ArtifactSourceType, FileType};

use super::ProjectDomainImpl;
use common::error::{Result, bail_err, err};

use crate::enrich_ctx;

/// Artifact 列表查询参数。
#[derive(Debug, Clone)]
pub struct ListArtifactsParams {
    pub project_id: String,
    pub task_id: Option<String>,
    pub file_type: Option<FileType>,
    pub source_type: Option<ArtifactSourceType>,
    pub pagination: common::api::PaginationParams,
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
        let ctx = enrich_ctx!(&ctx, &artifact);
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
        let ctx = enrich_ctx!(&ctx, &artifact);
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
        let ctx = enrich_ctx!(&ctx, &artifact);
        self.artifact_dal.create(ctx.clone(), &artifact).await?;
        Ok(artifact)
    }

    /// 根据 ID 获取产物
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Artifact>> {
        let Some(artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            return Ok(None);
        };
        let ctx = enrich_ctx!(&ctx, &artifact);
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
    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<Artifact>> {
        let Some(task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            bail_err!(NotFound, "Task not found: {}", task_id);
        };
        let Some(project_id) = task.po.project_id.as_deref() else {
            bail_err!(
                InvalidRequest,
                "Task {} does not belong to a project",
                task_id
            );
        };
        let ctx = enrich_ctx!(&ctx, &task);
        self.validate_project_access(ctx.clone(), project_id)
            .await?;
        self.artifact_dal.list_by_task(ctx, task_id).await
    }

    /// 按项目范围查询产物，支持 task/file/source/limit 过滤。
    async fn list(
        &self,
        ctx: RequestContext,
        params: ListArtifactsParams,
    ) -> Result<common::api::PagedResult<Artifact>> {
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
                    pagination: params.pagination,
                },
            )
            .await
    }

    /// 通用查询产物，支持完整查询条件 + 分页。
    ///
    /// 注意：调用方需自行确保 project_id 权限校验。
    /// 如果 query 中提供了 project_id，会做项目访问权限校验。
    async fn query(
        &self,
        ctx: RequestContext,
        query: ArtifactQuery,
    ) -> Result<common::api::PagedResult<Artifact>> {
        if let Some(ref pid) = query.project_id {
            self.validate_project_access(ctx.clone(), pid).await?;
            if let Some(ref tid) = query.task_id {
                self.validate_project_and_task(ctx.clone(), pid, Some(tid.as_str()))
                    .await?;
            }
        }
        self.artifact_dal.query(ctx, query).await
    }

    /// 删除产物
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let Some(artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            bail_err!(NotFound, "Artifact not found: {}", id);
        };
        let ctx = enrich_ctx!(&ctx, &artifact);
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
        let ctx = enrich_ctx!(&ctx, &artifact);
        // Validate user has access to this artifact via project ownership
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;

        if artifact.po.source_type != common::enums::ArtifactSourceType::GeneratedContent {
            bail_err!(
                InvalidRequest,
                "Cannot read content directly from artifact source type {:?}, only GeneratedContent artifacts support direct content access.",
                artifact.po.source_type
            );
        }

        let _content = self.artifact_dal.read_content(ctx, &artifact).await?;
        // Content is handled by the handler separately, return artifact metadata only
        Ok(Some(artifact))
    }

    async fn read_content(&self, ctx: RequestContext, artifact: &Artifact) -> Result<Vec<u8>> {
        let ctx = enrich_ctx!(&ctx, artifact);
        // Validate user has access to this artifact via project ownership
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;

        if artifact.po.source_type != common::enums::ArtifactSourceType::GeneratedContent {
            bail_err!(
                InvalidRequest,
                "Cannot read content directly from artifact source type {:?}, only GeneratedContent artifacts support direct content access.",
                artifact.po.source_type
            );
        }

        self.artifact_dal
            .read_content(ctx, artifact)
            .await?
            .ok_or_else(|| -> common::error::Error {
                err!(NotFound, "Artifact content not found: {}", artifact.id())
            })
    }

    /// Update artifact content and/or metadata (partial update).
    async fn update_artifact(
        &self,
        ctx: RequestContext,
        id: &str,
        content: Option<Vec<u8>>,
        name: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
        expected_updated_at: Option<i64>,
    ) -> Result<Artifact> {
        let Some(mut artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            bail_err!(NotFound, "Artifact not found: {}", id);
        };
        let ctx = enrich_ctx!(&ctx, &artifact);
        // Validate user has access to this artifact via project ownership
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;

        // Optimistic locking check (applies to all updates)
        if let Some(expected) = expected_updated_at
            && artifact.po.updated_at != expected
        {
            bail_err!(
                Conflict,
                "Conflict: expected updated_at = {}, current updated_at = {}. Please reload and try again.",
                expected,
                artifact.po.updated_at
            );
        }

        // Update content if provided (only for GeneratedContent artifacts)
        if let Some(content_bytes) = content {
            if artifact.po.source_type != common::enums::ArtifactSourceType::GeneratedContent {
                bail_err!(
                    InvalidRequest,
                    "Cannot update content directly for artifact source type {:?}, only GeneratedContent artifacts support direct content update.",
                    artifact.po.source_type
                );
            }
            self.artifact_dal
                .write_content(ctx.clone(), &artifact, &content_bytes)
                .await?;
            artifact.po.file_meta.0.file_size = content_bytes.len() as u64;
        }

        // Update metadata if provided
        if let Some(new_name) = name {
            if new_name.trim().is_empty() {
                bail_err!(InvalidRequest, "name不能为空");
            }
            artifact.po.name = new_name;
        }
        if let Some(new_desc) = description {
            artifact.po.description = new_desc;
        }
        if let Some(new_tags) = tags {
            artifact.po.set_tags(new_tags, ctx.uid());
        }

        // Update timestamp and modifier
        let now = common::constants::utils::current_timestamp_ms();
        artifact.po.updated_at = now;
        artifact.po.modified_by = ctx.uid();

        self.artifact_dal.update(ctx, &artifact).await?;
        Ok(artifact)
    }

    /// Create a generated-content artifact with text content.
    #[allow(clippy::too_many_arguments)]
    async fn create_generated_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        content: Vec<u8>,
        file_name: String,
        mime_type: String,
        file_type: FileType,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact> {
        self.validate_project_and_task(ctx.clone(), &project_id, task_id.as_deref())
            .await?;

        let file_meta = FileMeta::new(file_name, mime_type, content.len() as u64);
        let mut artifact = if let Some(task_id) = task_id {
            Artifact::new_task_with_source_type(
                project_id,
                task_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        } else {
            Artifact::new_project_with_source_type(
                project_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        };
        artifact.po.set_tags(tags, created_by);
        let ctx = enrich_ctx!(&ctx, &artifact);

        self.artifact_dal.create(ctx.clone(), &artifact).await?;

        if let Err(e) = self
            .artifact_dal
            .write_content(ctx.clone(), &artifact, &content)
            .await
        {
            let _ = self.artifact_dal.delete(ctx, &artifact.po.id).await;
            return Err(e);
        }

        Ok(artifact)
    }

    /// Create a generated-content artifact by copying a file from a source path.
    #[allow(clippy::too_many_arguments)]
    async fn create_generated_artifact_from_file(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        source_path: std::path::PathBuf,
        file_name: String,
        mime_type: String,
        file_type: FileType,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact> {
        self.validate_project_and_task(ctx.clone(), &project_id, task_id.as_deref())
            .await?;

        let metadata = std::fs::metadata(&source_path)
            .map_err(|e| err!(InvalidRequest, "Failed to read source file metadata: {}", e))?;
        if !metadata.is_file() {
            bail_err!(
                InvalidRequest,
                "Source path is not a file: {:?}",
                source_path
            );
        }

        let file_meta = FileMeta::new(file_name.clone(), mime_type, metadata.len());
        let mut artifact = if let Some(task_id) = task_id {
            Artifact::new_task_with_source_type(
                project_id,
                task_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        } else {
            Artifact::new_project_with_source_type(
                project_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        };
        artifact.po.set_tags(tags, created_by);
        let ctx = enrich_ctx!(&ctx, &artifact);

        self.artifact_dal.create(ctx.clone(), &artifact).await?;

        let config = crate::config::get();
        let target_dir = config.artifact_path(&artifact.po.project_id, &artifact.po.id);
        let target_path = target_dir.join(&file_name);

        // Safety: target_path must be under artifacts_dir to prevent path traversal.
        if !target_path.starts_with(config.artifacts_dir()) {
            let _ = self.artifact_dal.delete(ctx, &artifact.po.id).await;
            bail_err!(
                InvalidRequest,
                "Invalid target path: path traversal attempt detected"
            );
        }

        if let Err(e) = std::fs::create_dir_all(&target_dir) {
            let _ = self.artifact_dal.delete(ctx, &artifact.po.id).await;
            return Err(err!(
                InvalidRequest,
                "Failed to create target directory: {}",
                e
            ));
        }

        if let Err(e) = std::fs::copy(&source_path, &target_path) {
            let _ = self.artifact_dal.delete(ctx, &artifact.po.id).await;
            return Err(err!(InvalidRequest, "Failed to copy file: {}", e));
        }

        Ok(artifact)
    }
}

// ==================== browser 工具截图产物存储器 ====================

/// browser 内置工具的截图产物存储器（pkg `ScreenshotStorer` 的 Domain 实现）
///
/// 复用 `create_generated_artifact_from_file`：截图拷贝入项目产物目录并落
/// GeneratedContent 产物记录（ctx 带 task_id 时挂任务级，否则项目级），
/// 由 `service::init` 注册给 pkg 层 browser 工具。
pub struct ProjectScreenshotStorer;

#[async_trait::async_trait]
impl crate::pkg::tool_registry::browser::ScreenshotStorer for ProjectScreenshotStorer {
    async fn store_screenshot(
        &self,
        ctx: RequestContext,
        source_path: std::path::PathBuf,
        file_name: String,
    ) -> Result<crate::pkg::tool_registry::browser::ScreenshotArtifact> {
        let Some(project_id) = ctx.project_id.clone() else {
            bail_err!(
                InvalidRequest,
                "当前上下文缺少项目上下文（project_id），无法归档截图产物"
            );
        };
        let artifact = super::domain()
            .artifact_manage()
            .create_generated_artifact_from_file(
                ctx.clone(),
                project_id,
                ctx.task_id.clone(),
                file_name.clone(),
                "browser 工具截图".to_string(),
                source_path,
                file_name,
                "image/png".to_string(),
                FileType::Image,
                vec!["browser".to_string(), "screenshot".to_string()],
                ctx.uid().to_string(),
            )
            .await?;
        Ok(crate::pkg::tool_registry::browser::ScreenshotArtifact {
            artifact_id: artifact.po.id.clone(),
            name: artifact.po.name.clone(),
        })
    }
}

// ==================== mark_artifact 工具产物注册器 ====================

/// mark_artifact 内置工具的产物注册器（pkg `ArtifactRegistrar` 的 Domain 实现）
///
/// 复用 `create_generated_artifact_from_file`：把工具运行日志复制晋升入项目
/// 产物目录并落 GeneratedContent 产物记录（带 tool-output 标签，可治理），
/// 由 `service::init` 注册给 pkg 层 mark_artifact 工具。
/// 与 ① 层运行日志生命周期解耦：TTL 清理不触碰产物副本。
pub struct ProjectToolOutputRegistrar;

#[async_trait::async_trait]
impl crate::pkg::tool_registry::mark_artifact::ArtifactRegistrar for ProjectToolOutputRegistrar {
    async fn register_tool_output(
        &self,
        ctx: RequestContext,
        call_id: String,
        log_path: std::path::PathBuf,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
    ) -> Result<crate::pkg::tool_registry::mark_artifact::ToolOutputArtifact> {
        let file_name = format!("tool-output-{}.log", call_id);
        let artifact = super::domain()
            .artifact_manage()
            .create_generated_artifact_from_file(
                ctx.clone(),
                project_id,
                task_id,
                name,
                description,
                log_path,
                file_name,
                "text/plain".to_string(),
                FileType::Document,
                vec!["tool-output".to_string(), call_id.clone()],
                ctx.uid().to_string(),
            )
            .await?;
        Ok(
            crate::pkg::tool_registry::mark_artifact::ToolOutputArtifact {
                artifact_id: artifact.po.id.clone(),
                name: artifact.po.name.clone(),
            },
        )
    }
}

impl ProjectDomainImpl {
    async fn validate_project_access(&self, ctx: RequestContext, project_id: &str) -> Result<()> {
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
                bail_err!(
                    InvalidRequest,
                    "Task {} does not belong to project {}",
                    task_id,
                    project_id
                );
            }
        }

        Ok(())
    }
}
