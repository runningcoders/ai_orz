//! 创建 Tool

use axum::{
    extract::{Extension, Json},
    http::StatusCode,
};
use common::api::{ApiResponse, CreateToolRequest, CreateToolResponse};
use common::enums::ToolProtocol;

use crate::error::AppError;
use crate::models::tool::{Tool, ToolPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 创建 Tool
/// POST /tools
pub async fn create_tool(
    Extension(ctx): Extension<RequestContext>,
    Json(req): Json<CreateToolRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateToolResponse>>), AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }
    if matches!(req.protocol, ToolProtocol::Builtin) {
        return Err(AppError::BadRequest(
            "内置 Tool 由系统同步，不允许通过管理接口创建".to_string(),
        ));
    }

    let mut tool_po = ToolPo::new(
        String::new(),
        req.name.clone(),
        req.description.clone(),
        req.protocol,
        req.config.clone(),
        req.parameters_schema.clone(),
        req.tags.clone(),
        Some(user_id.clone()),
    );
    if let Some(control_mode) = req.control_mode {
        tool_po.control_mode = control_mode;
    }
    if let Some(status) = req.status {
        tool_po.status = status;
    }
    let tool = Tool::from_po_for_management(tool_po);

    domain()
        .tool_provider_manage()
        .create_tool(ctx, &tool)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(to_detail(&tool))),
    ))
}
