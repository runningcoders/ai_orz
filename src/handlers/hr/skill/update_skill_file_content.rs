//! Handler: PUT /api/v1/skills/{skill_id}/files/{*filename} - 创建或更新 Skill 文件

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, UpdateSkillFileContentRequest};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

/// PUT /hr/skills/{skill_id}/files/{filename}
///
/// 创建或更新 Skill 指定文件的 UTF-8 文本内容。
/// 如果文件不存在则创建，已存在则覆盖。支持乐观锁通过 expected_updated_at。
pub async fn update_skill_file_content_handler(
    Extension(ctx): Extension<RequestContext>,
    Path((skill_id, filename)): Path<(String, String)>,
    Json(req): Json<UpdateSkillFileContentRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    domain()
        .skill_manage()
        .update_skill_file_content(
            ctx,
            &skill_id,
            &filename,
            &req.content,
            req.expected_updated_at,
        )
        .await?;

    Ok(Json(ApiResponse::<()>::ok()))
}
