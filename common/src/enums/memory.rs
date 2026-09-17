//! Memory 相关枚举
//!
//! - `MemoryStatus` - 记忆状态（活跃/已遗忘）
//! - `MemoryRole` - 记忆条目角色（user / assistant / system / summary）
//! - `KnowledgeRelationType` - 知识节点关系类型
//! - `MemoryType` - 记忆类型（用于过滤查询）

#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// 归一化 API 传来的枚举键：忽略首尾空白、大小写与**下划线**。
///
/// 抹掉下划线是因为同一枚举有两套写法 —— API 文档 / 工具参数用 snake_case
/// （`knowledge_node`），Rust 侧 `Display` 给的是 PascalCase（`KnowledgeNode`）。
/// 不归一就会让其中一套在查询侧静默失配（拼写"没错"却解析失败）。
fn normalize_enum_key(raw: &str) -> String {
    raw.trim().replace('_', "").to_ascii_lowercase()
}

/// 记忆状态
///
/// 用于短期记忆索引和长期知识节点的状态管理：
/// - `Forgotten` = 0：已遗忘（归档，默认不参与检索，降低信息过载）
/// - `Active` = 1：活跃（正常可检索，参与问答和搜索）
/// - `Settled` = 2：已沉淀（短期记忆已总结为长期知识，默认不参与检索）
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MemoryStatus {
    /// 已遗忘 - 0，默认过滤不查询，保留数据可恢复
    Forgotten = 0,
    /// 活跃 - 1，正常可检索
    #[default]
    Active = 1,
    /// 已沉淀 - 2，短期记忆已总结为长期知识
    Settled = 2,
}

impl From<i32> for MemoryStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => MemoryStatus::Forgotten,
            1 => MemoryStatus::Active,
            2 => MemoryStatus::Settled,
            _ => MemoryStatus::default(),
        }
    }
}

impl From<i64> for MemoryStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl MemoryStatus {
    /// Convert to i32 for database storage
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }

    /// API 侧的合法取值清单（供错误提示列出，让调用方能自我纠正）。
    pub const ACCEPTED_VALUES: &'static str = "active, settled, forgotten";

    /// API 字符串 → 枚举；`None` 表示**非法值**。
    ///
    /// 接受名称（`active`）与判别值字符串（`"1"`），忽略首尾空白、大小写不敏感。
    ///
    /// ⚠️ 非法值必须由调用方报 **400**，**禁止静默降级**成 [`MemoryStatus::Active`]：
    /// 降级会把「拼错状态」变成「静默只查 active」，而响应看起来完全正常。
    pub fn parse(raw: &str) -> Option<Self> {
        match normalize_enum_key(raw).as_str() {
            "active" | "1" => Some(MemoryStatus::Active),
            "settled" | "2" => Some(MemoryStatus::Settled),
            "forgotten" | "0" => Some(MemoryStatus::Forgotten),
            _ => None,
        }
    }
}

// ==================== MemoryRole ====================

/// 记忆条目角色
///
/// 标识这条记忆是谁说的
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryRole {
    /// 系统提示
    System,
    /// 用户输入
    User,
    /// AI 助手输出
    Assistant,
    /// 归纳总结
    Summary,
}

impl std::fmt::Display for MemoryRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryRole::System => write!(f, "system"),
            MemoryRole::User => write!(f, "user"),
            MemoryRole::Assistant => write!(f, "assistant"),
            MemoryRole::Summary => write!(f, "summary"),
        }
    }
}

impl From<String> for MemoryRole {
    fn from(s: String) -> Self {
        match s.as_str() {
            "system" => MemoryRole::System,
            "user" => MemoryRole::User,
            "assistant" => MemoryRole::Assistant,
            "summary" => MemoryRole::Summary,
            _ => MemoryRole::User, // 默认当作用户
        }
    }
}

// ==================== KnowledgeRelationType ====================

/// 知识节点关系类型枚举
///
/// 预定义常见的知识图谱关系类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeRelationType {
    /// 相关关系：两个节点内容相关
    Related,
    /// 包含关系：源节点包含目标节点（父 → 子）
    Contains,
    /// 被包含关系：源节点被目标节点包含（子 → 父）
    ContainedBy,
    /// 依赖关系：源节点依赖目标节点
    Depends,
    /// 被依赖关系：目标节点依赖源节点
    DependedBy,
    /// 前置关系：源节点是目标节点的前置知识
    Prerequisite,
    /// 后续关系：源节点是目标节点的后续知识
    Followup,
    /// 相似关系：两个节点内容相似
    Similar,
    /// 相反关系：两个节点内容相反/矛盾
    Opposite,
    /// 因果关系：源节点导致目标节点
    Causes,
    /// 被因果关系：源节点由目标节点导致
    CausedBy,
    /// 实例关系：源节点是目标节点的一个实例
    InstanceOf,
    /// 分类关系：源节点分类到目标节点
    CategoryOf,
    /// 属性关系：源节点是目标节点的一个属性
    AttributeOf,
    /// 值关系：源节点是目标节点属性的值
    ValueOf,
    /// 自定义关系（留扩展）
    Custom,
}

impl std::fmt::Display for KnowledgeRelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnowledgeRelationType::Related => write!(f, "related"),
            KnowledgeRelationType::Contains => write!(f, "contains"),
            KnowledgeRelationType::ContainedBy => write!(f, "contained_by"),
            KnowledgeRelationType::Depends => write!(f, "depends"),
            KnowledgeRelationType::DependedBy => write!(f, "depended_by"),
            KnowledgeRelationType::Prerequisite => write!(f, "prerequisite"),
            KnowledgeRelationType::Followup => write!(f, "followup"),
            KnowledgeRelationType::Similar => write!(f, "similar"),
            KnowledgeRelationType::Opposite => write!(f, "opposite"),
            KnowledgeRelationType::Causes => write!(f, "causes"),
            KnowledgeRelationType::CausedBy => write!(f, "caused_by"),
            KnowledgeRelationType::InstanceOf => write!(f, "instance_of"),
            KnowledgeRelationType::CategoryOf => write!(f, "category_of"),
            KnowledgeRelationType::AttributeOf => write!(f, "attribute_of"),
            KnowledgeRelationType::ValueOf => write!(f, "value_of"),
            KnowledgeRelationType::Custom => write!(f, "custom"),
        }
    }
}

impl KnowledgeRelationType {
    /// 规范词表（`Display` 的 snake_case 形式）：提示词与文档引用它，映射也以它为 key
    pub const VOCABULARY: [&'static str; 16] = [
        "related",
        "contains",
        "contained_by",
        "depends",
        "depended_by",
        "prerequisite",
        "followup",
        "similar",
        "opposite",
        "causes",
        "caused_by",
        "instance_of",
        "category_of",
        "attribute_of",
        "value_of",
        "custom",
    ];

    /// 关系**原文** → 展示标签（词表内中文名，词表外原样）
    ///
    /// - 命中规范词（忽略首尾空白、大小写不敏感）→ 中文短标签（`contains` → 包含）
    /// - 未命中 → **原样返回**写入方标注的原文
    ///
    /// ⚠️ 未命中**绝不**替换成「自定义」。关系类型落库存的是原文
    /// （`knowledge_node_relation.relation_type`），这里只是展示期美化；
    /// 把词表外的标注抹成 `Custom`，Agent 明确的语义就永久消失了 ——
    /// 这也正是 `From<String>` 不可用于写入路径的原因。
    pub fn zh_label_from_display(raw: &str) -> &str {
        let trimmed = raw.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "related" => "相关",
            "contains" => "包含",
            "contained_by" => "属于",
            "depends" => "依赖",
            "depended_by" => "被依赖",
            "prerequisite" => "前置",
            "followup" => "后续",
            "similar" => "相似",
            "opposite" => "相反",
            "causes" => "导致",
            "caused_by" => "源于",
            "instance_of" => "实例",
            "category_of" => "分类",
            "attribute_of" => "属性",
            "value_of" => "取值",
            "custom" => "自定义",
            _ => trimmed,
        }
    }
}

impl From<String> for KnowledgeRelationType {
    /// 原文 → 枚举**归类**（供查询 / 统计这类需要判别值的场景）
    ///
    /// ⚠️ **禁止用在写入路径**：词表外的值会塌成 `Custom`，原文永久丢失。
    /// 关系类型落库一律保存原文；展示走 [`KnowledgeRelationType::zh_label_from_display`]。
    fn from(s: String) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "related" => KnowledgeRelationType::Related,
            "contains" => KnowledgeRelationType::Contains,
            "contained_by" => KnowledgeRelationType::ContainedBy,
            "depends" => KnowledgeRelationType::Depends,
            "depended_by" => KnowledgeRelationType::DependedBy,
            "prerequisite" => KnowledgeRelationType::Prerequisite,
            "followup" => KnowledgeRelationType::Followup,
            "similar" => KnowledgeRelationType::Similar,
            "opposite" => KnowledgeRelationType::Opposite,
            "causes" => KnowledgeRelationType::Causes,
            "caused_by" => KnowledgeRelationType::CausedBy,
            "instance_of" => KnowledgeRelationType::InstanceOf,
            "category_of" => KnowledgeRelationType::CategoryOf,
            "attribute_of" => KnowledgeRelationType::AttributeOf,
            "value_of" => KnowledgeRelationType::ValueOf,
            "custom" => KnowledgeRelationType::Custom,
            _ => KnowledgeRelationType::Custom, // 默认自定义
        }
    }
}

// ==================== MemoryType ====================

/// 记忆类型（用于过滤查询）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// 原始记忆追踪
    Trace,
    /// 短期记忆索引
    ShortTerm,
    /// 长期知识节点
    KnowledgeNode,
    /// 知识节点关系
    Relation,
    /// 所有类型
    All,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Trace => write!(f, "Trace"),
            MemoryType::ShortTerm => write!(f, "ShortTerm"),
            MemoryType::KnowledgeNode => write!(f, "KnowledgeNode"),
            MemoryType::Relation => write!(f, "Relation"),
            MemoryType::All => write!(f, "All"),
        }
    }
}

impl MemoryType {
    /// API 侧的合法取值清单（snake_case，供错误提示列出，让调用方能自我纠正）。
    ///
    /// `all` 是显式表达的「不过滤」，与省略该参数同义。
    pub const ACCEPTED_VALUES: &'static str = "short_term, knowledge_node, trace, relation, all";

    /// API 字符串 → 枚举；`None` 表示**非法值**。
    ///
    /// 归一化：忽略首尾空白、大小写，以及**下划线**（API 文档用 snake_case，
    /// 而 [`Display`](std::fmt::Display) 给的是 PascalCase，两者都要能用：
    /// `knowledge_node` / `KnowledgeNode` / `knowledgenode` 等价）。
    ///
    /// ⚠️ 非法值必须由调用方报 **400**，**禁止静默降级**成 [`MemoryType::All`]：
    /// 降级会让调用方拼错一个词就拿到**全量结果**，而响应看起来完全成功 ——
    /// 这是最难被发现的一类「参数写错却像查对了」。
    pub fn parse(raw: &str) -> Option<Self> {
        match normalize_enum_key(raw).as_str() {
            "shortterm" => Some(MemoryType::ShortTerm),
            "knowledgenode" => Some(MemoryType::KnowledgeNode),
            "trace" => Some(MemoryType::Trace),
            "relation" => Some(MemoryType::Relation),
            "all" => Some(MemoryType::All),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_maps_to_distinct_chinese_labels() {
        let mut labels: Vec<&str> = Vec::new();
        for key in KnowledgeRelationType::VOCABULARY {
            let label = KnowledgeRelationType::zh_label_from_display(key);
            assert_ne!(label, key, "规范词 {key} 必须有中文标签，不能原样返回");
            assert!(!label.is_empty(), "规范词 {key} 的中文标签不能为空");
            labels.push(label);
        }
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "规范词的中文标签必须两两不同");
    }

    #[test]
    fn unknown_relation_is_shown_verbatim_never_as_custom() {
        // 词表外的标注是写入方的原始语义，展示期必须原样透出。
        // 一旦这里退化成「自定义」，Agent 标了什么都看不出来了。
        assert_eq!(KnowledgeRelationType::zh_label_from_display("实现"), "实现");
        assert_eq!(
            KnowledgeRelationType::zh_label_from_display("is_prerequisite_of"),
            "is_prerequisite_of"
        );
        assert_eq!(
            KnowledgeRelationType::zh_label_from_display("父节点"),
            "父节点"
        );
    }

    #[test]
    fn label_mapping_is_case_insensitive_and_trims() {
        assert_eq!(
            KnowledgeRelationType::zh_label_from_display("  Contains "),
            "包含"
        );
        assert_eq!(
            KnowledgeRelationType::zh_label_from_display("CAUSES"),
            "导致"
        );
        // 未命中时返回的是 trim 后的原文（首尾空白不该出现在边标签里）
        assert_eq!(
            KnowledgeRelationType::zh_label_from_display("  实现  "),
            "实现"
        );
    }

    #[test]
    fn memory_type_parse_matches_accepted_values_and_rejects_garbage() {
        // ACCEPTED_VALUES 必须与实际可解析集合一致，否则错误提示会误导调用方
        for token in MemoryType::ACCEPTED_VALUES.split(',') {
            assert!(
                MemoryType::parse(token.trim()).is_some(),
                "ACCEPTED_VALUES 里的 `{}` 必须可解析",
                token.trim()
            );
        }
        // 大小写不敏感 + trim + PascalCase（Display）都要能用
        assert_eq!(
            MemoryType::parse("  Knowledge_Node "),
            Some(MemoryType::KnowledgeNode)
        );
        assert_eq!(MemoryType::parse("ShortTerm"), Some(MemoryType::ShortTerm));
        assert_eq!(MemoryType::parse("all"), Some(MemoryType::All));
        // ⚠️ 拼错必须返回 None（由调用方报 400），绝不能悄悄变成 All
        assert_eq!(MemoryType::parse("knowledge"), None);
        assert_eq!(MemoryType::parse("knowlege_node"), None);
        assert_eq!(MemoryType::parse(""), None);
    }

    #[test]
    fn memory_status_parse_accepts_names_and_discriminants() {
        for token in MemoryStatus::ACCEPTED_VALUES.split(',') {
            let token = token.trim();
            assert!(
                MemoryStatus::parse(token).is_some(),
                "ACCEPTED_VALUES 里的 `{token}` 必须可解析"
            );
        }
        assert_eq!(MemoryStatus::parse("ACTIVE"), Some(MemoryStatus::Active));
        assert_eq!(
            MemoryStatus::parse(" settled "),
            Some(MemoryStatus::Settled)
        );
        // 判别值字符串（历史调用方形式）也要兼容
        assert_eq!(MemoryStatus::parse("0"), Some(MemoryStatus::Forgotten));
        assert_eq!(MemoryStatus::parse("2"), Some(MemoryStatus::Settled));
        // 拼错必须返回 None，不能静默变 Active
        assert_eq!(MemoryStatus::parse("actived"), None);
        assert_eq!(MemoryStatus::parse(""), None);
    }
}
