//! 列出 Project

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use axum::{
    Json,
    extract::{Extension, Query},
};
use common::api::{ApiResponse, ListProjectsQuery, ProjectListItem};

use super::response;

/// 列出 Project
/// GET /projects
pub async fn list_projects(
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Json<ApiResponse<Vec<ProjectListItem>>>, AppError> {
    let root_user_id = query.root_user_id.unwrap_or_else(|| ctx.uid());
    if root_user_id.is_empty() {
        return Err(AppError::BadRequest("root_user_id 不能为空".to_string()));
    }

    let projects = domain()
        .project_manage()
        .list(ctx, &root_user_id, query.status, query.limit)
        .await?;
    let response_items = projects.iter().map(response::to_list_item).collect();

    Ok(Json(ApiResponse::success(response_items)))
}
