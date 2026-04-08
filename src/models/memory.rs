//! Memory 记忆系统模型
//!
//! 定义记忆系统相关的实体类型：
//! - MemoryRole - 记忆条目角色（user / assistant / system）
//! - MemoryTrace - 记忆追踪条目，一条原始记忆，包含完整信息，ID = 内容 hash
//! - ShortTermMemoryIndexPo - 短期记忆索引（SQLite 持久化）
//! - LongTermKnowledgeNodePo - 长期知识图谱节点（SQLite 持久化）
//! - KnowledgeReferencePo - 知识节点引用原始短期索引

use serde::{Deserialize, Serialize};
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
    pub fn new(agent_id: String, role: MemoryRole, content: String) -> Self {
        let content_hash = sha256::digest(content.as_bytes());
        let now = chrono::Utc::now().timestamp();
        Self {
            id: content_hash,
            agent_id,
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
/// 原始内容存储在每日文件中
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermMemoryIndexPo {
    /// 唯一 ID = 多个原始记忆细节 id 拼接后二次 hash
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 聚合了哪些原始记忆细节的 id 列表
    pub trace_ids: Vec<String>,
    /// 角色
    pub role: String,
    /// 归纳摘要（用于全文检索）
    pub summary: String,
    /// 标签列表（用于过滤检索）
    pub tags: Vec<String>,
    /// 日期文件相对路径：long_term_memory/{agent_id}/YYYY-MM-DD.md
    pub date_path: String,
    /// 在文件中的起始字节偏移
    pub byte_start: u64,
    /// 内容字节长度
    pub byte_length: u64,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 长期知识图谱节点 PO
///
/// 经过归纳总结得到的知识节点，存储在 SQLite
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 关系列表：[{target_node_id, relation_type}]
    pub relations: Vec<KnowledgeRelation>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 知识关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelation {
    /// 目标节点 ID
    pub target_node_id: String,
    /// 关系类型：related / contains / depends / ...
    pub relation_type: String,
}

/// 知识节点引用原始短期索引
///
/// 记录知识节点引用了哪些原始短期记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReferencePo {
    /// 唯一 ID
    pub id: String,
    /// 知识节点 ID
    pub knowledge_id: String,
    /// 引用的短期索引 ID
    pub short_term_id: String,
    /// 创建时间戳
    pub created_at: i64,
}
