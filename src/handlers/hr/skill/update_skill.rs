//! 更新 Skill

use axum::{
    Json as AxumJson,
    extract::{Extension, Json, Path},
};
use common::api::{ApiResponse, UpdateSkillRequest, UpdateSkillResponse};

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::domain::hr::{UpdateSkillParams, domain};

use super::response::to_detail;

/// 更新 Skill
/// PUT /hr/skills/{id}
pub async fn update_skill(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSkillRequest>,
) -> Result<AxumJson<ApiResponse<UpdateSkillResponse>>, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }

    let mut skill = domain()
        .skill_manage()
        .get_skill(ctx.clone(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", id)))?;

    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest("技能名称不能为空".to_string()));
        }
        skill.po.name = name;
    }
    if let Some(description) = req.description {
        skill.po.description = description;
    }
    if let Some(tags) = req.tags {
        skill.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(category) = req.category {
        if category.trim().is_empty() {
            return Err(AppError::BadRequest("技能分类不能为空".to_string()));
        }
        skill.po.category = category;
    }
    if let Some(status) = req.status {
        skill.po.status = status;
    }
    skill.po.modifier_id = user_id;
    skill.po.updated_at = chrono::Utc::now().timestamp_millis();

    let file_writes = req
        .content
        .as_deref()
        .map(|content| vec![("skill.md", content)])
        .unwrap_or_default();

    domain()
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                file_writes,
                file_deletes: vec![],
            },
        )
        .await?;

    let updated = domain()
        .skill_manage()
        .get_skill(ctx, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", id)))?;

    Ok(AxumJson(ApiResponse::success(to_detail(&updated))))
}
