//! Memory 记忆系统模型
//!
//! 定义记忆系统相关的实体类型：
//! - MemoryTrace - 记忆追踪条目，一条原始记忆，包含完整信息，ID = 内容 hash
//! - ShortTermMemoryIndexPo - 短期记忆索引（SQLite 持久化）
//! - LongTermKnowledgeNodePo - 长期知识图谱节点（SQLite 持久化）
//! - KnowledgeReferencePo - 知识节点引用原始短期索引
//! - Memory - 记忆业务实体（包含 PO + 搜索匹配信息）

use crate::models::vector::{MatchType, SearchMatchInfo, VectorPayload, Vectorizable};
use common::api::{MemoryResult, MemorySearchMatch};
use common::enums::KnowledgeRelationType;
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

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: Some(self.agent_id.clone()),
            task_id: self.task_id.clone(),
            status: Some(self.status.to_i32().to_string()),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
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
    /// 是否被标记为**高价值/高影响力**（tags 含 "published" 时为 true）
    ///
    /// 冗余字段，与 tags 中的 "published" 标签同步，用于走索引排序。
    /// ⚠️ 它**不是可见性控制位**：知识节点在蜂巢内对所有 Agent 可见，与是否 published 无关。
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

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: Some(self.agent_id.clone()),
            entity_type: Some(self.node_type.clone()),
            status: Some(self.status.to_i32().to_string()),
            is_published: Some(self.is_published),
            tags: Some(self.tags.clone()),
            ..Default::default()
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
    /// 关系类型**原文** —— 写入方标注的字符串，落库不做任何归一化
    ///
    /// ⚠️ 这里存 `String` 而不是枚举：枚举只有十几个变体，词表外的标注一旦
    /// 走 `KnowledgeRelationType::from()` 就会被塌成 `Custom`，**原文永久丢失**
    /// （图谱上只能显示「自定义」，Agent 明明标了具体的语义却看不出来）。
    /// 中文/美化映射只发生在展示期，见 `KnowledgeRelationType::zh_label_from_display`。
    pub relation_type: String,
    /// 关系强度（0.0~1.0，越大越强），`None` = 未标注
    ///
    /// 由写入方（`save_long_term_memory` 的 `relations[].weight`）声明，
    /// 图谱按它调线宽与浓淡；`None` 渲染基准线宽。
    /// ⚠️ 与「强度 0」不是一回事：0 是「明确很弱」，`None` 是「没人标过」。
    pub weight: Option<f32>,
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

    /// 转换为 API 响应 DTO（`search_memory` / `query_memory` 共用）
    ///
    /// ⚠️ **单一实现**：这两个 handler 此前各持一份逐字相同的拷贝，改一处漏一处。
    /// 关系边两处都用 `format!("{:?}")` 输出关系类型 —— Debug 得到的是 Rust 变体名
    /// （`"Causes"`），而前端 `zh_label_from_display` 的词表 key 是 `Display` 的
    /// snake_case（`"causes"`），查表失败原样返回 → 画布上所有连线标签退化成英文词、
    /// 详情面板「内容」显示同一串英文，等于没有信息。
    ///
    /// `score` 语义 = **相关度（0.0~1.0，越大越相关）**，由向量距离换算而来
    /// （距离 0 = 完全相似 → 1.0）。⚠️ 旧实现直接吐 `vector_distance`（越小越相似），
    /// UI 标签却写「匹配分数」，方向正好相反；且关键词命中时距离为空 →
    /// 永远显示 `N/A`。只有向量参与过命中才有分值：`traverse_graph` 展开出来的
    /// 邻居节点与关系边没有匹配过程，保持 `None`，前端应整行不渲染而不是显示 `N/A`。
    pub fn to_api_result(&self) -> MemoryResult {
        let search_match = self.search_match.as_ref().map(|m| MemorySearchMatch {
            match_type: match m.match_type {
                MatchType::Hybrid => "hybrid",
                MatchType::Vector => "vector",
                MatchType::Keyword => "keyword",
            }
            .to_string(),
            vector_distance: m.vector_distance,
            fts_rank: m.fts_rank,
        });
        let score = self
            .search_match
            .as_ref()
            .and_then(|m| m.vector_distance)
            .map(|distance| (1.0 - distance).clamp(0.0, 1.0));

        match &self.po {
            MemoryPo::Trace(trace) => MemoryResult {
                id: trace.id.clone(),
                name: None,
                content: trace.input.clone(),
                memory_type: "trace".to_string(),
                score,
                summary: None,
                source_node_id: None,
                target_node_id: None,
                relation_type: None,
                weight: None,
                tags: None,
                search_match,
            },
            // 短期记忆 PO 只有一个文本字段 summary（无独立标题/正文）：
            // content 放完整 summary；summary 置 None，由前端显示层默认取
            // content 前几行作预览，避免「内容 + 摘要」渲染出同样的文本。
            MemoryPo::ShortTerm(st) => MemoryResult {
                id: st.id.clone(),
                name: None,
                content: st.summary.clone(),
                memory_type: "short_term".to_string(),
                score,
                summary: None,
                source_node_id: None,
                target_node_id: None,
                relation_type: None,
                weight: None,
                tags: Some(business_tags(&st.tags)),
                search_match,
            },
            MemoryPo::KnowledgeNode(kn) => MemoryResult {
                id: kn.id.clone(),
                name: Some(kn.node_name.clone()),
                content: kn.node_description.clone(),
                memory_type: "knowledge_node".to_string(),
                score,
                // 空串要归一成 None：写入侧未给摘要时可能是 `""`，
                // 前端 `if let Some(summary)` 会渲染一个空的摘要块
                summary: Some(kn.summary.clone()).filter(|s| !s.trim().is_empty()),
                source_node_id: None,
                target_node_id: None,
                relation_type: None,
                weight: None,
                tags: Some(business_tags(&kn.tags)),
                search_match,
            },
            MemoryPo::Relation(rel) => {
                // 原文落库、展示期映射：
                // - `relation_type` 给**原文**（前端着色/词表以它为输入）
                // - `name` / `content` 给映射后的展示标签（图谱连线与 hover 用）
                // 空串归一成 `None`：画布只认非空 tag，空串会让 hover 直接失效
                // （`tag: None` → tooltip 提前 return），回退「关联」才是对的。
                let relation_key = rel.relation_type.trim().to_string();
                // 展示标签：词表内 → 中文短标签，词表外 → 原文；空类型回退「关联」。
                // 不给空标签 —— 画布只认非空 tag，空串会让连线 hover 直接失效。
                let relation_label =
                    match KnowledgeRelationType::zh_label_from_display(&relation_key) {
                        "" => "关联".to_string(),
                        label => label.to_string(),
                    };
                MemoryResult {
                    id: rel.id.clone(),
                    // 关系边在数据层只有类型一个可用字段，把展示标签同时给
                    // name/content —— 详情与 hover 才不至于显示一串英文枚举名
                    name: Some(relation_label.clone()),
                    content: relation_label,
                    memory_type: "relation".to_string(),
                    score,
                    summary: None,
                    source_node_id: Some(rel.source_node_id.clone()),
                    target_node_id: Some(rel.target_node_id.clone()),
                    relation_type: Some(relation_key).filter(|s| !s.is_empty()),
                    // 关系强度透传给图谱：线宽/浓淡据此派生，`None` = 未标注
                    weight: rel.weight,
                    tags: None,
                    search_match,
                }
            }
        }
    }
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
pub(crate) fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}

/// 入库用的可见性控制标记。
///
/// 写节点时它同时落进 `tags` 且置冗余字段 [`LongTermKnowledgeNodePo::is_published`]
/// （供查询走索引）。它是**控制位不是业务标签**，回给前端会在卡片上多出一个
/// 没有意义的英文胶囊。
const PUBLISHED_TAG: &str = "published";

/// 结果侧业务标签：剔除 `published` 这类控制标记
fn business_tags(tags_json: &str) -> Vec<String> {
    parse_tags_json(tags_json)
        .into_iter()
        .filter(|t| t != PUBLISHED_TAG)
        .collect()
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

/// 知识图谱推荐起点（domain 层结构）
///
/// 包含知识节点 PO + 关联度数统计信息，
/// 由 DAL 层 `recommend_seed_nodes` 方法返回。
#[derive(Debug, Clone)]
pub struct SeedNodeRecommendation {
    /// 知识节点 PO
    pub node: LongTermKnowledgeNodePo,
    /// 关联度数（入边 + 出边总数）
    pub degree: usize,
    /// 入边数（被其他节点引用次数）
    pub incoming_count: usize,
    /// 出边数（引用其他节点次数）
    pub outgoing_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::MemoryStatus;

    fn node(summary: &str) -> Memory {
        Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
            id: "kn_1".to_string(),
            agent_id: "agent-a".to_string(),
            node_name: "订单状态机".to_string(),
            node_description: "订单状态机描述订单从创建到完成的流转。".to_string(),
            node_type: "general".to_string(),
            summary: summary.to_string(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 0,
            updated_at: 0,
        }))
    }

    fn relation(kind: &str) -> Memory {
        Memory::new(MemoryPo::Relation(KnowledgeNodeRelationPo {
            id: "kr_1".to_string(),
            source_node_id: "kn_a".to_string(),
            target_node_id: "kn_b".to_string(),
            relation_type: kind.to_string(),
            // 未标注示例：没有强度时不能写成 0.0（那是「明确很弱」）
            weight: None,
            created_at: 0,
            updated_at: 0,
        }))
    }

    /// 关系类型：DTO 透出**原文**，展示走词表映射（词表外原样）。
    ///
    /// 回归 1：`format!("{:?}")` 会输出 Rust 变体名 `"Causes"`，前端查不到中文
    /// 而原样显示英文 —— 画布上所有连线标签、详情面板「内容」都是这串英文。
    /// 回归 2：写入方标注的词表外原文（如「实现」）**必须**原样透出，不能因为
    /// 匹配不上就归一成 `Custom` / 显示成「自定义」—— 那会把对方明确的语义抹掉。
    #[test]
    fn relation_type_keeps_raw_text_and_maps_label() {
        let dto = relation("causes").to_api_result();
        assert_eq!(dto.relation_type.as_deref(), Some("causes"));
        assert_eq!(dto.content, "导致", "关系边的内容应是人可读的中文标签");
        assert_eq!(dto.name.as_deref(), Some("导致"));

        let dto = relation("contained_by").to_api_result();
        assert_eq!(dto.relation_type.as_deref(), Some("contained_by"));
        assert_eq!(dto.content, "属于");

        // 词表外：原文与展示标签都必须保留
        let dto = relation("实现").to_api_result();
        assert_eq!(dto.relation_type.as_deref(), Some("实现"));
        assert_eq!(dto.content, "实现");
        assert_eq!(dto.name.as_deref(), Some("实现"));

        // 空白类型：不给空标签（空 tag 会让画布 hover 失效），回退「关联」
        let dto = relation("   ").to_api_result();
        assert_eq!(dto.relation_type, None);
        assert_eq!(dto.content, "关联");
    }

    /// 空摘要归一成 `None`：前端 `if let Some(summary)` 否则会渲染一个空块。
    #[test]
    fn blank_summary_is_none() {
        assert!(node("").to_api_result().summary.is_none());
        assert!(node("   ").to_api_result().summary.is_none());
        assert_eq!(
            node("独立摘要").to_api_result().summary.as_deref(),
            Some("独立摘要")
        );
    }

    /// `score` 是**相关度**（越大越相关），由向量距离换算而来。
    #[test]
    fn score_is_relevance_not_distance() {
        let memory = node("").with_search_match(SearchMatchInfo {
            match_type: MatchType::Vector,
            vector_distance: Some(0.2),
            ..Default::default()
        });
        let dto = memory.to_api_result();
        let score = dto.score.expect("向量命中应有相关度");
        assert!((score - 0.8).abs() < 1e-6, "距离 0.2 应换算成相关度 0.8");
        assert_eq!(
            dto.search_match.as_ref().map(|m| m.match_type.as_str()),
            Some("vector"),
            "匹配方式要一并暴露，否则用户分不清语义还是关键词命中"
        );
    }

    /// `published` 是「重要性/影响力」标记（与 `is_published` 冗余字段同源），不是业务标签，
    /// 回给前端会在卡片上多出一个没有意义的英文胶囊。它不承载可见性语义（蜂巢全域可见）。
    #[test]
    fn published_control_tag_is_not_exposed_as_label() {
        let mut memory = node("");
        if let MemoryPo::KnowledgeNode(kn) = &mut memory.po {
            kn.tags = r#"["published","架构"]"#.to_string();
        }
        assert_eq!(
            memory.to_api_result().tags,
            Some(vec!["架构".to_string()]),
            "只保留业务标签"
        );
    }

    /// 没有匹配过程（图谱遍历展开）就没有相关度，前端据此整行不渲染。
    #[test]
    fn no_match_means_no_score() {
        let dto = node("").to_api_result();
        assert!(dto.score.is_none());
        assert!(dto.search_match.is_none());
    }
}
