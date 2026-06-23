//! Handler: POST /api/v1/skills - 创建新 Skill

use crate::error::AppError;
use crate::models::skill::{Skill, SkillFile, SkillPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateSkillRequest, CreateSkillResponse};
use common::enums::skill::SkillAuthorType;

use super::response::to_detail;

/// Create a new empty skill with optional initial content and files. Returns the created skill detail.
#[register_handler_tool(
    id = "create_skill",
    name = "create_skill",
    description = "Create a new skill. You can provide initial markdown content and multiple files. Returns the created skill detail.",
    params = "common::api::CreateSkillRequest"
)]
#[generate_http_handler]
pub async fn create_skill(
    ctx: RequestContext,
    params: CreateSkillRequest,
) -> Result<CreateSkillResponse, AppError> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        return Err(AppError::BadRequest("当前请求缺少用户上下文".to_string()));
    }
    if params.name.trim().is_empty() {
        return Err(AppError::BadRequest("技能名称不能为空".to_string()));
    }

    let skill_id = uuid::Uuid::now_v7().to_string();
    let mut skill_po = SkillPo::new(
        skill_id.clone(),
        params.name,
        params.description,
        params.tags,
        params
            .category
            .filter(|category| !category.trim().is_empty())
            .unwrap_or_else(|| "uncategorized".to_string()),
        String::new(),
        user_id,
        SkillAuthorType::User,
        format!("skills/{}", skill_id),
    );
    if let Some(status) = params.status {
        skill_po.status = status;
    }

    let mut skill = Skill::from_po(skill_po);
    if let Some(content) = params.content {
        skill.files.push(SkillFile {
            filename: "skill.md".to_string(),
            file_size: content.len() as u64,
            content: Some(content),
        });
    }

    // 处理初始多文件
    if let Some(initial_files) = params.initial_files {
        for (filename, content) in initial_files {
            // 跳过 skill.md（已经由 content 处理了，如果重复保留 content）
            if filename == "skill.md" {
                continue;
            }
            // 校验文件名合法性，防止路径遍历攻击
            crate::service::domain::hr::skill::validate_skill_import_target_path(&filename)?;
            skill.files.push(SkillFile {
                filename,
                file_size: content.len() as u64,
                content: Some(content),
            });
        }
    }

    domain()
        .skill_manage()
        .create_skill(ctx.clone(), &skill)
        .await?;

    let created = domain()
        .skill_manage()
        .get_skill(ctx, &skill_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Skill {} not found", skill_id)))?;

    Ok(to_detail(&created))
}
