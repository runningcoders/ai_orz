//! Handler: PUT /api/v1/skills/{skill_id} - Update skill metadata and content

use common::error::{err, bail_err, Result};
use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::{SkillFileImport, UpdateSkillParams, domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateSkillRequest, UpdateSkillResponse};

use super::response::to_detail;

/// Update an existing skill's metadata, main content, and add new attached files from uploads.
#[register_handler_tool(
    id = "update_skill",
    name = "update_skill",
    description = "Update an existing skill's metadata, main content, and add new attached files from uploads.",
    params = "common::api::UpdateSkillRequest"
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

    let file_writes = params
        .content
        .as_deref()
        .map(|content| vec![("skill.md", content)])
        .unwrap_or_default();

    let mut file_imports = Vec::new();
    for file in params.files.unwrap_or_default() {
        if file.attachment_id.trim().is_empty() {
            bail_err!(InvalidRequest, "attachment_id 不能为空");
        }
        if file.target_path.trim().is_empty() {
            bail_err!(InvalidRequest, "Skill 文件目标路径不能为空");
        }

        let attachment = finance_domain()
            .attachment_manage()
            .get_attachment(
                ctx.clone(),
                &file.attachment_id,
                AttachmentGetOptions {
                    include_file_content: true,
                },
            )
            .await?
            .ok_or_else(|| err!(InvalidRequest, "附件 {} 不存在或无权访问", file.attachment_id))?;

        let read_result = attachment.read_results.into_iter().next().ok_or_else(|| {
            err!(InvalidRequest, "附件 {} 内容读取失败", file.attachment_id)
        })?;

        file_imports.push(SkillFileImport {
            target_path: file.target_path,
            bytes: read_result.bytes,
        });
    }

    domain()
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                file_writes,
                file_deletes: vec![],
                file_imports,
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
