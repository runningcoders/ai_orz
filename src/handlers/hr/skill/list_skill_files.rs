//! Handler: GET /api/v1/skills/{skill_id}/files - 列出 Skill 所有文件

use axum::{
    Json,
    extract::{Extension, Path},
};
use common::api::{ApiResponse, ListSkillFilesResponse};

use crate::error::AppError;
use crate::models::skill::SkillFile;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use common::api::SkillFileItem;

/// GET /hr/skills/{skill_id}/files
///
/// 列出 Skill 目录下所有文件，返回文件名、大小和 has_content 标记。
pub async fn list_skill_files_handler(
    Extension(ctx): Extension<RequestContext>,
    Path((skill_id,)): Path<(String,)>,
) -> Result<Json<ApiResponse<ListSkillFilesResponse>>, AppError> {
    let result = domain()
        .skill_manage()
        .list_skill_files(ctx, &skill_id)
        .await?;

    match result {
        None => Err(AppError::NotFound(format!("Skill not found: {}", skill_id))),
        Some(files) => {
            // 转换内部 SkillFile 到 API SkillFileItem
            let file_items: Vec<SkillFileItem> = files
                .into_iter()
                .map(|f| SkillFileItem {
                    filename: f.filename,
                    file_size: f.file_size,
                    has_content: f.content.is_some(),
                })
                .collect();

            Ok(Json(ApiResponse::success(ListSkillFilesResponse {
                files: file_items,
            })))
        }
    }
}
