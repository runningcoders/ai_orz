//! /a2a 双模鉴权中间件（跨组织业务调用方案 P1+P2）
//!
//! `/a2a` JSON-RPC 入口的两类合法调用方：
//! 1. **本地用户**：JWT（Cookie/Bearer）——既有语义，原样保留；
//! 2. **建联对端节点**：`Authorization: Bearer <link access_token>`（连接级
//!    凭证，`authenticate_link_call` 哈希匹配 Active 连接）+ 可选
//!    `X-Federation-Caller` 身份声明（方案②凭证直传，明文 JSON，见
//!    [`common::api::FederationCallerDeclaration`]）。
//!
//! 身份注入（联邦路径，供内层 request_context_middleware 读取）：
//! - `X-Organization-Id` = link.local_org_id（**目标组织**，数据作用域 = B）
//! - `X-Caller-Organization-Id` = link.peer_org_id（**发起组织**，iss 语义 = A，
//!   R3 计量维度）
//! - `X-User-Id` = 声明 caller_user，缺省 `federation:{peer_org_id}`（合成身份）
//! - `X-Caller-Type` = 声明携带 caller_agent 时 `agent`，否则 `user`
//! - 不注入 `X-User-Role`（联邦调用不获得任何本地角色权限）
//!
//! fail-closed：声明头存在但非法 JSON、或 caller_org 与连接 peer_org_id 不一致
//! → 401（防止跨连接冒充发起组织）。

use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use common::api::{ApiResponse, CAPABILITY_A2A_TASK, FederationCallerDeclaration};
use common::constants::http_header;
use common::error::Result;

use crate::middleware::jwt_auth::try_jwt_auth;
use crate::pkg::RequestContext;
use crate::service::domain::organization;

/// /a2a 双模鉴权中间件
pub async fn a2a_auth_middleware(mut req: Request, next: Next) -> Result<Response> {
    // 1) 本地 JWT 优先（既有语义不变）
    if try_jwt_auth(&mut req) {
        return Ok(next.run(req).await);
    }

    // 2) 联邦凭证回退：Bearer 必须存在（Cookie 请求不参与联邦鉴权）
    let Some(credential) = extract_bearer_token(&req) else {
        return Ok(unauthorized("缺少认证凭证"));
    };

    // 3) 连接凭证鉴权（哈希匹配 Active 连接；无效/吊销统一 401 防枚举）
    let sys_ctx = RequestContext::new_system();
    let link = match organization::domain()
        .organization_manage()
        .authenticate_link_call(sys_ctx, &credential)
        .await
    {
        Ok(link) => link,
        Err(e) => {
            sys_debug!("federation credential rejected: {}", e);
            return Ok(unauthorized("联邦契约凭证无效"));
        }
    };

    // 4) 连接级能力白名单门禁（P3）：未开放 a2a_task 的连接不允许跨组织委派
    if !link.has_capability(CAPABILITY_A2A_TASK) {
        return Ok(forbidden("这条连接未开放 a2a_task 能力"));
    }

    // 5) 解析身份声明（缺省 = 连接级匿名调用；非法 = fail-closed 401）
    let declaration = match req.headers().get(http_header::FEDERATION_CALLER) {
        Some(value) => match value
            .to_str()
            .ok()
            .and_then(FederationCallerDeclaration::from_header_value)
        {
            Some(d) => Some(d),
            None => return Ok(unauthorized("X-Federation-Caller 声明格式非法")),
        },
        None => None,
    };

    // 6) 声明一致性：caller_org 与连接归属不符 → 401（防跨连接冒充）
    if let Some(decl) = &declaration
        && let Some(declared_org) = &decl.caller_org
        && declared_org != &link.peer_org_id
    {
        return Ok(unauthorized("声明组织与连接归属不一致"));
    }

    // 7) 注入联邦身份 headers（内层 request_context_middleware 据此建 ctx）
    let peer_org = &link.peer_org_id;
    let user_id = declaration
        .as_ref()
        .and_then(|d| d.caller_user.clone())
        .unwrap_or_else(|| format!("federation:{}", peer_org));
    let caller_type = if declaration
        .as_ref()
        .is_some_and(|d| d.caller_agent.is_some())
    {
        "agent"
    } else {
        "user"
    };
    if let Ok(v) = HeaderValue::from_str(&user_id) {
        req.headers_mut().insert(http_header::USER_ID, v);
    }
    if let Ok(v) = HeaderValue::from_str(&format!("federation:{}", peer_org)) {
        req.headers_mut().insert(http_header::USERNAME, v);
    }
    if let Ok(v) = HeaderValue::from_str(&link.local_org_id) {
        req.headers_mut().insert(http_header::ORGANIZATION_ID, v);
    }
    if let Ok(v) = HeaderValue::from_str(peer_org) {
        req.headers_mut()
            .insert(http_header::CALLER_ORGANIZATION_ID, v);
    }
    req.headers_mut().insert(
        http_header::CALLER_TYPE,
        HeaderValue::from_static(caller_type),
    );

    Ok(next.run(req).await)
}

/// 提取 `Authorization: Bearer <token>`（联邦路径只认 header，不认 Cookie）
fn extract_bearer_token(req: &Request) -> Option<String> {
    let auth = req.headers().get(axum::http::header::AUTHORIZATION)?;
    let auth = auth.to_str().ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// 403 JSON（能力白名单拒绝）
fn forbidden(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiResponse::<()>::error(403, message.to_string())),
    )
        .into_response()
}

/// 401 JSON（与 jwt_auth 中间件的 API 响应同形）
fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiResponse::<()>::error(401, message.to_string())),
    )
        .into_response()
}
