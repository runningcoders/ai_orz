//! Seed 配置迁移 API
//!
//! 后端已将 save/load/apply-default 改为异步后台任务，统一返回 `TaskIdResponse`。
//! 前端通过 `get_task_progress` 轮询 `GET /api/v1/system/tasks/{task_id}/progress` 获取进度。
//! list/get_file/delete_file 仍为同步接口。

use super::{ApiError, api_delete, api_get, api_post};

/// 列出所有 seed 文件
pub async fn list_seeds() -> Result<common::api::ListSeedsResponse, ApiError> {
    api_get("/api/v1/system/seed/list").await
}

/// 读取 seed 文件内容
pub async fn get_seed_file(name: &str) -> Result<common::api::GetSeedFileResponse, ApiError> {
    api_get(&format!("/api/v1/system/seed/file/{}", name)).await
}

/// 删除 seed 文件
pub async fn delete_seed_file(name: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/system/seed/file/{}", name)).await
}

/// 保存当前组织配置到文件（异步，返回 task_id）
pub async fn save_seed(
    req: common::api::SaveSeedRequest,
) -> Result<common::api::TaskIdResponse, ApiError> {
    api_post("/api/v1/system/seed/save", &req).await
}

/// 从文件加载快照（异步，返回 task_id）
pub async fn load_seed(
    name: &str,
    req: common::api::LoadSeedRequest,
) -> Result<common::api::TaskIdResponse, ApiError> {
    api_post(&format!("/api/v1/system/seed/load/{}", name), &req).await
}

/// 应用默认模板（异步，返回 task_id）
pub async fn apply_default(
    req: common::api::ApplyDefaultSeedRequest,
) -> Result<common::api::TaskIdResponse, ApiError> {
    api_post("/api/v1/system/seed/apply-default", &req).await
}

/// 查询统一后台任务进度
///
/// 调用 `GET /api/v1/system/tasks/{task_id}/progress`，所有后台任务（初始化、
/// 向量重建、seed 导出/导入等）共用此接口。
pub async fn get_task_progress(
    task_id: &str,
) -> Result<common::api::TaskProgressSnapshot, ApiError> {
    api_get(&format!("/api/v1/system/tasks/{}/progress", task_id)).await
}
