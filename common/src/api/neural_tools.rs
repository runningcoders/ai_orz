//! Neural tools API request/response DTOs - shared between backend and frontend

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 搜索记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchMemoryParams {
    /// 搜索关键词。
    pub query: String,
    /// 返回最大结果数。
    pub max_results: Option<i32>,
    /// 记忆类型筛选。
    pub memory_type: Option<String>,
    /// 图谱遍历深度，默认0=不遍历。
    pub traversal_depth: Option<i32>,
    /// 每层展开广度，默认0=不限制。
    pub traversal_breadth: Option<i32>,
    /// 遍历策略：breadth_first / depth_first。
    pub traversal_strategy: Option<String>,
    /// 种子节点ID列表，跳过语义搜索直接遍历。
    pub seed_node_ids: Option<Vec<String>>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
    /// 按任务 ID 过滤，聚焦到特定任务的记忆。
    /// 不传则不过滤（跨任务全局搜索）。
    pub task_id: Option<String>,
    /// 指定查询的 Agent ID。
    /// 不传则使用当前请求上下文的 agent_id。
    pub agent_id: Option<String>,
}

/// 搜索记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchMemoryResponse {
    /// 搜索结果列表。
    pub results: Vec<MemoryResult>,
}

/// 单条记忆结果。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MemoryResult {
    /// 记忆 ID。
    pub id: String,
    /// 记忆名称（知识节点 = `node_name`）。
    ///
    /// ⚠️ 与 `content` 不是一回事：知识节点的 `content` 是 `node_description`（正文），
    /// 名称单独存放；前端图谱卡片第一行展示名称、正文留给 hover 详情。
    /// 短期记忆 / 调用记录 / 关系没有独立名称，为 `None`（前端回退取正文首行）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 记忆内容（知识节点 = 描述正文；关系边 = 关系类型中文标签）。
    pub content: String,
    /// 记忆类型。
    pub memory_type: String,
    /// 匹配相关度（0.0~1.0，**越大越相关**）。
    ///
    /// 由向量距离换算（距离 0 = 完全相似 → 1.0），前端按百分比展示。
    /// 仅「向量参与过命中」的条目有值；只有关键词命中、以及图谱遍历
    /// 展开出来的邻居节点/关系边都没有匹配过程，为 `None`（前端整行不渲染，
    /// 不要显示成 `N/A` —— 那会被读成「匹配度 0」）。
    pub score: Option<f32>,
    /// 记忆摘要。
    pub summary: Option<String>,
    /// 关系类型：源节点 ID（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    /// 关系类型：目标节点 ID（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
    /// 关系类型名称（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_type: Option<String>,
    /// 关系强度（0.0~1.0，越大越强；仅 relation 类型有值）。
    ///
    /// 写入方声明，图谱据此调线宽与浓淡、hover 展示数值。
    /// `None` = **未标注**（存量边或产出时未声明）——前端渲染基准线宽，
    /// 别回退成 0.0（那是「明确很弱」，与「没人标过」不是一回事）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
    /// 标签列表（仅 short_term / knowledge_node 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// 匹配元信息（仅 search_memory 语义搜索有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_match: Option<MemorySearchMatch>,
}

/// 记忆匹配元信息（语义搜索场景）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MemorySearchMatch {
    /// 匹配类型：hybrid（FTS5+向量）/ vector（仅向量）/ keyword（仅关键词）。
    pub match_type: String,
    /// 向量相似度距离（越小越相似，仅向量匹配时有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_distance: Option<f32>,
    /// FTS5 BM25 相关性评分（越小越相关，仅关键词匹配时有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fts_rank: Option<f32>,
}

/// 查询记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryMemoryParams {
    /// Agent ID 筛选。
    pub agent_id: Option<String>,
    /// 记忆类型筛选。
    pub memory_type: Option<String>,
    /// 返回数量限制。
    pub limit: Option<i32>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
    /// 按任务 ID 过滤，聚焦到特定任务的记忆。
    /// 不传则不过滤（跨任务全局查询）。
    pub task_id: Option<String>,
    /// 按状态过滤（active/settled/forgotten）
    pub status: Option<String>,
}

/// 查询记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct QueryMemoryResponse {
    /// 查询结果列表。
    pub results: Vec<MemoryResult>,
}

/// 创建记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateMemoryParams {
    /// 记忆类型。
    pub memory_type: String,
    /// 记忆内容。
    pub content: String,
    /// 记忆摘要。
    pub summary: Option<String>,
    /// 标签列表。
    pub tags: Option<Vec<String>>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 创建记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreateMemoryResponse {
    /// 新建记忆的 ID。
    pub memory_id: String,
}

/// 更新记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateMemoryParams {
    /// 记忆 ID。
    pub memory_id: String,
    /// 更新内容。
    pub content: Option<String>,
    /// 更新摘要。
    pub summary: Option<String>,
    /// 更新标签。
    pub tags: Option<Vec<String>>,
    /// 新增：更新记忆状态（如把短期记忆标记为 Settled）
    pub status: Option<String>,
    /// 新增：更新知识节点的 tags（与 tags 字段区分，tags 用于 ShortTerm，node_tags 用于 KnowledgeNode）
    pub node_tags: Option<Vec<String>>,
}

/// 更新记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 删除记忆请求参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteMemoryParams {
    /// 记忆 ID。
    #[param(source = "path")]
    pub memory_id: String,
}

/// 删除记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DeleteMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 推荐知识图谱起点节点请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RecommendSeedNodesParams {
    /// 指定 Agent ID。
    /// 不传则跨 Agent 全局推荐（仅考虑 published 节点）。
    pub agent_id: Option<String>,
    /// 返回推荐节点数量上限，默认 5。
    pub limit: Option<usize>,
}

/// 推荐起点响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecommendSeedNodesResponse {
    /// 推荐节点列表（按关联度数倒序）。
    pub recommendations: Vec<SeedNodeRecommendation>,
}

/// 单个推荐起点节点。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct SeedNodeRecommendation {
    /// 节点 ID。
    pub node_id: String,
    /// 节点名称。
    pub node_name: String,
    /// 节点描述。
    pub node_description: String,
    /// 节点类型（concept/fact/skill/pattern...）。
    pub node_type: String,
    /// 节点摘要。
    pub summary: String,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 关联度数（入边 + 出边总数）。
    pub degree: usize,
    /// 入边数（被其他节点引用的次数）。
    pub incoming_count: usize,
    /// 出边数（引用其他节点的次数）。
    pub outgoing_count: usize,
}

/// 发送消息请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendMessageParams {
    /// 接收用户 ID。
    pub to_user_id: String,
    /// 消息内容。
    pub content: String,
    /// 关联项目 ID。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
    /// 回复的消息 ID。
    pub reply_to_id: Option<String>,
}

/// 发送消息响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendMessageResponse {
    /// 消息 ID。
    pub message_id: String,
}

/// 发送消息给 Agent 请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendMessageToAgentParams {
    /// 接收 Agent ID（可选）
    ///
    /// 协作关系类比：
    /// - 默认对话框：用户选定 Agent 时传，未选定时为 None（后端走 resolve_agent 兜底）
    /// - Project 对话框：从 project.owner_agent_id 取；若为 None 也走 resolve_agent 兜底
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_agent_id: Option<String>,
    /// 消息内容。
    pub content: String,
    /// 关联项目 ID（默认对话框场景为 None）。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
    /// 回复的消息 ID。
    pub reply_to_id: Option<String>,
    /// 附件 ID 列表。
    /// 发送方已经上传到 Attachment 模块的附件 ID 列表，
    /// 后端会为每个附件创建一条附件消息（Image/File/Audio/Video），
    /// 紧跟在文本消息之前（按数组顺序排列）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachment_ids: Option<Vec<String>>,
}

/// 发送消息给 Agent 响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendMessageToAgentResponse {
    /// 消息 ID。
    pub message_id: String,
}

/// 请求工具调用参数（同步）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RequestToolCallParams {
    /// 工具 ID。
    pub tool_id: String,
    /// 工具名称。
    pub tool_name: String,
    /// 工具调用参数。
    pub params: serde_json::Value,
    /// 关联项目 ID。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 请求工具调用响应（同步）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RequestToolCallResponse {
    /// 工具调用 ID。
    pub tool_call_id: String,
    /// 调用状态。
    pub status: String,
    /// 工具执行结果。
    pub result: serde_json::Value,
}

/// 发送工具调用消息参数（异步）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendToolCallMessageParams {
    /// 工具 ID。
    pub tool_id: String,
    /// 工具名称。
    pub tool_name: String,
    /// 工具调用参数。
    pub params: serde_json::Value,
    /// 关联项目 ID。
    pub project_id: Option<String>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 发送工具调用消息响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendToolCallMessageResponse {
    /// 请求 ID。
    pub request_id: String,
    /// 消息 ID。
    pub message_id: String,
    /// 派发状态。
    pub status: String,
}

/// 标记完成请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct MarkDoneParams {
    /// 任务 ID。
    pub task_id: String,
    /// 完成摘要。
    pub summary: Option<String>,
}

/// 标记完成响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MarkDoneResponse {
    /// 任务 ID。
    pub task_id: String,
    /// 任务状态。
    pub status: String,
}

/// 发送任务分配消息参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SendTaskAssignmentMessageParams {
    /// 任务 ID。
    pub task_id: String,
    /// 任务标题。
    pub task_title: String,
    /// 任务描述。
    pub task_description: Option<String>,
    /// 接收 Agent ID。
    pub to_agent_id: String,
    /// 关联项目 ID。
    pub project_id: Option<String>,
}

/// 发送任务分配消息响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendTaskAssignmentMessageResponse {
    /// 消息 ID。
    pub message_id: String,
}

/// 知识图谱关联关系参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct KnowledgeRelationParam {
    /// 源节点 ID。
    pub source_node_id: String,
    /// 目标节点 ID。
    pub target_node_id: String,
    /// 关系类型。
    pub relation_type: String,
    /// 关系强度（0.0~1.0）：
    /// 这条关联有多强/多确定，用于图谱边的粗细与浓淡，hover 时展示。
    /// 只在确实有判断时给（例如“直接依赖”接近 1.0、“顺带提到”接近 0.2）；
    /// 拿不准就省略，省略会在图上渲染为基准线宽（好过随手给一个 0.5）。
    pub weight: Option<f32>,
}

impl KnowledgeRelationParam {
    /// 归一化关系强度：非有限值（NaN/Inf）与越界值一律夹紧到 0.0~1.0。
    ///
    /// 未提供时返回 `None`（未标注），**不回退成默认值**：0.5 这种“看起来
    /// 合理”的缺省会把「模型没标」变成「模型标了中等强度」，图上的粗细
    /// 就再也不能反映真实判断了。
    pub fn normalized_weight(&self) -> Option<f32> {
        self.weight
            .filter(|w| w.is_finite())
            .map(|w| w.clamp(0.0, 1.0))
    }
}

/// 保存短期记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SaveShortTermMemoryParams {
    /// 记忆摘要。
    pub summary: String,
    /// 标签列表。
    pub tags: Option<Vec<String>>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
    /// 详细内容，可选。
    pub content: Option<String>,
    /// 关联的思考 trace ID 列表（记录本条记忆从哪些 trace 中提炼）。
    pub trace_ids: Option<Vec<String>>,
}

/// 保存短期记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SaveShortTermMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
}

/// 保存长期记忆请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SaveLongTermMemoryParams {
    /// 节点名称。
    pub node_name: String,
    /// 节点描述。
    pub node_description: String,
    /// 节点类型，如 concept/fact/skill/pattern。
    pub node_type: String,
    /// 节点摘要。
    pub summary: Option<String>,
    /// 标签列表（用于过滤检索 + 全文索引）。
    pub tags: Option<Vec<String>>,
    /// 关联关系列表。
    pub relations: Option<Vec<KnowledgeRelationParam>>,
    /// 关联任务 ID。
    pub task_id: Option<String>,
}

/// 保存长期记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SaveLongTermMemoryResponse {
    /// 节点 ID。
    pub node_id: String,
    /// 创建的关系 ID 列表，可能为空。
    pub relation_ids: Vec<String>,
}

/// 沉淀记忆请求参数。
///
/// 将未沉淀的短期记忆总结并沉淀为长期知识图谱。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SettleMemoryParams {
    /// 每次处理的短期记忆数量上限；不传时用框架自适应上限，实际批量还会按模型上下文预算截断。
    pub limit: Option<usize>,
}

/// 沉淀记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SettleMemoryResponse {
    /// 本批完成沉淀流程的短期记忆条数（沉淀的产出是知识图谱节点/关系，由记忆工具调用记录追溯）。
    ///
    /// 0 表示无待沉淀记忆或 Agent 非空闲。
    pub settled_count: usize,
}

/// 搜索技能请求参数。
///
/// 按关键词或标签搜索技能库，返回技能摘要列表（不含完整内容）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchSkillParams {
    /// 搜索关键词（匹配技能名称、描述、tags）。
    pub keyword: Option<String>,
    /// 按 tag 过滤（OR 语义，命中任一即可）。
    pub tags: Option<Vec<String>>,
    /// 返回数量限制，默认 10。
    pub limit: Option<usize>,
}

/// 搜索技能响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchSkillResponse {
    /// 搜索结果列表。
    pub skills: Vec<SkillSummary>,
}

/// 技能摘要（不含完整内容，用于搜索/列表展示）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SkillSummary {
    /// 技能 ID。
    pub skill_id: String,
    /// 技能名称。
    pub name: String,
    /// 技能描述。
    pub description: String,
    /// 标签列表。
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(weight: Option<f32>) -> KnowledgeRelationParam {
        KnowledgeRelationParam {
            source_node_id: "kn_a".into(),
            target_node_id: "kn_b".into(),
            relation_type: "related".into(),
            weight,
        }
    }

    /// 关系强度归一化：越界夹紧、NaN/Inf 丢弃，**未提供保持未标注**
    ///
    /// 重点是最后一条：缺省绝不能回退成某个「看起来合理」的中间值（0.5），
    /// 否则「模型没标」会被渲染成「模型标了中等强度」，图上的粗细就不再是信息。
    #[test]
    fn relation_weight_is_normalized_without_inventing_a_default() {
        assert_eq!(
            param(None).normalized_weight(),
            None,
            "未标注必须保持 None，不能回退成默认值"
        );
        assert_eq!(param(Some(0.8)).normalized_weight(), Some(0.8));
        assert_eq!(param(Some(1.7)).normalized_weight(), Some(1.0), "上界夹紧");
        assert_eq!(param(Some(-0.3)).normalized_weight(), Some(0.0), "下界夹紧");
        assert_eq!(
            param(Some(0.0)).normalized_weight(),
            Some(0.0),
            "0 是合法值"
        );
        assert_eq!(param(Some(f32::NAN)).normalized_weight(), None);
        assert_eq!(param(Some(f32::INFINITY)).normalized_weight(), None);
    }

    /// 未标注时响应里**不出现** weight 键（与 score 同款契约：前端据此整行不渲染）
    #[test]
    fn unset_weight_is_omitted_from_payload() {
        let result = MemoryResult {
            id: "kn_a".into(),
            name: Some("订单状态机".into()),
            content: "正文".into(),
            memory_type: "knowledge_node".into(),
            score: None,
            summary: None,
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            weight: None,
            tags: None,
            search_match: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(
            !json.contains("\"weight\""),
            "未标注的条目不应带 weight 键: {json}"
        );
    }
}
