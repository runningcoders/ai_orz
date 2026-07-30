//! GET /api/v1/system/tasks - 列出所有后台任务
//!
//! 支持按 task_type 和 status 筛选。返回 TaskProgressSnapshot 列表（按 started_at 降序）。
//! 客户端分页：后端返回全部匹配任务，前端自行分页（任务数量通常不大）。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{ListBackgroundTasksRequest, ListBackgroundTasksResponse};
use common::error::Result;

#[generate_http_handler]
pub async fn list_tasks(
    _ctx: RequestContext,
    params: ListBackgroundTasksRequest,
) -> Result<ListBackgroundTasksResponse> {
    let mut snapshots = system::domain()
        .background_task_registry()
        .list_all_progress()
        .await;

    // 按 task_type 筛选（字符串匹配）
    if let Some(ref task_type_str) = params.task_type {
        snapshots.retain(|s| &s.task_type == task_type_str);
    }

    // 按 status 筛选
    if let Some(status) = params.status {
        snapshots.retain(|s| s.status == status);
    }

    // 按 started_at 降序排序（最新的在前）
    snapshots.sort_by_key(|s| std::cmp::Reverse(s.started_at));

    let total = snapshots.len();
    Ok(ListBackgroundTasksResponse {
        tasks: snapshots,
        total,
    })
}
