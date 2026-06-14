//! Project 核心业务
//!
//! 负责项目的创建、查询、状态流转

use crate::error::AppError;
use crate::models::project::Project;
use crate::pkg::RequestContext;
use common::enums::project::ProjectStatus;
use std::sync::Arc;
use uuid::Uuid;

/// Project 业务领域
#[derive(Clone)]
pub struct ProjectDomain {
    dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>,
}

impl ProjectDomain {
    /// 创建 ProjectDomain 实例
    pub fn new(dal: Arc<dyn crate::service::dal::project::ProjectDal + Send + Sync>) -> Self {
        Self { dal }
    }

    /// 创建新项目
    pub async fn create(
        &self,
        ctx: RequestContext,
        name: String,
        description: String,
        priority: i32,
        tags: Vec<String>,
        root_user_id: String,
        created_by: String,
    ) -> Result<Project, AppError> {
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

        self.dal.create(ctx.clone(), &project).await?;
        Ok(project)
    }

    /// 根据 ID 获取项目
    pub async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>, AppError> {
        self.dal.find_by_id(ctx, id).await
    }

    /// 获取用户的所有项目
    pub async fn list_by_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<Vec<Project>, AppError> {
        self.dal.list_by_root_user(ctx, root_user_id, None).await
    }

    /// 启动项目
    pub async fn start(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<(), AppError> {
        let Some(mut project) = self.dal.find_by_id(ctx.clone(), project_id).await? else {
            return Err(AppError::NotFound(format!(
                "Project not found: {}",
                project_id
            )));
        };
        project.start();
        project.po.modified_by = modified_by;
        self.dal.update(ctx, &project).await?;
        Ok(())
    }

    /// 完成项目
    pub async fn complete(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<(), AppError> {
        let Some(mut project) = self.dal.find_by_id(ctx.clone(), project_id).await? else {
            return Err(AppError::NotFound(format!(
                "Project not found: {}",
                project_id
            )));
        };
        project.complete();
        project.po.modified_by = modified_by;
        self.dal.update(ctx, &project).await?;
        Ok(())
    }

    /// 归档项目
    pub async fn archive(
        &self,
        ctx: RequestContext,
        project_id: &str,
        modified_by: String,
    ) -> Result<(), AppError> {
        let Some(mut project) = self.dal.find_by_id(ctx.clone(), project_id).await? else {
            return Err(AppError::NotFound(format!(
                "Project not found: {}",
                project_id
            )));
        };
        project.po.status = ProjectStatus::Archived;
        project.po.modified_by = modified_by;
        self.dal.update(ctx, &project).await?;
        Ok(())
    }

    /// 更新项目基本信息
    pub async fn update_basic(
        &self,
        ctx: RequestContext,
        project_id: &str,
        name: Option<String>,
        description: Option<String>,
        priority: Option<i32>,
        modified_by: String,
    ) -> Result<Project, AppError> {
        let Some(mut project) = self.dal.find_by_id(ctx.clone(), project_id).await? else {
            return Err(AppError::NotFound(format!(
                "Project not found: {}",
                project_id
            )));
        };

        if let Some(name) = name {
            project.po.name = name;
        }
        if let Some(description) = description {
            project.po.description = description;
        }
        if let Some(priority) = priority {
            project.po.priority = priority;
        }
        project.po.modified_by = modified_by;

        self.dal.update(ctx, &project).await?;
        Ok(project)
    }
}
