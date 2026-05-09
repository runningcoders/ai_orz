//! 项目执行实现

use super::ProjectExecution;
use crate::error::AppError;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::constants::utils;
use common::enums::ProjectStatus;

/// ProjectExecution trait 实现
///
/// 在 ProjectDomainImpl 上实现 ProjectExecution trait
#[async_trait::async_trait]
impl ProjectExecution for super::ProjectDomainImpl {
    async fn start_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError> {
        // 先获取现有项目
        let mut project = self
            .project_dal
            .find_by_id(ctx.clone(), project_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project not found: {}", project_id)))?;

        // 检查状态是否允许开始
        if project.status != ProjectStatus::Active {
            return Err(AppError::BadRequest(format!(
                "Project status {:?} cannot be started, must be Active",
                project.status
            )));
        }

        // 更新状态和开始时间
        project.status = ProjectStatus::InProgress;
        project.start_at = Some(utils::current_timestamp());
        project.modified_by = ctx.uid().to_string();
        project.updated_at = utils::current_timestamp();

        self.project_dal.update(ctx, &project).await?;
        Ok(())
    }

    async fn complete_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError> {
        // 先获取现有项目
        let mut project = self
            .project_dal
            .find_by_id(ctx.clone(), project_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project not found: {}", project_id)))?;

        // 检查状态是否允许完成
        if project.status != ProjectStatus::InProgress {
            return Err(AppError::BadRequest(format!(
                "Project status {:?} cannot be completed, must be InProgress",
                project.status
            )));
        }

        // 更新状态和结束时间
        project.status = ProjectStatus::Completed;
        project.end_at = Some(utils::current_timestamp());
        project.modified_by = ctx.uid().to_string();
        project.updated_at = utils::current_timestamp();

        self.project_dal.update(ctx, &project).await?;
        Ok(())
    }

    async fn reactivate_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<(), AppError> {
        // 先获取现有项目
        let mut project = self
            .project_dal
            .find_by_id(ctx.clone(), project_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project not found: {}", project_id)))?;

        // 检查状态是否允许重新激活
        if project.status != ProjectStatus::Completed && project.status != ProjectStatus::Archived {
            return Err(AppError::BadRequest(format!(
                "Project status {:?} cannot be reactivated, must be Completed or Archived",
                project.status
            )));
        }

        // 更新状态，清空结束时间（如果是已完成的）
        project.status = ProjectStatus::Active;
        if project.end_at.is_some() {
            project.end_at = None;
        }
        project.modified_by = ctx.uid().to_string();
        project.updated_at = utils::current_timestamp();

        self.project_dal.update(ctx, &project).await?;
        Ok(())
    }
}
