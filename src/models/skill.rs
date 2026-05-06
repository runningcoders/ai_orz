//! Skill 持久化对象

use common::enums::skill::SkillAuthorType;
use common::enums::SkillStatus;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 技能附属文件信息（纯数据结构，DAO 层使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    /// 文件名（相对于技能目录）
    pub filename: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 文件内容（小文件直接预读，大文件按需加载）
    pub content: Option<String>,
}

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

// ==================== Skill 业务聚合实体 ====================

/// 技能完整业务实体（PO + 文件系统内容）
/// 
/// - SkillPo：数据库持久化元数据
/// - files：关联文件列表（小文件预读，大文件按需加载）
#[derive(Debug, Clone)]
pub struct Skill {
    /// 数据库持久化元数据
    pub po: SkillPo,
    /// 关联文件列表
    pub files: Vec<SkillFile>,
}

impl Skill {
    /// 获取主文件内容（如果已加载）
    pub fn main_content(&self) -> Option<&str> {
        self.files.iter()
            .find(|f| f.filename == "skill.md")
            .and_then(|f| f.content.as_deref())
    }

    /// 获取指定文件名的内容（如果已加载）
    pub fn file_content(&self, filename: &str) -> Option<&str> {
        self.files.iter()
            .find(|f| f.filename == filename)
            .and_then(|f| f.content.as_deref())
    }
}
