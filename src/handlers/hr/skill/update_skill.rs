//! Handler: PUT /api/v1/skills/{skill_id} - Update skill metadata and content

use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::{SkillFileImport, UpdateSkillParams, domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateSkillRequest, UpdateSkillResponse, validate_filenames_path_safety};
use common::error::{Result, bail_err, err};

use super::response::to_detail;

/// Update an existing skill's metadata and content from text, URL, or file attachments.
#[register_handler_tool(
    id = "update_skill",
    name = "Update Skill",
    description = "Update an existing skill's metadata and content from direct text, an HTTPS URL, or file attachments.",
    params = "common::api::UpdateSkillRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn update_skill(
    ctx: RequestContext,
    params: UpdateSkillRequest,
) -> Result<UpdateSkillResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let mut skill = domain()
        .skill_manage()
        .get_skill(ctx.clone(), &params.skill_id)
        .await?
        .ok_or_else(|| err!(NotFound, "Skill {} not found", params.skill_id))?;

    // 更新元数据
    if let Some(name) = params.name {
        if name.trim().is_empty() {
            bail_err!(InvalidRequest, "skill name 不能为空");
        }
        skill.po.name = name;
    }
    if let Some(description) = params.description {
        skill.po.description = description;
    }
    if let Some(tags) = params.tags {
        skill.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(category) = params.category {
        if category.trim().is_empty() {
            bail_err!(InvalidRequest, "skill category 不能为空");
        }
        skill.po.category = category;
    }
    if let Some(status) = params.status {
        skill.po.status = status;
    }
    skill.po.modifier_id = user_id;
    skill.po.updated_at = chrono::Utc::now().timestamp_millis();

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

    let file_deletes_owned: Vec<&str> = params
        .file_deletes
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|s| s.as_str())
        .collect();
    if !file_deletes_owned.is_empty() {
        validate_filenames_path_safety(params.file_deletes.as_deref().unwrap_or(&[]))
            .map_err(|e| err!(InvalidRequest, "{}", e))?;
    }

    domain()
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports,
                file_deletes: file_deletes_owned,
                remote_source,
            },
        )
        .await?;

    let updated = domain()
        .skill_manage()
        .get_skill(ctx, &params.skill_id)
        .await?
        .ok_or_else(|| err!(NotFound, "Skill {} not found", params.skill_id))?;

    Ok(to_detail(&updated))
}
