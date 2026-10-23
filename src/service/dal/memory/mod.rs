//! Memory DAL - 记忆数据访问层（业务逻辑层）
//!
//! 职责：跨 DAO 流程编排
//! - 获取 Embedding Provider
//! - 生成查询向量
//! - 混合搜索（全文 + 向量）
//! - 结果聚合排序
//!
//! 文件组织（按业务消费面拆分；`MemoryDal` trait 仍是单一事实源）：
//! - `search.rs`     —— 混合搜索 + 关系型查询
//! - `store.rs`      —— 创建 / 更新 / 删除 / 状态流转 / 向量重建
//! - `graph.rs`      —— 知识图谱聚合与遍历
//! - `properties.rs` —— 实体属性（ABox）读写与按属性召回
//! - `trace.rs`      —— 记忆轨迹查询
//!
//! ⚠️ trait 对同一类型只能有一个 `impl` 块（E0119），因此业务实现放在各文件的
//! **固有 impl** 中，本文件用 `memory_dal_impl!` 生成唯一的 trait impl 做 1:1 转发。

use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgePropertyPo, Memory, MemoryCreateParams, MemoryPo, MemoryTrace,
};
use crate::pkg::RequestContext;
use crate::pkg::background_task::TaskProgressCounter;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{
    MemoryDao, MemoryQuery, MemorySearch, MemoryTraceQuery, MemoryVectorDao,
};
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::ontology::OntologyDao;
use async_trait::async_trait;
use common::api::PagedResult;
use common::enums::ontology::PropertyTargetType;
use common::error::Result;
use common::ontology_property::{PropertyEntry, PropertyFilter, PropertySaveReport};
use std::collections::HashSet;
use std::sync::Arc;

mod graph;
mod properties;
mod search;
mod store;
mod trace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalStrategy {
    BreadthFirst,
    DepthFirst,
}

impl TraversalStrategy {
    /// 缺省策略：宽度优先（按跳数逐层展开，层级语义最直观）。
    pub const DEFAULT: TraversalStrategy = TraversalStrategy::BreadthFirst;

    /// API 侧的合法取值清单（供错误提示列出，让调用方能自我纠正）。
    pub const ACCEPTED_VALUES: &'static str = "breadth_first, depth_first";

    /// API 字符串 → 枚举；`None` 表示**非法值**。
    ///
    /// 与 `MemoryType::parse` 同口径：忽略首尾空白、大小写与下划线，
    /// 因此 `breadth_first` / `BreadthFirst` 等价。
    ///
    /// ⚠️ 非法值必须由调用方报 **400**，**禁止静默降级**成
    /// [`TraversalStrategy::BreadthFirst`]：遍历策略决定节点集与边集的形状，
    /// 静默换策略会让调用方拿到一张结构对不上的图，却看不出参数写错了。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().replace('_', "").to_ascii_lowercase().as_str() {
            "breadthfirst" => Some(TraversalStrategy::BreadthFirst),
            "depthfirst" => Some(TraversalStrategy::DepthFirst),
            _ => None,
        }
    }
}

// ⚠️ 历史注记：这里曾经有一个 `TraverseScope { agent_id, include_shared }` 可见性作用域，已删除。
// 原因是**知识节点是蜂巢共享资产**：任何 Agent 都能看到全部知识节点，遍历侧不存在
// 「归属门槛」这个概念，也就不需要一个「由谁传进来的作用域」。
//
// 历史教训（写下来防止回退）：可见性过滤曾经用 `ctx.agent_id()` 兜底，而 HTTP 场景
// （前端图谱页）的 ctx 里**没有** Agent 身份，取到空串会把目标 Agent 的私有节点全部滤掉、
// 边却不受同一约束 → 「只剩边、节点全丢」→ 前端只能把端点画成「未命名节点」。
// 现在归属只作为**显式筛选**（`MemoryQuery::agent_id`，空串 = 不过滤）存在，不再有隐式门槛。

/// 遍历过程中的可变状态包：把 3 个 &mut 累积器打包成单个 struct，
/// 用于 `traverse_bfs` / `traverse_dfs` 内部方法的参数瘦身。
struct TraverseState<'a> {
    visited_nodes: &'a mut HashSet<String>,
    visited_relations: &'a mut HashSet<String>,
    result_relations: &'a mut Vec<KnowledgeNodeRelationPo>,
}

// ==================== Factory + Singleton ====================

static MEMORY_DAL_INSTANCE: std::sync::OnceLock<Arc<dyn MemoryDal>> = std::sync::OnceLock::new();

pub fn new(
    memory_dao: Arc<dyn MemoryDao>,
    memory_vector_dao: Arc<dyn MemoryVectorDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
    cortex_dao: Arc<dyn CortexDao>,
    ontology_dao: Arc<dyn OntologyDao>,
) -> Arc<dyn MemoryDal> {
    Arc::new(MemoryDalImpl {
        memory_dao,
        memory_vector_dao,
        model_provider_dao,
        cortex_dao,
        ontology_dao,
    })
}

pub fn init() {
    crate::service::dao::ontology::init();
    let _ = MEMORY_DAL_INSTANCE.set(new(
        crate::service::dao::memory::dao(),
        crate::service::dao::memory::vector_dao(),
        crate::service::dao::model_provider::dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::ontology::dao(),
    ));
}

pub fn dal() -> Arc<dyn MemoryDal> {
    MEMORY_DAL_INSTANCE.get().cloned().unwrap()
}

// ==================== DAL Trait ====================

#[async_trait]
pub trait MemoryDal: Send + Sync {
    /// 🔍 统一混合搜索（关键词 + 向量语义）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走传统全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果
    /// - memory_type 过滤 → 只搜索指定类型
    ///
    /// ⚠️ 结果里的关系边保证**两端节点同在结果内**（见 `drop_dangling_relations`）：
    /// 关系搜索会带出命中节点的全部入/出边，其中「远端没命中」的边没有意义，一律丢弃。
    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>>;

    /// 📋 通用关系型查询（纯数据库查询，无向量）
    ///
    /// 支持所有组合过滤条件，可单独指定查询哪种记忆类型
    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>>;

    /// 🧾 分页查询记忆轨迹（原始思考闭环落盘记录）
    async fn query_traces(
        &self,
        ctx: RequestContext,
        query: MemoryTraceQuery,
    ) -> Result<PagedResult<MemoryTrace>>;

    /// 🔎 按 trace ID 读取单条记忆轨迹详情（完整 input/output）
    async fn get_trace(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        trace_id: &str,
    ) -> Result<Option<MemoryTrace>>;

    /// 🎯 推荐知识图谱起点节点
    ///
    /// 按节点关联度数（入边 + 出边总数）倒序返回 Top N 节点。
    /// 用于知识图谱页面"推荐起点"功能，帮助用户快速定位核心节点。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: 归属筛选；`None`（或空串）= 蜂巢全域节点都进推荐池
    /// - limit: 返回数量上限，默认 5
    ///
    /// 排序：连接度倒序；度数持平时 `is_published`（重要性/影响力）优先。
    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>>;

    /// 🕸️ 获取知识图谱全量数据（全局点线视图聚合）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: 归属筛选；`None`（或空串）= 蜂巢全域（知识节点是共享资产，
    ///   全局视图默认全量展示）
    ///
    /// # 口径
    /// - R1：仅知识节点（`long_term_knowledge_node`）+ 关系边，不含短期记忆/trace；
    /// - R2：边严格双端活跃——source/target 任一不在活跃节点集内的边一律丢弃；
    /// - R4：度数 = in + out，仅计生效边。
    async fn get_knowledge_graph(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
    ) -> Result<crate::models::memory::KnowledgeGraphData>;

    /// ✍️ 创建记忆（按 MemoryCreateParams 变体分发）
    ///
    /// 聚合流程：
    /// - `Trace` / `BatchTrace` → 写 daily JSONL（不向量化）
    /// - `ShortTerm` → 写库 + 向量化 summary（向量失败仅 warn 降级）
    /// - `KnowledgeNode` → 写库 + 写引用 + 向量化 summary（向量失败仅 warn 降级）
    /// - `Relation` → 写库（不向量化）
    async fn create(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>>;

    /// 🔄 更新记忆（仅支持 ShortTerm / KnowledgeNode）
    ///
    /// 自动重新向量化。Trace / Relation 返回 `common::error::Error::Unsupported`。
    async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory>;

    /// 🗑️ 删除记忆（支持 ShortTerm / KnowledgeNode / Relation）
    ///
    /// 入参为业务实体本身，便于 DAL 内做删除前校验/审计而无需重新查询：
    /// - `ShortTerm` → 删库 + 删向量索引
    /// - `KnowledgeNode` → 级联：删入边/出边关系 + 删引用 + 删节点 + 删向量
    /// - `Relation` → 软删除：标记边为 `Deleted`，行保留支持恢复（仅降级生效边）
    /// - `Trace` → 返回 `common::error::Error::Unsupported`
    async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()>;

    /// 🌐 知识图谱遍历
    ///
    /// 从种子节点出发，按指定策略遍历知识图谱。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - seed_node_ids: 种子节点 ID 列表（**必返回**，见下）
    /// - max_depth: 最大遍历深度（0 = 不展开，只返回种子节点）
    /// - max_breadth: 每个节点最多展开的出边数（0=不限制）
    /// - strategy: 遍历策略
    ///
    /// # 层级语义（**节点维度**）
    /// - 第 0 层 = 种子节点；每沿一条边走到一个新节点 = +1 层
    /// - `max_depth = N` ⇒ 节点集 = 与任一种子相距 ≤ N 跳的节点；
    ///   `N = 0` ⇒ 只有种子节点，不返回任何边（不展开）
    /// - `max_breadth` = **每个节点**最多展开的出边数（不是节点数）
    ///
    /// # 不变量一：种子恒返回
    /// 种子是调用方**点名**要的节点（前端点击展开的中心节点、关键词命中的起点），
    /// 无论它归属哪个 Agent 都必须出现在结果里 —— 中心节点缺席，整张展开图就没意义了。
    /// 因此取种子时不施加任何归属筛选（仅受软删除 `status` 约束）。
    ///
    /// # 不变量二：蜂巢可见性，遍历不过滤归属
    /// 知识节点是全体 Agent 共享的资产（`published` 只是「重要性/影响力」标记，
    /// 不是可见性控制位），遍历出来的邻居不因归属被丢弃。
    ///
    /// # 不变量三：边只随两端节点一起返回
    /// 见 `drop_dangling_relations`：端点不在批内的边一律丢弃（层级是节点维度的概念，
    /// 边不是层级实体，孤立在批外的边没有意义）。
    async fn traverse_knowledge_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
    ) -> Result<Vec<Memory>>;

    // ========== 实体属性（ABox 泛化挂载：节点 / 边共用一张表） ==========

    /// ✍️ 写入一个载体的属性（带本体门闩，部分成功语义）
    ///
    /// # 门闩顺序（每条独立判定，一条坏不连累一批）
    /// 1. `applies_to` 载体门闩 —— 定义声明了适用载体却没命中 ⇒ 告警后照写；
    /// 2. `data_type` 类型门闩 —— 值形态与声明不符 ⇒ **拒绝本条**（这是写错，
    ///    不是认知分歧，没有解释空间）；
    /// 3. `value_range` 值域 / 白名单 —— 越界 ⇒ 告警后照写（本体是包容的抽象）；
    /// 4. `cardinality` 基数 —— 单值属性被重复写 ⇒ 拒绝本条（无处安置）。
    ///
    /// 本体未定义的 `term_key` 走**兜底写入**：按值自身类型落库并记
    /// `UndefinedProperty` 告警，不丢数据（术语漂移是常态，写侧不裁决）。
    ///
    /// # 返回值语义
    /// `Ok(report)` **不代表全部写入成功**——非法条目在 `report.issues` 里。
    /// 这是刻意对齐本体的软门禁哲学：漂移不拦截、不撤回，只标记待审。
    ///
    /// # 单值属性的槽位归一
    /// `Cardinality::Single` 一律强制 `ordinal = 0`，调用方传什么都一样：
    /// 「同一个槽位被新值取代」正是版本化修正的本意，不算基数冲突。
    async fn save_entity_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
        entries: Vec<PropertyEntry>,
    ) -> Result<PropertySaveReport>;

    /// 📖 取一个载体的全部生效属性（历史版本不返回）
    async fn list_entity_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
    ) -> Result<Vec<KnowledgePropertyPo>>;

    /// 📖 批量取多个载体的生效属性（防 N+1；搜索结果回显一趟查完）
    async fn list_entity_properties_batch(
        &self,
        ctx: RequestContext,
        targets: &[(PropertyTargetType, String)],
    ) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>>;

    /// 🔍 按属性条件**精确召回**载体 id（结构化过滤入口）
    ///
    /// # 为什么是独立召回通道而不是搜索过滤器
    /// `MemoryQuery` 的字段会被转译成向量库谓词下推（见 `dao/memory/vector.rs`），
    /// 而 `VectorField` 只有九个取值、属性值不在向量 payload 里 ⇒ 挂到
    /// `MemoryQuery` 上会被向量通道**静默忽略**、只被 SQL 通道生效，两条召回
    /// 通道语义割裂。这里直接反查业务表拿 id，由调用方注入
    /// `MemoryQuery.ids`（该字段明确定位为「回业务表过滤兜底」，对三条通道一视同仁）。
    ///
    /// # AND 语义
    /// 多个条件逐个反查后取交集；运算符与值类型不匹配（如文本用 `Gt`）直接报错，
    /// **不做静默放宽**——否则会返回过宽的 id 集，让调用方以为筛过了。
    async fn query_entities_by_properties(
        &self,
        ctx: RequestContext,
        filters: &[PropertyFilter],
    ) -> Result<Vec<String>>;

    /// 🏛️ 将未沉淀的短期记忆总结并沉淀为长期知识
    ///
    /// 流程：
    /// 1. 查询 Agent 的活跃短期记忆（status = Active）
    /// 2. 按时间/主题分组聚合
    /// 3. 创建知识节点（summary 作为节点描述）
    /// 4. 创建引用关系（关联原始短期记忆）
    /// 5. 标记短期记忆为已沉淀（status = Settled）
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// 批量把短期记忆标记为「已沉淀」（`Active` → `Settled`）
    ///
    /// # 用途
    ///
    /// 沉淀流程的**状态闭环**由框架负责，不依赖 LLM 自觉。沉淀 prompt 已明确告知
    /// Agent 无需自行调用 `update_memory` 改状态（技能文档不改，因其可能被其它
    /// 框架复用，那里未必有框架兜底）。
    ///
    /// # 语义
    ///
    /// - 只翻转传入 id 中**当前仍为 Active** 的记录；已被 Agent 处理过的（无论
    ///   改成什么状态）保持原状，不覆盖模型的判断。
    /// - 逐条更新，单条失败只记 warn 并继续，不中断整批。
    /// - 不创建任何知识节点——知识整合是 Agent 在沉淀循环里用记忆工具完成的，
    ///   本函数只管状态。
    ///
    /// # 参数
    /// - `agent_id`: 归属校验用，防止跨 Agent 误改
    /// - `memory_ids`: 本批沉淀处理过的短期记忆 id
    ///
    /// # 返回
    /// 实际被置为 Settled 的条数
    async fn mark_short_term_settled(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        memory_ids: &[String],
    ) -> Result<usize>;

    /// 🔄 重建所有记忆的向量索引
    ///
    /// 清空 short_term 和 knowledge_node 两个向量集合后，
    /// **分页**查询全量短期记忆和知识节点，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录；每页处理完通过 `progress` 上报条数。
    async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
    ) -> Result<()>;
}

// ==================== Implementation ====================

pub struct MemoryDalImpl {
    pub(super) memory_dao: Arc<dyn MemoryDao>,
    pub(super) memory_vector_dao: Arc<dyn MemoryVectorDao>,
    pub(super) model_provider_dao: Arc<dyn ModelProviderDao>,
    pub(super) cortex_dao: Arc<dyn CortexDao>,
    pub(super) ontology_dao: Arc<dyn OntologyDao>,
}

/// 丢弃**端点缺失**的关系边 —— 「节点 + 边」批次的全局不变式。
///
/// ⚠️ 层级（`traversal_depth`）是**节点维度**的概念：第 0 层是种子节点，沿一条边走到
/// 新节点算 +1 层。**边不是层级实体**，它只是连接两个「已经在本批结果里的节点」的线；
/// 端点不在批内的边没有意义：
/// - 前端只能把缺失端点画成「未命名节点」（没有名称、没有正文、hover 没内容的一屏卡片）；
/// - LLM 拿到的也只是「A → ?」的半条信息。
///
/// 因此**任何**「节点 + 边同批返回」的路径（`search` / `traverse_knowledge_graph`）都要
/// 过这道闸：宁可少一条边，也不返回半条。前端 `build_graph_from_results` 是同一不变式的
/// 第二道防线（它同样不画端点缺失的边，且**不再**为缺失端点造占位节点）。
pub(super) fn drop_dangling_relations(memories: &mut Vec<Memory>) {
    if !memories
        .iter()
        .any(|m| matches!(m.po, MemoryPo::Relation(_)))
    {
        return;
    }
    // 先收集端点集合再 retain：否则端点集合的借用会与 retain 的可变借用打架
    let node_ids: HashSet<String> = memories
        .iter()
        .filter_map(|m| match &m.po {
            MemoryPo::KnowledgeNode(n) => Some(n.id.clone()),
            MemoryPo::ShortTerm(s) => Some(s.id.clone()),
            _ => None,
        })
        .collect();
    memories.retain(|m| match &m.po {
        MemoryPo::Relation(r) => {
            node_ids.contains(&r.source_node_id) && node_ids.contains(&r.target_node_id)
        }
        _ => true,
    });
}

// ==================== Trait 桥接 ====================
//
// E0119：同一 trait 对同一具体类型只能有一个 `impl` 块，因此业务实现分散为
// 各子模块的**固有 impl**，这里生成唯一 trait impl 做 1:1 转发。
// 固有方法优先于 trait 方法参与解析，`MemoryDalImpl::search` 不会递归到本 impl。

macro_rules! memory_dal_impl {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty),* $(,)?) => {
        #[async_trait]
        impl MemoryDal for MemoryDalImpl {
            $(
                async fn $name(&self, $($arg: $ty),*) -> $ret {
                    MemoryDalImpl::$name(self, $($arg),*).await
                }
            )*
        }
    };
}

memory_dal_impl! {
    search(ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>>,
    query(ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>>,
    query_traces(ctx: RequestContext, query: MemoryTraceQuery) -> Result<PagedResult<MemoryTrace>>,
    get_trace(ctx: RequestContext, agent_id: &str, trace_id: &str) -> Result<Option<MemoryTrace>>,
    recommend_seed_nodes(ctx: RequestContext, agent_id: Option<String>, limit: usize)
        -> Result<Vec<crate::models::memory::SeedNodeRecommendation>>,
    get_knowledge_graph(ctx: RequestContext, agent_id: Option<String>)
        -> Result<crate::models::memory::KnowledgeGraphData>,
    create(ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>>,
    update(ctx: RequestContext, memory: Memory) -> Result<Memory>,
    delete(ctx: RequestContext, memory: Memory) -> Result<()>,
    traverse_knowledge_graph(
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy
    ) -> Result<Vec<Memory>>,
    save_entity_properties(
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
        entries: Vec<PropertyEntry>
    ) -> Result<PropertySaveReport>,
    list_entity_properties(
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str
    ) -> Result<Vec<KnowledgePropertyPo>>,
    list_entity_properties_batch(
        ctx: RequestContext,
        targets: &[(PropertyTargetType, String)]
    ) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>>,
    query_entities_by_properties(ctx: RequestContext, filters: &[PropertyFilter])
        -> Result<Vec<String>>,
    mark_short_term_settled(ctx: RequestContext, agent_id: &str, memory_ids: &[String])
        -> Result<usize>,
    rebuild_vectors(ctx: RequestContext, progress: &TaskProgressCounter) -> Result<()>,
}

#[cfg(test)]
mod graph_test;
#[cfg(test)]
mod properties_test;
#[cfg(test)]
mod search_test;
#[cfg(test)]
mod store_test;
#[cfg(test)]
mod test_support;
