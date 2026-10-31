//! Skill management API request/response DTOs - shared between backend and frontend

/// 技能主文件名（根引导文件）——唯一经接口直出内容的技能文件。
pub const SKILL_MAIN_FILE: &str = "skill.md";

use crate::api::{PagedResult, PaginationParams};
use crate::enums::SkillStatus;
use crate::enums::skill::SkillAuthorType;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
/// 技能内容输入源（3 种内容源统一子结构）。
///
/// 前后端共享的纯方法复用群（is_empty / classify / validate_*）集中在 common DTO impl，
/// 避免双端校验逻辑漂移。与后端同源，规则变动需同步。
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct SkillContentInput {
    /// 直接文本内容（skill.md 主文件）。
    pub content: Option<String>,
    /// HTTPS URL（单 md 或技能 zip 包，zip 解压后 skill.md 须在根目录）。
    pub url: Option<String>,
    /// 文件装配列表（每项 = attachment_id 或 artifact_id 二选一 + target_path 映射）。
    pub files: Option<Vec<SkillFileInput>>,
}

/// 技能内容源分类（6 变体）。
///
/// 优先级：files > content > url。content 与 url 同时 Some 时为 MixedContentOverridesUrl
/// （提示用户忽略 URL）；files + content 同时 Some 为 MixedTextAttachments（两者均处理）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillContentKind {
    /// 无内容输入。
    None,
    /// 仅直接文本。
    DirectText,
    /// 仅 URL。
    RemoteUrl,
    /// 仅文件来源（附件 attachment 或任务产物 artifact）。
    Attachments,
    /// 文本 + URL 同时提供（文本优先，URL 被忽略）。
    MixedContentOverridesUrl,
    /// 文件来源（附件 / 产物）+ 文本同时提供（两者均处理）。
    MixedTextAttachments,
}

impl SkillContentInput {
    /// 是否所有内容源都为空。
    pub fn is_empty(&self) -> bool {
        self.classify() == SkillContentKind::None
    }

    /// 分类内容源组合（6 变体）。
    pub fn classify(&self) -> SkillContentKind {
        let has_content = self.content.as_ref().is_some_and(|c| !c.is_empty());
        let has_url = self.url.as_ref().is_some_and(|u| !u.is_empty());
        let has_files = self.files.as_ref().is_some_and(|f| !f.is_empty());

        match (has_content, has_url, has_files) {
            (false, false, false) => SkillContentKind::None,
            (true, false, false) => SkillContentKind::DirectText,
            (false, true, false) => SkillContentKind::RemoteUrl,
            (false, false, true) => SkillContentKind::Attachments,
            (true, true, false) => SkillContentKind::MixedContentOverridesUrl,
            (true, _, true) => SkillContentKind::MixedTextAttachments,
            (false, true, true) => SkillContentKind::Attachments,
        }
    }

    /// 校验 URL 必须为 HTTPS（纯字符串前缀校验，前后端共享）。
    pub fn validate_url_https_only(&self) -> Result<(), String> {
        if let Some(url) = &self.url
            && !url.is_empty()
            && !url.starts_with("https://")
        {
            return Err(format!("URL 必须为 HTTPS 协议: {}", url));
        }
        Ok(())
    }

    /// 校验附件 target_path 唯一性（纯逻辑，前后端共享）。
    pub fn validate_files_unique_target(&self) -> Result<(), String> {
        if let Some(files) = &self.files {
            let mut seen = std::collections::HashSet::new();
            for f in files {
                let tp = f.target_path.trim();
                if tp.is_empty() {
                    continue;
                }
                if !seen.insert(tp) {
                    return Err(format!("target_path 重复: {}", tp));
                }
            }
        }
        Ok(())
    }

    /// 校验附件 target_path 路径安全（禁止路径穿越）。
    pub fn validate_files_path_safety(&self) -> Result<(), String> {
        if let Some(files) = &self.files {
            for f in files {
                let path = f.target_path.trim();
                if path.is_empty() {
                    continue;
                }
                if path.contains("..") || path.starts_with('/') || path.contains('\\') {
                    return Err(format!("target_path 存在路径穿越风险: {}", path));
                }
            }
        }
        Ok(())
    }

    /// 校验每个文件条目**有且仅有一个来源**（attachment_id / artifact_id）。
    ///
    /// 两个都给 → 无法确定该读哪一份；两个都空 → 下游只能报语义含糊的
    /// “无内容来源”。在入口一次说清，Agent 与前端拿到都可据此改参重试。
    pub fn validate_files_single_source(&self) -> Result<(), String> {
        if let Some(files) = &self.files {
            for (idx, f) in files.iter().enumerate() {
                let n = idx + 1;
                match (f.has_attachment(), f.has_artifact()) {
                    (true, true) => {
                        return Err(format!(
                            "第 {n} 个文件同时提供了 attachment_id 与 artifact_id，二者只能二选一"
                        ));
                    }
                    (false, false) => {
                        return Err(format!(
                            "第 {n} 个文件缺少来源：必须提供 attachment_id 或 artifact_id 之一"
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// 综合校验（URL HTTPS + target 唯一 + 路径安全 + 来源唯一）。
    pub fn validate_all(&self) -> Result<(), String> {
        self.validate_url_https_only()?;
        self.validate_files_unique_target()?;
        self.validate_files_path_safety()?;
        self.validate_files_single_source()?;
        Ok(())
    }
}

/// 校验文件名列表路径安全（公共函数，供 file_deletes 等场景复用）。
///
/// 与 SkillContentInput 的列表校验不同：这里所有 names 都必须非空（不存在空字符串跳过场景，
/// 因为空文件名本身无意义），任何含 `..` / 以 `/` 开头 / 含 `\` 的路径都视为路径穿越。
pub fn validate_filenames_path_safety(names: &[String]) -> Result<(), String> {
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("文件名不能为空".to_string());
        }
        if trimmed.contains("..") || trimmed.starts_with('/') || trimmed.contains('\\') {
            return Err(format!("文件名存在路径穿越风险: {}", trimmed));
        }
    }
    Ok(())
}

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
    /// 内容输入源（文本 / URL / 附件三选一或组合）。
    pub content_input: Option<SkillContentInput>,
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
    /// 内容输入源（文本 / URL / 附件三选一或组合）。
    pub content_input: Option<SkillContentInput>,
}

/// Skill 附加文件导入输入。
///
/// 文件来源二选一（**有且仅有一个**）：`attachment_id`（通用附件）或 `artifact_id`
/// （任务产出物）。任务过程中生成的技能 zip 包属于后者，不经由附件中转。
///
/// 两者同时给出、或同时为空，都由 [`SkillContentInput::validate_files_single_source`]
/// 在入口拦下——不要让调用方走到下游才撞低级报错。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct SkillFileInput {
    /// 已上传的通用 Attachment ID（与 artifact_id 二选一）。
    pub attachment_id: Option<String>,
    /// 任务产出 Artifact ID（与 attachment_id 二选一；须为 GeneratedContent 型）。
    pub artifact_id: Option<String>,
    /// 导入到 Skill 内容目录下的目标相对路径。
    ///
    /// 两种语义（由来源形态决定）：
    /// - **单文件**：落盘名（如 `scripts/run.sh`；md 不写时按 `default_target_path` 推断）；
    /// - **zip 技能包**：**指定包内哪个条目是主文件**（如 `main.md`），该条目会被归位
    ///   写成小写 `skill.md`。包内没有 `skill.md`、或根目录有多个 md 时用它消歧。
    ///
    /// 指定的条目必须存在于包内（忽略大小写）且位于 zip 根目录，否则入口报错。
    pub target_path: String,
}

/// 附件未指定 target_path 时的默认归位规则（前后端 / 后端导入共用 SSOT）。
///
/// 图片 → `assets/`、脚本 → `scripts/`、Markdown → `skill.md`、其余保持原名。
/// 前端上传后预填这个值，后端导入推断链第 4 级也走它，避免双标漂移
/// （前端把 `a.png` 归到 `assets/a.png`、后端却按原名平铺）。
pub fn default_target_path(original_name: &str) -> String {
    let ext = original_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => format!("assets/{}", original_name),
        "py" | "js" | "sh" | "ts" => format!("scripts/{}", original_name),
        "md" => SKILL_MAIN_FILE.to_string(),
        _ => original_name.to_string(),
    }
}

impl SkillFileInput {
    /// 是否提供了 attachment_id（空串 / 纯空白视同未提供）。
    pub fn has_attachment(&self) -> bool {
        self.attachment_id
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    }

    /// 是否提供了 artifact_id（空串 / 纯空白视同未提供）。
    pub fn has_artifact(&self) -> bool {
        self.artifact_id
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    }
}

/// Skill 文件摘要。
///
/// **访问契约（前后端 / Agent 共用）**：接口只直出根 `skill.md` 的内容
/// （`is_main=true`，走 `get_skill_file_content` 或 `SkillDetail.content`）；
/// 其余附件**不经过接口回传内容**，只给 `abs_path`，由 Agent 用 `read_file`
/// 工具按需读、用 shell 工具按需执行（技能包里的脚本/可执行文件）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillFileItem {
    /// 文件名（相对技能目录，可含子目录前缀，如 `scripts/run.sh`）。
    pub filename: String,
    /// 文件大小（字节）。
    pub file_size: u64,
    /// 是否为根引导文件 `skill.md`（唯一可通过接口直读内容的文件）。
    pub is_main: bool,
    /// 文件在磁盘上的绝对路径，供 read_file / shell 工具按需访问。
    pub abs_path: String,
    /// 是否带可执行位（脚本类附件可用 shell 工具执行）。
    pub executable: bool,
}

/// Skill 列表项响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
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
    /// 当前调用方是否有权发布**这个**技能。
    ///
    /// 发布门槛：仅 Admin / SuperAdmin 用户（详见 `models::skill::can_publish_by_role`）。
    /// 前端据此决定是否显示「发布」按钮——普通成员的技能只能留在 Draft，由管理员或
    /// 收口人 Agent 审核后发布。默认 `false`：拿不到判定依据时宁可不显示按钮，
    /// 也不要让用户点出一个 403。
    #[serde(default)]
    pub can_publish: bool,
}

/// 创建 Skill 响应。
pub type CreateSkillResponse = SkillDetail;

/// 获取 Skill 请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 获取 Skill 响应。
pub type GetSkillResponse = SkillDetail;

/// Agent-Skill 安装请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct InstallSkillToAgentRequest {
    /// 目标 Agent ID。
    #[param(source = "path")]
    pub agent_id: String,

    /// 源 Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListAgentSkillsRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,
}

/// 列出 Agent 安装的 Skills 响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListAgentSkillsResponse {
    /// Skill 列表。
    pub skills: Vec<SkillListItem>,
}

/// Skill 列表查询请求（语法糖：只接受分页参数，内部固定排除 Expired + updated_at DESC）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSkillsRequest {
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// Skill 列表查询响应。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListSkillsResponse {
    /// Skill 列表。
    pub skills: Vec<SkillListItem>,
}

/// Skill 通用查询请求（POST body，支持完整查询能力）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SkillQueryRequest {
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 关键词搜索
    pub keyword: Option<String>,
    /// 状态
    pub status: Option<SkillStatus>,
    /// 分类
    pub category: Option<String>,
    /// 作者 ID
    pub author_id: Option<String>,
    /// 作者类型（0=用户，1=Agent），用于区分"我创建的技能" vs "Agent 同步过来的技能"
    pub author_type: Option<SkillAuthorType>,
    /// 父技能 ID
    pub parent_skill_id: Option<String>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Skill 列表项响应别名（前端兼容）
pub type ListSkillsResponseItem = SkillListItem;

/// Skill 搜索请求（POST body，支持完整过滤条件 + 关键词搜索）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchSkillsRequest {
    /// 搜索关键词（支持 FTS5 全文搜索 + 向量语义搜索）
    pub keyword: Option<String>,
    /// 按 ID 批量查询
    pub ids: Option<Vec<String>>,
    /// 状态筛选
    pub status: Option<SkillStatus>,
    /// 分类
    pub category: Option<String>,
    /// 作者 ID
    pub author_id: Option<String>,
    /// 作者类型（0=用户，1=Agent），与 SkillQueryRequest 语义一致
    pub author_type: Option<SkillAuthorType>,
    /// 父技能 ID
    pub parent_skill_id: Option<String>,
    /// 标签列表
    pub tags: Option<Vec<String>>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Skill 搜索响应（分页）
pub type SearchSkillsResponse = PagedResult<SkillListItem>;

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
    ///
    /// **不接受 Published**：「非 Published → Published」的跃迁会被拒绝，
    /// 发布请走 [`PublishSkillRequest`]（`POST /skills/{skill_id}/publish`）。
    /// 发布是单一入口，因为它附带 Agent 根技能的回写副本副作用。
    pub status: Option<SkillStatus>,
    /// 内容输入源（文本 / URL / 附件三选一或组合）。
    pub content_input: Option<SkillContentInput>,
    /// 要删除的技能文件名列表（相对技能目录的路径，禁止 skill.md）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_deletes: Option<Vec<String>>,
}

/// 更新 Skill 响应。
pub type UpdateSkillResponse = SkillDetail;

/// 删除 Skill 请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 安装技能包请求。
///
/// 按 tag 批量安装已发布技能到 Agent 目录。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallSkillPackRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,

    /// 技能包 tag。
    #[param(source = "path")]
    pub tag: String,

    /// 是否同时删除 Agent 侧的技能副本（默认 false，仅移除 tag 关联）
    #[param(source = "query")]
    pub delete_copies: Option<bool>,
}

/// 卸载技能包响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UninstallSkillPackResponse {}

/// 列出已安装技能包请求。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
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

/// Skill tags 聚合请求（无参数，仅用于满足 handler 宏签名）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSkillTagsRequest {}

/// Skill tags 聚合响应（distinct tags from Published skills）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSkillTagsResponse {
    /// 所有已发布技能的不重复 tag 列表
    pub tags: Vec<String>,
}

/// 单技能卸载请求（从 Agent 目录删除技能副本）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallSkillFromAgentRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,
    /// Skill ID（Agent 目录中的副本 ID）。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 单技能卸载响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UninstallSkillFromAgentResponse {
    /// Agent ID。
    pub agent_id: String,
    /// Skill ID。
    pub skill_id: String,
    /// 是否删除成功。
    pub deleted: bool,
}

/// 列出 Agent 名下**仅 Expired** 的技能副本（与 list_agent_skills 互斥）。
/// 用于 Agent 详情页「📦 已过期技能」虚拟 pack 点击时的异步数据加载。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListExpiredAgentSkillsRequest {
    /// Agent ID。
    #[param(source = "path")]
    pub agent_id: String,
}

/// 列出 Agent 过期技能副本响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListExpiredAgentSkillsResponse {
    /// Agent 名下所有 Expired 状态的技能（扁平，按 id 唯一）。
    pub skills: Vec<SkillListItem>,
}

/// 恢复技能请求（将 Expired → Draft，仅允许当前状态为 Expired 的技能）。
///
/// 权限：技能作者本人 / Admin / SuperAdmin /（当技能作者是 Agent 时）该 Agent 的创建者。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RestoreSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 恢复技能响应（返回恢复后的 SkillDetail，用于前端局部刷新）。
pub type RestoreSkillResponse = SkillDetail;

/// 发布技能请求（Draft → Published）。
///
/// 发布是**共享库治理动作**，收敛为唯一入口：`update_skill` 不再接受
/// 「非 Published → Published」的跃迁（会被拒绝），必须走本接口。
/// 独立入口的理由有两条，缺一不可：
/// - 发布带副作用（Agent 自有根技能发布后，要为作者 Agent 回写一条本地工作副本）；
///   塞进通用 update 会让「改个描述」的请求也走发布逻辑；
/// - 前端需要一个语义明确的按钮，`update_skill(status=Published)` 既不可见也不可解释。
///
/// 权限：技能作者本人 / Admin / SuperAdmin /（作者为 Agent 时）该 Agent 的创建者。
/// **Agent 上下文一律拒绝**（Agent 的自进化只发生在自己的 Draft 副本里）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct PublishSkillRequest {
    /// Skill ID。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 发布技能响应（返回发布后的 SkillDetail，用于前端局部刷新）。
pub type PublishSkillResponse = SkillDetail;
