//! 工作台场景投影 API
//!
//! 单端点拉取当前投影快照；DTO 复用 `common::api::scene` 共享契约，此处只做请求封装，
//! 不引入任何判定或渲染逻辑（语义在后端，表现由页面层决定）。

pub use common::api::{GetSceneProjectionRequest, SceneProjectionResponse};

use super::{ApiError, api_get, build_query_string};

/// 拉取场景投影快照
///
/// v1 走 HTTP 轮询；后续换 SSE 时消费方（`SceneSource`）不变，故页面不直接调用本函数。
pub async fn get_scene_projection(
    req: GetSceneProjectionRequest,
) -> Result<SceneProjectionResponse, ApiError> {
    let qs = build_query_string(&[("seat_limit", req.seat_limit.map(|v| v.to_string()))]);
    api_get(&format!("/api/v1/system/scene/projection{}", qs)).await
}
