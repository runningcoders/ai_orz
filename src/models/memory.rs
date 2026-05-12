//! Memory 记忆系统模型
//!
//! 定义记忆系统相关的实体类型：
//! - MemoryTrace - 记忆追踪条目，一条原始记忆，包含完整信息，ID = 内容 hash
//! - ShortTermMemoryIndexPo - 短期记忆索引（SQLite 持久化）
//! - LongTermKnowledgeNodePo - 长期知识图谱节点（SQLite 持久化）
//! - KnowledgeReferencePo - 知识节点引用原始短期索引

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

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
    pub role: common::enums::MemoryRole,
    /// 原始内容（完整细节）
    pub content: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 元数据（可扩展存储额外信息）
    pub metadata: HashMap<String, String>,
    /// 物理位置（DAO 写入或查询后回填，不参与序列化）
    #[serde(skip)]
    pub position: Option<MemoryTracePosition>,
}

/// MemoryTrace 在每日 JSONL 文件中的物理位置
///
/// DAO 层 `append_trace` / `batch_append_traces` 写入后返回，
/// DAL 层可用于：
/// - 构造 `KnowledgeReferencePo`（date_path + line_number 定位原始内容）
/// - 后续创建 `ShortTermMemoryIndexPo` 时关联 trace_ids
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryTracePosition {
    /// trace 的内容 hash ID
    pub trace_id: String,
    /// 日期文件名，如 "20260512.jsonl"
    pub date_filename: String,
    /// 行号（0-indexed）
    pub line_number: u64,
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
        role: common::enums::MemoryRole,
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
            position: None,
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
            common::enums::MemoryRole::System => "**System**",
            common::enums::MemoryRole::User => "**User**",
            common::enums::MemoryRole::Assistant => "**Assistant**",
            common::enums::MemoryRole::Summary => "**Summary**",
        };

        format!(
            r#"
---
ID: {}
Role: {}
Created: {}

{}
"#,
            self.id, role, self.created_at, self.content
        )
        .trim()
        .to_string()
            + "\n\n"
    }
}

/// 短期记忆索引 PO
///
/// 每条短期记忆聚合了多条相关记忆细节，存储在 SQLite
/// 通过 trace_ids 显式记录聚合的 trace id 列表
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
    /// 标签列表（用于过滤检索，JSON 数组字符串）
    pub tags: String,
    /// 聚合的 trace id 列表（JSON 数组字符串）
    pub trace_ids: String,
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
    pub relation_type: common::enums::KnowledgeRelationType,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 知识节点引用原始记忆细节
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

/// 记忆底层 PO 统一枚举
#[derive(Debug, Clone)]
pub enum MemoryPo {
    /// 原始记忆追踪（来自 JSONL，无 SQLite 表）
    Trace(MemoryTrace),
    /// 短期记忆索引
    ShortTerm(ShortTermMemoryIndexPo),
    /// 长期知识节点
    KnowledgeNode(LongTermKnowledgeNodePo),
    /// 知识节点关系
    Relation(KnowledgeNodeRelationPo),
}

/// 记忆写入参数
///
/// 写入分为两阶段：
/// 1. trace 先入库（`AppendTraces`），不做向量化
/// 2. 归纳总结后再写短期记忆索引（`CreateShortTerm`），自动向量化
///
/// 长期知识节点（带可选引用）/ 关系单独走 `CreateKnowledgeNode` / `CreateRelations`
#[derive(Debug, Clone)]
pub enum MemoryCreateParams {
    /// 阶段 1：仅写 trace 细节（不向量化、不创建索引）
    AppendTraces(Vec<MemoryTrace>),

    /// 阶段 2：基于已存在的 trace 创建短期记忆索引
    /// PO 内的 trace_ids 字段已包含阶段 1 返回的 id 列表
    CreateShortTerm(ShortTermMemoryIndexPo),

    /// 长期知识节点（可选附带引用关系）
    CreateKnowledgeNode {
        node: LongTermKnowledgeNodePo,
        references: Vec<KnowledgeReferencePo>,
    },

    /// 知识关系列表
    CreateRelations(Vec<KnowledgeNodeRelationPo>),
}
