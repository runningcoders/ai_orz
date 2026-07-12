//! Skill management API request/response DTOs - shared between backend and frontend

use crate::enums::SkillStatus;
use crate::enums::skill::SkillAuthorType;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 创建 Skill 请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
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
    /// 初始多文件内容：filename -> content 映射，创建时直接生成所有文件。
    pub initial_files: Option<HashMap<String, String>>,
}

/// Skill 列表查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SkillSearchQuery {
    /// 关键词。
    pub keyword: Option<String>,
    /// 可选状态筛选。
    pub status: Option<SkillStatus>,
    /// 可选分类筛选。
    pub category: Option<String>,
    /// 可选作者筛选。
    pub author_id: Option<String>,
    /// 返回数量限制。
    pub limit: Option<usize>,
}

/// 更新 Skill 请求（原始）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateSkillRequestOriginal {
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
    /// 附加文件导入列表。
    pub files: Option<Vec<SkillFileInput>>,
}

/// Skill 附加文件导入输入。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SkillFileInput {
    /// 已上传的通用 Attachment ID。
    pub attachment_id: String,
    /// 导入到 Skill 内容目录下的目标相对路径。
    pub target_path: String,
}

/// Skill 文件摘要。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillFileItem {
    /// 文件名。
    pub filename: String,
    /// 文件大小（字节）。
    pub file_size: u64,
    /// 是否已预读内容。
    pub has_content: bool,
}

/// Skill 列表项响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

/// 创建 Skill 响应。
pub type CreateSkillResponse = SkillDetail;

/// 获取 Skill 请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 获取 Skill 响应。
pub type GetSkillResponse = SkillDetail;

/// Agent-Skill 安装请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct InstallSkillToAgentRequest {
    /// 源 Skill ID。
    #[param(source = "path")]
    pub skill_id: String,

    /// 目标 Agent ID。
    pub agent_id: String,
}

/// Agent-Skill 安装响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstallSkillToAgentResponse {
    /// Agent ID。
    pub agent_id: String,
    /// 源 Skill ID。
    pub source_skill_id: String,
    /// 安装后创建的 Agent 私有 Skill。
    pub skill: SkillDetail,
}

/// 列出 Skill 文件请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSkillFilesRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 列出 Skill 文件响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSkillFilesResponse {
    /// 文件列表。
    pub files: Vec<SkillFileItem>,
}

/// 获取 Skill 文件内容请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetSkillFileContentRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,

    /// 文件路径。
    #[param(source = "path")]
    pub filename: String,
}

/// 获取 Skill 文件内容响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetSkillFileContentResponse {
    /// 文件内容。
    pub content: String,
}

/// 更新 Skill 文件内容请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateSkillFileContentRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,

    /// 文件路径。
    #[param(source = "path")]
    pub filename: String,

    /// 文件内容。
    pub content: String,

    /// 乐观锁：期望的 Skill 更新时间戳（秒），不匹配返回 409 Conflict。
    pub expected_updated_at: Option<i64>,
}

/// 更新 Skill 文件内容响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateSkillFileContentResponse {}

/// 列出 Agent 安装的 Skills 请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentSkillsRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,
}

/// 列出 Agent 安装的 Skills 响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListAgentSkillsResponse {
    /// Skill 列表。
    pub skills: Vec<SkillListItem>,
}

/// Skill 列表查询请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSkillsRequest {
    /// 可选状态筛选。
    #[param(source = "query")]
    pub status: Option<SkillStatus>,

    /// 可选分类筛选。
    #[param(source = "query")]
    pub category: Option<String>,

    /// 可选作者筛选。
    #[param(source = "query")]
    pub author_id: Option<String>,

    /// 可选关键词筛选。
    #[param(source = "query")]
    pub keyword: Option<String>,

    /// 返回数量限制。
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// Skill 列表查询响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListSkillsResponse {
    /// Skill 列表。
    pub skills: Vec<SkillListItem>,
}

/// Skill 列表项响应别名（前端兼容）
pub type ListSkillsResponseItem = SkillListItem;

/// Skill 搜索请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchSkillsRequest {
    /// 搜索关键词。
    #[param(source = "query")]
    pub keyword: Option<String>,

    /// 可选状态筛选。
    #[param(source = "query")]
    pub status: Option<SkillStatus>,

    /// 可选分类筛选。
    #[param(source = "query")]
    pub category: Option<String>,

    /// 可选作者筛选。
    #[param(source = "query")]
    pub author_id: Option<String>,

    /// 返回数量限制。
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// Skill 搜索响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchSkillsResponse {
    /// 搜索结果列表。
    pub skills: Vec<SkillListItem>,
}

/// 更新 Skill 请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,

    /// 新技能名称。
    pub name: Option<String>,
    /// 新技能描述。
    pub description: Option<String>,
    /// 新标签列表。
    pub tags: Option<Vec<String>>,
    /// 新技能分类。
    pub category: Option<String>,
    /// 新技能状态。
    pub status: Option<SkillStatus>,
    /// 新主内容文件 skill.md 内容。
    pub content: Option<String>,
    /// 附加文件导入列表。
    pub files: Option<Vec<SkillFileInput>>,
}

/// 更新 Skill 响应。
pub type UpdateSkillResponse = SkillDetail;

/// 删除 Skill 请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 安装技能包请求。
///
/// 按 tag 批量安装已发布技能到 Agent 目录。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct InstallSkillPackRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,

    /// 技能包 tag（如 "project_management"）。
    #[param(source = "path")]
    pub tag: String,
}

/// 安装技能包响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstallSkillPackResponse {
    /// 成功安装的技能数量。
    pub installed_count: usize,
}

/// 卸载技能包请求。
///
/// 从 Agent 的 installed_skill_packs 中移除指定 tag，保留技能副本。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallSkillPackRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,

    /// 技能包 tag。
    #[param(source = "path")]
    pub tag: String,
}

/// 卸载技能包响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UninstallSkillPackResponse {}

/// 列出已安装技能包请求。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListInstalledSkillPacksRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,
}

/// 列出已安装技能包响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListSkillPacksResponse {
    /// 已安装的技能包 tags。
    pub skill_packs: Vec<String>,
}

/// 列出已安装技能包响应别名（前端兼容）
pub type ListInstalledSkillPacksResponse = ListSkillPacksResponse;

/// 删除 Skill 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSkillResponse {
    /// 是否删除成功
    pub success: bool,
}
