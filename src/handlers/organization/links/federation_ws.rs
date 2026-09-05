//! Handler: GET /api/v1/organization/links/ws
//!
//! 联邦 WS 长连接服务端入口（机器侧，对端拨入）。鉴权前移到连接层：
//! 握手时用 link 凭证鉴权**一次**（复用 `resolve_federation_identity`，
//! 含能力门禁），会话持有 peer_org / 本端 org / 接待用户，此后每条消息
//! 的 ctx 由会话注入——**帧信封绝不携带身份字段**（P0 红线）。
//! 重连必须重新握手鉴权（无会话恢复）。
//!
//! 裸 axum handler（upgrade 协议，不走 `generate_http_handler`），
//! 在 router root 层直挂，与 verify/directory 同前缀同模式。

use axum::Json;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use common::api::{ApiResponse, FederationCallerDeclaration};
use common::error::ErrorCode;

use crate::middleware::federation_identity::resolve_federation_identity;
use crate::pkg::ws::serve_server;
use crate::service::dao::organization_link::ws::FederationWsSession;

/// 联邦 WS 长连接升级入口
pub async fn federation_ws_handler(ws: WebSocketUpgrade, headers: HeaderMap) -> Response {
    // 1) Bearer 凭证（WS 握手只认 header）
    let Some(credential) = extract_bearer_token(&headers) else {
        return unauthorized("缺少认证凭证");
    };

    // 2) 身份声明（缺省 = 连接级匿名调用；非法 = fail-closed 401）
    let declaration = match headers.get(common::constants::http_header::FEDERATION_CALLER) {
        Some(value) => match value
            .to_str()
            .ok()
            .and_then(FederationCallerDeclaration::from_header_value)
        {
            Some(d) => Some(d),
            None => return unauthorized("X-Federation-Caller 声明格式非法"),
        },
        None => None,
    };

    // 3) 身份解析（凭证鉴权 + 能力门禁 + 声明一致性 + 接待用户映射）
    let identity = match resolve_federation_identity(&credential, declaration.as_ref()).await {
        Ok(identity) => identity,
        Err(e) => {
            return match e.code {
                ErrorCode::Forbidden => json_status(StatusCode::FORBIDDEN, 403, &e.msg),
                ErrorCode::Internal => json_status(StatusCode::INTERNAL_SERVER_ERROR, 500, &e.msg),
                _ => json_status(StatusCode::UNAUTHORIZED, 401, &e.msg),
            };
        }
    };

    // 4) upgrade → pkg 通用会话循环（帧路由在联邦 adapter）。
    //    入站命令的业务 ctx 由 consumer 按事件经 domain 解析（接待用户），
    //    session 只持有两端组织 ID 对，不持有业务身份。
    let session =
        FederationWsSession::new(identity.local_org_id.clone(), identity.peer_org_id.clone());
    ws.on_upgrade(move |socket| serve_server(socket, std::sync::Arc::new(session)))
}

/// 提取 `Authorization: Bearer <token>`
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get(axum::http::header::AUTHORIZATION)?;
    let auth = auth.to_str().ok()?;
    let token = auth.strip_prefix("Bearer ")?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn json_status(status: StatusCode, code: i32, message: &str) -> Response {
    (
        status,
        Json(ApiResponse::<()>::error(code, message.to_string())),
    )
        .into_response()
}

fn unauthorized(message: &str) -> Response {
    json_status(StatusCode::UNAUTHORIZED, 401, message)
}
