//! Project 核心业务
//!
//! 负责项目的创建、查询、状态流转

use crate::models::project::Project;
use crate::models::project::progress_summary_from_tasks;
use crate::pkg::RequestContext;
use crate::pkg::stats::ProjectEvent;
use crate::pkg::utils::graph::MermaidDirection;
use common::constants::utils;
use common::enums::project::ProjectStatus;
use uuid::Uuid;

use super::ProjectDomainImpl;
use super::task_graph::build_task_graph_mermaid;
use common::error::{Result, bail_err};

use crate::enrich_ctx;
use crate::record_event;

#[async_trait::async_trait]
impl super::ProjectManage for ProjectDomainImpl {
    /// 创建新项目
    ///
    /// `owner_agent_id` 由上层（handler）按需组合传入，Project domain 只做纯粹持久化。
    async fn create(
        &self,
        ctx: RequestContext,
        name: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        owner_agent_id: Option<String>,
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
            owner_agent_id, // 由上层 handler 透传
            None,           // start_at
            None,           // due_at
            None,           // end_at
            created_by.clone(),
        );

        self.project_dal.create(ctx.clone(), &project).await?;

        let _ = record_event!(
            ctx.clone(),
            ProjectEvent {
                project_id: project.po.id.clone(),
                event_type: "created".to_string(),
                organization_id: ctx.organization_id.clone(),
                operator_type: Some(ctx.caller_type().as_str().to_string()),
                operator_id: ctx.caller_id(),
                root_user_id: Some(project.po.root_user_id.clone()),
                owner_type: project
                    .po
                    .owner_agent_id
                    .as_ref()
                    .map(|_| "agent".to_string()),
                owner_id: project.po.owner_agent_id.clone(),
                from_status: None,
                to_status: Some(format!("{:?}", project.po.status)),
                duration_ms: None,
                priority: project.po.priority,
            }
        );

        Ok(project)
    }

    /// 根据 ID 获取项目
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>> {
        self.project_dal.find_by_id(ctx, id).await
    }

    /// 根据 ID 获取项目（带附带信息选项）
    ///
    /// Domain 层聚合：在 DAL 返回基础 Project 后，按 options 注入：
    /// - task_graph: 调用 task_dal 查询项目任务，用 graph 组件生成 mermaid
    /// - artifacts: 调用 artifact_dal 查询项目级产物列表
    /// - progress_summary: 调用 task_dal 查询项目任务，实时聚合进度汇总
    async fn get_project(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::project::ProjectFetchOptions,
    ) -> Result<Option<Project>> {
        // 先调 DAL 拿基础 project（含 stats / model_call_stats）
        let mut project = self
            .project_dal
            .get_project(ctx.clone(), id, options.clone())
            .await?;

        if let Some(project) = project.as_mut() {
            // 注入 task_graph
            if options.with_task_graph.unwrap_or(false) {
                let tasks = self.task_dal.list_by_project(ctx.clone(), id, None).await?;
                let mermaid = build_task_graph_mermaid(&tasks, MermaidDirection::LR);
                project.task_graph = Some(mermaid);
            }

            // 注入 artifacts
            if options.with_artifacts.unwrap_or(false) {
                let artifacts = self.artifact_dal.list_by_project(ctx.clone(), id).await?;
                project.artifacts = Some(
                    artifacts
                        .iter()
                        .map(super::artifact_to_detail)
                        .collect::<Vec<_>>(),
                );
            }

            // 注入 progress_summary（实时按任务状态聚合）
            if options.with_progress_summary.unwrap_or(false) {
                let tasks = self.task_dal.list_by_project(ctx.clone(), id, None).await?;
                project.progress_summary = Some(progress_summary_from_tasks(&tasks));
            }
        }

        Ok(project)
    }

    /// 获取用户的所有项目
    async fn list_by_user(&self, ctx: RequestContext, root_user_id: &str) -> Result<Vec<Project>> {
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

    /// 查询所有进行中且有 Owner Agent 的项目
    ///
    /// 用于系统级调度（如 Agent Loop Engine）：忽略 root_user_id 过滤，
    /// 查询所有 InProgress 状态的项目，并仅保留 owner_agent_id 不为空的记录。
    async fn list_in_progress_with_owner(&self, ctx: RequestContext) -> Result<Vec<Project>> {
        // 系统级查询：查询所有 InProgress 项目（不限 root_user_id）
        let projects = self
            .project_dal
            .list_all_by_status(ctx, ProjectStatus::InProgress, None)
            .await?;

        // 过滤出有 owner_agent_id 的项目
        Ok(projects
            .into_iter()
            .filter(|p| p.po.owner_agent_id.is_some())
            .collect())
    }

    /// 通用查询（核心方法）
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::project::ProjectQuery,
    ) -> Result<common::api::PagedResult<Project>> {
        self.project_dal.query(ctx, query).await
    }

    /// 搜索 Project（关键词 + 向量语义混合搜索）
    ///
    /// 返回分页结果，支持完整过滤条件。
    /// 与 query 的区别：search 重在"语义相关性"（FTS5 + 向量语义混合搜索），
    /// query 重在"条件过滤"。两者现在都支持完整过滤条件和分页返回。
    async fn search(
        &self,
        ctx: RequestContext,
        search: crate::service::dao::project::ProjectSearch,
    ) -> Result<common::api::PagedResult<Project>> {
        self.project_dal.search(ctx, search).await
    }

    /// 统计符合查询条件的项目数量（透传 DAL count）
    async fn count_projects(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::project::ProjectQuery,
    ) -> Result<u64> {
        self.project_dal.count(ctx, query).await
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

        let _ = record_event!(
            ctx.clone(),
            ProjectEvent {
                project_id: project.po.id.clone(),
                event_type: "started".to_string(),
                organization_id: ctx.organization_id.clone(),
                operator_type: Some(ctx.caller_type().as_str().to_string()),
                operator_id: ctx.caller_id(),
                root_user_id: Some(project.po.root_user_id.clone()),
                owner_type: project
                    .po
                    .owner_agent_id
                    .as_ref()
                    .map(|_| "agent".to_string()),
                owner_id: project.po.owner_agent_id.clone(),
                from_status: Some(from_status),
                to_status: Some(format!("{:?}", project.po.status)),
                duration_ms: None,
                priority: project.po.priority,
            }
        );

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

        let _ = record_event!(
            ctx.clone(),
            ProjectEvent {
                project_id: project.po.id.clone(),
                event_type: "completed".to_string(),
                organization_id: ctx.organization_id.clone(),
                operator_type: Some(ctx.caller_type().as_str().to_string()),
                operator_id: ctx.caller_id(),
                root_user_id: Some(project.po.root_user_id.clone()),
                owner_type: project
                    .po
                    .owner_agent_id
                    .as_ref()
                    .map(|_| "agent".to_string()),
                owner_id: project.po.owner_agent_id.clone(),
                from_status: Some(from_status),
                to_status: Some(format!("{:?}", project.po.status)),
                duration_ms: None,
                priority: project.po.priority,
            }
        );

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

        let _ = record_event!(
            ctx.clone(),
            ProjectEvent {
                project_id: project.po.id.clone(),
                event_type: "archived".to_string(),
                organization_id: ctx.organization_id.clone(),
                operator_type: Some(ctx.caller_type().as_str().to_string()),
                operator_id: ctx.caller_id(),
                root_user_id: Some(project.po.root_user_id.clone()),
                owner_type: project
                    .po
                    .owner_agent_id
                    .as_ref()
                    .map(|_| "agent".to_string()),
                owner_id: project.po.owner_agent_id.clone(),
                from_status: Some(from_status),
                to_status: Some(format!("{:?}", project.po.status)),
                duration_ms: None,
                priority: project.po.priority,
            }
        );

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
        execution_plan: Option<String>,
        execution_result: Option<String>,
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
        if let Some(execution_plan) = execution_plan {
            project.po.execution_plan = Some(execution_plan);
        }
        if let Some(execution_result) = execution_result {
            project.po.execution_result = Some(execution_result);
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
            bail_err!(
                InvalidRequest,
                "Project 删除不允许通过状态接口执行，请使用删除/归档 action"
            );
        }

        let is_valid_transition = match (current_status, target_status) {
            (a, b) if a == b => true,
            (ProjectStatus::Active, ProjectStatus::PendingReview) => true,
            (ProjectStatus::Active, ProjectStatus::InProgress) => true,
            (ProjectStatus::PendingReview, ProjectStatus::Active) => true,
            (ProjectStatus::PendingReview, ProjectStatus::InProgress) => true,
            (ProjectStatus::InProgress, ProjectStatus::Completed) => true,
            // 项目重启：Completed 可回到 InProgress，支持 Project Owner 重新规划新任务
            (ProjectStatus::Completed, ProjectStatus::InProgress) => true,
            (ProjectStatus::Completed, ProjectStatus::Archived) => true,
            (ProjectStatus::Active, ProjectStatus::Archived) => true,
            (ProjectStatus::PendingReview, ProjectStatus::Archived) => true,
            (ProjectStatus::InProgress, ProjectStatus::Archived) => true,
            _ => false,
        };

        if !is_valid_transition {
            bail_err!(
                InvalidRequest,
                "非法项目状态流转：{:?} → {:?}",
                current_status,
                target_status
            );
        }

        if current_status == target_status {
            return Ok(());
        }

        let from_status = format!("{:?}", current_status);

        match target_status {
            ProjectStatus::InProgress => {
                project.po.status = ProjectStatus::InProgress;
                if project.po.start_at.is_none() {
                    project.po.start_at = Some(utils::current_timestamp_ms());
                }
            }
            ProjectStatus::Completed => {
                project.po.status = ProjectStatus::Completed;
                project.po.end_at = Some(utils::current_timestamp_ms());
            }
            ProjectStatus::Archived | ProjectStatus::Active | ProjectStatus::PendingReview => {
                project.po.status = target_status;
            }
            ProjectStatus::Deleted => unreachable!("Deleted rejected above"),
        }
        project.po.modified_by = ctx.uid();

        self.project_dal.update(ctx.clone(), project).await?;

        let _ = record_event!(
            ctx.clone(),
            ProjectEvent {
                project_id: project.po.id.clone(),
                event_type: "status_changed".to_string(),
                organization_id: ctx.organization_id.clone(),
                operator_type: Some(ctx.caller_type().as_str().to_string()),
                operator_id: ctx.caller_id(),
                root_user_id: Some(project.po.root_user_id.clone()),
                owner_type: project
                    .po
                    .owner_agent_id
                    .as_ref()
                    .map(|_| "agent".to_string()),
                owner_id: project.po.owner_agent_id.clone(),
                from_status: Some(from_status),
                to_status: Some(format!("{:?}", project.po.status)),
                duration_ms: None,
                priority: project.po.priority,
            }
        );

        Ok(())
    }
}
