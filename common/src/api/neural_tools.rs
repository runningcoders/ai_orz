//! Neural tools API request/response DTOs - shared between backend and frontend

use crate::enums::ontology::PropertyTargetType;
use crate::ontology_property::{PropertyEntry, PropertyFilter, PropertyFilterOp, PropertyValue};
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
    /// 记忆类型筛选：`short_term` / `knowledge_node` / `trace` / `relation` / `all`
    /// （等价写法 `KnowledgeNode`；`all` 与不传同义）。
    ///
    /// ⚠️ 词表外的值报 `invalid_request`，**不会**静默降级成搜全部。
    pub memory_type: Option<String>,
    /// 图谱遍历深度，默认0=不遍历（`seed_node_ids` 非空时为 0 表示只看种子节点本身）。
    pub traversal_depth: Option<i32>,
    /// 每个节点最多展开的出边数，默认0=不限制。
    pub traversal_breadth: Option<i32>,
    /// 遍历策略：`breadth_first`（默认） / `depth_first`。
    ///
    /// ⚠️ 词表外的值报 `invalid_request`，**不会**静默退化成宽度优先
    /// （策略决定节点集与边集的形状，静默换策略会让调用方拿到结构对不上的图）。
    pub traversal_strategy: Option<String>,
    /// 种子节点ID列表：跳过语义搜索直接沿图展开。
    ///
    /// ⚠️ 种子节点**一定会出现在结果里**（调用方点名的中心节点不该缺席），
    /// 且展开不受 `agent_id` 约束（知识节点蜂巢共享）。
    pub seed_node_ids: Option<Vec<String>>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
    /// 按任务 ID 过滤，聚焦到特定任务的记忆。
    /// 不传则不过滤（跨任务全局搜索）。
    pub task_id: Option<String>,
    /// 归属筛选：只看该 Agent 沉淀的记忆。**不是可见性门槛**。
    ///
    /// 知识节点是蜂巢共享资产，所有 Agent 都能读到全部知识节点；
    /// 该参数只约束「从谁的节点起步」（关键词搜索/列表）。
    ///
    /// **不传 = 全库（hive 共享）**：刻意**不**回退 `ctx.agent_id()`——回退会让
    /// Agent 的检索静默收窄成「只看自己沉淀的节点」，与「所有 Agent 都能看到
    /// 所有知识节点」相反。短期记忆（私有）需要的归属回退由 DAL 内部完成，
    /// 那里才分得清「私有要作用域」与「共享不要门槛」。回包用 `agent_scope`
    /// 自述本次生效范围。
    pub agent_id: Option<String>,
    /// 按属性值**精确**过滤（多个条件 AND 语义）。
    ///
    /// 与 `query` 语义检索互补：语义解决「像不像」，属性解决「是不是」
    /// （如「置信度 ≥ 0.8 的规范性结论」「source = 官方手册 的条目」）。
    /// 走的是独立精确召回通道（先按属性反查命中节点 id，再直取节点），
    /// **不是**在语义 top_k 里再筛一遍——所以命中率不依赖语义排序。
    ///
    /// ⚠️ 与 `seed_node_ids` 互斥：点名种子展开时种子恒返回，叠加过滤会
    /// 造成「传了过滤却仍返回未命中节点」的静默失真，直接报 400。
    /// 数值运算符（`gt`/`gte`/`lt`/`lte`）必须配 `number` 值，配其他类型报 400。
    pub property_filters: Option<Vec<PropertyFilterParam>>,
    /// 是否回显属性历史版本（默认 `false`）。
    ///
    /// - `false`（或未传）：结果的 `properties` 只含**生效值**（`status = 1`），
    ///   响应 JSON 中不出现 `is_current` 字段（缺席 ⇒ 视为 `true`）；
    /// - `true`：一并回显被取代的**历史版本**（`status = 0`），历史行带
    ///   `"is_current": false`，生效行不带该字段。
    ///
    /// 复用同一个 `properties` 数组（不另开 `property_history`）；记录按
    /// `term_key` / `ordinal` / `status` 排序，生效行在前。显式软删行
    /// （`status = 2`）任何情况下都不返回。
    pub include_history: Option<bool>,
}

/// 搜索记忆响应。
///
/// 除 `results` 外的五个字段全部是 **additive 回包自述**：不传
/// `property_filters` 时它们的序列化形状与旧版完全一致（`None` 直接跳过），
/// 因此老调用方的 JSON 解析零影响。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchMemoryResponse {
    /// 搜索结果列表。
    pub results: Vec<MemoryResult>,
    /// 属性过滤精确通道命中的节点数（= `property_hit_ids.len()`）。
    ///
    /// 仅在请求传了 `property_filters` 时有值；与 [`Self::returned`] 成对出现，
    /// 便于一眼判断「命中 N 个、只返回 M 条」——M < N 说明结果被 memory_type /
    /// 归属等其它条件收敛掉了，不是命中集本身有问题。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_total: Option<u64>,
    /// 本次实际返回条数（= `results.len()`）。
    ///
    /// 仅在请求传了 `property_filters` 时有值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returned: Option<u64>,
    /// 本次生效的归属作用域自述：`"hive"`（未传 `agent_id`）/
    /// `"explicit:<id>"`（显式传入）。
    ///
    /// **不存在 `context_fallback` 取值**：`agent_id` 不传就是全库（蜂巢共享），
    /// 刻意不回退 `ctx.agent_id()`（见 `SearchMemoryParams::agent_id` 文档）。
    /// 总是回显，让调用方能确认「这次查的到底是谁的范围」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_scope: Option<String>,
    /// 向量通道是否**真实参与**了本次召回（三态）。
    ///
    /// - `Some(true)`：结果中观测到 `vector_distance` ⇒ 向量参与了本次召回；
    /// - `Some(false)`：有结果且全部无 `vector_distance` ⇒ 确定未参与
    ///   （纯关键词 / 纯遍历通道）；
    /// - `None`：**无结果 ⇒ 无法判定**。
    ///
    /// 第三态是刻意的：空结果下无法区分「向量通道被短路」与「跑了但无候选」，
    /// 此时给 `false` 属于伪报。做监控时请把 `None` 单列，不要计入分母。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_used: Option<bool>,
    /// 空结果的**枚举原因**（仅 `results` 为空时有值；非空结果恒为 `None`）。
    ///
    /// 五值封闭，不得出现第六种：
    /// - `filter_no_match`：传了 `property_filters` 且精确通道命中集为空
    ///   （**最高优先**，即使同时传了显式 `agent_id`）；
    /// - `agent_scope_mismatch`：命中集非空，但被显式 `agent_id` 筛空；
    /// - `property_filter_not_applied`：命中集非空却零结果，且无显式 `agent_id`
    ///   —— 说明召回通道没把命中集注入（实现异常的自述）；
    /// - `agent_scope_explicit_empty`：未传 `property_filters`、传了显式
    ///   `agent_id`，该作用域下确实没有任何记忆；
    /// - `no_recall_hit`：既无 `property_filters` 也无显式 `agent_id`，全库未召回。
    ///
    /// 空结果必须给出原因：只返回 `[]` 会让调用方分不清「查错范围」「筛太狠」
    /// 与「库里真没有」，进而反复重试同一条件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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
    /// 结构化属性（仅 knowledge_node 且该节点**确实挂了属性**时有值；
    /// 无属性的节点为 `None`，不是空数组）。
    ///
    /// `search_memory` 会回显命中节点的全部生效属性（历史版本不回显），
    /// 让模型看得到自己写过什么、也能照抄回显结果回填进写入参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<KnowledgePropertyItem>>,
    /// 匹配元信息（仅 search_memory 语义搜索有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_match: Option<MemorySearchMatch>,
    /// **只读**系统标记（仅 knowledge_node 有值；无标记的节点为 `None`）。
    ///
    /// 与 `tags` 严格分工：`tags` 是用户/Agent **可写**的业务标签，本字段是
    /// **系统维护、只读**的控制标记（当前仅 `published`），由 `is_published`
    /// 派生而来 —— **不落列、没有独立写入口**。
    ///
    /// 想改动它请用 `save_long_term_memory` / `update_memory` 的 `is_published`
    /// 参数；**不要**把 `published` 塞进 `tags`（那是拆分前的旧行为，会让发布
    /// 标记混进业务标签、在「读全字段原样回写」时被静默清掉）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_flags: Option<Vec<String>>,
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
    /// Agent ID 筛选（**归属筛选，不是权限门槛**）。
    ///
    /// 知识节点蜂巢共享，任何 Agent 都能查到全部知识节点；该字段只用于「只看某个
    /// Agent 沉淀的记忆」。短期记忆是私有的，必须显式给出 agent_id 才会返回。
    pub agent_id: Option<String>,
    /// 记忆类型筛选：`short_term` / `knowledge_node` / `trace` / `relation` / `all`
    /// （等价写法 `KnowledgeNode`；`all` 与不传同义）。
    ///
    /// ⚠️ 词表外的值报 `invalid_request`，**不会**静默降级成查全部。
    pub memory_type: Option<String>,
    /// 返回数量限制。
    pub limit: Option<i32>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
    /// 按任务 ID 过滤，聚焦到特定任务的记忆。
    /// 不传则不过滤（跨任务全局查询）。
    pub task_id: Option<String>,
    /// 按状态过滤：`active` / `settled` / `forgotten`（也接受判别值 `"1"` / `"2"` / `"0"`）。
    ///
    /// ⚠️ 词表外的值报 `invalid_request`，**不会**静默降级成只查 active。
    pub status: Option<String>,
    /// 是否回显属性历史版本（默认 `false`）。
    ///
    /// 与 `search_memory` 的同名参数**同语义、同实现**（唯一装配源）：
    /// - `false`（或未传）：`results[].properties` 只含**生效值**（`status = 1`），
    ///   响应 JSON 中不出现 `is_current` 字段（缺席 ⇒ 视为 `true`）；
    /// - `true`：一并回显被取代的**历史版本**（`status = 0`），历史行带
    ///   `"is_current": false`，生效行不带该字段。
    ///
    /// 显式软删行（`status = 2`）任何情况下都不返回。
    pub include_history: Option<bool>,
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
    /// 更新记忆状态：`active` / `settled` / `forgotten`
    /// （也接受判别值 `"1"` / `"2"` / `"0"`，如把短期记忆标记为 settled）。
    ///
    /// ⚠️ 词表外的值报 `invalid_request`，**不会**静默兜底成 active ——
    /// 兜底会让「拼错状态」变成一次静默写入。
    pub status: Option<String>,
    /// 更新知识节点的**可写业务标签**（与 `tags` 区分：`tags` 作用于短期记忆，
    /// `node_tags` 作用于知识节点）。落 `user_tags` 列（过度期与 `tags` 双写）。
    ///
    /// ⚠️ 含 `published` 时不写入 `user_tags`，而是折算成 `is_published = true`
    /// （语义收编：系统标记不该躺在业务标签里）。
    pub node_tags: Option<Vec<String>>,
    /// 发布标记（**写 `is_published`，唯一真相源**）。
    ///
    /// - `None`（默认）= **保持不变**（不因本次更新触碰到发布状态）；
    /// - `Some(true)` = 置为已发布；
    /// - `Some(false)` = 取消发布。
    pub is_published: Option<bool>,
    /// 新建关系边（仅知识节点有效；短期记忆传入报 400）。
    ///
    /// 给**既有节点**补边的唯一路径：沉淀时发现新经验与已有节点相关，
    /// 用本参数落成显式关系边。正文里提及 / 引用其他节点不算建边——
    /// 图谱遍历只认关系表，正文引用对 traversal 不可达。
    pub relations: Option<Vec<KnowledgeRelationParam>>,
    /// 给**既有节点**补 / 改属性的唯一路径（仅知识节点有效；短期记忆传入报 400）。
    ///
    /// 与 `save_long_term_memory` 的 `properties` 同形同语义：本体里声明过的
    /// `term_key` 才受门闩保护，未声明的按值自身类型兜底写入。
    /// 单值属性重复写 = **软取代**（旧值留历史、读侧只见新值），不是报错。
    pub properties: Option<Vec<KnowledgePropertyParam>>,
}

/// 更新记忆响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateMemoryResponse {
    /// 记忆 ID。
    pub memory_id: String,
    /// 属性写入的问题清单（`term_key: 原因`），空数组表示全部写入成功。
    ///
    /// ⚠️ 记忆本身**已更新成功**：属性写入是「部分成功」语义，坏条目只跳过。
    /// 与 `save_long_term_memory` 同口径——静默吞掉会让模型以为写成功了。
    pub property_issues: Vec<String>,
    /// 逐条建边 keyed 回执（V1 并存过渡：旧消费方忽略此字段不受影响）。
    /// 仅入参携带 relations 时出现；每项 key/status/relation_id/error_code/message。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations_result: Option<Vec<RelationWriteResult>>,
    /// 属性写入问题清单 V2（结构化 code/message/field，与 V1 串同内容并存一版）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_issues_v2: Option<Vec<PropertyIssueEntry>>,
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
    /// 不传则跨 Agent 全局推荐（池子为全部知识节点；`published` 仅作连接度持平时的排序决胜）。
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

/// 知识图谱全局视图请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetKnowledgeGraphParams {
    /// 按 Agent 归属筛选（只返回该 Agent 沉淀的知识节点）。
    /// 不传则返回蜂巢全量（知识节点是全体 Agent 共享资产，全局视图默认全域）。
    pub agent_id: Option<String>,
}

/// 知识图谱全局视图响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetKnowledgeGraphResponse {
    /// 全部活跃知识节点（字段按全局视图裁剪口径精简，详情走卡片页既有通道）。
    pub nodes: Vec<GraphNode>,
    /// 双端活跃的关系边。
    pub edges: Vec<GraphEdge>,
    /// 生成时间戳（毫秒）。
    pub generated_at: i64,
}

/// 全局视图节点（字段裁剪口径：全局视角关注模块/区域/实体间关系，裁细节字段）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GraphNode {
    /// 节点 ID。
    pub id: String,
    /// 节点名称。
    pub node_name: String,
    /// 节点类型（concept/fact/skill/pattern...）。
    pub node_type: String,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 所属 Agent ID。
    pub agent_id: String,
    /// 高价值/高影响力标记（非可见性控制位）。
    pub is_published: bool,
    /// 连接度（入边 + 出边，仅计双端活跃的生效边）。
    pub degree: usize,
    /// 入边数。
    pub incoming_count: usize,
    /// 出边数。
    pub outgoing_count: usize,
    /// 创建时间戳。
    pub created_at: i64,
}

/// 全局视图边。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GraphEdge {
    /// 边 ID。
    pub id: String,
    /// 源节点 ID。
    pub source: String,
    /// 目标节点 ID。
    pub target: String,
    /// 关系类型原文（写入方标注，落库不归一化）。
    pub relation_type: String,
    /// 边方向（"directed" / "undirected"，服务端按词表 resolve 后带出，前端零词表映射）。
    pub direction: String,
    /// 关系强度（0.0~1.0）；`None` = 未标注（渲染基准线宽）。
    pub weight: Option<f32>,
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
    /// 附件 ID 列表（已上传到 Attachment 模块）。
    ///
    /// 主路径是**把引用写进 content**：`[文件名](attachment:{id})` 与文字同处一条
    /// 混排消息，展示与唤醒都从正文解析，附件不再独立成消息。本字段是随消息携带
    /// 的附件声明清单，后端据此做归属校验；若 content 里漏写了某个 id 的引用
    /// （例如只传数组的调用方），后端会按原文件名兜底补进正文。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachment_ids: Option<Vec<String>>,
    /// 知会模式（无需回复）。
    /// 置 true 时消息类型为 AgentNotify，接收方处理后不会把最终输出自动回发给来源方；
    /// 适用于「接下来的工作与来源 Agent 无关」的单向告知场景，避免 Agent 间乒乓。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify_only: Option<bool>,
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
    /// 关系类型 —— **原样保存、原样展示**，不做归一化。
    ///
    /// 优先用规范词：`related` / `contains` / `contained_by` / `depends` /
    /// `depended_by` / `prerequisite` / `followup` / `similar` / `opposite` /
    /// `causes` / `caused_by` / `instance_of` / `category_of` / `attribute_of` /
    /// `value_of`（图谱上显示为 相关 / 包含 / 属于 / 依赖 / 被依赖 / 前置 / 后续 /
    /// 相似 / 相反 / 导致 / 源于 / 实例 / 分类 / 属性 / 取值）。
    ///
    /// 词表都不贴切时，**直接写你判断的关系名**（中文短词最好，如「实现」
    /// 「被测试覆盖」「退化自」）：原样保存、原样展示，不会被替换成「自定义」。
    /// 语义准确 > 用词规范 —— 别为了凑词表选一个不准的类型。
    pub relation_type: String,
    /// 关系强度（0.0~1.0）：
    /// 这条关联有多强/多确定，用于图谱边的粗细与浓淡，hover 时展示。
    /// 只在确实有判断时给（例如“直接依赖”接近 1.0、“顺带提到”接近 0.2）；
    /// 拿不准就省略，省略会在图上渲染为基准线宽（好过随手给一个 0.5）。
    pub weight: Option<f32>,
}

/// 待写入的实体属性（神经工具入参形态）
///
/// `value` 是**带标签的联合类型**，标签决定落 `knowledge_property` 的哪一列
/// （`number` → `value_num`、`text`/`date`/`enum` → `value_text`、
/// `reference` → `value_ref`、`boolean` → `value_bool`）。
/// JSON 形态示例：`{"number": 0.8}` / `{"text": "gpt-4"}` / `{"boolean": true}`。
///
/// ⚠️ 标签与本体里该属性声明的 `data_type` 必须一致，否则本条会被**拒绝写入**
/// （值类型错属于写错，不是认知分歧，没有解释空间）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct KnowledgePropertyParam {
    /// 属性语义锚点（本体 `ontology_properties.term_key`，如 `model_name`）
    pub term_key: String,
    /// 属性值（标签决定类型）
    pub value: PropertyValue,
    /// 多值属性的槽位，从 0 递增；单值属性传 0（服务端会强制归一为 0）
    #[serde(default)]
    pub ordinal: i32,
}

impl KnowledgePropertyParam {
    /// 转为 DAL 写侧入参
    pub fn into_entry(self) -> PropertyEntry {
        PropertyEntry {
            term_key: self.term_key,
            value: self.value,
            ordinal: self.ordinal,
        }
    }
}

/// 读回的属性条目（`search_memory` 结果回显形态）
///
/// 与 [`KnowledgePropertyParam`] 刻意**同形**（term_key + 带标签值 + 槽位）：
/// 模型想把某条属性改个值，可以直接照抄回显结果回填进写入参数，
/// 不用再猜 JSON 形态。多值属性的 `ordinal` 是槽位，单值恒 0。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct KnowledgePropertyItem {
    /// 属性语义锚点（本体 `ontology_properties.term_key`）
    pub term_key: String,
    /// 属性值（标签决定类型；与写入入参同形）
    pub value: PropertyValue,
    /// 多值属性槽位；单值恒 0
    pub ordinal: i32,
    /// 该行是否为**当前生效值**。
    ///
    /// - `true`：生效值（`knowledge_property.status = 1`）
    /// - `false`：已被取代的历史版本（`status = 0`）
    ///
    /// ⚠️ **字段缺席 ⇒ 视为 `true`**：默认查询路径（`include_history` 未开启）
    /// 只回显生效值，本字段不序列化，响应形状与旧版逐字兼容；仅当
    /// `include_history=true` 且该行是历史版本时才出现 `"is_current": false`。
    /// 显式软删行（`status = 2`）任何情况下都不回显，故不存在第三种含义。
    #[serde(
        default = "default_is_current",
        skip_serializing_if = "is_current_is_default"
    )]
    pub is_current: bool,
}

/// `is_current` 的 serde 默认值：字段缺席即视为当前生效值。
///
/// 旧消费方（不认识 `is_current` 的客户端 / 模型）拿到的是三字段形状，
/// 语义上等价于「回显的都是生效值」——默认路径本就只查生效行。
fn default_is_current() -> bool {
    true
}

/// `is_current == true` 时跳过序列化，保持旧响应形状（缺席 ⇒ `true`）。
fn is_current_is_default(value: &bool) -> bool {
    *value
}

/// 属性过滤运算符（工具面枚举，非法值在反序列化阶段即 400）
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PropertyFilterOpParam {
    /// 等于（文本 / 布尔 / 枚举 / 引用 / 日期都用它）
    Eq,
    /// 大于（仅数值）
    Gt,
    /// 大于等于（仅数值）
    Gte,
    /// 小于（仅数值）
    Lt,
    /// 小于等于（仅数值）
    Lte,
}

impl PropertyFilterOpParam {
    /// 是否数值专用（非 `Eq` 的运算符全部只对 `Number` 有意义）
    pub fn requires_number(self) -> bool {
        !matches!(self, Self::Eq)
    }

    /// 转 DAL 过滤运算符
    pub fn into_op(self) -> PropertyFilterOp {
        match self {
            Self::Eq => PropertyFilterOp::Eq,
            Self::Gt => PropertyFilterOp::Gt,
            Self::Gte => PropertyFilterOp::Gte,
            Self::Lt => PropertyFilterOp::Lt,
            Self::Lte => PropertyFilterOp::Lte,
        }
    }
}

/// 属性过滤条件（`search_memory` 的结构化精确召回入口）
///
/// 多个条件 AND 语义。与语义 `query` 同时给出时取交集——
/// 属性过滤走的是**独立精确召回通道**（先反查命中 id 再直取节点），
/// 不是「在语义 top_k 里再筛一遍」。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct PropertyFilterParam {
    /// 属性语义锚点（本体 `ontology_properties.term_key`）
    pub term_key: String,
    /// 比较运算符；省略 = `eq`
    #[serde(default)]
    pub op: Option<PropertyFilterOpParam>,
    /// 比较值（标签决定类型；数值运算符必须配 `number` 值）
    pub value: PropertyValue,
}

impl PropertyFilterParam {
    /// 转 DAL 过滤条件（工具面只开放**节点**载体的属性过滤）
    pub fn into_filter(self) -> PropertyFilter {
        PropertyFilter {
            term_key: self.term_key,
            target_type: PropertyTargetType::Node,
            op: self.op.unwrap_or(PropertyFilterOpParam::Eq).into_op(),
            value: self.value,
        }
    }
}

/// 逐条建边的写入状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationWriteStatus {
    /// 该条已成功落库。
    Success,
    /// 该条被拒绝或落库失败（整批仍返回 200，不整体报错）。
    Failed,
}

/// 逐条建边回执：与 property_issues 的「部分成功」哲学同构——一条坏不连累一批，
/// 每条独立给出 key / 状态 / 错误码，调用方逐条补写而不是整批重试。
///
/// ⚠️ `relation_id` 是**实际落库**的边 ID（仅 success 项有值）——不是预生成的假 ID；
/// 顶层 `relation_ids` 与本表 success 项严格一致。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RelationWriteResult {
    /// 条目定位键：`{index}:{source_node_id}->{target_node_id}`
    /// （index 为主、复合串辅助——单字段同时给出批次序号与两端节点）。
    pub key: String,
    /// 写入状态。
    pub status: RelationWriteStatus,
    /// 实际落库的关系边 ID（仅 success 且真的写了库才有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_id: Option<String>,
    /// 失败错误码（snake_case，取自平台 error 词表），仅 failed 有值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// 失败原因（人类可读；底层/下游错误时为其 msg 摘要），仅 failed 有值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 结构化错误上下文（对齐 `Error.field` 的自由形态，如 `{"node_id": "..."}`），
    /// 仅 failed 且底层错误携带 field 时有值——把原来只打日志的错误带到外边来。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<serde_json::Value>,
}

/// 属性写入问题清单 V2 条目：结构对齐平台 `Error{code,msg,field}`，
/// 取代 V1 的 `"<term_key>: <原因>"` 自由文本串。
///
/// ⚠️ 过渡期 V1 `property_issues`（字符串数组）**并存一版**，新旧同内容——
/// 旧消费方按 V1 解析不受影响，新消费方直接读 V2 的结构化 code / field。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PropertyIssueEntry {
    /// 错误码（snake_case，取自平台 error 词表属性域，如 `property_type_mismatch`）。
    pub code: String,
    /// 人类可读原因（与 V1 串的 `<原因>` 部分同文案）。
    pub message: String,
    /// 结构化上下文：至少携带 `term_key`，类型不符时另带 `expected` 等字段。
    pub field: serde_json::Value,
    /// 严重度：`error` = 本条**被拒、未写库**（硬拦）；`warning` = 已记录**照常写入**
    /// （本体软门禁：认知分歧不阻断写入）。
    ///
    /// 唯一判据是 `PropertyWriteIssue::rejects_write()`——**不另设第二套判定**，
    /// 避免出现「拒绝但 severity=warning」这种自相矛盾的回执。V1 串
    /// （`property_issues`）的文案与格式不受本字段影响，一个字都不变。
    pub severity: String,
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
    /// 标签列表 —— **用户/Agent 可写的业务标签**（用于过滤检索 + 全文索引）。
    ///
    /// 落 `user_tags` 列（过度期与 `tags` 双写）。**不要**把 `published` 塞进来：
    /// 它是系统标记，请改用 `is_published` 参数；即使塞进来也会被剥离出
    /// `user_tags` 并折算成 `is_published = true`（语义收编），回显在
    /// `system_flags` 里。
    pub tags: Option<Vec<String>>,
    /// 发布标记（**写 `is_published`，唯一真相源**）。
    ///
    /// - `None`（默认）= 按 `tags` 里有无 `published` 推导，与拆分前行为一致；
    /// - `Some(true)` = 置为已发布；
    /// - `Some(false)` = 明确不发布。
    ///
    /// 有了它，「读全字段 → 原样回写」才不会再静默清掉发布标记：
    /// 调用方既可以把回显的 `system_flags` 折成本参数，也可以直接省略（不清空）。
    pub is_published: Option<bool>,
    /// 关联关系列表。
    pub relations: Option<Vec<KnowledgeRelationParam>>,
    /// 本节点的**来源短期记忆 id 列表**（沉淀场景必填）。
    ///
    /// 语义：本节点是从哪些短期记忆归纳出来的。框架据此展开每条短期记忆的
    /// `trace_ids`，按「一个 trace 一行」写入 `knowledge_reference`，从而让知识节点
    /// 能顺着「短期记忆 → 原始 trace → JSONL 文件+行号」追回当时的原始记录。
    ///
    /// ⚠️ 不填 = 本节点在图谱里无源头（与 `relations` 不同，这里没有框架兜底：
    /// 框架不知道「哪个节点来自哪条记忆」，只能靠调用方声明）。
    pub source_short_term_ids: Option<Vec<String>>,
    /// 节点属性列表（结构化字段，如「模型名」「生效时间」「置信度」）。
    ///
    /// 只有**可复用、可比较、需要被结构化过滤**的事实才适合放这里；
    /// 叙述性内容放 `node_description`（走全文检索），不要塞进属性。
    pub properties: Option<Vec<KnowledgePropertyParam>>,
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
    /// 属性写入的问题清单（`term_key: 原因`），空数组表示全部写入成功。
    ///
    /// ⚠️ 节点本身**已创建成功**：属性写入是「部分成功」语义，坏条目只跳过。
    /// 模型必须读这个字段——静默吞掉会让它以为属性写成功了，下次不再补。
    pub property_issues: Vec<String>,
    /// 逐条建边 keyed 回执（仅入参携带 relations 时出现）。
    /// ⚠️ 顶层 `relation_ids` 已收紧为「实际落库成功边 ID」，与本表 success 项一致；
    /// 占位符/不存在节点/批内重复等失败项逐条进本表（status=failed + 错误码），
    /// 整批仍返回 200。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations_result: Option<Vec<RelationWriteResult>>,
    /// 属性写入问题清单 V2（结构化 code/message/field，与 V1 串同内容并存一版）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_issues_v2: Option<Vec<PropertyIssueEntry>>,
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
            properties: None,
            search_match: None,
            system_flags: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(
            !json.contains("\"weight\""),
            "未标注的条目不应带 weight 键: {json}"
        );
    }

    /// relations_result / property_issues_v2 未提供时**不出现**在序列化载荷里
    /// （与 weight 同款契约：老消费方拿到的 JSON 与升级前逐字节同形）。
    #[test]
    fn v2_fields_are_omitted_when_not_provided() {
        let save = SaveLongTermMemoryResponse {
            node_id: "kn_a".into(),
            relation_ids: vec!["kr_1".into()],
            property_issues: vec![],
            relations_result: None,
            property_issues_v2: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        assert!(
            !json.contains("relations_result") && !json.contains("property_issues_v2"),
            "缺省时 V2 字段不应出现: {json}"
        );

        let update = UpdateMemoryResponse {
            memory_id: "kn_a".into(),
            property_issues: vec![],
            relations_result: None,
            property_issues_v2: None,
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(
            !json.contains("relations_result") && !json.contains("property_issues_v2"),
            "缺省时 V2 字段不应出现: {json}"
        );
    }

    /// 成功项只带 relation_id、失败项只带错误码：回执形态的契约自检。
    #[test]
    fn relation_write_result_shape() {
        let ok = RelationWriteResult {
            key: "0:kn_a->kn_b".into(),
            status: RelationWriteStatus::Success,
            relation_id: Some("kr_1".into()),
            error_code: None,
            message: None,
            field: None,
        };
        let json = serde_json::to_string(&ok).unwrap();
        assert!(
            json.contains("\"success\""),
            "状态序列化为 snake_case: {json}"
        );
        assert!(!json.contains("error_code"), "成功项不应带错误码: {json}");

        let bad = RelationWriteResult {
            key: "1:SELF->kn_b".into(),
            status: RelationWriteStatus::Failed,
            relation_id: None,
            error_code: Some("placeholder_node_id_rejected".into()),
            message: Some("占位符".into()),
            field: None,
        };
        let json = serde_json::to_string(&bad).unwrap();
        assert!(json.contains("placeholder_node_id_rejected"));
        assert!(
            !json.contains("relation_id"),
            "失败项不应带 relation_id: {json}"
        );
    }

    /// A1-7 ①：默认路径（未开 `include_history`）的属性条目**不出现** `is_current` 字段。
    ///
    /// 旧消费方只认识 `{term_key, value, ordinal}`。一旦 `is_current` 变成恒出现，
    /// 等于对所有既有调用方做了一次非预期 schema 变更 —— 本条把它钉死。
    #[test]
    fn current_property_omits_is_current_field() {
        let item = KnowledgePropertyItem {
            term_key: "lifecycle_status".into(),
            value: PropertyValue::Enum("现行有效".into()),
            ordinal: 0,
            is_current: true,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("is_current"),
            "生效值不得序列化 is_current（缺席 ⇒ true）: {json}"
        );
        assert!(json.contains("lifecycle_status"));
        assert!(json.contains("现行有效"), "值本身不能被吞掉: {json}");
    }

    /// A1-7 ②：`include_history=true` 时**历史行带** `"is_current": false`、
    /// **生效行不带**该字段 —— 同一数组内两种形状并存，且可被反序列化回落。
    #[test]
    fn history_property_marks_is_current_false_only_on_superseded_rows() {
        let items = vec![
            KnowledgePropertyItem {
                term_key: "event_time".into(),
                value: PropertyValue::Date("2026-11-04".into()),
                ordinal: 0,
                is_current: true,
            },
            KnowledgePropertyItem {
                term_key: "event_time".into(),
                value: PropertyValue::Date("2026-10-28".into()),
                ordinal: 0,
                is_current: false,
            },
        ];
        let json = serde_json::to_string(&items).unwrap();
        assert_eq!(
            json.matches("\"is_current\":false").count(),
            1,
            "恰有一条历史行带 is_current=false: {json}"
        );
        assert!(
            !json.contains("\"is_current\":true"),
            "生效行不得出现 is_current=true（应完全缺席）: {json}"
        );
        assert!(json.contains("2026-10-28"), "历史值必须回显: {json}");

        // 回落语义：字段缺席 ⇒ 视为 true（未来消费方按默认值解读不会翻错）
        let back: KnowledgePropertyItem =
            serde_json::from_str(r#"{"term_key":"k","value":{"text":"v"},"ordinal":0}"#).unwrap();
        assert!(back.is_current, "字段缺席必须反序列化为 true");
    }
}
