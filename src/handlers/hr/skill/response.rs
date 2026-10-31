use common::api::{SkillDetail, SkillFileItem, SkillListItem};

use crate::models::skill::{Skill, SkillFile};

/// 技能 → 列表项 DTO（领域打包方法）。
///
/// 供本域各 handler 复用（如 get_agent 装配技能全景），避免各处重复实现转换。
pub(crate) fn to_list_item(skill: &Skill) -> SkillListItem {
    SkillListItem {
        id: skill.po.id.clone(),
        name: skill.po.name.clone(),
        description: skill.po.description.clone(),
        tags: skill.po.parse_tags(),
        category: skill.po.category.clone(),
        parent_skill_id: skill.po.parent_skill_id.clone(),
        author_id: skill.po.author_id.clone(),
        author_type: skill.po.author_type,
        status: skill.po.status,
        created_at: skill.po.created_at,
        updated_at: skill.po.updated_at,
    }
}

pub(super) fn to_detail(skill: &Skill) -> SkillDetail {
    SkillDetail {
        id: skill.po.id.clone(),
        name: skill.po.name.clone(),
        description: skill.po.description.clone(),
        tags: skill.po.parse_tags(),
        category: skill.po.category.clone(),
        parent_skill_id: skill.po.parent_skill_id.clone(),
        author_id: skill.po.author_id.clone(),
        author_type: skill.po.author_type,
        modifier_id: skill.po.modifier_id.clone(),
        status: skill.po.status,
        content: skill.main_content().map(ToString::to_string),
        files: skill.files.iter().map(to_file_item).collect(),
        created_at: skill.po.created_at,
        updated_at: skill.po.updated_at,
        // 默认 false：只有详情入口（`get_skill`）知道调用方角色，由它覆写成真实值。
        // 其它入口（create/update/publish/restore）返回的详情不参与「按钮是否显示」，
        // 保守取 false 可以避免「改完描述后按钮突然出现在普通成员面前」。
        can_publish: false,
    }
}

fn to_file_item(file: &SkillFile) -> SkillFileItem {
    SkillFileItem {
        filename: file.filename.clone(),
        file_size: file.file_size,
        // is_main 由 DAO 扫盘时按磁盘实况标注（旧包的主文件可能不叫 skill.md）
        is_main: file.is_main,
        abs_path: file.abs_path.to_string_lossy().to_string(),
        executable: file.executable,
    }
}
