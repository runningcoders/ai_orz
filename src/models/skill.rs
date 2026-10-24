//! Skill 持久化对象

use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 技能描述中的「触发条件 / 内容概要」分隔符。
///
/// 约定写法：`<何时该读>必读全文：<读了有什么>`，例如
/// 「写记忆、建图谱或检索记忆前必读全文：Soul 出发的四层记忆模型…」。
/// 库内约 2/3 的技能遵循该约定；不遵循的退化为原单行摘要，不会渲染出错。
pub(crate) const TRIGGER_SEPARATOR: &str = "必读全文";

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
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default)]
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
    #[allow(clippy::too_many_arguments)]
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

    /// 格式化技能摘要用于 Prompt 注入
    /// 只输出名称、id 和描述，不包含完整 skill.md 内容，避免 Token 膨胀。
    /// id 必须注入：Agent 副本 id 是安装时生成的 UUID，无法从名称推知，
    /// 缺失时 Agent 每次读全文都需先 search_skill / list_skills 多绕一轮；
    /// 有了 id 可直接 get_skill(skill_id) 一步按需读取全文（读自身副本无条件放行）。
    pub fn to_prompt_summary(&self) -> String {
        // 触发条件前置：约定 description 用「…必读全文：…」把「何时该读」与「读了有什么」
        // 隔开，这里按该约定切成两行渲染。
        //
        // 为什么要拆：单行摘要里触发条件埋在长句中，模型扫不到——实测全库 trace 里
        // get_skill / search_skill 等技能类工具调用次数为 0，而「记忆认知」技能明确写着
        // 「写记忆、建图谱或检索记忆前必读全文」，save_short_term_memory 却调了 2800+ 次
        // 一次全文都没读。拆成两行让「何时读」可竖向扫读，是触发条件能被看到的前提。
        //
        // 不拆数据模型：description 保持自由文本，无分隔符的技能退化为原单行格式。
        match self.description.split_once(TRIGGER_SEPARATOR) {
            Some((trigger, rest)) => {
                let trigger = trigger.trim();
                // 分隔符后可能紧跟全角/半角冒号与空格
                let rest = rest.trim_start_matches(['：', ':', ' ', '\n']).trim();
                if trigger.is_empty() {
                    return format!("- {}（id: {}）：{}", self.name, self.id, self.description);
                }
                if rest.is_empty() {
                    return format!(
                        "- {}（id: {}）\n  ⚠️ 读全文的时机：{}",
                        self.name, self.id, trigger
                    );
                }
                format!(
                    "- {}（id: {}）\n  ⚠️ 读全文的时机：{}\n  全文含：{}",
                    self.name, self.id, trigger, rest
                )
            }
            None => format!("- {}（id: {}）：{}", self.name, self.id, self.description),
        }
    }

    /// 获取标签列表
    pub fn get_tags(&self) -> Vec<String> {
        if self.tags.is_empty() {
            return Vec::new();
        }
        serde_json::from_str(&self.tags).unwrap_or_default()
    }
}

// ==================== Skill 业务聚合实体 ====================

use crate::models::vector::{SearchMatchInfo, VectorPayload, Vectorizable};

/// 技能完整业务实体（PO + 文件系统内容 + 搜索元信息）
///
/// - SkillPo：数据库持久化元数据
/// - files：关联文件列表（小文件预读，大文件按需加载）
/// - search_match：搜索匹配元信息（仅搜索结果有值）
#[derive(Debug, Clone, Builder)]
#[builder(default)]
#[derive(Default)]
pub struct Skill {
    /// 数据库持久化元数据
    pub po: SkillPo,
    /// 关联文件列表
    pub files: Vec<SkillFile>,
    /// ✅ 搜索匹配元信息（可选）
    /// - 普通查询返回：None
    /// - 搜索返回：Some(包含匹配类型、距离、命中等元信息)
    pub search_match: Option<SearchMatchInfo>,
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for Skill {
    fn vectorize_text(&self) -> String {
        // Skill 向量化：名称 + 描述 + 标签
        let tags = self.po.parse_tags().join(" ");
        format!("{} {} {}", self.po.name, self.po.description, tags)
    }

    fn vector_collection() -> &'static str {
        "skills"
    }

    fn vector_id(&self) -> &str {
        self.po.vector_id()
    }

    fn vector_payload(&self) -> VectorPayload {
        self.po.vector_payload()
    }
}

/// ✅ SkillPo 也实现 Vectorizable（DAL 层直接使用 PO）
impl Vectorizable for SkillPo {
    fn vectorize_text(&self) -> String {
        // SkillPo 向量化：名称 + 描述 + 标签
        let tags = self.parse_tags().join(" ");
        format!("{} {} {}", self.name, self.description, tags)
    }

    fn vector_collection() -> &'static str {
        "skills"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            tags: Some(self.tags.clone()),
            entity_type: Some(self.category.clone()),
            ..Default::default()
        }
    }
}

impl Skill {
    /// 从 PO 快速创建实体
    pub fn from_po(po: SkillPo) -> Self {
        Self {
            po,
            files: Vec::new(),
            search_match: None,
        }
    }

    /// 从 PO 快速创建实体（保留 from 别名用于兼容性）
    pub fn from(po: SkillPo) -> Self {
        Self::from_po(po)
    }

    /// 获取技能 ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取技能名称
    pub fn name(&self) -> &str {
        &self.po.name
    }

    /// 获取主文件内容（如果已加载）
    pub fn main_content(&self) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.filename == "skill.md")
            .and_then(|f| f.content.as_deref())
    }

    /// 获取指定文件名的内容（如果已加载）
    pub fn file_content(&self, filename: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.filename == filename)
            .and_then(|f| f.content.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn po(description: &str) -> SkillPo {
        SkillPo {
            name: "记忆认知".to_string(),
            id: "skill-1".to_string(),
            description: description.to_string(),
            ..Default::default()
        }
    }

    /// 有「必读全文」分隔符 → 拆成「何时读 / 全文含」两行，触发条件可竖扫
    #[test]
    fn summary_splits_trigger_from_content() {
        let s = po("写记忆、建图谱前必读全文：四层记忆模型与图谱模式").to_prompt_summary();
        assert!(s.contains("⚠️ 读全文的时机：写记忆、建图谱前"), "{s}");
        assert!(s.contains("全文含：四层记忆模型与图谱模式"), "{s}");
        // 分隔符本身不应再出现在渲染结果里（已被语义标签取代）
        assert!(!s.contains(TRIGGER_SEPARATOR), "{s}");
    }

    /// 无分隔符 → 退化为原单行格式，不能渲染出错或丢内容
    #[test]
    fn summary_falls_back_to_single_line_without_trigger() {
        let s = po("通用接待 SOP：接待四步").to_prompt_summary();
        assert_eq!(s, "- 记忆认知（id: skill-1）：通用接待 SOP：接待四步");
    }

    /// 只有触发条件、没有内容概要 → 不渲染空的「全文含：」行
    #[test]
    fn summary_omits_empty_content_line() {
        let s = po("开始沉淀前必读全文").to_prompt_summary();
        assert!(s.contains("读全文的时机：开始沉淀前"), "{s}");
        assert!(!s.contains("全文含"), "{s}");
    }

    /// 分隔符在开头（无触发条件内容）→ 退化为单行，避免出现空标签
    #[test]
    fn summary_degrades_when_trigger_is_empty() {
        let s = po("必读全文：只有内容没有时机").to_prompt_summary();
        assert_eq!(s, "- 记忆认知（id: skill-1）：必读全文：只有内容没有时机");
    }
}
