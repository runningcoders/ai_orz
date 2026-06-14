//! Task 核心业务
//!
//! 负责任务的创建、查询、状态流转

use crate::error::AppError;
use crate::models::task::Task;
use crate::pkg::RequestContext;
use common::enums::AssigneeType;
use std::sync::Arc;
use uuid::Uuid;

/// Task 业务领域
#[derive(Clone)]
pub struct TaskDomain {
    dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>,
}

impl TaskDomain {
    /// 创建 TaskDomain 实例
    pub fn new(dal: Arc<dyn crate::service::dal::task::TaskDal + Send + Sync>) -> Self {
        Self { dal }
    }

    /// 创建新任务
    pub async fn create(
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
    ) -> Result<Task, AppError> {
        let task_id = Uuid::now_v7().to_string();

        let task = Task::new(
            task_id,
            title,
            description,
            priority,
            tags,
            None,       // due_at
            None,       // start_at
            None,       // end_at
            Vec::new(), // dependencies
            root_user_id,
            assignee_type,
            assignee_id,
            project_id,
            created_by.clone(),
        );

        self.dal.create(ctx.clone(), &task).await?;
        Ok(task)
    }

    /// 根据 ID 获取任务
    pub async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Task>, AppError> {
        self.dal.find_by_id(ctx, id).await
    }

    /// 获取项目下的所有任务
    pub async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Task>, AppError> {
        self.dal.list_by_project(ctx, project_id, None).await
    }

    /// 获取分配给 Agent 的所有任务
    pub async fn list_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Task>, AppError> {
        let list = self
            .dal
            .list_by_assignee(ctx.clone(), Some(AssigneeType::Agent), agent_id, None)
            .await?;
        Ok(list.into_iter().map(Task::from_po).collect())
    }

    /// 开始任务
    pub async fn start(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<(), AppError> {
        let Some(mut task) = self.dal.find_by_id(ctx.clone(), task_id).await? else {
            return Err(AppError::NotFound(format!("Task not found: {}", task_id)));
        };
        task.start();
        task.po.modified_by = modified_by;
        self.dal.update(ctx, &task).await?;
        Ok(())
    }

    /// 完成任务
    pub async fn complete(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<(), AppError> {
        let Some(mut task) = self.dal.find_by_id(ctx.clone(), task_id).await? else {
            return Err(AppError::NotFound(format!("Task not found: {}", task_id)));
        };
        task.complete();
        task.po.modified_by = modified_by;
        self.dal.update(ctx, &task).await?;
        Ok(())
    }

    /// 取消任务
    pub async fn cancel(
        &self,
        ctx: RequestContext,
        task_id: &str,
        modified_by: String,
    ) -> Result<(), AppError> {
        let Some(mut task) = self.dal.find_by_id(ctx.clone(), task_id).await? else {
            return Err(AppError::NotFound(format!("Task not found: {}", task_id)));
        };
        task.cancel();
        task.po.modified_by = modified_by;
        self.dal.update(ctx, &task).await?;
        Ok(())
    }
}
