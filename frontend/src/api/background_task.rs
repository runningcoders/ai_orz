//! 后台任务管理 API
//!
//! 封装后台任务列表查询、清理、进度查询接口。

use super::{ApiError, api_get, api_post, build_query_string};
use common::api::{
    CleanupTasksResponse, ListBackgroundTasksRequest, ListBackgroundTasksResponse,
    TaskProgressSnapshot,
};

/// 查询后台任务进度
///
/// `GET /api/v1/system/tasks/{task_id}/progress`
pub async fn get_task_progress(task_id: &str) -> Result<TaskProgressSnapshot, ApiError> {
    api_get(&format!("/api/v1/system/tasks/{}/progress", task_id)).await
}

/// 列出所有后台任务（支持筛选）
///
/// `GET /api/v1/system/tasks?task_type=xxx&status=xxx`
pub async fn list_tasks(
    req: &ListBackgroundTasksRequest,
) -> Result<ListBackgroundTasksResponse, ApiError> {
    let qs = build_query_string(&[
        ("task_type", req.task_type.clone()),
        (
            "status",
            req.status
                .map(|s| serde_json::to_string(&s).unwrap_or_default()),
        ),
    ]);
    api_get(&format!("/api/v1/system/tasks{}", qs)).await
}

/// 清理已完成的旧任务
///
/// `POST /api/v1/system/tasks/cleanup?max_count=10`
pub async fn cleanup_tasks(max_count: Option<usize>) -> Result<CleanupTasksResponse, ApiError> {
    let qs = build_query_string(&[("max_count", max_count.map(|v| v.to_string()))]);
    let body = serde_json::json!({});
    api_post(&format!("/api/v1/system/tasks/cleanup{}", qs), &body).await
}
