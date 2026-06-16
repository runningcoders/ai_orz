//! 创建 Skill

use axum::{
    Json,
    extract::{Extension, Json as RequestJson},
    http::StatusCode,
};
use common::api::{ApiResponse, CreateSkillRequest, CreateSkillResponse};
use common::enums::skill::SkillAuthorType;

use crate::error::AppError;
use crate::models::skill::{Skill, SkillFile, SkillPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;

use super::response::to_detail;

/// 创建 Skill
/// POST /hr/skills
pub async fn create_skill(
    Extension(ctx): Extension<RequestContext>,
    RequestJson(req): RequestJson<CreateSkillRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateSkillResponse>>), AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("技能名称不能为空".to_string()));
    }

    let skill_id = uuid::Uuid::now_v7().to_string();
    let mut skill_po = SkillPo::new(
        skill_id.clone(),
        req.name,
        req.description,
        req.tags,
        req.category
            .filter(|category| !category.trim().is_empty())
            .unwrap_or_else(|| "uncategorized".to_string()),
        String::new(),
        user_id,
        SkillAuthorType::User,
        format!("skills/{}", skill_id),
    );
    if let Some(status) = req.status {
        skill_po.status = status;
    }

    let mut skill = Skill::from_po(skill_po);
    if let Some(content) = req.content {
        skill.files.push(SkillFile {
            filename: "skill.md".to_string(),
            file_size: content.len() as u64,
            content: Some(content),
        });
    }

    domain().skill_manage().create_skill(ctx, &skill).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(to_detail(&skill))),
    ))
}
