//! Skill 持久化对象

use common::enums::skill::SkillAuthorType;
use common::enums::SkillStatus;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Skill 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SkillPo {
    /// 技能ID: "name-slug--hash"
    pub id: String,
    /// 技能显示名称
    pub name: String,
    /// 技能描述：什么时候用这个技能
    pub description: String,
    /// JSON 数组：标签列表
    pub tags: String,
    /// 单一分类
    pub category: String,
    /// 父技能ID（继承来源，技能树演进）
    pub parent_skill_id: String,
    /// 创建人ID（用户ID 或 Agent ID）
    pub author_id: String,
    /// 作者类型
    pub author_type: SkillAuthorType,
    /// 最后修改人ID（用户ID 或 Agent ID）
    pub modifier_id: String,
    /// 技能状态
    pub status: SkillStatus,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
    /// 相对 base_data_path 的技能目录路径
    pub content_path: String,
}

impl SkillPo {
    /// 创建新的 SkillPo
    pub fn new(
        id: String,
        name: String,
        description: String,
        tags: Vec<String>,
        category: String,
        parent_skill_id: String,
        author_id: String,
        author_type: SkillAuthorType,
        content_path: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        SkillPo {
            id,
            name,
            description,
            tags: serde_json::to_string(&tags).unwrap_or_default(),
            category,
            parent_skill_id,
            author_id: author_id.clone(),
            author_type,
            modifier_id: author_id,
            status: SkillStatus::default(),
            created_at: now,
            updated_at: now,
            content_path,
        }
    }

    /// 解析 tags 为 Vec<String>
    pub fn parse_tags(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }
}
