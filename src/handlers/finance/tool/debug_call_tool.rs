//! Handler: POST /api/v1/tools/{id}/debug-call - 工具调试调用（管理员专用）
//!
//! 面向前端工具详情页的调试入口，直接调用 call_tool_by_id（不经过 Agent 授权）。
//! 仅 Admin 及以上权限可用（由路由层 require_role_middleware 校验）。

use crate::pkg::RequestContext;
use crate::service::domain::runtime;
use ai_orz_macros::generate_http_handler;
use common::api::{DebugCallToolRequest, DebugCallToolResponse};
use common::error::Result;

/// 工具调试调用（同步）
///
/// 管理员在工具详情页直接调用工具进行调试，无需 Agent 上下文。
/// 直接走 `call_tool_by_id`（协议路由 + 状态检查），跳过 Agent installed_tags 授权。
/// 权限校验由路由层 `require_role_middleware(UserRole::Admin)` 完成。
/// 注意：此 handler 不注册为 Agent 工具（admin only，绕过 Agent 授权）。
#[generate_http_handler]
pub async fn debug_call_tool(
    ctx: RequestContext,
    params: DebugCallToolRequest,
) -> Result<DebugCallToolResponse> {
    let result = runtime::domain()
        .tool_execution()
        .call_tool_by_id(ctx, params.id, params.args)
        .await?;

    Ok(DebugCallToolResponse {
        success: true,
        result: result.result,
        tool_call_id: result.trace_ref.call_id,
        status: "completed".to_string(),
    })
}
