//! Handler: POST /api/v1/skills - 创建新 Skill

use crate::models::attachment::AttachmentGetOptions;
use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::{CreateSkillParams, SkillFileImport, domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateSkillRequest, CreateSkillResponse};
use common::enums::skill::SkillAuthorType;

use super::response::to_detail;
use common::error::{Result, bail_err, err};

/// Create a new skill with optional content from text, URL, or file attachments. Returns the created skill detail.
#[register_handler_tool(
    id = "create_skill",
    name = "create_skill",
    description = "Create a new skill. You can provide content from direct text, an HTTPS URL, or file attachments. Returns the created skill detail.",
    params = "common::api::CreateSkillRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn create_skill(
    ctx: RequestContext,
    params: CreateSkillRequest,
) -> Result<CreateSkillResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.name.trim().is_empty() {
        bail_err!(InvalidRequest, "skill name 不能为空");
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

    let skill = Skill::from_po(skill_po);

    let mut imports: Vec<SkillFileImport> = Vec::new();
    let mut remote_source: Option<&str> = None;

    if let Some(ci) = &params.content_input {
        ci.validate_all()
            .map_err(|e| err!(InvalidRequest, "{}", e))?;

        if let Some(content) = &ci.content
            && !content.is_empty()
        {
            imports.push(SkillFileImport {
                target_path: Some("skill.md".to_string()),
                source_abs_path: None,
                content_bytes: Some(content.as_bytes().to_vec()),
                suggested_name: None,
            });
        }

        if let Some(files) = &ci.files {
            for file in files {
                if file.attachment_id.trim().is_empty() {
                    bail_err!(InvalidRequest, "attachment_id 不能为空");
                }

                let attachment = finance_domain()
                    .attachment_manage()
                    .get_attachment(
                        ctx.clone(),
                        &file.attachment_id,
                        AttachmentGetOptions {
                            include_file_content: false,
                        },
                    )
                    .await?
                    .ok_or_else(|| {
                        err!(
                            InvalidRequest,
                            "附件 {} 不存在或无权访问",
                            file.attachment_id
                        )
                    })?;

                let abs_path = finance_domain()
                    .attachment_manage()
                    .file_abs_path(&attachment)?;

                let target_path = if file.target_path.trim().is_empty() {
                    None
                } else {
                    Some(file.target_path.clone())
                };

                imports.push(SkillFileImport {
                    target_path,
                    source_abs_path: Some(abs_path),
                    content_bytes: None,
                    suggested_name: Some(attachment.po.original_name.clone()),
                });
            }
        }

        if let Some(url) = &ci.url
            && !url.is_empty()
        {
            remote_source = Some(url.as_str());
        }
    }

    let create_params = CreateSkillParams {
        skill: &skill,
        imports,
        remote_source,
    };

    domain()
        .skill_manage()
        .create_skill(ctx.clone(), create_params)
        .await?;

    let created = domain()
        .skill_manage()
        .get_skill(ctx, &skill_id)
        .await?
        .ok_or_else(|| err!(NotFound, "Skill {} not found", skill_id))?;

    Ok(to_detail(&created))
}
