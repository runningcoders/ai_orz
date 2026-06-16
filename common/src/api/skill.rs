//! Skill management API request/response DTOs - shared between backend and frontend

use crate::enums::SkillStatus;
use crate::enums::skill::SkillAuthorType;
use serde::{Deserialize, Serialize};

/// 创建 Skill 请求。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateSkillRequest {
    /// 技能名称。
    pub name: String,
    /// 技能描述。
    pub description: String,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 技能分类；为空时默认 uncategorized。
    pub category: Option<String>,
    /// 初始状态；为空时默认 Draft。
    pub status: Option<SkillStatus>,
    /// 主内容文件 skill.md 内容。
    pub content: Option<String>,
}

/// Skill 列表查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SkillListQuery {
    /// 可选状态筛选。
    pub status: Option<SkillStatus>,
    /// 可选分类筛选。
    pub category: Option<String>,
    /// 可选作者筛选。
    pub author_id: Option<String>,
    /// 可选关键词筛选。
    pub keyword: Option<String>,
    /// 返回数量限制。
    pub limit: Option<usize>,
}

/// Skill 搜索查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SkillSearchQuery {
    /// 关键词。
    pub keyword: Option<String>,
    /// 可选状态筛选。
    pub status: Option<SkillStatus>,
    /// 可选分类筛选。
    pub category: Option<String>,
    /// 返回数量限制。
    pub limit: Option<usize>,
}

/// 更新 Skill 请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct UpdateSkillRequest {
    /// 技能名称。
    pub name: Option<String>,
    /// 技能描述。
    pub description: Option<String>,
    /// 标签列表。
    pub tags: Option<Vec<String>>,
    /// 技能分类。
    pub category: Option<String>,
    /// 技能状态。
    pub status: Option<SkillStatus>,
    /// 主内容文件 skill.md 内容。
    pub content: Option<String>,
}

/// Skill 文件摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFileItem {
    /// 文件名。
    pub filename: String,
    /// 文件大小（字节）。
    pub file_size: u64,
    /// 是否已预读内容。
    pub has_content: bool,
}

/// Skill 列表项响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillListItem {
    /// Skill ID。
    pub id: String,
    /// 技能名称。
    pub name: String,
    /// 技能描述。
    pub description: String,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 技能分类。
    pub category: String,
    /// 父技能 ID。
    pub parent_skill_id: String,
    /// 作者 ID。
    pub author_id: String,
    /// 作者类型。
    pub author_type: SkillAuthorType,
    /// 技能状态。
    pub status: SkillStatus,
    /// 创建时间戳。
    pub created_at: i64,
    /// 更新时间戳。
    pub updated_at: i64,
}

/// Skill 详情响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    /// Skill ID。
    pub id: String,
    /// 技能名称。
    pub name: String,
    /// 技能描述。
    pub description: String,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 技能分类。
    pub category: String,
    /// 父技能 ID。
    pub parent_skill_id: String,
    /// 作者 ID。
    pub author_id: String,
    /// 作者类型。
    pub author_type: SkillAuthorType,
    /// 最后修改人 ID。
    pub modifier_id: String,
    /// 技能状态。
    pub status: SkillStatus,
    /// 主内容文件 skill.md 内容。
    pub content: Option<String>,
    /// 文件列表摘要。
    pub files: Vec<SkillFileItem>,
    /// 创建时间戳。
    pub created_at: i64,
    /// 更新时间戳。
    pub updated_at: i64,
}

/// Agent-Skill 安装响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallSkillToAgentResponse {
    /// Agent ID。
    pub agent_id: String,
    /// 源 Skill ID。
    pub source_skill_id: String,
    /// 安装后创建的 Agent 私有 Skill。
    pub skill: SkillDetail,
}

/// 创建 Skill 响应。
pub type CreateSkillResponse = SkillDetail;

/// 获取 Skill 响应。
pub type GetSkillResponse = SkillDetail;

/// 更新 Skill 响应。
pub type UpdateSkillResponse = SkillDetail;
