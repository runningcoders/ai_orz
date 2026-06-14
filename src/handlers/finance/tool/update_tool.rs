//! 更新 Tool

use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateToolRequest, UpdateToolResponse};
use common::enums::ToolProtocol;

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 更新 Tool
/// PUT /tools/{id}
pub async fn update_tool(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateToolRequest>,
) -> Result<AxumJson<ApiResponse<UpdateToolResponse>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Tool {} not found", id)))?;

    if matches!(tool.po.protocol, ToolProtocol::Builtin) {
        return Err(AppError::BadRequest(
            "内置 Tool 不允许通过管理接口修改".to_string(),
        ));
    }
    if matches!(req.protocol, Some(ToolProtocol::Builtin)) {
        return Err(AppError::BadRequest(
            "非内置 Tool 不允许被修改为内置协议".to_string(),
        ));
    }

    if let Some(name) = req.name {
        tool.po.name = name;
    }
    if let Some(description) = req.description {
        tool.po.description = description;
    }
    if let Some(protocol) = req.protocol {
        tool.po.protocol = protocol;
    }
    if let Some(control_mode) = req.control_mode {
        tool.po.control_mode = control_mode;
    }
    if let Some(config) = req.config {
        tool.po.config = config;
    }
    if let Some(parameters_schema) = req.parameters_schema {
        tool.po.parameters_schema = Some(parameters_schema);
    }
    if let Some(tags) = req.tags {
        tool.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    }
    tool.po.touch(Some(user_id));

    domain()
        .tool_provider_manage()
        .update_tool(ctx, &tool)
        .await?;

    Ok(AxumJson(ApiResponse::success(to_detail(&tool))))
}
