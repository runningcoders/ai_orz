use common::api::{SkillDetail, SkillFileItem, SkillListItem};

use crate::models::skill::{Skill, SkillFile};
use common::bail_err;

pub(super) fn to_list_item(skill: &Skill) -> SkillListItem {
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
    }
}

fn to_file_item(file: &SkillFile) -> SkillFileItem {
    SkillFileItem {
        filename: file.filename.clone(),
        file_size: file.file_size,
        has_content: file.content.is_some(),
    }
}
