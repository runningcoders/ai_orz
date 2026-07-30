//! POST /api/v1/system/tasks/cleanup - 清理已完成的旧任务
//!
//! 保留每个 task_type 最近 max_count 个已完成/失败的任务，其余移除。
//! 运行中或等待中的任务不受影响。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{CleanupTasksRequest, CleanupTasksResponse, TaskStatus};
use common::error::Result;

#[generate_http_handler]
pub async fn cleanup_tasks(
    _ctx: RequestContext,
    params: CleanupTasksRequest,
) -> Result<CleanupTasksResponse> {
    let max_count = params.max_count.unwrap_or(10);

    // 清理前统计已完成/失败的任务数量
    let before = system::domain()
        .background_task_registry()
        .list_all_progress()
        .await;
    let before_count = before
        .iter()
        .filter(|p| p.status == TaskStatus::Completed || p.status == TaskStatus::Failed)
        .count();

    // 执行清理
    system::domain()
        .background_task_registry()
        .cleanup_finished(max_count)
        .await;

    // 清理后统计
    let after = system::domain()
        .background_task_registry()
        .list_all_progress()
        .await;
    let after_count = after
        .iter()
        .filter(|p| p.status == TaskStatus::Completed || p.status == TaskStatus::Failed)
        .count();

    let cleaned = before_count.saturating_sub(after_count);
    Ok(CleanupTasksResponse { cleaned })
}
