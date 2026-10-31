//! Handler: POST /api/v1/skills - 创建新 Skill

use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use crate::service::domain::hr::{CreateSkillParams, SkillFileImport, domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateSkillRequest, CreateSkillResponse};
use common::enums::skill::SkillAuthorType;

use super::file_import_sources::to_skill_file_imports;
use super::response::to_detail;
use common::error::{Result, bail_err, err};

/// Create a new skill with optional content from text, URL, or file attachments. Returns the created skill detail.
#[register_handler_tool(
    id = "create_skill",
    name = "Create Skill",
    description = "Create a new skill with optional content sourced from inline text (stored as skill.md), an HTTPS URL, or existing files referenced by attachment_id or artifact_id. Each content_input.files item takes exactly one id: use attachment_id for uploaded attachments, artifact_id for files produced inside a task (skill zip packages are unpacked into the skill directory; single files are copied, sources are left untouched). For a zip package, item.target_path designates which entry inside the package is the root guide file (it is written as skill.md) — use it when the package has no skill.md or has several root Markdown files; for a single file, target_path is the destination name inside the skill. Owned by the calling agent when invoked in an agent context (agent-private skill, stored under the agent's skill directory), otherwise owned by the current user. Returns the created skill detail. Fails if the name is empty, a file item lacks exactly one source id, or a referenced attachment/artifact does not exist.",
    params = "common::api::CreateSkillRequest",
    // self_evolution 是「Agent 自进化」专用面：只有沉淀/自进化场景需要它，
    // 与 skill_management（通用技能管理，含删除/装卸等运维动作）分开，
    // 使 Settle 场景只需放通自进化这一类，而不必放通整个技能管理的破坏性工具。
    tags = "skill_management,self_evolution"
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
    // Agent 上下文（工具调用）：技能归属调用 Agent 本身，content_path 与
    // DAL install_to_agent 的副本路径约定（agents/{agent_id}/skills/{id}）保持一致；
    // 用户上下文（HTTP API）：归属当前用户，路径沿用 skills/{id}。
    let (author_id, author_type, content_path) = match ctx.agent_id() {
        Some(agent_id) => (
            agent_id.clone(),
            SkillAuthorType::Agent,
            format!("agents/{}/skills/{}", agent_id, skill_id),
        ),
        None => (
            user_id,
            SkillAuthorType::User,
            format!("skills/{}", skill_id),
        ),
    };
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
        author_id,
        author_type,
        content_path,
    );
    // M2 修复 C：Agent 上下文禁止创建即正式发布（发布收敛为共享库根技能单一入口）
    super::validate_agent_skill_status_change(
        ctx.agent_id().is_some(),
        skill_po.author_type,
        &skill_po.parent_skill_id,
        params.status,
    )?;
    // 发布闸门同样覆盖「创建即发布」：否则普通成员直接 create 一条 Published 就
    // 绕过了 `publish_skill` 的审核收口。Agent 上下文已由上一行拒绝；系统/预置链路
    // 走 domain 直调（`apply_preset_skills`），不经本 handler，因此不受此限。
    if params.status == Some(common::enums::SkillStatus::Published)
        && !crate::models::skill::can_publish_by_role(ctx.user_role())
    {
        bail_err!(
            Forbidden,
            "仅管理员及以上可以创建即发布的技能（当前角色={:?}）；\
             普通成员请先创建草稿，再由管理员或收口人 Agent 审核发布",
            ctx.user_role()
        );
    }
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

#[cfg(test)]
mod m2_status_guard_tests {
    use super::super::validate_agent_skill_status_change;
    use common::enums::skill::{SkillAuthorType, SkillStatus};

    /// M2 修复 C：Agent 上下文或 Agent 安装副本禁止 status 直改 Published；其余组合放行。
    #[test]
    fn agent_skill_status_publish_is_restricted() {
        // 条件①：Agent 上下文 + 目标 Published → 拒绝（无论形态）
        assert!(
            validate_agent_skill_status_change(
                true,
                SkillAuthorType::Agent,
                "",
                Some(SkillStatus::Published)
            )
            .is_err(),
            "Agent 上下文设置 Published 应被拒绝"
        );
        // Agent 上下文 + 草稿 → 放行
        assert!(
            validate_agent_skill_status_change(
                true,
                SkillAuthorType::Agent,
                "",
                Some(SkillStatus::Draft)
            )
            .is_ok()
        );
        // Agent 上下文 + 不改状态 → 放行
        assert!(validate_agent_skill_status_change(true, SkillAuthorType::Agent, "", None).is_ok());

        // 条件②：用户上下文 + Agent 安装副本（parent 非空）+ Published → 拒绝
        assert!(
            validate_agent_skill_status_change(
                false,
                SkillAuthorType::Agent,
                "source-x",
                Some(SkillStatus::Published)
            )
            .is_err(),
            "Agent 安装副本直改 Published 应被拒绝"
        );
        // 用户上下文 + Agent 自有草稿根技能（parent 空）+ Published → 放行
        assert!(
            validate_agent_skill_status_change(
                false,
                SkillAuthorType::Agent,
                "",
                Some(SkillStatus::Published)
            )
            .is_ok(),
            "Agent 自有草稿根技能由用户发布应放行"
        );
        // 用户上下文 + User 根技能 + Published → 放行（共享库根技能发布入口）
        assert!(
            validate_agent_skill_status_change(
                false,
                SkillAuthorType::User,
                "",
                Some(SkillStatus::Published)
            )
            .is_ok()
        );
    }
}
