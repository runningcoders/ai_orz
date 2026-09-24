//! Skill 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_skill;
pub mod delete_skill;
pub mod get_skill;
pub mod get_skill_file_content;
pub mod install_skill_to_agent;
pub mod list_agent_skills;
pub mod list_expired_agent_skills;
pub mod list_skill_files;
pub mod list_skill_tags;
pub mod list_skills;
pub mod query_skills;
pub mod response;
pub mod restore_skill;
pub mod search_skill;
pub mod search_skills;
pub mod uninstall_skill_from_agent;
pub mod update_skill;
pub mod update_skill_file_content;

pub use create_skill::create_skill_handler;
pub use delete_skill::delete_skill_handler;
pub use get_skill::get_skill_handler;
pub use get_skill_file_content::get_skill_file_content_handler;
pub use install_skill_to_agent::install_skill_to_agent_handler;
pub use list_agent_skills::list_agent_skills_handler;
pub use list_expired_agent_skills::list_expired_agent_skills_handler;
pub use list_skill_files::list_skill_files_handler;
pub use list_skill_tags::list_skill_tags_handler;
pub use list_skills::list_skills_handler;
pub use query_skills::query_skills_handler;
pub use restore_skill::restore_skill_handler;
pub use search_skill::search_skill_handler;
pub use search_skills::search_skills_handler;
pub use uninstall_skill_from_agent::uninstall_skill_from_agent_handler;
pub use update_skill::update_skill_handler;
pub use update_skill_file_content::update_skill_file_content_handler;

// ==================== M2 修复 C：技能状态变更服务端校验 ====================

/// Agent 上下文禁止将技能状态直接设置为正式发布（Published）。
///
/// M2 修复 C：发布动作收敛为共享库根技能的单一入口（用户上下文），
/// 防止 Agent 安装副本或自有草稿通过 create/update 直改 status 被刷成
/// Published，混入共享库与 Agent 持有列表。
/// Agent 上下文携带 status=Published 时拒绝（InvalidRequest），其余组合放行。
pub(crate) fn validate_agent_skill_status_change(
    is_agent_context: bool,
    author_type: common::enums::skill::SkillAuthorType,
    parent_skill_id: &str,
    requested_status: Option<common::enums::skill::SkillStatus>,
) -> common::error::Result<()> {
    use common::enums::skill::{SkillAuthorType, SkillStatus};
    if requested_status != Some(SkillStatus::Published) {
        return Ok(());
    }
    // 条件①：Agent 上下文（无论目标技能形态）不允许直改 Published
    if is_agent_context {
        common::error::bail_err!(
            InvalidRequest,
            "Agent 上下文不允许将技能状态直接设置为正式发布（Published）；发布请走共享库根技能发布流程"
        );
    }
    // 条件②：Agent 安装副本（author_type=Agent 且 parent 非空）不允许被直改 Published
    if author_type == SkillAuthorType::Agent && !parent_skill_id.is_empty() {
        common::error::bail_err!(
            InvalidRequest,
            "Agent 安装副本不允许直接设置为正式发布（Published）；发布请走共享库根技能发布流程"
        );
    }
    Ok(())
}
