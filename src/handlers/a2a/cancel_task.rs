//! tasks/cancel — 取消任务
//!
//! 流程：
//! 1. 查询 project
//! 2. 归档 project（对应 A2A canceled 状态）
//! 3. 查询 messages + artifacts
//! 4. 转为 A2aTask 返回

use common::api::a2a::{A2aTask, CancelTaskParams};
use common::error::Result;

use crate::handlers::a2a::mapper::build_a2a_task;
use crate::pkg::RequestContext;
use crate::service::domain::message;
use crate::service::domain::project::domain as project_domain;

/// 处理 tasks/cancel 请求
pub async fn handle_cancel_task(ctx: RequestContext, params: CancelTaskParams) -> Result<A2aTask> {
    // 1. 查询 project（确保存在）
    let _project = project_domain()
        .project_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    // 2. 归档 project（对应 A2A canceled）
    let user_id = ctx.uid();
    project_domain()
        .project_manage()
        .archive(ctx.clone(), &params.id, user_id)
        .await?;

    // 3. 重新查询 project 获取最新状态
    let project = project_domain()
        .project_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Task {} not found", params.id)))?;

    // 4. 查询 messages + artifacts
    let messages = message::domain()
        .management()
        .list_by_project_id(ctx.clone(), &params.id)
        .await?;
    let artifacts = project_domain()
        .artifact_manage()
        .list_by_project(ctx, &params.id)
        .await?;

    // 5. 转为 A2aTask
    let task = build_a2a_task(&params.id, project.po.status, &messages, &artifacts, None);

    Ok(task)
}
