//! JSON-RPC 2.0 入口
//!
//! POST /a2a
//! 挂 JWT 中间件，解析 JSON-RPC 请求并按 method 分发。
//!
//! RequestContext 通过 request_context_middleware 注入到 Extension，
//! handler 内用 `Extension(ctx): Extension<RequestContext>` 提取。
//! 配置通过全局单例 `crate::config::get()` 读取。

use axum::Extension;
use axum::response::Json;
use common::api::a2a::{
    error_codes, A2aTask, CancelTaskParams, GetTaskParams, JsonRpcRequest, JsonRpcResponse,
    SendTaskParams,
};
use serde_json::Value;

use crate::handlers::a2a::{cancel_task, get_task, send_task};
use crate::pkg::RequestContext;

/// JSON-RPC 入口 handler
pub async fn handle_jsonrpc(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let config = crate::config::get();

    // 检查 A2A Server 是否启用
    if !config.a2a_server.enabled {
        return Json(JsonRpcResponse::error(
            req.id,
            error_codes::METHOD_NOT_FOUND,
            "A2A Server 未启用".to_string(),
        ));
    }

    // 验证 jsonrpc 版本
    if req.jsonrpc != "2.0" {
        return Json(JsonRpcResponse::error(
            req.id,
            error_codes::INVALID_REQUEST,
            "Unsupported JSON-RPC version".to_string(),
        ));
    }

    // 按方法分发
    let result = match req.method.as_str() {
        "tasks/send" => dispatch_send(ctx, req.params).await,
        "tasks/get" => dispatch_get(ctx, req.params).await,
        "tasks/cancel" => dispatch_cancel(ctx, req.params).await,
        _ => {
            return Json(JsonRpcResponse::error(
                req.id,
                error_codes::METHOD_NOT_FOUND,
                format!("Method not found: {}", req.method),
            ));
        }
    };

    match result {
        Ok(task) => Json(JsonRpcResponse::success(
            req.id,
            serde_json::to_value(&task).unwrap_or(Value::Null),
        )),
        Err(e) => {
            // Error 实现了 Display，输出形如 "[error_code] msg"
            let message = format!("{}", e);
            Json(JsonRpcResponse::error(
                req.id,
                error_codes::INTERNAL_ERROR,
                message,
            ))
        }
    }
}

async fn dispatch_send(
    ctx: RequestContext,
    params: Value,
) -> common::error::Result<A2aTask> {
    let params: SendTaskParams = serde_json::from_value(params)
        .map_err(|e| common::error::Error::bad_request(format!("Invalid params: {}", e)))?;
    send_task::handle_send_task(ctx, params).await
}

async fn dispatch_get(
    ctx: RequestContext,
    params: Value,
) -> common::error::Result<A2aTask> {
    let params: GetTaskParams = serde_json::from_value(params)
        .map_err(|e| common::error::Error::bad_request(format!("Invalid params: {}", e)))?;
    get_task::handle_get_task(ctx, params).await
}

async fn dispatch_cancel(
    ctx: RequestContext,
    params: Value,
) -> common::error::Result<A2aTask> {
    let params: CancelTaskParams = serde_json::from_value(params)
        .map_err(|e| common::error::Error::bad_request(format!("Invalid params: {}", e)))?;
    cancel_task::handle_cancel_task(ctx, params).await
}
