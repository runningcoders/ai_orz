//! Task 核心业务
//!
//! 负责任务的创建、查询、状态流转

use common::error::{err, Result, bail_err};
use crate::models::task::Task;
use crate::pkg::RequestContext;
use crate::pkg::stats::TaskEvent;
use common::constants::utils;
use common::enums::{AssigneeType, TaskStatus};
use uuid::Uuid;

use super::ProjectDomainImpl;

use crate::enrich_ctx;
use crate::record_event;

#[async_trait::async_trait]
impl super::TaskManage for ProjectDomainImpl {
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
    ) -> Result<Task> {
        self.create_with_options(
            ctx,
            title,
            description,
            priority,
            tags,
            root_user_id,
            assignee_type,
            assignee_id,
            project_id,
            None,
            Vec::new(),
            created_by,
        )
        .await
    }

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
    ) -> Result<Task> {
        let task_id = Uuid::now_v7().to_string();

        let task = Task::new(
            task_id,
            title,
            description,
            priority,
            tags,
            due_at,
            None, // start_at
            None, // end_at
            dependencies,
            root_user_id,
            assignee_type,
            assignee_id,
            project_id,
            created_by,
        );

        self.task_dal.create(ctx.clone(), &task).await?;

        let _ = record_event!(ctx.clone(), TaskEvent {
            task_id: task.po.id.clone(),
            project_id: task.po.project_id.clone(),
            event_type: "created".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(task.po.root_user_id.clone()),
            assignee_type: Some(format!("{:?}", task.po.assignee_type)),
            assignee_id: Some(task.po.assignee_id.clone()),
            from_assignee_id: None,
            from_status: None,
            to_status: Some(format!("{:?}", task.po.status)),
            duration_ms: None,
            priority: task.po.priority,
        });

        Ok(task)
    }

    /// 根据 ID 获取任务
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>> {
        self.task_dal.find_by_id(ctx, id).await
    }

    /// 根据 ID 获取任务（带附带信息选项）
    async fn get_task(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::task::TaskFetchOptions,
    ) -> Result<Option<Task>> {
        self.task_dal.get_task(ctx, id, options).await
    }

    /// 获取项目下的所有任务
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Task>> {
        let ctx = ctx.to_builder().project_id(project_id).build();
        self.task_dal.list_by_project(ctx, project_id, None).await
    }

    /// 获取分配给 Agent 的所有任务
    async fn list_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Task>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.list(
            ctx,
            None,
            Some(AssigneeType::Agent),
            Some(agent_id),
            None,
            None,
        )
        .await
    }

    /// 查询任务列表
    async fn list(
        &self,
        ctx: RequestContext,
        project_id: Option<&str>,
        assignee_type: Option<AssigneeType>,
        assignee_id: Option<&str>,
        status: Option<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Task>> {
        let page = self
            .task_dal
            .query(
                ctx,
                crate::service::dao::task::TaskQuery {
                    assignee_type,
                    assignee_id: assignee_id.map(str::to_string),
                    project_id: project_id.map(str::to_string),
                    status_in: status.map(|status| vec![status]),
                    pagination: common::api::PaginationParams { limit, offset: None },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    /// 通用查询（核心方法）
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::task::TaskQuery,
    ) -> Result<common::api::PagedResult<Task>> {
        self.task_dal.query(ctx, query).await
    }

    /// 统计符合查询条件的任务数量（透传 DAL count）
    async fn count_tasks(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::task::TaskQuery,
    ) -> Result<u64> {
        self.task_dal.count(ctx, query).await
    }

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
    ) -> Result<Task> {
        let Some(mut task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            bail_err!(NotFound, "Task not found: {}", task_id);
        };

        let ctx = enrich_ctx!(&ctx, &task);

        if let Some(title) = title {
            task.po.title = title;
        }
        if let Some(description) = description {
            task.po.description = description;
        }
        if let Some(priority) = priority {
            task.po.priority = priority;
        }
        if let Some(tags) = tags {
            task.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        }
        if due_at.is_some() {
            task.po.due_at = due_at;
        }
        if let Some(dependencies) = dependencies {
            task.po.dependencies = if dependencies.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&dependencies).unwrap_or_else(|_| "[]".to_string()))
            };
        }
        task.po.modified_by = ctx.uid();

        self.task_dal.update(ctx, &task).await?;
        Ok(task)
    }

    /// 开始任务
    async fn start(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<()> {
        let Some(mut task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            bail_err!(NotFound, "Task not found: {}", task_id);
        };

        let ctx = enrich_ctx!(&ctx, &task);

        let from_status = format!("{:?}", task.po.status);
        task.start();
        task.po.modified_by = modified_by;
        self.task_dal.update(ctx.clone(), &task).await?;

        let _ = record_event!(ctx.clone(), TaskEvent {
            task_id: task.po.id.clone(),
            project_id: task.po.project_id.clone(),
            event_type: "started".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(task.po.root_user_id.clone()),
            assignee_type: Some(format!("{:?}", task.po.assignee_type)),
            assignee_id: Some(task.po.assignee_id.clone()),
            from_assignee_id: None,
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", task.po.status)),
            duration_ms: None,
            priority: task.po.priority,
        });

        Ok(())
    }

    /// 完成任务
    async fn complete(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<()> {
        let Some(mut task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            bail_err!(NotFound, "Task not found: {}", task_id);
        };

        let ctx = enrich_ctx!(&ctx, &task);

        let from_status = format!("{:?}", task.po.status);
        task.complete();
        task.po.modified_by = modified_by;
        self.task_dal.update(ctx.clone(), &task).await?;

        let _ = record_event!(ctx.clone(), TaskEvent {
            task_id: task.po.id.clone(),
            project_id: task.po.project_id.clone(),
            event_type: "completed".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(task.po.root_user_id.clone()),
            assignee_type: Some(format!("{:?}", task.po.assignee_type)),
            assignee_id: Some(task.po.assignee_id.clone()),
            from_assignee_id: None,
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", task.po.status)),
            duration_ms: None,
            priority: task.po.priority,
        });

        Ok(())
    }

    /// 取消任务
    async fn cancel(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<()> {
        let Some(mut task) = self.task_dal.find_by_id(ctx.clone(), task_id).await? else {
            bail_err!(NotFound, "Task not found: {}", task_id);
        };

        let ctx = enrich_ctx!(&ctx, &task);

        let from_status = format!("{:?}", task.po.status);
        task.cancel();
        task.po.modified_by = modified_by;
        self.task_dal.update(ctx.clone(), &task).await?;

        let _ = record_event!(ctx.clone(), TaskEvent {
            task_id: task.po.id.clone(),
            project_id: task.po.project_id.clone(),
            event_type: "cancelled".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(task.po.root_user_id.clone()),
            assignee_type: Some(format!("{:?}", task.po.assignee_type)),
            assignee_id: Some(task.po.assignee_id.clone()),
            from_assignee_id: None,
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", task.po.status)),
            duration_ms: None,
            priority: task.po.priority,
        });

        Ok(())
    }

    /// 统一任务状态流转
    async fn transition_status(
        &self,
        ctx: RequestContext,
        task: &mut Task,
        target_status: TaskStatus,
    ) -> Result<()> {
        // 补充 Task 上下文
        let ctx = enrich_ctx!(&ctx, &*task);

        let current_status = task.po.status;

        if target_status == TaskStatus::Cancelled {
            bail_err!(InvalidRequest, "Task 取消不允许通过状态接口执行，请使用取消/删除 action");
        }

        let is_valid_transition = match (current_status, target_status) {
            (a, b) if a == b => true,
            (TaskStatus::PendingReview, TaskStatus::Pending) => true,
            (TaskStatus::PendingReview, TaskStatus::InProgress) => true,
            (TaskStatus::PendingReview, TaskStatus::Archived) => true,
            (TaskStatus::Pending, TaskStatus::InProgress) => true,
            (TaskStatus::Pending, TaskStatus::Archived) => true,
            (TaskStatus::InProgress, TaskStatus::Completed) => true,
            (TaskStatus::InProgress, TaskStatus::Archived) => true,
            (TaskStatus::Completed, TaskStatus::Archived) => true,
            _ => false,
        };

        if !is_valid_transition {
            bail_err!(InvalidRequest, "非法任务状态流转：{:?} → {:?}", current_status, target_status);
        }

        if current_status == target_status {
            return Ok(());
        }

        let from_status = format!("{:?}", current_status);

        match target_status {
            TaskStatus::InProgress => {
                task.po.status = TaskStatus::InProgress;
                if task.po.start_at.is_none() {
                    task.po.start_at = Some(utils::current_timestamp());
                }
            }
            TaskStatus::Completed => {
                task.po.status = TaskStatus::Completed;
                task.po.end_at = Some(utils::current_timestamp());
            }
            TaskStatus::PendingReview | TaskStatus::Pending | TaskStatus::Archived => {
                task.po.status = target_status;
            }
            TaskStatus::Cancelled => unreachable!("Cancelled rejected above"),
        }
        task.po.modified_by = ctx.uid();

        self.task_dal.update(ctx.clone(), task).await?;

        let _ = record_event!(ctx.clone(), TaskEvent {
            task_id: task.po.id.clone(),
            project_id: task.po.project_id.clone(),
            event_type: "status_changed".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(task.po.root_user_id.clone()),
            assignee_type: Some(format!("{:?}", task.po.assignee_type)),
            assignee_id: Some(task.po.assignee_id.clone()),
            from_assignee_id: None,
            from_status: Some(from_status),
            to_status: Some(format!("{:?}", task.po.status)),
            duration_ms: None,
            priority: task.po.priority,
        });

        Ok(())
    }

    async fn update_progress(
        &self,
        ctx: RequestContext,
        task_id: &str,
        progress: i32,
    ) -> Result<Task> {
        let mut task = self
            .task_dal
            .find_by_id(ctx.clone(), task_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Task not found: {}", task_id))?;

        let ctx = enrich_ctx!(&ctx, &task);

        task.set_progress(progress);
        task.po.modified_by = ctx.uid();

        self.task_dal.update(ctx.clone(), &task).await?;

        let _ = record_event!(ctx.clone(), TaskEvent {
            task_id: task.po.id.clone(),
            project_id: task.po.project_id.clone(),
            event_type: "progress_updated".to_string(),
            organization_id: ctx.organization_id.clone(),
            operator_type: Some(if ctx.agent_id().is_some() { "agent".to_string() } else { "user".to_string() }),
            operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
            root_user_id: Some(task.po.root_user_id.clone()),
            assignee_type: Some(format!("{:?}", task.po.assignee_type)),
            assignee_id: Some(task.po.assignee_id.clone()),
            from_assignee_id: None,
            from_status: None,
            to_status: None,
            duration_ms: None,
            priority: task.po.priority,
        });

        Ok(task)
    }
}