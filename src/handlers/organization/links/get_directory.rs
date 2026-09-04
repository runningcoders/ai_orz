//! Handler: GET /api/v1/organization/links/directory
//!
//! 返回本节点组织目录（机器侧，对端节点调用，契约凭证鉴权）。
//! 在 router 中 root 层直挂，不进 `protected_routes` 的 JWT 链（评审稿 D7）。
//!
//! 裸 axum handler（非 `generate_http_handler`）：需读取 `Authorization` 头，
//! 宏生成的签名固定为 (ctx, params) 不支持额外 extractor；a2a callback 先例。

use axum::Json;
use axum::http::HeaderMap;
use common::api::{ApiResponse, DirectoryResponse};
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// 从 Authorization 头提取 Bearer 凭证（缺失/格式错统一 unauthorized，防枚举）
pub(super) fn extract_bearer_credential(headers: &HeaderMap) -> Result<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::unauthorized("缺少联邦契约凭证"))?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .ok_or_else(|| Error::unauthorized("联邦契约凭证格式无效"))?;
    Ok(token.trim().to_string())
}

/// 返回本节点组织目录（对端节点调用）
pub async fn get_directory_handler(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<DirectoryResponse>>> {
    let credential = extract_bearer_credential(&headers)?;
    let domain = organization::domain();

    // 契约凭证鉴权（无效/吊销统一 unauthorized）
    domain
        .organization_manage()
        .authenticate_link_call(ctx.clone(), &credential)
        .await?;

    let orgs = domain.organization_manage().get_directory(ctx).await?;

    // 出站响应统一过 redact!（EXPORT policy，评审稿 §6.2）。
    // 本结构仅目录白名单字段、无凭证类字段，脱敏不会破坏协议
    // （对比：verify 响应携带 peer_token，禁止 redact!——KeyRule "token"
    // 会命中遮蔽导致建联损坏）。
    let response = DirectoryResponse { orgs };
    Ok(Json(ApiResponse::success(crate::redact!(response)?)))
}
