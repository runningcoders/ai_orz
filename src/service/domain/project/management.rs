//! 项目管理实现

use super::{CreateProjectCommand, ProjectManagement, UpdateProjectCommand};
use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use async_trait::async_trait;
use common::constants::utils;
use common::enums::ProjectStatus;
use uuid::Uuid;

/// ProjectManagement trait 实现
///
/// 在 ProjectDomainImpl 上实现 ProjectManagement trait
#[async_trait::async_trait]
impl ProjectManagement for super::ProjectDomainImpl {
    async fn create_project(
        &self,
        ctx: RequestContext,
        cmd: CreateProjectCommand<'_>,
    ) -> Result<ProjectPo, AppError> {
        let project_id = Uuid::now_v7().to_string();
        let project = ProjectPo::new(
            project_id,
            cmd.name.to_string(),
            cmd.description.to_string(),
            cmd.workflow.map(|s| s.to_string()),
            cmd.guidance.map(|s| s.to_string()),
            cmd.priority,
            cmd.tags,
            cmd.root_user_id.to_string(),
            cmd.owner_agent_id.map(|s| s.to_string()),
            cmd.start_at,
            cmd.due_at,
            None,
            ctx.uid().to_string(),
        );

        self.project_dal.create(ctx, &project).await?;
        Ok(project)
    }

    async fn get_project_by_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Option<ProjectPo>, AppError> {
        self.project_dal.find_by_id(ctx, project_id).await
    }

    async fn list_user_projects(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError> {
        self.project_dal
            .list_by_root_user(ctx, root_user_id, limit)
            .await
    }

    async fn list_user_projects_by_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>, AppError> {
        self.project_dal
            .list_by_root_user_and_status(ctx, root_user_id, status, limit)
            .await
    }

    async fn query_projects(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<Vec<ProjectPo>, AppError> {
        self.project_dal.query(ctx, query).await
    }

    async fn update_project(
        &self,
        ctx: RequestContext,
        cmd: UpdateProjectCommand<'_>,
    ) -> Result<ProjectPo, AppError> {
        // 先获取现有项目
        let mut project = self
            .project_dal
            .find_by_id(ctx.clone(), cmd.project_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project not found: {}", cmd.project_id)))?;

        // 更新字段
        if let Some(name) = cmd.name {
            project.name = name.to_string();
        }
        if let Some(description) = cmd.description {
            project.description = description.to_string();
        }
        if let Some(workflow) = cmd.workflow {
            project.workflow = Some(workflow.to_string());
        }
        if let Some(guidance) = cmd.guidance {
            project.guidance = Some(guidance.to_string());
        }
        if let Some(priority) = cmd.priority {
            project.priority = priority;
        }
        if let Some(tags) = cmd.tags {
            project.tags = serde_json::to_string(&tags).unwrap_or_default();
        }
        if let Some(owner_agent_id) = cmd.owner_agent_id {
            project.owner_agent_id = Some(owner_agent_id.to_string());
        }
        if cmd.start_at.is_some() {
            project.start_at = cmd.start_at;
        }
        if cmd.due_at.is_some() {
            project.due_at = cmd.due_at;
        }

        // 更新修改人和时间
        project.modified_by = ctx.uid().to_string();
        project.updated_at = utils::current_timestamp();

        // 保存更新
        self.project_dal.update(ctx, &project).await?;
        Ok(project)
    }

    async fn update_project_status(
        &self,
        ctx: RequestContext,
        project_id: &str,
        status: ProjectStatus,
    ) -> Result<(), AppError> {
        let uid = ctx.uid();
        self.project_dal
            .update_status(ctx, project_id, status, uid.as_str())
            .await
    }

    async fn archive_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError> {
        let uid = ctx.uid();
        self.project_dal.archive(ctx, project_id, uid.as_str()).await
    }

    async fn count_user_projects(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64, AppError> {
        self.project_dal.count_by_root_user(ctx, root_user_id).await
    }

    async fn count_user_projects_by_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64, AppError> {
        self.project_dal
            .count_by_root_user_and_status(ctx, root_user_id, status)
            .await
    }
}
