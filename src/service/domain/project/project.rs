//! Project 核心业务
//!
//! 负责项目的创建、查询、状态流转

use crate::models::project::Project;
use crate::pkg::RequestContext;
use crate::pkg::stats::ProjectEvent;
use common::constants::utils;
use common::enums::project::ProjectStatus;
use uuid::Uuid;

use super::ProjectDomainImpl;
use common::error::{Result, err, bail_err};

use crate::enrich_ctx;
use crate::record_event;

#[async_trait::async_trait]
impl super::ProjectManage for ProjectDomainImpl {
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
    ) -> Result<Project> {
        let project_id = Uuid::now_v7().to_string();

        let project = Project::new(
            project_id,
            name,
            description,
            None, // workflow
            None, // guidance
            priority,
            tags,
            root_user_id,
            None, // owner_agent_id
            None, // start_at
            None, // due_at
            None, // end_at
            created_by.clone(),
        );

        self.project_dal.create(ctx.clone(), &project).await?;

        let _ = record_event!(ctx.clone(), ProjectEvent {
            project_id: project.po.id.clone(),
            event_type: "created".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(project.po.root_user_id.clone()),
            owner_type: project.po.owner_agent_id.as_ref().map(|_| "agent".to_string()),
            owner_id: project.po.owner_agent_id.clone(),
            from_status: None,
            to_status: Some(format!("{:?}", project.po.status)),
            duration_ms: None,
            priority: project.po.priority,
        });

        Ok(project)
    }

    /// 根据 ID 获取项目
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>> {
        self.project_dal.find_by_id(ctx, id).await
    }

    /// 获取用户的所有项目
    async fn list_by_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<Vec<Project>> {
        self.project_dal
            .list_by_root_user(ctx, root_user_id, None)
            .await
    }

    /// 查询用户项目列表
    async fn list(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Option<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Project>> {
        if let Some(status) = status {
            self.project_dal
                .list_by_root_user_and_status(ctx, root_user_id, vec![status], limit)
                .await
        } else {
            self.project_dal
                .list_by_root_user(ctx, root_user_id, limit)
                .await
        }
    }

    /// 启动项目
    async fn start(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<()> {
        let Some(mut project) = self.project_dal.find_by_id(ctx.clone(), project_id).await? else {
            bail_err!(NotFound, "Project not found: {}", project_id);
        };

        // 补充 Project 上下文到 ctx
        let ctx = enrich_ctx!(&ctx, &project);

        let from_status = format!("{:?}", project.po.status);
        project.start();
        project.po.modified_by = modified_by;
        self.project_dal.update(ctx.clone(), &project).await?;

        let _ = record_event!(ctx.clone(), ProjectEvent {
            project_id: project.po.id.clone(),
            event_type: "started".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(project.po.root_user_id.clone()),
            owner_type: project.po.owner_agent_id.as_ref().map(|_| "agent".to_string()),
            owner_id: project.po.owner_agent_id.clone(),
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", project.po.status)),
            duration_ms: None,
            priority: project.po.priority,
        });

        Ok(())
    }

    /// 完成项目
    async fn complete(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<()> {
        let Some(mut project) = self.project_dal.find_by_id(ctx.clone(), project_id).await? else {
            bail_err!(NotFound, "Project not found: {}", project_id);
        };

        let ctx = enrich_ctx!(&ctx, &project);

        let from_status = format!("{:?}", project.po.status);
        project.complete();
        project.po.modified_by = modified_by;
        self.project_dal.update(ctx.clone(), &project).await?;

        let _ = record_event!(ctx.clone(), ProjectEvent {
            project_id: project.po.id.clone(),
            event_type: "completed".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(project.po.root_user_id.clone()),
            owner_type: project.po.owner_agent_id.as_ref().map(|_| "agent".to_string()),
            owner_id: project.po.owner_agent_id.clone(),
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", project.po.status)),
            duration_ms: None,
            priority: project.po.priority,
        });

        Ok(())
    }

    /// 归档项目
    async fn archive(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<()> {
        let Some(mut project) = self.project_dal.find_by_id(ctx.clone(), project_id).await? else {
            bail_err!(NotFound, "Project not found: {}", project_id);
        };

        let ctx = enrich_ctx!(&ctx, &project);

        let from_status = format!("{:?}", project.po.status);
        project.po.status = ProjectStatus::Archived;
        project.po.modified_by = modified_by;
        self.project_dal.update(ctx.clone(), &project).await?;

        let _ = record_event!(ctx.clone(), ProjectEvent {
            project_id: project.po.id.clone(),
            event_type: "archived".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(project.po.root_user_id.clone()),
            owner_type: project.po.owner_agent_id.as_ref().map(|_| "agent".to_string()),
            owner_id: project.po.owner_agent_id.clone(),
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", project.po.status)),
            duration_ms: None,
            priority: project.po.priority,
        });

        Ok(())
    }

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
    ) -> Result<Project> {
        let Some(mut project) = self.project_dal.find_by_id(ctx.clone(), project_id).await? else {
            bail_err!(NotFound, "Project not found: {}", project_id);
        };

        let ctx = enrich_ctx!(&ctx, &project);

        if let Some(name) = name {
            project.po.name = name;
        }
        if let Some(description) = description {
            project.po.description = description;
        }
        if let Some(priority) = priority {
            project.po.priority = priority;
        }
        if let Some(tags) = tags {
            project.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        }
        project.po.modified_by = modified_by;

        self.project_dal.update(ctx, &project).await?;
        Ok(project)
    }

    /// 统一项目状态流转
    async fn transition_status(
        &self,
        ctx: RequestContext,
        project: &mut Project,
        target_status: ProjectStatus,
    ) -> Result<()> {
        // 补充 Project 上下文
        let ctx = enrich_ctx!(&ctx, &*project);

        let current_status = project.po.status;

        if target_status == ProjectStatus::Deleted {
            bail_err!(InvalidRequest, "Project 删除不允许通过状态接口执行，请使用删除/归档 action");
        }

        let is_valid_transition = match (current_status, target_status) {
            (a, b) if a == b => true,
            (ProjectStatus::Active, ProjectStatus::PendingReview) => true,
            (ProjectStatus::Active, ProjectStatus::InProgress) => true,
            (ProjectStatus::PendingReview, ProjectStatus::Active) => true,
            (ProjectStatus::PendingReview, ProjectStatus::InProgress) => true,
            (ProjectStatus::InProgress, ProjectStatus::Completed) => true,
            (ProjectStatus::Completed, ProjectStatus::Archived) => true,
            (ProjectStatus::Active, ProjectStatus::Archived) => true,
            (ProjectStatus::PendingReview, ProjectStatus::Archived) => true,
            (ProjectStatus::InProgress, ProjectStatus::Archived) => true,
            _ => false,
        };

        if !is_valid_transition {
            bail_err!(InvalidRequest, "非法项目状态流转：{:?} → {:?}", current_status, target_status);
        }

        if current_status == target_status {
            return Ok(());
        }

        let from_status = format!("{:?}", current_status);

        match target_status {
            ProjectStatus::InProgress => {
                project.po.status = ProjectStatus::InProgress;
                if project.po.start_at.is_none() {
                    project.po.start_at = Some(utils::current_timestamp());
                }
            }
            ProjectStatus::Completed => {
                project.po.status = ProjectStatus::Completed;
                project.po.end_at = Some(utils::current_timestamp());
            }
            ProjectStatus::Archived | ProjectStatus::Active | ProjectStatus::PendingReview => {
                project.po.status = target_status;
            }
            ProjectStatus::Deleted => unreachable!("Deleted rejected above"),
        }
        project.po.modified_by = ctx.uid();

        self.project_dal.update(ctx.clone(), project).await?;

        let _ = record_event!(ctx.clone(), ProjectEvent {
            project_id: project.po.id.clone(),
            event_type: "status_changed".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(project.po.root_user_id.clone()),
            owner_type: project.po.owner_agent_id.as_ref().map(|_| "agent".to_string()),
            owner_id: project.po.owner_agent_id.clone(),
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", project.po.status)),
            duration_ms: None,
            priority: project.po.priority,
        });

        Ok(())
    }
}