//! Handler: PUT /api/v1/tools/{id} - Update tool configuration

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateToolRequest, UpdateToolResponse};
use common::enums::{ToolProtocol, ToolStatus};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// Update an existing custom tool's configuration (name, description, credentials, etc.)
#[register_handler_tool(
    id = "update_tool",
    name = "update_tool",
    description = "Update an existing custom tool's configuration (name, description, credentials, etc.)",
    params = "common::api::UpdateToolRequest"
)]
#[generate_http_handler]
pub async fn update_tool(
    ctx: RequestContext,
    params: UpdateToolRequest,
) -> Result<UpdateToolResponse, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", params.id)))?;

    if matches!(tool.po.protocol, ToolProtocol::Builtin) {
        return Err(AppError::BadRequest(
            "内置 Tool 不允许通过管理接口修改".to_string(),
        ));
    }
    if matches!(params.protocol, Some(ToolProtocol::Builtin)) {
        return Err(AppError::BadRequest(
            "非内置 Tool 不允许被修改为内置协议".to_string(),
        ));
    }

    if let Some(name) = params.name {
        tool.po.name = name;
    }
    if let Some(description) = params.description {
        tool.po.description = description;
    }
    if let Some(protocol) = params.protocol {
        tool.po.protocol = protocol;
    }
    if let Some(control_mode) = params.control_mode {
        tool.po.control_mode = control_mode;
    }
    if let Some(config) = params.config {
        tool.po.config = config;
    }
    if let Some(parameters_schema) = params.parameters_schema {
        tool.po.parameters_schema = Some(parameters_schema);
    }
    if let Some(tags) = params.tags {
        tool.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(enabled) = params.enabled {
        let target_status = if enabled {
            ToolStatus::Enabled
        } else {
            ToolStatus::Disabled
        };
        tool.transition_status(target_status, user_id.clone())
            .map_err(AppError::BadRequest)?;
    }
    tool.po.touch(Some(user_id));

    domain()
        .tool_provider_manage()
        .update_tool(ctx, &tool)
        .await?;

    Ok(to_detail(&tool))
}
