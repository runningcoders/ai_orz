//! Handler: GET /api/v1/skills/{skill_id}/files/{*filename} - 读取 Skill 文件内容

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, GetSkillFileContentResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// GET /hr/skills/{skill_id}/files/{filename}
///
/// 读取 Skill 指定文件的 UTF-8 文本内容。
pub async fn get_skill_file_content_handler(
    Extension(ctx): Extension<RequestContext>,
    Path((skill_id, filename)): Path<(String, String)>,
) -> Result<Json<ApiResponse<GetSkillFileContentResponse>>, AppError> {
    let result = domain()
        .skill_manage()
        .get_skill_file_content(ctx, &skill_id, &filename)
        .await?;

    match result {
        None => Err(AppError::NotFound(format!(
            "Skill or file not found: {}/{}",
            skill_id, filename
        ))),
        Some(content) => Ok(Json(ApiResponse::success(content))),
    }
}
