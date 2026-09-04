//! Handler: POST /api/v1/organization/links/directory/sync
//!
//! 接收对端推送的组织目录（机器侧，对端节点调用，契约凭证鉴权）。
//! 在 router 中 root 层直挂，不进 `protected_routes` 的 JWT 链（评审稿 D7）。
//! 裸 axum handler（需读取 `Authorization` 头），a2a callback 先例。

use axum::Json;
use axum::http::HeaderMap;
use common::api::{ApiResponse, DirectorySyncRequest, DirectorySyncResponse};
use common::error::Result;

use crate::handlers::organization::links::get_directory::extract_bearer_credential;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// 接收对端推送的目录（逐条 Remote 影子 upsert）
pub async fn sync_directory_handler(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    headers: HeaderMap,
    Json(req): Json<DirectorySyncRequest>,
) -> Result<Json<ApiResponse<DirectorySyncResponse>>> {
    let credential = extract_bearer_credential(&headers)?;
    let domain = organization::domain();

    // 契约凭证鉴权（无效/吊销统一 unauthorized）
    domain
        .organization_manage()
        .authenticate_link_call(ctx.clone(), &credential)
        .await?;

    let _written = domain
        .organization_manage()
        .handle_directory_sync(ctx, req)
        .await?;

    Ok(Json(ApiResponse::success(DirectorySyncResponse {})))
}
