//! tasks/get — 查询任务状态
//!
//! 流程：
//! 1. 根据 task_id（= project_id）查询 project
//! 2. 查询关联 messages
//! 3. 查询关联 artifacts
//! 4. 转为 A2aTask 返回

use common::api::a2a::{A2aTask, GetTaskParams};
use common::error::Result;

use crate::handlers::a2a::mapper::build_a2a_task;
use crate::pkg::RequestContext;
use crate::service::domain::message;
use crate::service::domain::project::domain as project_domain;

/// 处理 tasks/get 请求
pub async fn handle_get_task(
    ctx: RequestContext,
    params: GetTaskParams,
) -> Result<A2aTask> {
    // 1. 查询 project
    let project = project_domain()
        .project_manage()
        .get(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("Task {} not found", params.id))
        })?;

    // 2. 查询 messages
    let messages = message::domain()
        .management()
        .list_by_project_id(ctx.clone(), &params.id)
        .await?;

    // 3. 查询 artifacts
    let artifacts = project_domain()
        .artifact_manage()
        .list_by_project(ctx, &params.id)
        .await?;

    // 4. 转为 A2aTask
    let task = build_a2a_task(
        &params.id,
        project.po.status,
        &messages,
        &artifacts,
        None, // session_id 不持久化，get 时不返回
    );

    Ok(task)
}
