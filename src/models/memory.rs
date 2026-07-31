//! Memory 记忆系统模型
//!
//! 定义记忆系统相关的实体类型：
//! - MemoryTrace - 记忆追踪条目，一条原始记忆，包含完整信息，ID = 内容 hash
//! - ShortTermMemoryIndexPo - 短期记忆索引（SQLite 持久化）
//! - LongTermKnowledgeNodePo - 长期知识图谱节点（SQLite 持久化）
//! - KnowledgeReferencePo - 知识节点引用原始短期索引
//! - Memory - 记忆业务实体（包含 PO + 搜索匹配信息）

use crate::models::vector::{SearchMatchInfo, Vectorizable};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

/// 记忆追踪条目
///
/// 一条原始记忆，对应一次完整的思考闭环（输入 → 模型思考 → 输出）
/// 所有字段都使用统一的 trace_id 贯穿，模型可以看到 trace_id 并自引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTrace {
    /// 唯一 ID = trace-{agent_id}-{timestamp}
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

    // ========== 思考闭环字段 ==========
    /// 思考输入（完整 Prompt）
    pub input: String,
    /// 思考输出（模型返回，可能为空表示中断）
    pub output: Option<String>,
    /// 思考创建时间（输入时间）
    pub created_at: i64,
    /// 思考完成时间（输出写入时间）
    pub completed_at: Option<i64>,

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
    /// 创建新的 MemoryTrace（思考闭环）
    ///
    /// 自动生成 trace_id = trace-{agent_id}-{timestamp}
    pub fn new(
        agent_id: String,
        log_id: String,
        user_id: String,
        organization_id: String,
        role: common::enums::MemoryRole,
        input: String,
        task_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        let created_at = now.timestamp();
        // 加随机后缀避免同一 agent 并发处理两条消息时 timestamp_nanos 相同导致 trace_id 碰撞
        let trace_id = format!(
            "trace-{}-{}-{}",
            agent_id,
            now.timestamp_nanos_opt().unwrap_or(0),
            rand::random::<u16>()
        );
        Self {
            id: trace_id,
            agent_id,
            task_id,
            log_id,
            user_id,
            organization_id,
            role,
            input,
            output: None,
            created_at,
            completed_at: None,
            metadata: HashMap::new(),
            position: None,
        }
    }

    /// 完成思考，回填输出
    pub fn complete(&mut self, output: String) {
        self.output = Some(output);
        self.completed_at = Some(chrono::Utc::now().timestamp());
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

        let mut content = format!(
            r#"
---
ID: {}
Role: {}
Created: {}
"#,
            self.id, role, self.created_at,
        )
        .trim()
        .to_string();

        // 写入 Input
        content.push_str(&format!("\n\n### Input\n\n{}\n", self.input));

        // 如果有 Output，也写入
        if let Some(output) = &self.output {
            content.push_str(&format!("\n### Output\n\n{}\n", output));
        }

        // 如果有完成时间，写入
        if let Some(completed_at) = self.completed_at {
            content.push_str(&format!("\nCompleted: {}\n", completed_at));
        }

        content + "\n\n"
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

impl ShortTermMemoryIndexPo {
    /// 构建用于向量索引的文本（summary + tags 拼接）
    ///
    /// tags 为 JSON 数组字符串，会展平为空格分隔的纯文本；
    /// 空标签或解析失败时仅返回 summary
    fn vector_text(&self) -> String {
        let tags = flatten_tags(&self.tags);
        if tags.is_empty() {
            self.summary.clone()
        } else {
            format!("{}\n{}", self.summary, tags)
        }
    }
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for ShortTermMemoryIndexPo {
    fn vectorize_text(&self) -> String {
        self.vector_text()
    }

    fn vector_collection() -> &'static str {
        "memory:short_term"
    }
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
    /// 标签列表（用于过滤检索 + 全文索引，JSON 数组字符串）
    pub tags: String,
    /// 记忆状态
    pub status: common::enums::MemoryStatus,
    /// 是否已发布到蜂巢（tags 含 "published" 时为 true）
    /// 冗余字段，与 tags 中的 "published" 标签同步，用于加速查询
    pub is_published: bool,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

impl LongTermKnowledgeNodePo {
    /// 构建用于向量索引的文本（node_description + summary + tags 拼接）
    ///
    /// tags 为 JSON 数组字符串，会展平为空格分隔的纯文本；
    /// 空标签或解析失败时仅返回 node_description + summary
    fn vector_text(&self) -> String {
        let tags = flatten_tags(&self.tags);
        if tags.is_empty() {
            format!("{}\n{}", self.node_description, self.summary)
        } else {
            format!("{}\n{}\n{}", self.node_description, self.summary, tags)
        }
    }
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for LongTermKnowledgeNodePo {
    fn vectorize_text(&self) -> String {
        self.vector_text()
    }

    fn vector_collection() -> &'static str {
        "memory:knowledge_node"
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

impl MemoryPo {
    /// 将记忆格式化为 Prompt 可读的摘要字符串
    ///
    /// 目前仅 ShortTerm 类型的记忆会返回摘要，
    /// 其他类型的记忆暂不组装到 prompt 中
    pub fn to_prompt_summary(&self) -> Option<String> {
        match self {
            MemoryPo::ShortTerm(st) => {
                if st.summary.is_empty() {
                    None
                } else {
                    Some(st.summary.clone())
                }
            }
            // 其他类型的记忆暂时不组装到 prompt 中
            _ => None,
        }
    }
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

/// 记忆业务实体（包含 PO + 搜索匹配信息）
/// 对齐 Skill/Tool 命名模式：Memory = MemoryPo + search_match
#[derive(Debug, Clone)]
pub struct Memory {
    pub po: MemoryPo,
    pub search_match: Option<SearchMatchInfo>,
}

impl Memory {
    /// 创建新的 Memory 业务实体
    pub fn new(po: MemoryPo) -> Self {
        Self {
            po,
            search_match: None,
        }
    }

    /// 将记忆格式化为 Prompt 可读的摘要字符串
    ///
    /// 委托给 MemoryPo::to_prompt_summary()
    pub fn to_prompt_summary(&self) -> Option<String> {
        self.po.to_prompt_summary()
    }

    /// 设置搜索匹配信息
    pub fn with_search_match(mut self, search_match: SearchMatchInfo) -> Self {
        self.search_match = Some(search_match);
        self
    }
}

/// 将 tags JSON 数组字符串展平为空格分隔的纯文本，便于向量化
///
/// 输入示例：`["rust","memory","向量"]` → `rust memory 向量`
/// 解析失败或空数组返回空字符串
fn flatten_tags(tags_json: &str) -> String {
    serde_json::from_str::<Vec<String>>(tags_json)
        .unwrap_or_default()
        .join(" ")
}
