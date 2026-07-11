//! Memory 相关枚举
//!
//! - `MemoryStatus` - 记忆状态（活跃/已遗忘）
//! - `MemoryRole` - 记忆条目角色（user / assistant / system / summary）
//! - `KnowledgeRelationType` - 知识节点关系类型
//! - `MemoryType` - 记忆类型（用于过滤查询）

#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// 记忆状态
///
/// 用于短期记忆索引和长期知识节点的状态管理：
/// - `Forgotten` = 0：已遗忘（归档，默认不参与检索，降低信息过载）
/// - `Active` = 1：活跃（正常可检索，参与问答和搜索）
/// - `Settled` = 2：已沉淀（短期记忆已总结为长期知识，默认不参与检索）
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MemoryStatus {
    /// 已遗忘 - 0，默认过滤不查询，保留数据可恢复
    Forgotten = 0,
    /// 活跃 - 1，正常可检索
    Active = 1,
    /// 已沉淀 - 2，短期记忆已总结为长期知识
    Settled = 2,
}

impl Default for MemoryStatus {
    fn default() -> Self {
        MemoryStatus::Active // 默认活跃
    }
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

impl ToString for MemoryRole {
    fn to_string(&self) -> String {
        match self {
            MemoryRole::System => "system".to_string(),
            MemoryRole::User => "user".to_string(),
            MemoryRole::Assistant => "assistant".to_string(),
            MemoryRole::Summary => "summary".to_string(),
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

impl ToString for KnowledgeRelationType {
    fn to_string(&self) -> String {
        match self {
            KnowledgeRelationType::Related => "related".to_string(),
            KnowledgeRelationType::Contains => "contains".to_string(),
            KnowledgeRelationType::ContainedBy => "contained_by".to_string(),
            KnowledgeRelationType::Depends => "depends".to_string(),
            KnowledgeRelationType::DependedBy => "depended_by".to_string(),
            KnowledgeRelationType::Prerequisite => "prerequisite".to_string(),
            KnowledgeRelationType::Followup => "followup".to_string(),
            KnowledgeRelationType::Similar => "similar".to_string(),
            KnowledgeRelationType::Opposite => "opposite".to_string(),
            KnowledgeRelationType::Causes => "causes".to_string(),
            KnowledgeRelationType::CausedBy => "caused_by".to_string(),
            KnowledgeRelationType::InstanceOf => "instance_of".to_string(),
            KnowledgeRelationType::CategoryOf => "category_of".to_string(),
            KnowledgeRelationType::AttributeOf => "attribute_of".to_string(),
            KnowledgeRelationType::ValueOf => "value_of".to_string(),
            KnowledgeRelationType::Custom => "custom".to_string(),
        }
    }
}

impl From<String> for KnowledgeRelationType {
    fn from(s: String) -> Self {
        match s.as_str() {
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
