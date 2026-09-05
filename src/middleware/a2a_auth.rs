//! /a2a 双模鉴权中间件（跨组织业务调用方案 P1+P2）
//!
//! `/a2a` JSON-RPC 入口的两类合法调用方：
//! 1. **本地用户**：JWT（Cookie/Bearer）——既有语义，原样保留；
//! 2. **建联对端节点**：`Authorization: Bearer <link access_token>`（连接级
//!    凭证，`authenticate_link_call` 哈希匹配 Active 连接）+ 可选
//!    `X-Federation-Caller` 身份声明（方案②凭证直传，明文 JSON，见
//!    [`common::api::FederationCallerDeclaration`]）。
//!
//! 本文件只做 **HTTP 协议适配**：提取 Bearer、解析声明头、把错误码映射成
//! HTTP 状态码。身份解析的全部业务判定下沉到
//! [`crate::middleware::federation_identity::resolve_federation_identity`]——
//! 该纯函数不感知传输层，未来的长连接握手（方向性组网 P8）复用同一份实现，
//! 避免两条链路的鉴权逻辑漂移。
//!
//! fail-closed：声明头存在但非法 JSON → 401。

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use common::api::{ApiResponse, FederationCallerDeclaration};
use common::constants::http_header;
use common::error::{ErrorCode, Result};

use crate::middleware::federation_identity::resolve_federation_identity;
use crate::middleware::jwt_auth::try_jwt_auth;

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

    // 3) 解析身份声明（缺省 = 连接级匿名调用；非法 JSON = fail-closed 401）
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

    // 4) 联邦身份解析（凭证鉴权 + 能力门禁 + 声明一致性 + 接待用户映射）
    let identity = match resolve_federation_identity(&credential, declaration.as_ref()).await {
        Ok(identity) => identity,
        Err(e) => {
            return Ok(match e.code {
                ErrorCode::Forbidden => forbidden(&e.msg),
                ErrorCode::Internal => internal_error(&e.msg),
                _ => unauthorized(&e.msg),
            });
        }
    };

    // 5) 注入身份头（供 request_context_middleware 读取）
    identity.apply_to_headers(req.headers_mut());

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

/// 500 JSON（服务端配置/内部错误，如组织无可用接待用户）
fn internal_error(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiResponse::<()>::error(500, message.to_string())),
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
