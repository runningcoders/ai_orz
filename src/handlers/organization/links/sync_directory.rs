//! Handler: POST /api/v1/organization/links/directory/sync
//!
//! 接收对端推送的组织目录（机器侧，对端节点调用，联邦签名鉴权）。
//! 在 router 中 root 层直挂，不进 `protected_routes` 的 JWT 链（评审稿 D7）。
//! 裸 axum handler（需读取联邦签名头），get_directory 先例。
//! 请求体用 `Bytes` 接收：签名覆盖原始字节，必须先算哈希再反序列化。

use axum::Json;
use axum::body::Bytes;
use axum::http::{HeaderMap, Uri};
use common::api::{ApiResponse, DirectorySyncRequest, DirectorySyncResponse};
use common::error::{Error, Result};

use crate::handlers::organization::links::get_directory::authenticate_machine_request;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// 接收对端推送的目录（逐条 Remote 影子 upsert）
pub async fn sync_directory_handler(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<DirectorySyncResponse>>> {
    authenticate_machine_request(ctx.clone(), &headers, "POST", &uri, &body).await?;

    let req: DirectorySyncRequest = serde_json::from_slice(&body)
        .map_err(|e| Error::bad_request(format!("目录同步请求体格式非法: {}", e)))?;

    let _written = organization::domain()
        .organization_manage()
        .handle_directory_sync(ctx, req)
        .await?;

    Ok(Json(ApiResponse::success(DirectorySyncResponse {})))
}
