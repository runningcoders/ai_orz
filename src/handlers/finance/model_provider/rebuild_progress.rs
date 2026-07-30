//! Handler: GET /api/v1/finance/model-providers/rebuild-progress - Get rebuild progress
//!
//! 从通用后台任务注册中心查询最近一个 RebuildVectors 任务的进度，
//! 装饰为 `RebuildProgressResponse` 保持向后兼容。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{
    GetRebuildProgressRequest, RebuildProgressResponse, RebuildStatus, TaskStatus, TaskType,
};
use common::error::{Error, Result};

/// Get vector index rebuild progress (returns the latest rebuild task)
#[generate_http_handler]
pub async fn get_rebuild_progress(
    _ctx: RequestContext,
    _params: GetRebuildProgressRequest,
) -> Result<RebuildProgressResponse> {
    let snapshots = system::domain()
        .background_task_registry()
        .list_progress_by_type(TaskType::RebuildVectors)
        .await;

    let snapshot = snapshots
        .into_iter()
        .max_by_key(|p| p.started_at)
        .ok_or_else(|| Error::not_found("没有向量重建任务"))?;

    let status = match snapshot.status {
        TaskStatus::Pending => RebuildStatus::Pending,
        TaskStatus::Running => RebuildStatus::Running,
        TaskStatus::Completed => RebuildStatus::Completed,
        TaskStatus::Failed => RebuildStatus::Failed,
    };

    Ok(RebuildProgressResponse {
        task_id: snapshot.task_id,
        status,
        current_entity: Some(snapshot.step_message),
        current_entity_index: snapshot.current_step,
        total_entities: snapshot.total_steps,
        processed_records: 0,
        total_records: 0,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        error: snapshot.error,
    })
}
