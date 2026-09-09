//! /a2a 双模鉴权中间件（S2 联邦签名升级）
//!
//! `/a2a` JSON-RPC 入口的两类合法调用方：
//! 1. **本地用户**：JWT（Cookie/Bearer）——既有语义，原样保留；
//! 2. **建联对端节点**：每请求 Ed25519 签名——`X-Federation-Key-Id / Timestamp /
//!    Nonce / Signature` 四头，签名串 = `method\npath\ntimestamp\nnonce\nsha256(body)`，
//!    另可选 `X-Federation-Caller` 身份声明（明文 JSON，结构见
//!    [`common::api::FederationCallerDeclaration`]）。
//!
//! 本文件只做 **HTTP 协议适配**：缓冲请求体算 SHA-256、提取签名头、解析声明头、
//! 把错误码映射成 HTTP 状态码。身份解析的全部业务判定下沉到
//! [`crate::middleware::federation_identity::resolve_federation_identity`]——
//! 该函数不感知传输层，机器侧 handler 与联邦 WS 握手复用同一份实现，
//! 避免多条链路的鉴权逻辑漂移。
//!
//! fail-closed：签名头缺失 / 声明头存在但非法 JSON → 401，无回退路径。

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use common::api::{ApiResponse, FederationCallerDeclaration};
use common::constants::http_header;
use common::error::{ErrorCode, Result};

use crate::middleware::federation_identity::{
    extract_federation_proof, resolve_federation_identity,
};
use crate::middleware::jwt_auth::try_jwt_auth;

/// /a2a 请求体大小上限（JSON-RPC 任务提交，8MB 足够且防滥用）
const A2A_BODY_LIMIT: usize = 8 * 1024 * 1024;

/// /a2a 双模鉴权中间件
pub async fn a2a_auth_middleware(mut req: Request, next: Next) -> Result<Response> {
    // 1) 本地 JWT 优先（既有语义不变；JWT 请求不参与联邦签名）
    if try_jwt_auth(&mut req) {
        return Ok(next.run(req).await);
    }

    // 2) 联邦签名四件套（缺失即 401，无回退路径）
    let (parts, body) = req.into_parts();
    let Some(proof) = extract_federation_proof(&parts.headers) else {
        return Ok(unauthorized("缺少联邦签名头"));
    };

    // 3) 解析身份声明（缺省 = 连接级匿名调用；非法 JSON = fail-closed 401）
    let declaration = match parts.headers.get(http_header::FEDERATION_CALLER) {
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

    // 4) 缓冲请求体计算 SHA-256（签名规范串末项），再原样回填转发
    let method = parts.method.as_str().to_string();
    let path_with_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    let bytes = match axum::body::to_bytes(body, A2A_BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(e) => {
            sys_debug!("a2a body read failed: {}", e);
            return Ok(unauthorized("请求体读取失败"));
        }
    };
    let body_hash = crate::pkg::crypto::did::body_sha256_hex(&bytes);
    let mut req = Request::from_parts(parts, Body::from(bytes));

    // 5) 联邦身份解析（验签 + 能力门禁 + 声明一致性 + 接待用户映射）
    let identity = match resolve_federation_identity(
        &proof,
        &method,
        &path_with_query,
        &body_hash,
        declaration.as_ref(),
    )
    .await
    {
        Ok(identity) => identity,
        Err(e) => {
            return Ok(match e.code {
                ErrorCode::Forbidden => forbidden(&e.msg),
                ErrorCode::Internal => internal_error(&e.msg),
                _ => unauthorized(&e.msg),
            });
        }
    };

    // 6) 注入身份头（供 request_context_middleware 读取）
    identity.apply_to_headers(req.headers_mut());

    Ok(next.run(req).await)
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
