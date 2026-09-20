//! Memory DAL - 记忆数据访问层（业务逻辑层）
//!
//! 职责：跨 DAO 流程编排
//! - 获取 Embedding Provider
//! - 生成查询向量
//! - 混合搜索（全文 + 向量）
//! - 结果聚合排序

use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, Memory,
    MemoryCreateParams, MemoryPo, MemoryTrace, ShortTermMemoryIndexPo,
};
use crate::models::vector::{
    MatchType, ReindexDecision, SearchMatchInfo, VectorIndexParams, Vectorizable,
};
use crate::pkg::RequestContext;
use crate::pkg::background_task::TaskProgressCounter;
use crate::service::dal::VECTOR_REBUILD_PAGE_SIZE;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::memory::{MemoryDao, MemoryQuery, MemorySearch, MemoryVectorDao};
use crate::service::dao::model_provider::ModelProviderDao;
use async_trait::async_trait;
use common::enums::MemoryStatus;
use common::enums::MemoryType;
use common::error::{Result, bail_err};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

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
) -> Arc<dyn MemoryDal> {
    Arc::new(MemoryDalImpl {
        memory_dao,
        memory_vector_dao,
        model_provider_dao,
        cortex_dao,
    })
}

pub fn init() {
    let _ = MEMORY_DAL_INSTANCE.set(new(
        crate::service::dao::memory::dao(),
        crate::service::dao::memory::vector_dao(),
        crate::service::dao::model_provider::dao(),
        crate::service::dao::cortex::dao(),
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
        progress: &crate::pkg::background_task::TaskProgressCounter,
    ) -> Result<()>;
}

// ==================== Implementation ====================

pub struct MemoryDalImpl {
    memory_dao: Arc<dyn MemoryDao>,
    memory_vector_dao: Arc<dyn MemoryVectorDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
    cortex_dao: Arc<dyn CortexDao>,
}

#[async_trait]
impl MemoryDal for MemoryDalImpl {
    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>> {
        let memory_type = search.filters.memory_type.unwrap_or(MemoryType::All);
        let mut results: Vec<Memory> = Vec::new();

        // 1. 搜索短期记忆
        if memory_type == MemoryType::All || memory_type == MemoryType::ShortTerm {
            // 短期记忆按**归属**隔离：调用方没给作用域时回退到「请求上下文里自己的 Agent」，
            // 这样 Agent 的检索天然只看到自己的便签。
            // ⚠️ 知识节点**不做**这个回退：它是蜂巢共享资产，回退会让 Agent 只搜到自己的节点。
            // ⚠️ 上下文里也没有归属时（人类在前端浏览），这里保持**不过滤**的既有行为，
            // 不在本次改动里收紧 —— 见 `private_agent_scope` 的说明。
            let mut short_term_search = search.clone();
            short_term_search.filters = private_agent_scope(&ctx, search.filters.clone());
            let short_term_results = self
                .search_short_term_internal(ctx.clone(), short_term_search)
                .await?;
            results.extend(short_term_results);
        }

        // 2. 搜索知识节点
        if memory_type == MemoryType::All || memory_type == MemoryType::KnowledgeNode {
            let knowledge_results = self
                .search_knowledge_nodes_internal(ctx.clone(), search.clone())
                .await?;
            results.extend(knowledge_results);
        }

        // 3. Relation 类型不支持向量搜索，但支持关键词查询（如果有关键词）
        if (memory_type == MemoryType::All || memory_type == MemoryType::Relation)
            && search.keyword.is_some()
        {
            let relation_results = self
                .search_relations_internal(ctx.clone(), search.clone())
                .await?;
            results.extend(relation_results);
        }

        // 3.5 图批次不变式：边必须两端节点同在批内。
        // `search_relations_internal` 会把命中节点的全部入/出边一并带出，
        // 其中「远端节点没命中」的边必须在这里丢掉，否则前端只能把远端画成
        // 「未命名节点」、LLM 拿到「A → ?」的半条信息。
        drop_dangling_relations(&mut results);

        // 4. 统一排序：Hybrid 优先 → Vector 次之 → Keyword/None 最后
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        results.sort_by(|a, b| {
            let a_type = a.search_match.as_ref().map(|m| m.match_type);
            let b_type = b.search_match.as_ref().map(|m| m.match_type);
            let order_a = match a_type {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            let order_b = match b_type {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            order_a.cmp(&order_b).then_with(|| match (a_type, b_type) {
                (Some(MatchType::Hybrid), Some(MatchType::Hybrid))
                | (Some(MatchType::Vector), Some(MatchType::Vector)) => {
                    let a_dist = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    let b_dist = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    a_dist
                        .partial_cmp(&b_dist)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                _ => {
                    let a_rank = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    let b_rank = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    a_rank
                        .partial_cmp(&b_rank)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            })
        });

        // 5. 应用 limit
        if let Some(limit) = search.filters.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>> {
        let memory_type = query.memory_type.unwrap_or(MemoryType::All);
        let mut results: Vec<Memory> = Vec::new();

        // 1. 查询短期记忆（用 DAO 的通用 query
        //    短期记忆按归属隔离：作用域缺省时回退 ctx 自己的 Agent（Agent 调用天然只看自己）；
        //    ctx 里也没有归属时（人类浏览全局页面）保持**不过滤**的既有行为。
        //    知识节点的蜂巢共享**不**延伸到短期记忆。
        if memory_type == MemoryType::All || memory_type == MemoryType::ShortTerm {
            let pos = self
                .memory_dao
                .query_short_term(ctx.clone(), private_agent_scope(&ctx, query.clone()))
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::ShortTerm(po),
                search_match: None,
            }));
        }

        // 2. 查询知识节点
        if memory_type == MemoryType::All || memory_type == MemoryType::KnowledgeNode {
            let pos = self
                .memory_dao
                .query_knowledge_nodes(ctx.clone(), query.clone())
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::KnowledgeNode(po),
                search_match: None,
            }));
        }

        // 3. 查询关系（按 ids 精确取边；关系是依附节点的派生视图，
        //    无 ids 时 DAO 返回空 —— All 的列表语义天然不包含独立边）
        if memory_type == MemoryType::All || memory_type == MemoryType::Relation {
            let pos = self
                .memory_dao
                .query_knowledge_relations(ctx.clone(), query)
                .await?;
            results.extend(pos.into_iter().map(|po| Memory {
                po: MemoryPo::Relation(po),
                search_match: None,
            }));
        }

        Ok(results)
    }

    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>> {
        use crate::models::memory::SeedNodeRecommendation;
        use crate::service::dao::memory::MemoryQuery;
        use common::enums::{MemoryStatus, MemoryType};

        // 1. 拉取知识节点
        //
        // ⚠️ 蜂巢语义：不选 Agent 时（`agent_id = None`）池子是**全部** Agent 的知识节点，
        // 不再是「只挑 published」。`published` 降级为「重要性/影响力」信号，只在
        // 连接度持平时作为决胜项（见下方排序），不再决定可见性/入池资格。
        let query = MemoryQuery {
            memory_type: Some(MemoryType::KnowledgeNode),
            agent_id: agent_id.clone().filter(|s| !s.is_empty()),
            status: Some(MemoryStatus::Active),
            exclude_status: Some(MemoryStatus::Forgotten),
            limit: Some(500), // 上限保护，避免节点过多拖慢统计
            ..Default::default()
        };
        let nodes = self
            .memory_dao
            .query_knowledge_nodes(ctx.clone(), query)
            .await?;

        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 批量查询这批节点的所有关系
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let relations = self.memory_dao.list_relations_batch(ctx, &node_ids).await?;

        // 3. 应用层统计每个节点的度数
        use std::collections::HashMap;
        let mut degree_map: HashMap<String, (usize, usize)> = HashMap::new();
        for rel in &relations {
            // 出边：rel.source_node_id 指向 rel.target_node_id
            degree_map.entry(rel.source_node_id.clone()).or_default().1 += 1;
            // 入边：rel.target_node_id 被 rel.source_node_id 引用
            degree_map.entry(rel.target_node_id.clone()).or_default().0 += 1;
        }

        // 4. 组装推荐列表：连接度倒序；度数持平用 `is_published`（重要性/影响力）决胜
        //    —— 这是该标记在新语义下唯一的用途：不是门槛，只是「同等连接度时更值得当起点」。
        let mut recommendations: Vec<SeedNodeRecommendation> = nodes
            .into_iter()
            .map(|node| {
                let (incoming, outgoing) = degree_map.get(&node.id).copied().unwrap_or((0, 0));
                SeedNodeRecommendation {
                    degree: incoming + outgoing,
                    incoming_count: incoming,
                    outgoing_count: outgoing,
                    node,
                }
            })
            .collect();
        recommendations.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.degree),
                std::cmp::Reverse(r.node.is_published),
            )
        });

        // 5. 截断到 limit
        recommendations.truncate(limit);
        Ok(recommendations)
    }

    async fn create(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>> {
        match params {
            MemoryCreateParams::AppendTraces(traces) => {
                self.create_append_traces(ctx, traces).await
            }
            MemoryCreateParams::CreateShortTerm(index) => self.create_short_term(ctx, index).await,
            MemoryCreateParams::CreateKnowledgeNode { node, references } => {
                self.create_knowledge_node(ctx, node, references).await
            }
            MemoryCreateParams::CreateRelations(relations) => {
                self.create_relations(ctx, relations).await
            }
        }
    }

    async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory> {
        match memory.po {
            crate::models::memory::MemoryPo::ShortTerm(short_term) => {
                // 更新 SQLite 索引
                self.memory_dao
                    .update_short_term_index(ctx.clone(), short_term.clone())
                    .await?;

                // 重新向量化 summary + tags（三态：Skip / PayloadOnly / FullReindex）
                match try_build_vector_params_for_entity(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &short_term,
                    ShortTermMemoryIndexPo::vector_collection(),
                    &short_term.id,
                )
                .await
                {
                    Ok(VectorIndexAction::Reindex(vec_params)) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_short_term_vector(ctx.clone(), &short_term.id, &vec_params)
                            .await
                        {
                            log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量索引更新失败，已降级");
                        }
                    }
                    Ok(VectorIndexAction::Skipped) => {
                        log_debug!(ctx, "vector_index", memory_id= %short_term.id, "向量索引未变化，跳过");
                    }
                    Ok(VectorIndexAction::PayloadRefreshed) => {}
                    Err(e) => {
                        log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量化失败，跳过向量索引更新");
                    }
                }

                Ok(Memory {
                    po: crate::models::memory::MemoryPo::ShortTerm(short_term),
                    search_match: memory.search_match,
                })
            }
            crate::models::memory::MemoryPo::KnowledgeNode(node) => {
                // 更新 SQLite 节点
                self.memory_dao
                    .update_knowledge_node(ctx.clone(), &node)
                    .await?;

                // 重新向量化（node_description + summary + tags 拼接；三态：Skip / PayloadOnly / FullReindex）
                match try_build_vector_params_for_entity(
                    ctx.clone(),
                    &self.cortex_dao,
                    &self.model_provider_dao,
                    &node,
                    LongTermKnowledgeNodePo::vector_collection(),
                    &node.id,
                )
                .await
                {
                    Ok(VectorIndexAction::Reindex(vec_params)) => {
                        if let Err(e) = self
                            .memory_vector_dao
                            .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                            .await
                        {
                            log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量索引更新失败，已降级");
                        }
                    }
                    Ok(VectorIndexAction::Skipped) => {
                        log_debug!(ctx, "vector_index", knowledge_id= %node.id, "向量索引未变化，跳过");
                    }
                    Ok(VectorIndexAction::PayloadRefreshed) => {}
                    Err(e) => {
                        log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量化失败，跳过向量索引更新");
                    }
                }

                Ok(Memory {
                    po: crate::models::memory::MemoryPo::KnowledgeNode(node),
                    search_match: memory.search_match,
                })
            }
            crate::models::memory::MemoryPo::Trace(_) => {
                bail_err!(UnsupportedOperation, "原始记忆 Trace 不可修改");
            }
            crate::models::memory::MemoryPo::Relation(_) => {
                bail_err!(UnsupportedOperation, "记忆 Relation 不可修改，需删除后重建");
            }
        }
    }

    async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()> {
        match memory.po {
            crate::models::memory::MemoryPo::ShortTerm(short_term) => {
                // 软删除 SQLite 索引
                self.memory_dao
                    .forget_short_term_index(ctx.clone(), &short_term.id)
                    .await?;
                // 删除向量索引（忽略失败，不影响主流程）
                if let Err(e) = self
                    .memory_vector_dao
                    .delete_short_term_vector(ctx.clone(), &short_term.id)
                    .await
                {
                    log_warn!(ctx, "vector_index", memory_id= %short_term.id, error = ?e, "短期记忆向量索引删除失败，已降级");
                }
                Ok(())
            }
            crate::models::memory::MemoryPo::KnowledgeNode(node) => {
                // 级联删除 SQLite 节点（包含关系和引用）
                self.memory_dao
                    .delete_knowledge_node(ctx.clone(), &node.id)
                    .await?;
                // 删除向量索引（忽略失败，不影响主流程）
                if let Err(e) = self
                    .memory_vector_dao
                    .delete_knowledge_node_vector(ctx.clone(), &node.id)
                    .await
                {
                    log_warn!(ctx, "vector_index", knowledge_id= %node.id, error = ?e, "知识节点向量索引删除失败，已降级");
                }
                Ok(())
            }
            crate::models::memory::MemoryPo::Trace(_) => {
                bail_err!(UnsupportedOperation, "原始记忆 Trace 不可删除");
            }
            crate::models::memory::MemoryPo::Relation(rel) => {
                // 软删除：标记边为 Deleted(2)，行保留支持恢复；仅降级生效边的保护在 DAO 层
                self.memory_dao
                    .delete_knowledge_relation(ctx, &rel.id)
                    .await
            }
        }
    }

    async fn traverse_knowledge_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
    ) -> Result<Vec<Memory>> {
        if seed_node_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_relations: HashSet<String> = HashSet::new();
        let mut result_relations: Vec<KnowledgeNodeRelationPo> = Vec::new();

        // 不变量一：种子是被点名的节点，先无条件放入（取回时不施加归属筛选）
        for id in seed_node_ids {
            visited_nodes.insert(id.clone());
        }

        if max_depth <= 0 {
            let nodes = self.fetch_nodes_by_ids(ctx.clone(), &visited_nodes).await?;
            return Ok(self.build_graph_memories(nodes, result_relations));
        }

        match strategy {
            TraversalStrategy::BreadthFirst => {
                self.traverse_bfs(
                    ctx.clone(),
                    seed_node_ids,
                    max_depth,
                    max_breadth,
                    TraverseState {
                        visited_nodes: &mut visited_nodes,
                        visited_relations: &mut visited_relations,
                        result_relations: &mut result_relations,
                    },
                )
                .await?;
            }
            TraversalStrategy::DepthFirst => {
                self.traverse_dfs(
                    ctx.clone(),
                    seed_node_ids,
                    max_depth,
                    max_breadth,
                    TraverseState {
                        visited_nodes: &mut visited_nodes,
                        visited_relations: &mut visited_relations,
                        result_relations: &mut result_relations,
                    },
                )
                .await?;
            }
        }

        let nodes = self.fetch_nodes_by_ids(ctx.clone(), &visited_nodes).await?;
        Ok(self.build_graph_memories(nodes, result_relations))
    }

    async fn mark_short_term_settled(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        memory_ids: &[String],
    ) -> Result<usize> {
        if memory_ids.is_empty() {
            return Ok(0);
        }

        // 先按 id + agent 捞一遍，只翻转仍为 Active 的
        // （Agent 可能已自行处理掉一部分，不覆盖它的判断）
        let still_active = self
            .memory_dao
            .query_short_term(
                ctx.clone(),
                MemoryQuery {
                    ids: Some(memory_ids.to_vec()),
                    agent_id: Some(agent_id.to_string()),
                    status: Some(MemoryStatus::Active),
                    memory_type: Some(MemoryType::ShortTerm),
                    ..Default::default()
                },
            )
            .await?;

        let mut marked = 0usize;
        let now = common::constants::utils::current_timestamp_ms();
        for index in still_active {
            let mut updated = index.clone();
            updated.status = MemoryStatus::Settled;
            // DAO 的 update_short_term_index 是整行 UPDATE，updated_at 取传入值，
            // 这里必须显式刷新，否则状态改了但 updated_at 仍停在原值。
            updated.updated_at = now;
            match self
                .memory_dao
                .update_short_term_index(ctx.clone(), updated)
                .await
            {
                Ok(_) => marked += 1,
                Err(e) => log_warn!(
                    ctx,
                    "settle_memory",
                    memory_id = %index.id,
                    error = ?e,
                    "标记短期记忆为已沉淀失败"
                ),
            }
        }

        Ok(marked)
    }

    async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
    ) -> Result<()> {
        // 1. 获取当前启用的 Embedding Provider
        let Some(provider) = self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            log_debug!(
                &ctx,
                "rebuild_vectors",
                "无可用 Embedding Provider，跳过向量索引"
            );
            return Ok(());
        };
        let current_provider_id = provider.id.clone();

        // 2. 分别检查两个集合的 model_provider_id
        let short_term_stored = ctx
            .vector_store()
            .get_collection_model_provider_id(ShortTermMemoryIndexPo::vector_collection())
            .await?;
        let knowledge_node_stored = ctx
            .vector_store()
            .get_collection_model_provider_id(LongTermKnowledgeNodePo::vector_collection())
            .await?;

        let short_term_need_rebuild = short_term_stored.as_ref() != Some(&current_provider_id);
        let knowledge_node_need_rebuild =
            knowledge_node_stored.as_ref() != Some(&current_provider_id);

        if !short_term_need_rebuild && !knowledge_node_need_rebuild {
            log_info!(
                &ctx,
                "rebuild_vectors",
                provider_id = %current_provider_id,
                "记忆向量索引 model_provider_id 一致，跳过重建"
            );
            return Ok(());
        }

        // 3. 清空需要重建的集合
        if short_term_need_rebuild {
            ctx.vector_store()
                .clear_collection(ShortTermMemoryIndexPo::vector_collection())
                .await?;
        }
        if knowledge_node_need_rebuild {
            ctx.vector_store()
                .clear_collection(LongTermKnowledgeNodePo::vector_collection())
                .await?;
        }

        // 4. 重建短期记忆向量索引（如需要；清空后旧行不存在，必然 FullReindex）
        // 分页重建：排序键 `updated_at DESC, id DESC` 确定页序；期间被更新的记忆
        // 会由常规写路径自行 upsert 向量，故偏移漂移无副作用。
        if short_term_need_rebuild {
            let mut offset = 0usize;
            loop {
                let page = self
                    .memory_dao
                    .query_short_term(
                        ctx.clone(),
                        MemoryQuery {
                            limit: Some(VECTOR_REBUILD_PAGE_SIZE),
                            offset: Some(offset),
                            ..Default::default()
                        },
                    )
                    .await?;
                let fetched = page.len();
                if fetched == 0 {
                    break;
                }
                for index in &page {
                    match try_build_vector_params_for_entity(
                        ctx.clone(),
                        &self.cortex_dao,
                        &self.model_provider_dao,
                        index,
                        ShortTermMemoryIndexPo::vector_collection(),
                        &index.id,
                    )
                    .await
                    {
                        Ok(VectorIndexAction::Reindex(vec_params)) => {
                            if let Err(e) = self
                                .memory_vector_dao
                                .upsert_short_term_vector(ctx.clone(), &index.id, &vec_params)
                                .await
                            {
                                log_warn!(
                                    &ctx,
                                    "rebuild_vectors",
                                    memory_id = %index.id,
                                    error = ?e,
                                    "短期记忆向量索引重建失败"
                                );
                            }
                        }
                        Ok(VectorIndexAction::Skipped) => {
                            log_debug!(
                                &ctx,
                                "rebuild_vectors",
                                memory_id = %index.id,
                                "向量索引未变化，跳过"
                            );
                        }
                        Ok(VectorIndexAction::PayloadRefreshed) => {}
                        Err(e) => {
                            log_warn!(
                                &ctx,
                                "rebuild_vectors",
                                memory_id = %index.id,
                                error = ?e,
                                "短期记忆向量化失败，跳过"
                            );
                        }
                    }
                }
                offset += fetched;
                progress.advance(fetched);
                if fetched < VECTOR_REBUILD_PAGE_SIZE {
                    break;
                }
            }
            ctx.vector_store()
                .set_collection_model_provider_id(
                    ShortTermMemoryIndexPo::vector_collection(),
                    &current_provider_id,
                )
                .await?;
        }

        // 6. 重建知识节点向量索引（如需要；清空后旧行不存在，必然 FullReindex）
        if knowledge_node_need_rebuild {
            let mut offset = 0usize;
            loop {
                let page = self
                    .memory_dao
                    .query_knowledge_nodes(
                        ctx.clone(),
                        MemoryQuery {
                            limit: Some(VECTOR_REBUILD_PAGE_SIZE),
                            offset: Some(offset),
                            ..Default::default()
                        },
                    )
                    .await?;
                let fetched = page.len();
                if fetched == 0 {
                    break;
                }
                for node in &page {
                    match try_build_vector_params_for_entity(
                        ctx.clone(),
                        &self.cortex_dao,
                        &self.model_provider_dao,
                        node,
                        LongTermKnowledgeNodePo::vector_collection(),
                        &node.id,
                    )
                    .await
                    {
                        Ok(VectorIndexAction::Reindex(vec_params)) => {
                            if let Err(e) = self
                                .memory_vector_dao
                                .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                                .await
                            {
                                log_warn!(
                                    &ctx,
                                    "rebuild_vectors",
                                    knowledge_id = %node.id,
                                    error = ?e,
                                    "知识节点向量索引重建失败"
                                );
                            }
                        }
                        Ok(VectorIndexAction::Skipped) => {
                            log_debug!(
                                &ctx,
                                "rebuild_vectors",
                                knowledge_id = %node.id,
                                "向量索引未变化，跳过"
                            );
                        }
                        Ok(VectorIndexAction::PayloadRefreshed) => {}
                        Err(e) => {
                            log_warn!(
                                &ctx,
                                "rebuild_vectors",
                                knowledge_id = %node.id,
                                error = ?e,
                                "知识节点向量化失败，跳过"
                            );
                        }
                    }
                }
                offset += fetched;
                progress.advance(fetched);
                if fetched < VECTOR_REBUILD_PAGE_SIZE {
                    break;
                }
            }
            ctx.vector_store()
                .set_collection_model_provider_id(
                    LongTermKnowledgeNodePo::vector_collection(),
                    &current_provider_id,
                )
                .await?;
        }

        Ok(())
    }
}

// ==================== Internal Helper Methods ====================

/// 私有资产（短期记忆）的作用域补全：调用方没显式给归属时，回退到「本次请求自己的 Agent」。
///
/// 为什么只有私有资产能回退、知识节点不能：
/// - 短期记忆是 Agent 的工作便签，天然属于请求发起者。Agent 调 `search_memory` 时 ctx 里有
///   自己的 Agent 身份 → 自动收窄成「只看自己的便签」，与改动前行为一致。
/// - 知识节点是**蜂巢共享**资产。若也回退 ctx，Agent 调 `search_memory` 时会静默收窄成
///   「只看自己沉淀的节点」，与「知识不重复不遗漏」的蜂巢语义正好相反。
///
/// ⚠️ 回退后仍为空（人类在前端浏览，ctx 里没有 Agent 身份）时**保持不过滤**——这是本次
/// 改动之前就有的行为（`Some("")` 被 DAO 当作「不筛选」），刻意不在这次一起收紧：
/// 收紧会让「记忆搜索」页在不选 Agent 时查不到任何短期记忆，属于另一条独立的产品决策。
fn private_agent_scope(ctx: &RequestContext, mut query: MemoryQuery) -> MemoryQuery {
    if query.agent_id.is_none() {
        query.agent_id = ctx.agent_id().cloned().filter(|s| !s.is_empty());
    }
    query
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
fn drop_dangling_relations(memories: &mut Vec<Memory>) {
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

impl MemoryDalImpl {
    async fn traverse_bfs(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        state: TraverseState<'_>,
    ) -> Result<()> {
        let TraverseState {
            visited_nodes,
            visited_relations,
            result_relations,
        } = state;
        let mut queue: VecDeque<(String, i32)> = VecDeque::new();
        for id in seed_node_ids {
            queue.push_back((id.clone(), 0));
        }

        let mut current_depth = 0;
        while current_depth < max_depth && !queue.is_empty() {
            let mut current_level_nodes: Vec<String> = Vec::new();
            while let Some((node_id, depth)) = queue.front() {
                if *depth != current_depth {
                    break;
                }
                current_level_nodes.push(node_id.clone());
                queue.pop_front();
            }

            if current_level_nodes.is_empty() {
                break;
            }

            let all_relations = self
                .memory_dao
                .list_relations_batch(ctx.clone(), &current_level_nodes)
                .await?;

            let mut relations_by_node: HashMap<String, Vec<KnowledgeNodeRelationPo>> =
                HashMap::new();
            for rel in &all_relations {
                relations_by_node
                    .entry(rel.source_node_id.clone())
                    .or_default()
                    .push(rel.clone());
                relations_by_node
                    .entry(rel.target_node_id.clone())
                    .or_default()
                    .push(rel.clone());
            }

            for node_id in &current_level_nodes {
                let node_relations = relations_by_node.get(node_id).cloned().unwrap_or_default();
                let limited_relations = if max_breadth > 0 {
                    node_relations
                        .into_iter()
                        .take(max_breadth as usize)
                        .collect::<Vec<_>>()
                } else {
                    node_relations
                };

                for rel in limited_relations {
                    if !visited_relations.insert(rel.id.clone()) {
                        continue;
                    }
                    result_relations.push(rel.clone());

                    let neighbor_id = if rel.source_node_id == *node_id {
                        rel.target_node_id.clone()
                    } else {
                        rel.source_node_id.clone()
                    };

                    if visited_nodes.insert(neighbor_id.clone()) {
                        queue.push_back((neighbor_id, current_depth + 1));
                    }
                }
            }

            current_depth += 1;
        }

        Ok(())
    }

    async fn traverse_dfs(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        state: TraverseState<'_>,
    ) -> Result<()> {
        let TraverseState {
            visited_nodes,
            visited_relations,
            result_relations,
        } = state;
        let mut stack: Vec<(String, i32)> = Vec::new();
        for id in seed_node_ids.iter().rev() {
            stack.push((id.clone(), 0));
        }

        // 边预取缓存，避免逐节点查询的 N+1：
        // - fetched：已完整拉取过边的节点；batch 预取后所有 batch_ids 均有缓存条目（含空条目），
        //   故 fetched 命中即代表边已拉全，可跳过查询
        // - edge_cache：节点 → 其边（按双端点分组缓存）；注意仅作为边端点出现的节点也会获得
        //   部分边条目，这类节点不在 fetched 中，pop 时仍需预取其自身全部边
        let mut edge_cache: HashMap<String, Vec<KnowledgeNodeRelationPo>> = HashMap::new();
        let mut fetched: HashSet<String> = HashSet::new();

        while let Some((node_id, depth)) = stack.pop() {
            if depth >= max_depth {
                continue;
            }

            if !fetched.contains(&node_id) {
                // 未完整拉取过：当前节点 + 栈上未拉取的待展开节点一次批量预取
                let mut batch_ids: Vec<String> = vec![node_id.clone()];
                let mut seen: HashSet<&str> = HashSet::new();
                seen.insert(node_id.as_str());
                for (id, d) in &stack {
                    if *d < max_depth && !fetched.contains(id) && seen.insert(id.as_str()) {
                        batch_ids.push(id.clone());
                    }
                }
                let batch_relations = self
                    .memory_dao
                    .list_relations_batch(ctx.clone(), &batch_ids)
                    .await?;
                for id in &batch_ids {
                    fetched.insert(id.clone());
                }
                // 每条边挂到双端点名下的缓存；batch 内未被任何边引用的节点补空条目，
                // 保证 fetched 节点必有缓存条目，pop 时不会误判为「未拉取」重复查询
                for rel in batch_relations {
                    edge_cache
                        .entry(rel.source_node_id.clone())
                        .or_default()
                        .push(rel.clone());
                    edge_cache
                        .entry(rel.target_node_id.clone())
                        .or_default()
                        .push(rel);
                }
                for id in &batch_ids {
                    edge_cache.entry(id.clone()).or_default();
                }
            }

            // batch 内两节点之间的边会挂到双方名下，pop 时可能取到重复边：
            // 按 id 去重，避免重复边占用 take(max_breadth) 名额（旧实现单查无重复）
            let mut node_relations = edge_cache.remove(&node_id).unwrap_or_default();
            node_relations.sort_by_key(|rel| rel.created_at);
            node_relations.dedup_by(|a, b| a.id == b.id);

            let limited_relations: Vec<KnowledgeNodeRelationPo> = if max_breadth > 0 {
                node_relations
                    .into_iter()
                    .take(max_breadth as usize)
                    .collect()
            } else {
                node_relations
            };

            let mut neighbors: Vec<(String, KnowledgeNodeRelationPo)> = Vec::new();
            for rel in limited_relations {
                if !visited_relations.insert(rel.id.clone()) {
                    continue;
                }

                let neighbor_id = if rel.source_node_id == node_id {
                    rel.target_node_id.clone()
                } else {
                    rel.source_node_id.clone()
                };

                neighbors.push((neighbor_id, rel));
            }

            for (neighbor_id, rel) in neighbors.iter().rev() {
                result_relations.push(rel.clone());
                if visited_nodes.insert(neighbor_id.clone()) {
                    stack.push((neighbor_id.clone(), depth + 1));
                }
            }
        }

        Ok(())
    }

    /// 按 ID 批量取回知识节点（遍历用）。
    ///
    /// ⚠️ **不施加归属筛选** —— 这是「种子恒返回 + 蜂巢可见性」两条不变量的落点：
    /// - 种子是调用方点名的节点，无论归属哪个 Agent 都必须取回（否则点击展开缺中心节点）；
    /// - 邻居也是共享知识，不因归属被丢弃。
    ///
    /// 仍然受软删除约束：DAO 默认排除 `status = Forgotten`（遗忘的节点不该再出现在图上）。
    async fn fetch_nodes_by_ids(
        &self,
        ctx: RequestContext,
        node_ids: &HashSet<String>,
    ) -> Result<Vec<LongTermKnowledgeNodePo>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = node_ids.iter().cloned().collect();
        // 分块：SQLite 绑定参数上限 999，遍历的 visited 集合可能很大
        let mut nodes: Vec<LongTermKnowledgeNodePo> = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(crate::service::dao::memory::sqlite::IN_CLAUSE_CHUNK) {
            let query = MemoryQuery {
                ids: Some(chunk.to_vec()),
                ..Default::default()
            };
            nodes.extend(
                self.memory_dao
                    .query_knowledge_nodes(ctx.clone(), query)
                    .await?,
            );
        }
        Ok(nodes)
    }

    fn build_memories(
        &self,
        nodes: Vec<LongTermKnowledgeNodePo>,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Vec<Memory> {
        let mut memories = Vec::with_capacity(nodes.len() + relations.len());
        for node in nodes {
            memories.push(Memory {
                po: MemoryPo::KnowledgeNode(node),
                search_match: None,
            });
        }
        for rel in relations {
            memories.push(Memory {
                po: MemoryPo::Relation(rel),
                search_match: None,
            });
        }
        memories
    }

    /// 组装图谱批次（遍历路径），保证**边随其两端节点一起返回**。
    ///
    /// 走的是与 `search` 路径同一道闸 `drop_dangling_relations`：端点没通过可见性过滤
    /// （或没被遍历到）时整条边丢弃，绝不让半条边进入结果。
    fn build_graph_memories(
        &self,
        nodes: Vec<LongTermKnowledgeNodePo>,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Vec<Memory> {
        let mut memories = self.build_memories(nodes, relations);
        drop_dangling_relations(&mut memories);
        memories
    }

    /// 搜索短期记忆（内部实现）
    async fn search_short_term_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);
        // 向量召回条数（可配置，默认 50）
        let vector_top_k = search.top_k.unwrap_or(50);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用 try_build_vector_params 统一方式）
        if search.keyword.is_some()
            && let Some(keyword) = &search.keyword
        {
            match try_build_vector_params_for_search(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                keyword,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    // 向量搜索（前 top_k 条；业务过滤在 DAO 内转译为向量谓词下推）
                    match self
                        .memory_vector_dao
                        .search_short_term_vector(
                            ctx.clone(),
                            &vec_params.vector,
                            vector_top_k,
                            &search.filters,
                        )
                        .await
                    {
                        Ok(vector_results) => {
                            // 过滤距离小于阈值的结果
                            let filtered_results: Vec<(String, f32)> = vector_results
                                .into_iter()
                                .filter(|hit| hit.distance < vector_distance_threshold)
                                .map(|hit| (hit.row.id, hit.distance))
                                .collect();

                            vector_ids =
                                filtered_results.iter().map(|(id, _)| id.clone()).collect();
                            vector_scores = filtered_results.into_iter().collect();
                        }
                        Err(e) => {
                            // 向量搜索失败，降级到纯关键词搜索
                            log_warn!(
                                ctx,
                                "vector_search",
                                "短期记忆向量搜索失败，降级到关键词搜索: {}",
                                e
                            );
                        }
                    }
                }
                Ok(None) => {
                    log_debug!(
                        ctx,
                        "vector_search",
                        "无可用 Embedding Provider，跳过向量搜索"
                    );
                }
                Err(e) => {
                    log_warn!(ctx, "vector_search", error = ?e, "短期记忆向量化失败，跳过向量搜索");
                }
            }
        }

        // Step 3: 执行关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .memory_dao
            .search_short_term(ctx.clone(), search.clone())
            .await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<ShortTermMemoryIndexPo> = keyword_results
            .into_iter()
            .map(|(po, rank)| {
                if let Some(r) = rank {
                    fts_ranks.insert(po.id.clone(), r);
                }
                po
            })
            .collect();

        // Step 4: 聚合结果（如果有向量结果，用通用 query 批量获取，避免 N+1）
        let mut all_pos = keyword_pos.clone();

        if !vector_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 用通用 query 批量获取 ids_to_fetch 的结果
                let mut query_for_ids = search.filters.clone();
                query_for_ids.ids = Some(ids_to_fetch);
                let vector_pos = self
                    .memory_dao
                    .query_short_term(ctx.clone(), query_for_ids)
                    .await?;
                all_pos.extend(vector_pos);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象
        let mut memories = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let has_vector = vector_scores.contains_key(&po.id);
            let has_keyword = fts_ranks.contains_key(&po.id);
            let match_info = if has_vector && has_keyword {
                // 双命中：向量 + 关键词
                Some(SearchMatchInfo {
                    match_type: MatchType::Hybrid,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_vector {
                // 仅向量命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Vector,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_keyword {
                // 仅关键词命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else {
                None
            };
            memories.push(Memory {
                po: MemoryPo::ShortTerm(po),
                search_match: match_info,
            });
        }

        Ok(memories)
    }

    /// 搜索知识节点（内部实现）
    async fn search_knowledge_nodes_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        // 向量距离阈值（可配置，默认 0.8）
        let vector_distance_threshold = search.vector_distance_threshold.unwrap_or(0.8);
        // 向量召回条数（可配置，默认 50）
        let vector_top_k = search.top_k.unwrap_or(50);

        // Step 1: 准备向量搜索结果容器
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        let mut vector_ids: HashSet<String> = HashSet::new();

        // Step 2: 如果有关键词，执行向量搜索（用 try_build_vector_params 统一方式）
        if search.keyword.is_some()
            && let Some(keyword) = &search.keyword
        {
            match try_build_vector_params_for_search(
                ctx.clone(),
                &self.cortex_dao,
                &self.model_provider_dao,
                keyword,
            )
            .await
            {
                Ok(Some(vec_params)) => {
                    // 向量搜索（前 top_k 条；业务过滤在 DAO 内转译为向量谓词下推）
                    match self
                        .memory_vector_dao
                        .search_knowledge_node_vector(
                            ctx.clone(),
                            &vec_params.vector,
                            vector_top_k,
                            &search.filters,
                        )
                        .await
                    {
                        Ok(vector_results) => {
                            // 过滤距离小于阈值的结果
                            let filtered_results: Vec<(String, f32)> = vector_results
                                .into_iter()
                                .filter(|hit| hit.distance < vector_distance_threshold)
                                .map(|hit| (hit.row.id, hit.distance))
                                .collect();

                            vector_ids =
                                filtered_results.iter().map(|(id, _)| id.clone()).collect();
                            vector_scores = filtered_results.into_iter().collect();
                        }
                        Err(e) => {
                            // 向量搜索失败，降级到纯关键词搜索
                            log_warn!(
                                ctx,
                                "vector_search",
                                "知识节点向量搜索失败，降级到关键词搜索: {}",
                                e
                            );
                        }
                    }
                }
                Ok(None) => {
                    log_debug!(
                        ctx,
                        "vector_search",
                        "无可用 Embedding Provider，跳过向量搜索"
                    );
                }
                Err(e) => {
                    log_warn!(ctx, "vector_search", error = ?e, "知识节点向量化失败，跳过向量搜索");
                }
            }
        }

        // Step 3: 执行关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self
            .memory_dao
            .search_knowledge_nodes(ctx.clone(), search.clone())
            .await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let keyword_pos: Vec<LongTermKnowledgeNodePo> = keyword_results
            .into_iter()
            .map(|(po, rank)| {
                if let Some(r) = rank {
                    fts_ranks.insert(po.id.clone(), r);
                }
                po
            })
            .collect();

        // Step 4: 聚合结果（如果有向量结果，用通用 query 批量获取，避免 N+1）
        let mut all_pos = keyword_pos.clone();

        if !vector_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 用通用 query 批量获取 ids_to_fetch 的结果
                let mut query_for_ids = search.filters.clone();
                query_for_ids.ids = Some(ids_to_fetch);
                let vector_pos = self
                    .memory_dao
                    .query_knowledge_nodes(ctx.clone(), query_for_ids)
                    .await?;
                all_pos.extend(vector_pos);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建业务对象
        let mut nodes = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let has_vector = vector_scores.contains_key(&po.id);
            let has_keyword = fts_ranks.contains_key(&po.id);
            let match_info = if has_vector && has_keyword {
                // 双命中：向量 + 关键词
                Some(SearchMatchInfo {
                    match_type: MatchType::Hybrid,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_vector {
                // 仅向量命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Vector,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_keyword {
                // 仅关键词命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else {
                None
            };
            nodes.push(Memory {
                po: MemoryPo::KnowledgeNode(po),
                search_match: match_info,
            });
        }

        Ok(nodes)
    }

    /// 搜索关系（通过知识节点 FTS5 间接搜索关系）
    ///
    /// 关系表无独立 FTS 索引，因此先搜索匹配的知识节点，
    /// 再查询这些节点关联的所有关系（出入边），一并返回。
    ///
    /// ⚠️ 这里必然会带出「远端节点没命中」的边，**不在这里过滤**：
    /// 由调用方 `search` 统一过 `drop_dangling_relations`（图批次不变式）。
    async fn search_relations_internal(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        let keyword = search.keyword.as_deref().unwrap_or("");
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 1. 用 FTS5 搜索匹配的知识节点
        let nodes = self
            .memory_dao
            .search_knowledge_nodes(ctx.clone(), search.clone())
            .await?;
        let node_ids: Vec<String> = nodes.iter().map(|(n, _)| n.id.clone()).collect();

        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 查询这些节点的所有关系（出入边）
        let relations = self
            .memory_dao
            .list_relations_batch(ctx.clone(), &node_ids)
            .await?;

        // 3. 构建 Memory 列表（KnowledgeNode + Relation）
        let mut memories = Vec::with_capacity(nodes.len() + relations.len());
        for (node, fts_rank) in nodes {
            memories.push(Memory {
                po: MemoryPo::KnowledgeNode(node),
                search_match: Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank,
                    ..Default::default()
                }),
            });
        }
        for rel in relations {
            memories.push(Memory {
                po: MemoryPo::Relation(rel),
                search_match: None,
            });
        }

        Ok(memories)
    }

    // ==================== Create Internal Helpers ====================

    /// AppendTraces：批量追加 trace 到每日 JSONL 文件
    ///
    /// 仅写文件，不向量化（trace 永远不入向量库）。
    /// position 回填到 MemoryTrace.position 字段后包装为 Memory 返回。
    async fn create_append_traces(
        &self,
        ctx: RequestContext,
        mut traces: Vec<MemoryTrace>,
    ) -> Result<Vec<Memory>> {
        if traces.is_empty() {
            return Ok(Vec::new());
        }
        let positions = self.memory_dao.batch_append_traces(ctx, &traces).await?;
        // 回填 position 到 trace
        for (trace, pos) in traces.iter_mut().zip(positions) {
            trace.position = Some(pos);
        }
        Ok(traces
            .into_iter()
            .map(|t| Memory {
                po: MemoryPo::Trace(t),
                search_match: None,
            })
            .collect())
    }

    /// CreateShortTerm：写入短期记忆索引 + 向量化 summary（失败 warn 降级）
    async fn create_short_term(
        &self,
        ctx: RequestContext,
        index: ShortTermMemoryIndexPo,
    ) -> Result<Vec<Memory>> {
        // Step 1: 写 SQLite
        self.memory_dao
            .create_short_term_index(ctx.clone(), index.clone())
            .await?;

        // Step 2: 向量化 summary + tags（三态：Skip / PayloadOnly / FullReindex；失败 warn 降级，不影响主流程）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &index,
            ShortTermMemoryIndexPo::vector_collection(),
            &index.id,
        )
        .await
        {
            Ok(VectorIndexAction::Reindex(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_short_term_vector(ctx.clone(), &index.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", memory_id= %index.id, error = ?e, "短期记忆向量索引写入失败，已降级");
                }
            }
            Ok(VectorIndexAction::Skipped) => {
                log_debug!(ctx, "vector_index", memory_id= %index.id, "向量索引未变化，跳过");
            }
            Ok(VectorIndexAction::PayloadRefreshed) => {}
            Err(e) => {
                log_warn!(ctx, "vector_index", memory_id= %index.id, error = ?e, "短期记忆向量化失败，已降级");
            }
        }

        Ok(vec![Memory {
            po: MemoryPo::ShortTerm(index),
            search_match: None,
        }])
    }

    /// CreateKnowledgeNode：写知识节点 + 引用 + 向量化（node_description + summary 拼接）
    async fn create_knowledge_node(
        &self,
        ctx: RequestContext,
        node: LongTermKnowledgeNodePo,
        references: Vec<KnowledgeReferencePo>,
    ) -> Result<Vec<Memory>> {
        // Step 1: 写节点
        self.memory_dao
            .save_knowledge_node(ctx.clone(), &node)
            .await?;

        // Step 2: 写引用（如有）
        if !references.is_empty() {
            self.memory_dao
                .batch_add_knowledge_references(ctx.clone(), &references)
                .await?;
        }

        // Step 3: 向量化（node_description + summary + tags 拼接；三态：Skip / PayloadOnly / FullReindex）
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &node,
            LongTermKnowledgeNodePo::vector_collection(),
            &node.id,
        )
        .await
        {
            Ok(VectorIndexAction::Reindex(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_knowledge_node_vector(ctx.clone(), &node.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", node_id= %node.id, error = ?e, "知识节点向量索引写入失败，已降级");
                }
            }
            Ok(VectorIndexAction::Skipped) => {
                log_debug!(ctx, "vector_index", node_id= %node.id, "向量索引未变化，跳过");
            }
            Ok(VectorIndexAction::PayloadRefreshed) => {}
            Err(e) => {
                log_warn!(ctx, "vector_index", node_id= %node.id, error = ?e, "知识节点向量化失败，已降级");
            }
        }

        Ok(vec![Memory {
            po: MemoryPo::KnowledgeNode(node),
            search_match: None,
        }])
    }

    /// CreateRelations：批量添加知识节点关系（无向量化）
    async fn create_relations(
        &self,
        ctx: RequestContext,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Result<Vec<Memory>> {
        if relations.is_empty() {
            return Ok(Vec::new());
        }
        self.memory_dao
            .batch_add_knowledge_relations(ctx, &relations)
            .await?;
        Ok(relations
            .into_iter()
            .map(|r| Memory {
                po: MemoryPo::Relation(r),
                search_match: None,
            })
            .collect())
    }
}

// ==================== Helpers ====================

/// 尝试为查询文本构建向量索引参数（用于搜索场景）
///
/// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
/// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
async fn try_build_vector_params_for_search(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao>,
    model_provider_dao: &Arc<dyn ModelProviderDao>,
    text: &str,
) -> Result<Option<VectorIndexParams>> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(None);
    };

    let params = cortex_dao
        .embed_text_for_search(ctx.clone(), &provider, text)
        .await?;
    Ok(Some(params))
}

/// 统一向量索引三态决策结果
enum VectorIndexAction {
    /// 无可用 provider 或内容未变化：无需任何操作
    Skipped,
    /// 仅 payload 变化：已在内部完成刷新（未调 embedding）
    PayloadRefreshed,
    /// 需要完整重索引：调用方执行 upsert（Box 避免 enum 体积被最大变体撑大）
    Reindex(Box<VectorIndexParams>),
}

/// 尝试为可向量化实体构建向量索引参数（三态：Skip / PayloadOnly / FullReindex）
///
/// 三态判断取旧行比对双 hash（文本 hash + payload hash）；
/// PayloadOnly 直接刷新 payload 列，不重新调 embedding。
async fn try_build_vector_params_for_entity(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao>,
    model_provider_dao: &Arc<dyn ModelProviderDao>,
    entity: &dyn Vectorizable,
    collection: &str,
    id: &str,
) -> Result<VectorIndexAction> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(VectorIndexAction::Skipped);
    };

    // 三态：取旧行比对双 hash
    if let Some(row) = ctx.vector_store().get(collection, id).await? {
        match entity.reindex_decision(&row.meta.content_hash, &row.meta.payload_hash) {
            ReindexDecision::Skip => return Ok(VectorIndexAction::Skipped),
            ReindexDecision::PayloadOnly => {
                ctx.vector_store()
                    .update_payload(collection, id, &entity.vector_payload())
                    .await?;
                return Ok(VectorIndexAction::PayloadRefreshed);
            }
            ReindexDecision::FullReindex => {}
        }
    }

    let params = cortex_dao
        .embed_entity(ctx.clone(), &provider, entity)
        .await?;
    Ok(VectorIndexAction::Reindex(Box::new(params)))
}
