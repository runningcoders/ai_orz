//! Memory 记忆系统模型
//!
//! 定义记忆系统相关的实体类型：
//! - MemoryRole - 记忆条目角色（user / assistant / system）
//! - MemoryTrace - 记忆追踪条目，一条原始记忆，包含完整信息，ID = 内容 hash
//! - ShortTermMemoryIndexPo - 短期记忆索引（SQLite 持久化）
//! - LongTermKnowledgeNodePo - 长期知识图谱节点（SQLite 持久化）
//! - KnowledgeReferencePo - 知识节点引用原始短期索引

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

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

/// 记忆追踪条目
///
/// 一条原始记忆，包含完整信息：
/// - 既可以在内存中作为工作记忆使用
/// - 也可以写入每日文件归档
/// - ID = 内容 hash，唯一标识，防止重复
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTrace {
    /// 唯一 ID = 内容 hash
    pub id: String,
    /// 所属 Agent ID
    pub agent_id: String,
    /// 所属任务 ID（可选，用于追溯到具体任务）
    pub task_id: Option<String>,
    /// 请求日志 ID（来源溯源）
    pub log_id: String,
    /// 创建者用户 ID（来源溯源）
    pub user_id: String,
    /// 所属组织 ID（来源溯源）
    pub organization_id: String,
    /// 角色
    pub role: MemoryRole,
    /// 原始内容（完整细节）
    pub content: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 元数据（可扩展存储额外信息）
    pub metadata: HashMap<String, String>,
}

impl MemoryTrace {
    /// 创建新的 MemoryTrace
    ///
    /// 自动生成内容 hash 作为 ID
    pub fn new(
        agent_id: String,
        log_id: String,
        user_id: String,
        organization_id: String,
        role: MemoryRole,
        content: String,
        task_id: Option<String>,
    ) -> Self {
        let content_hash = sha256::digest(content.as_bytes());
        let now = chrono::Utc::now().timestamp();
        Self {
            id: content_hash,
            agent_id,
            task_id,
            log_id,
            user_id,
            organization_id,
            role,
            content,
            created_at: now,
            metadata: HashMap::new(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// 格式化为 markdown 写入每日文件
    pub fn to_markdown(&self) -> String {
        let role = match &self.role {
            MemoryRole::System => "**System**",
            MemoryRole::User => "**User**",
            MemoryRole::Assistant => "**Assistant**",
            MemoryRole::Summary => "**Summary**",
        };

        format!(
            r#"
---
ID: {}
Role: {}
Created: {}

{}
"#,
            self.id,
            role,
            self.created_at,
            self.content
        )
        .trim()
        .to_string() + "\n\n"
    }
}

/// 短期记忆索引 PO
///
/// 每条短期记忆聚合了多条相关记忆细节，存储在 SQLite
/// 原始记忆细节通过 knowledge_reference.short_term_id 反向关联
/// 原始记忆细节位置信息存储在 knowledge_reference 表中
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShortTermMemoryIndexPo {
    /// 唯一 ID = 多个原始记忆细节 id 拼接后二次 hash
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 所属任务 ID（可选，用于追溯到具体任务）
    pub task_id: Option<String>,
    /// 角色
    pub role: String,
    /// 归纳摘要（用于全文检索）
    pub summary: String,
    /// 标签列表（用于过滤检索）
    pub tags: String,
    /// 记忆状态
    pub status: common::enums::MemoryStatus,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 长期知识图谱节点 PO
///
/// 经过归纳总结得到的知识节点，存储在 SQLite
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LongTermKnowledgeNodePo {
    /// 唯一 ID
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 节点名称
    pub node_name: String,
    /// 节点描述
    pub node_description: String,
    /// 节点类型：concept / event / preference / skill / ...
    pub node_type: String,
    /// 综合总结
    pub summary: String,
    /// 记忆状态
    pub status: common::enums::MemoryStatus,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

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

/// 知识节点关系 PO
///
/// 专门存储知识节点之间的关系，独立表方便查询和维护
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeNodeRelationPo {
    /// 唯一 ID
    pub id: String,
    /// 源节点 ID
    pub source_node_id: String,
    /// 目标节点 ID
    pub target_node_id: String,
    /// 关系类型枚举
    pub relation_type: KnowledgeRelationType,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 知识引用原始记忆细节
///
/// 记录知识节点引用了哪些原始记忆细节，同时存储原始细节位置信息
/// 每条原始记忆细节单独一条引用记录，位置信息完整可追溯
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeReferencePo {
    /// 唯一 ID
    pub id: String,
    /// 知识节点 ID
    pub knowledge_id: String,
    /// 短期记忆索引 ID（这条原始细节属于哪个短期记忆索引）
    pub short_term_id: String,
    /// 原始记忆细节 ID（MemoryTrace.id）
    pub trace_id: String,
    /// 日期文件名：YYYYMMDD.jsonl，存储在 agent 目录下
    pub date_path: String,
    /// 在 JSONL 文件中的行号（0-based）
    pub line_number: i64, // SQLite 不支持 u64 直接存储，用 i64 足够
    /// 创建时间戳
    pub created_at: i64,
}
