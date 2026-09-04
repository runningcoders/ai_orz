//! Handler: GET /api/v1/organization/links/capabilities
//!
//! 能力发现（机器侧，对端节点调用，契约凭证鉴权，P3）：返回本节点开放给
//! **调用方这条连接**的能力白名单 + 可调用 Agent 列表（发起侧用于跨组织
//! @ 提及选择与 runtime 路由）。
//! 在 router 中 root 层直挂，不进 `protected_routes` 的 JWT 链（评审稿 D7）。
//! 裸 axum handler（需读取 `Authorization` 头），get_directory 先例。

use axum::Json;
use axum::http::HeaderMap;
use common::api::{ApiResponse, CapabilitiesResponse, FederationAgentEntry};
use common::enums::AgentKind;
use common::error::Result;

use crate::handlers::organization::links::get_directory::extract_bearer_credential;
use crate::pkg::RequestContext;
use crate::service::domain::{hr, organization};

/// 能力发现：这条连接开放的能力 + 可调用 Agent 列表
pub async fn get_capabilities_handler(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<CapabilitiesResponse>>> {
    let credential = extract_bearer_credential(&headers)?;

    // 契约凭证鉴权（无效/吊销统一 unauthorized）
    let link = organization::domain()
        .organization_manage()
        .authenticate_link_call(ctx.clone(), &credential)
        .await?;

    // 这条连接开放的能力白名单
    let capabilities = link.capabilities_list();

    // 可调用 Agent：Onboarded 且非 Remote（Remote 是指向外部 Agent 的指针，
    // 不是本节点可承诺执行的实体；A2A 委派的第一闭环只暴露本节点 Agent）
    let agents = hr::domain()
        .agent_manage()
        .list_agents(ctx)
        .await?
        .into_iter()
        .filter(|a| {
            a.po.status == common::enums::AgentStatus::Onboarded && a.po.kind != AgentKind::Remote
        })
        .map(|a| FederationAgentEntry {
            id: a.po.id,
            name: a.po.name,
            description: a.po.description,
        })
        .collect();

    let response = CapabilitiesResponse {
        agents,
        capabilities,
    };
    // 出站响应统一过 redact!（EXPORT policy）：仅 id/name/description 白名单字段
    Ok(Json(ApiResponse::success(crate::redact!(response)?)))
}
