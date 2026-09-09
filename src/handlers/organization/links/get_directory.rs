//! Handler: GET /api/v1/organization/links/directory
//!
//! 返回本节点组织目录（机器侧，对端节点调用，联邦签名鉴权）。
//! 在 router 中 root 层直挂，不进 `protected_routes` 的 JWT 链（评审稿 D7）。
//!
//! 裸 axum handler（非 `generate_http_handler`）：需读取联邦签名头与请求 URI，
//! 宏生成的签名固定为 (ctx, params) 不支持额外 extractor；a2a callback 先例。

use axum::Json;
use axum::http::{HeaderMap, Uri};
use common::api::{ApiResponse, DirectoryResponse};
use common::error::{Error, Result};

use crate::middleware::federation_identity::extract_federation_proof;
use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// 机器侧联邦签名鉴权（directory / sync / capabilities 三个裸 handler 共用）
///
/// 提取签名四件套（缺失/非法统一 unauthorized，防枚举），调 domain 统一验签入口
/// （连接归属 → 时间窗 → nonce → Ed25519 验签）。`body_bytes` 为原始请求体
/// （GET 传空）。
pub(super) async fn authenticate_machine_request(
    ctx: RequestContext,
    headers: &HeaderMap,
    method: &str,
    uri: &Uri,
    body_bytes: &[u8],
) -> Result<OrganizationLinkPo> {
    let proof =
        extract_federation_proof(headers).ok_or_else(|| Error::unauthorized("缺少联邦签名头"))?;
    let path_with_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or_else(|| uri.path());
    let body_hash = crate::pkg::crypto::did::body_sha256_hex(body_bytes);
    organization::domain()
        .organization_manage()
        .authenticate_federation_request(ctx, &proof, method, path_with_query, &body_hash)
        .await
}

/// 返回本节点组织目录（对端节点调用）
pub async fn get_directory_handler(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<DirectoryResponse>>> {
    authenticate_machine_request(ctx.clone(), &headers, "GET", &uri, b"").await?;

    let orgs = organization::domain()
        .organization_manage()
        .get_directory(ctx)
        .await?;

    // 出站响应统一过 redact!（EXPORT policy，评审稿 §6.2）。
    // 本结构仅目录白名单字段、无凭证类字段，脱敏不会破坏协议
    let response = DirectoryResponse { orgs };
    Ok(Json(ApiResponse::success(crate::redact!(response)?)))
}
