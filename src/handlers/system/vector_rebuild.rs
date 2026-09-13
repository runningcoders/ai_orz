//! Handler: POST /api/v1/system/vector-rebuild - 触发全量向量索引重建。
//!
//! 仅 SuperAdmin 可调用（handler 内部二次校验）。
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可进入。
//!
//! 语义：对 7 类实体（agent/memory/skill/task/project/message/tool）的向量索引
//! 做一次全量重建（分页逐条重新 embedding 并 upsert）。任务互斥——已有
//! RebuildVectors 任务运行中时返回 409。前端拿 `task_id` 轮询
//! `GET /api/v1/system/tasks/{task_id}/progress` 展示进度条。

use ai_orz_macros::generate_http_handler;
use common::api::{RebuildVectorsRequest, TaskIdResponse};
use common::error::Result;
use std::sync::Arc;

use crate::handlers::finance::model_provider::rebuild_vectors_task::RebuildVectorsTask;
use crate::pkg::RequestContext;
use crate::pkg::background_task::registry;

use super::backup::check_super_admin;

/// 触发全量向量索引重建（SuperAdmin 专用）
#[generate_http_handler]
pub async fn rebuild_vectors(
    ctx: RequestContext,
    _params: RebuildVectorsRequest,
) -> Result<TaskIdResponse> {
    check_super_admin(&ctx)?;

    // 任务体 run() 内部还会做「已有 Running 任务则 409」的互斥检查
    let task = Arc::new(RebuildVectorsTask::new(ctx));
    let task_id = registry().register(task).await;

    Ok(TaskIdResponse { task_id })
}
