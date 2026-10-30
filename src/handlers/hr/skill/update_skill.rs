//! Handler: PUT /api/v1/skills/{skill_id} - Update skill metadata and content

use crate::pkg::RequestContext;
use crate::service::domain::hr::{SkillFileImport, UpdateSkillParams, domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateSkillRequest, UpdateSkillResponse, validate_filenames_path_safety};
use common::error::{Result, bail_err, err};

use super::file_import_sources::to_skill_file_imports;
use super::response::to_detail;

/// Update an existing skill's metadata and content from text, URL, or file attachments.
#[register_handler_tool(
    id = "update_skill",
    name = "Update Skill",
    description = "Update a skill's metadata (name, description, tags, category, status) and optionally replace its content from inline text, an HTTPS URL, or existing files referenced by attachment_id or artifact_id (each content_input.files item takes exactly one id; skill zip packages are unpacked into the skill directory, single files are copied with sources left untouched); individual files can also be deleted via file_deletes. Returns the updated skill detail. Status notes: in an agent context, setting status=Expired retires the skill — it is removed from your own skill set and no longer appears in your prompt, while the record stays recoverable (restore_skill); use this instead of deleting when a skill is outdated, unneeded, or wrong. Setting status=Published is rejected for agents (publishing goes through the shared library).",
    params = "common::api::UpdateSkillRequest",
    // 见 create_skill 同名注释：self_evolution 是自进化专用面，与通用技能管理分开。
    tags = "skill_management,self_evolution"
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
    // M2 修复 C：Agent 上下文禁止把技能（含安装副本）直改成正式发布
    super::validate_agent_skill_status_change(
        ctx.agent_id().is_some(),
        skill.po.author_type,
        &skill.po.parent_skill_id,
        params.status,
    )?;
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
                executable: None,
            });
        }

        if let Some(files) = &ci.files {
            imports.extend(to_skill_file_imports(ctx.clone(), files).await?);
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
