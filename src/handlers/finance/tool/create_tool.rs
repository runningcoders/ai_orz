//! Handler: POST /api/v1/tools - Create a new custom tool

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateToolRequest, CreateToolResponse};
use common::enums::ToolProtocol;

use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use common::error::{Result, bail_err};

/// Create a new custom tool (HTTP/MCP). Built-in tools cannot be created via this API.
#[register_handler_tool(
    id = "create_tool",
    name = "create_tool",
    description = "Create a new custom tool (HTTP/MCP). Built-in tools cannot be created via this API.",
    params = "common::api::CreateToolRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn create_tool(
    ctx: RequestContext,
    params: CreateToolRequest,
) -> Result<CreateToolResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if matches!(params.protocol, ToolProtocol::Builtin) {
        bail_err!(InvalidRequest, "内置 Tool 由系统同步，不允许通过管理接口创建");
    }

    let tags = params.tags.clone().unwrap_or_default();
    let mut tool_po = ToolPo::new(
        String::new(),
        params.name.clone(),
        params.description.clone(),
        params.protocol,
        params.config.clone().unwrap_or_default(),
        params.parameters_schema.clone(),
        tags,
        Some(user_id.clone()),
    );
    if let Some(control_mode) = params.control_mode {
        tool_po.control_mode = control_mode;
    }
    if let Some(status) = params.enabled {
        tool_po.status = if status {
            common::enums::tool::ToolStatus::Enabled
        } else {
            common::enums::tool::ToolStatus::Disabled
        };
    }
    let tool = Tool::from_po_for_management(tool_po);

    domain()
        .tool_provider_manage()
        .create_tool(ctx.clone(), &tool)
        .await?;

    Ok(CreateToolResponse {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: tool.po.description.clone(),
        tool_type: tool.po.protocol.to_string(),
        created_at: tool.po.created_at,
    })
}