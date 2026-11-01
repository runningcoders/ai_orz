//! 检索节点（机械）：按意图关键词做知识混合检索 + 思维方法技能召回
//!
//! 不调模型——知识检索走 FTS/向量混合搜索，方法技能走 DAL 标签查询，两者都是
//! 机械动作。输入 = `bus[Intent]` + `deps.retrieval`（visited 差集），
//! 输出 = `bus[Knowledge]` 的 [`RetrievalPack`]。空词 / 零命中都是合法结果
//! （照常 Done）：检索是增强，不是闸门。

use super::context::{AwakenDeps, RetrievalPack, read_bus};
use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::domain::runtime::RuntimeMemory;
use crate::service::domain::runtime::types::IntentAnalysis;
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_RETRIEVE: &str = "retrieve";

/// 渲染预算：进 prompt 的知识节点条数（机械 Top-K）
///
/// 这是**消费端**的宽度，与召回宽度解耦：多召回、少渲染，中间那道筛
/// 由精选（开关打开时）或排序（关闭时）完成。
pub(crate) const KNOWLEDGE_TOP_K: usize = 5;

/// 召回宽度相对渲染预算的倍数
///
/// 召回是「便宜但漏一次就没了」的一侧：一次 FTS + 一次向量查询的成本远低于
/// 漏召回的代价，所以候选池按渲染预算的整数倍放宽，再由精选/排序收回 5 条。
///
/// 3 倍（= 15 条）是当前取值：足够让精选有取舍空间，又不至于让一次 Pick 的
/// 选项列表长到模型只看前几个。调大这一个是单点操作，不需要动结构。
const RECALL_WIDTH_FACTOR: usize = 3;

/// 关系边命中条数上限
///
/// 与知识节点**分开**截断：边是结构信息（「谁和谁什么关系」），
/// 和节点抢同一个名额会让图谱在提示里被挤掉。
const RELATION_TOP_K: usize = 4;

/// 方法技能条数上限
const METHOD_SKILLS_TOP_K: usize = 2;

/// 图谱扩展的种子节点数（从关键词命中里取前 N 个知识节点）
///
/// 与 [`KNOWLEDGE_TOP_K`] 对齐而不是取更小的常数：进 prompt 的每个知识节点
/// 都值得当一次种子——种子只是 SQL 的 `IN` 条件，加一个的边际成本可以忽略，
/// 而漏掉一个就等于那部分图谱结构永远进不了上下文。
const GRAPH_SEED_TOP_K: usize = KNOWLEDGE_TOP_K;

/// 图谱扩展深度：固定一跳
///
/// 再深一层会把整片图谱拖进来，词面命中这个「相关性的锚」就被稀释了。
const GRAPH_MAX_DEPTH: i32 = 1;

/// 图谱扩展宽度：每个种子最多展开多少邻居
const GRAPH_MAX_BREADTH: i32 = 6;

/// thinking_method 技能的 tag 约定值（与 seed 技能、沉淀登记共用同一真相）
pub(crate) const METHOD_SKILL_TAG: &str = "thinking_method";

/// visited 差集：候选词 trim、去重后剔除已检索过的词（保持原顺序）
///
/// 抽成纯函数是为了单测，以及 discover 节点对新词做同样的去重。
pub(crate) fn diff_terms(candidates: &[String], queried: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::with_capacity(candidates.len());
    for term in candidates {
        let term = term.trim();
        if term.is_empty()
            || queried.iter().any(|q| q.as_str() == term)
            || seen.iter().any(|s| s.as_str() == term)
        {
            continue;
        }
        seen.push(term.to_string());
    }
    seen
}

/// 命中集合 → prompt 行（知识节点 Top-K + 关系边 Top-K）
///
/// `Memory` 没有 `content` 字段（计划里的字段名以实际模型为准）：可读文本
/// 存在于 PO 变体里——短期记忆取 summary、知识节点取 node_name + 描述、
/// 关系边取「源 --关系--> 目标」。
///
/// ⚠️ 调用方已经把召回宽度收敛到 `filters.limit`（向量路与 FTS 路同口径），
/// 这里仍要截断：`limit` 是**候选池**大小，prompt 预算是另一回事。
///
/// `neighbors` 只用于**边两端的取名**，不渲染正文：扩展到一跳邻居的意义是
/// 「看清谁和谁有关系」，把邻居描述也塞进来就又回到噪声问题了——需要正文时
/// Agent 可以自己再检索。
///
/// 边有两个来源：检索路（命中节点的全部出入边）与图谱扩展路（种子一跳邻域）。
/// 两者大范围重叠，按 id 去重后**按结构信号排序**再截断——不调模型。
fn build_knowledge_lines(
    memories: &[Memory],
    neighbors: &[Memory],
    relations: &[Memory],
) -> Vec<String> {
    let mut node_names: std::collections::HashMap<&str, &str> = memories
        .iter()
        .filter_map(|m| match &m.po {
            MemoryPo::KnowledgeNode(k) => Some((k.id.as_str(), k.node_name.as_str())),
            _ => None,
        })
        .collect();
    for m in neighbors {
        if let MemoryPo::KnowledgeNode(k) = &m.po {
            node_names
                .entry(k.id.as_str())
                .or_insert(k.node_name.as_str());
        }
    }

    let mut node_lines: Vec<String> = Vec::new();
    let mut relation_lines: Vec<String> = Vec::new();

    for m in memories {
        match &m.po {
            MemoryPo::KnowledgeNode(k) => {
                let body = if k.node_description.is_empty() {
                    &k.summary
                } else {
                    &k.node_description
                };
                if body.is_empty() {
                    continue;
                }
                node_lines.push(format!("- {}：{}", k.node_name, truncate_chars(body, 160)));
            }
            MemoryPo::ShortTerm(st) if !st.summary.is_empty() => {
                node_lines.push(format!("- {}", truncate_chars(&st.summary, 160)));
            }
            _ => {}
        }
    }
    // 边池 = 检索带出的边 + 图谱扩展带出的边（同一 id 只留一次）
    let hit_ids: std::collections::HashSet<&str> = memories
        .iter()
        .filter(|m| !matches!(m.po, MemoryPo::Relation(_)))
        .filter_map(memory_id)
        .collect();
    let mut relation_pool: Vec<&Memory> = Vec::new();
    let mut seen_relation: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for m in memories.iter().chain(relations.iter()) {
        if let MemoryPo::Relation(r) = &m.po
            && seen_relation.insert(r.id.as_str())
        {
            relation_pool.push(m);
        }
    }

    // 结构排序（零成本，不调模型）：两端都命中的边信息量最高——
    // 「A 依赖 B」只有 A、B 都在本轮上下文里才闭环；远端没命中时
    // 那条边只能靠邻居取名，读者拿到的是半条信息。其次是边自身的 weight。
    relation_pool.sort_by(|a, b| {
        let key = |m: &Memory| match &m.po {
            MemoryPo::Relation(r) => (
                hit_ids.contains(r.source_node_id.as_str())
                    && hit_ids.contains(r.target_node_id.as_str()),
                r.weight.unwrap_or(0.0),
            ),
            _ => (false, 0.0),
        };
        let (both_a, weight_a) = key(a);
        let (both_b, weight_b) = key(b);
        both_b.cmp(&both_a).then_with(|| {
            weight_b
                .partial_cmp(&weight_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    for m in relation_pool.into_iter().take(RELATION_TOP_K) {
        if let MemoryPo::Relation(r) = &m.po {
            let (Some(src), Some(dst)) = (
                node_names.get(r.source_node_id.as_str()),
                node_names.get(r.target_node_id.as_str()),
            ) else {
                continue;
            };
            relation_lines.push(format!("- {src} --{}--> {dst}", r.relation_type));
        }
    }

    node_lines.truncate(KNOWLEDGE_TOP_K);
    node_lines.extend(relation_lines);
    node_lines
}

/// 召回宽度（候选池大小），与渲染预算 [`KNOWLEDGE_TOP_K`] 成固定倍数
///
/// 与精选开关**无关**：精选只决定「从候选池里留下哪几条」，不该反过来决定
/// 「先从库里捞多少条」。旧实现在关闭精选时把候选池收到 5，于是同一个库里
/// 同一句话，开不开精选会看到两种排序结果；统一口径后关掉精选只是少了那道
/// 语义筛选，召回面的宽度不变。
pub(crate) fn recall_width() -> usize {
    KNOWLEDGE_TOP_K * RECALL_WIDTH_FACTOR
}

/// 小脑精选：关闭 / 无大脑 / 降级 → 原样返回（老路径，取前 Top-K 条）
///
/// 精选是增强不是闸门：任何失败都退回「按召回序截断」，绝不把召回清空。
///
/// ⚠️ 关系边不参与精选：边的判据文本只有关系类型（`depends_on`），模型无从
/// 判断相关性，却要占掉一个选项位；而且精选只该决定「哪些实体进 prompt」，
/// 边由渲染侧按结构信号排序。所以先按 PO 分离，只排实体、边原样跟随。
async fn rerank_if_enabled(
    deps: &AwakenDeps,
    question: &str,
    memories: Vec<Memory>,
) -> Vec<Memory> {
    let Some(brain) = deps.agent.brain.as_ref() else {
        return memories;
    };
    if !brain.runtime_config.enable_memory_rerank {
        return memories;
    }

    let (edges, entities): (Vec<Memory>, Vec<Memory>) = memories
        .into_iter()
        .partition(|m| matches!(m.po, MemoryPo::Relation(_)));

    let ranked: Vec<Memory> =
        match super::memory_relevance::rank_with_default_dao(&deps.ctx, brain, question, &entities)
            .await
        {
            // 宽召回不能反过来把 prompt 撑爆：降级时按机械 Top-K 截断
            None => entities.into_iter().take(KNOWLEDGE_TOP_K).collect(),
            Some(kept) => kept,
        };
    ranked.into_iter().chain(edges).collect()
}

/// 一跳图谱扩展的产物
#[derive(Default)]
struct GraphExpansion {
    /// 邻接节点：**只用于边取名**，不进 prompt 正文
    neighbors: Vec<Memory>,
    /// 关系边：命中节点与邻居之间的结构信息
    relations: Vec<Memory>,
}

/// 以关键词命中的知识节点为种子，拉一跳邻域
///
/// 关键词路只按词面召回，命中节点的关联边默认进不了结果批——而「边本身就是
/// 信息」（A 依赖 B、C 取代了 D），全靠检索正好撞上纯属运气。这里把种子交给
/// `traverse_graph` 做一次批量 SQL 扩展（成本极低），只取边与邻居名字。
///
/// 失败降级为空扩展：检索是增强，不是闸门。
async fn expand_graph(deps: &AwakenDeps, memories: &[Memory]) -> GraphExpansion {
    let seeds: Vec<String> = memories
        .iter()
        .filter_map(|m| match &m.po {
            MemoryPo::KnowledgeNode(k) => Some(k.id.clone()),
            _ => None,
        })
        .take(GRAPH_SEED_TOP_K)
        .collect();
    if seeds.is_empty() {
        return GraphExpansion::default();
    }

    let expanded = match deps
        .domain
        .traverse_graph(
            deps.ctx.clone(),
            &seeds,
            GRAPH_MAX_DEPTH,
            GRAPH_MAX_BREADTH,
            TraversalStrategy::BreadthFirst,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            crate::log_warn!(
                &deps.ctx,
                "legacy_awaken",
                "graph expansion degraded to keyword hits: seeds={}, error={}",
                seeds.len(),
                e
            );
            return GraphExpansion::default();
        }
    };

    let hit_ids: std::collections::HashSet<&str> = memories.iter().filter_map(memory_id).collect();
    let mut relations: Vec<Memory> = Vec::new();
    let mut neighbors: Vec<Memory> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in expanded {
        let Some(id) = memory_id(&m) else {
            continue;
        };
        // 种子节点本身已在命中原批里，不再重复收集
        if hit_ids.contains(id) || !seen.insert(id.to_string()) {
            continue;
        }
        match &m.po {
            MemoryPo::Relation(_) => relations.push(m),
            MemoryPo::KnowledgeNode(_) => neighbors.push(m),
            _ => {}
        }
    }

    crate::log_info!(
        &deps.ctx,
        "legacy_awaken",
        "graph expansion done, seeds={}, relations={}, neighbors={}",
        seeds.len(),
        relations.len(),
        neighbors.len()
    );
    GraphExpansion {
        neighbors,
        relations,
    }
}

/// 取记忆实体 id（PO 变体各自的 id 字段同名，但没有统一 trait）
pub(super) fn memory_id(m: &Memory) -> Option<&str> {
    match &m.po {
        MemoryPo::Trace(t) => Some(t.id.as_str()),
        MemoryPo::ShortTerm(st) => Some(st.id.as_str()),
        MemoryPo::KnowledgeNode(k) => Some(k.id.as_str()),
        MemoryPo::Relation(r) => Some(r.id.as_str()),
    }
}

/// 截取前 `max` 个字符（按字符而不是字节，避免把中文切坏）
pub(super) fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// 写总线：pack 统一 Full 深度（prepare / deliberate 都按全量消费）
fn write_pack(bus: &mut ThinkBus, pack: &RetrievalPack) -> Result<()> {
    let value = serde_json::to_value(pack)
        .map_err(|e| common::error::err!(Internal, "encode retrieval pack failed: {e}"))?;
    bus.insert(ContextKey::Knowledge, LoadDepth::Full, value);
    Ok(())
}

#[derive(Debug)]
pub struct RetrieveNode {
    deps: Arc<AwakenDeps>,
}

impl RetrieveNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for RetrieveNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_RETRIEVE)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Triage
    }

    fn purpose(&self) -> &str {
        "知识与方法召回：按意图新词做混合检索（机械，不调模型）"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            // 只声明 Intent：queried_terms 走 deps.retrieval 状态槽位，
            // 不在总线契约里（它是链路级累计状态，不是节点间数据流）
            reads: vec![ContextKey::Intent],
            writes: vec![ContextKey::Knowledge],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;

        // 1. 读意图产物：分区缺失 / 解码失败按空结构继续（intent 节点降级路径的下游）
        let ia: IntentAnalysis =
            read_bus::<IntentAnalysis>(bus, ContextKey::Intent).unwrap_or_default();

        // 2. 快照后立刻放锁：MutexGuard 不跨 await（防死锁，照 BusinessProvider 的先例）
        let queried_snapshot = deps
            .retrieval
            .lock()
            .map(|s| s.queried_terms.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().queried_terms.clone());

        // 3. 候选词 = intent 关键词 + discover 回环写入的新词（design §3.2：retrieve
        //    的输入是「IntentAnalysis.key_terms **或 discover 新词**」）。
        //    discover 先写「新词 + 思考摘要」的过渡 pack，由本次 retrieve 覆写成
        //    完整产物（见 RetrievalPack 契约）；旧 pack 里已被检索过的词会被
        //    差集自然剔除。visited 差集为空 → 无新息不查库：写空 pack 短路
        //    （保留旧 pack 的思考摘要）
        let mut candidates = read_bus::<RetrievalPack>(bus, ContextKey::Knowledge)
            .ok()
            .map(|p| p.query_terms)
            .unwrap_or_default();
        candidates.extend(ia.key_terms.iter().cloned());
        let new_terms = diff_terms(&candidates, &queried_snapshot);
        if new_terms.is_empty() {
            let prior = read_bus::<RetrievalPack>(bus, ContextKey::Knowledge)
                .ok()
                .and_then(|p| p.prior_thinking_summary);
            let pack = RetrievalPack {
                query_terms: Vec::new(),
                knowledge_hits: Vec::new(),
                method_skills: Vec::new(),
                prior_thinking_summary: prior,
            };
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "retrieve node short-circuit (no new terms), agent_id={}, key_terms={}",
                deps.agent.po.id,
                ia.key_terms.len()
            );
            write_pack(bus, &pack)?;
            return Ok(NodeOutcome::new(NodeSignal::Done));
        }

        // 4. 知识混合检索：失败降级为空命中（检索是增强，不是闸门）
        //
        // 召回宽度必须**同时**给两个字段：`top_k` 只管向量路，关键词路（FTS）
        // 的条数由 `filters.limit` 决定（DAO 缺省 50）。只设 `top_k` 会让
        // 「宽召回 N 条」的语义脱钩——候选池变成 N + 50，小脑精选的输入分布不可控。
        let keyword = new_terms.join(" ");
        let width = recall_width();
        let knowledge_hits = match deps
            .domain
            .search(
                deps.ctx.clone(),
                MemorySearch {
                    keyword: Some(keyword.clone()),
                    top_k: Some(width as i32),
                    filters: MemoryQuery {
                        limit: Some(width),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
        {
            Ok(memories) => {
                // 宽召回 → 小脑精选（关闭 / 降级 → 原样，仍是老路径）
                let memories = rerank_if_enabled(deps, &keyword, memories).await;
                // 以命中节点为种子拉一跳邻域：关键词只按词面召回，
                // 关联边全靠撞运气——这里补一次便宜的图谱扩展。
                let graph = expand_graph(deps, &memories).await;
                build_knowledge_lines(&memories, &graph.neighbors, &graph.relations)
            }
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "knowledge search degraded to empty: agent_id={}, error={}",
                    deps.agent.po.id,
                    e
                );
                Vec::new()
            }
        };

        // 5. 方法技能召回：机械排序取前 2（description 与新词词面重叠者优先）
        let method_skills = match crate::service::dal::skill::dal()
            .list_published_by_tag(deps.ctx.clone(), METHOD_SKILL_TAG)
            .await
        {
            Ok(mut skills) => {
                skills.sort_by_key(|s| {
                    let desc = s.po.description.to_lowercase();
                    // 词面重叠者优先（重叠 = false 排前；稳定排序保证其余保序）
                    !new_terms.iter().any(|t| desc.contains(&t.to_lowercase()))
                });
                skills
                    .into_iter()
                    .take(METHOD_SKILLS_TOP_K)
                    .map(|s| s.po)
                    .collect::<Vec<_>>()
            }
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "method skill recall degraded to empty: agent_id={}, error={}",
                    deps.agent.po.id,
                    e
                );
                Vec::new()
            }
        };

        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "retrieve node done, agent_id={}, new_terms={}, knowledge_hits={}, method_skills={}",
            deps.agent.po.id,
            new_terms.len(),
            knowledge_hits.len(),
            method_skills.len()
        );

        // 6. 写总线 + 累积 visited（discover 回环下轮差集的输入）
        // 旧 pack 的思考摘要透传：discover 写入的摘要要保留到 prepare 消费
        let pack = RetrievalPack {
            query_terms: new_terms.clone(),
            knowledge_hits,
            method_skills,
            prior_thinking_summary: read_bus::<RetrievalPack>(bus, ContextKey::Knowledge)
                .ok()
                .and_then(|p| p.prior_thinking_summary),
        };
        write_pack(bus, &pack)?;

        // 锁中毒自愈：状态槽位只是记账，拿旧值继续比丢更新更接近正确
        let mut state = match deps.retrieval.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.queried_terms.extend(new_terms);

        Ok(NodeOutcome::new(NodeSignal::Done))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::memory::{KnowledgeNodeRelationPo, LongTermKnowledgeNodePo};
    use common::enums::{KnowledgeRelationStatus, MemoryStatus};

    fn node(id: &str, name: &str, desc: &str) -> Memory {
        Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
            id: id.to_string(),
            agent_id: "agent-test".to_string(),
            node_name: name.to_string(),
            node_description: desc.to_string(),
            node_type: "concept".to_string(),
            summary: String::new(),
            tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 0,
            updated_at: 0,
        }))
    }

    fn relation(src: &str, dst: &str, kind: &str) -> Memory {
        relation_with_weight(src, dst, kind, None)
    }

    /// id 由「两端 + 关系类型」唯一决定：测试里要能构造同端点不同类型的两条边
    fn relation_with_weight(src: &str, dst: &str, kind: &str, weight: Option<f32>) -> Memory {
        Memory::new(MemoryPo::Relation(KnowledgeNodeRelationPo {
            id: format!("rel-{src}-{dst}-{kind}"),
            source_node_id: src.to_string(),
            target_node_id: dst.to_string(),
            relation_type: kind.to_string(),
            weight,
            status: KnowledgeRelationStatus::Active,
            created_at: 0,
            updated_at: 0,
        }))
    }

    /// 召回面必须显著宽于渲染面，否则精选没有取舍空间
    ///
    /// 并固定「召回宽度与精选开关无关」这条口径：`recall_width` 不接 deps，
    /// 开不开精选看到的是同一个候选池。
    #[test]
    fn recall_width_is_a_multiple_of_render_budget() {
        let width = recall_width();
        assert!(
            width >= KNOWLEDGE_TOP_K * 2,
            "召回宽度 {width} 至少要 2 倍渲染预算 {}",
            KNOWLEDGE_TOP_K
        );
        // 候选池只有 1 条时精选会直接跳过（RankPolicy 不发单选项的题）
        assert!(width > 1);
    }

    /// 图谱扩展的核心价值：邻居节点只用来给边取名，正文不进 prompt
    #[test]
    fn relation_line_resolves_neighbor_name_without_dumping_its_body() {
        let hits = vec![node("k1", "订单状态机", "订单从创建到完成的流转")];
        let neighbors = vec![node("k2", "库存扣减", "这段邻居正文不应出现在 prompt")];
        let relations = vec![relation("k1", "k2", "depends_on")];

        let lines = build_knowledge_lines(&hits, &neighbors, &relations);

        assert_eq!(lines[0], "- 订单状态机：订单从创建到完成的流转");
        assert_eq!(lines[1], "- 订单状态机 --depends_on--> 库存扣减");
        assert!(
            !lines.iter().any(|l| l.contains("邻居正文")),
            "邻居节点正文不应渲染: {lines:?}"
        );
    }

    /// 对端不在批内的悬空边跳过（否则 prompt 里会出现裸 id）
    #[test]
    fn dangling_relation_is_skipped() {
        let hits = vec![node("k1", "A", "a")];
        let relations = vec![relation("k1", "k9", "rel")];

        let lines = build_knowledge_lines(&hits, &[], &relations);

        assert_eq!(lines, vec!["- A：a".to_string()]);
    }

    /// 渲染预算：候选再多也只在 prompt 里占 5 个节点位
    #[test]
    fn node_and_relation_budgets_are_enforced() {
        let nodes: Vec<Memory> = (0..9)
            .map(|i| node(&format!("k{i}"), &format!("节点{i}"), "描述"))
            .collect();
        let relations: Vec<Memory> = (0..7)
            .map(|i| relation(&format!("k{}", i % 9), &format!("k{}", (i + 1) % 9), "rel"))
            .collect();

        let lines = build_knowledge_lines(&nodes, &[], &relations);

        assert_eq!(
            lines.len(),
            KNOWLEDGE_TOP_K + RELATION_TOP_K,
            "应为 5 个节点 + 4 条边"
        );
        assert!(lines[KNOWLEDGE_TOP_K].contains("--rel-->"));
    }

    /// 边池 = 检索路 + 图谱路，同一条边只渲染一次
    ///
    /// 两条路大范围重叠（都是命中节点的出入边），不去重会让 prompt 里
    /// 同一条关系重复出现，白占边预算。
    #[test]
    fn edges_from_both_paths_are_merged_without_duplicates() {
        let edge = relation("k1", "k2", "depends_on");
        // 检索结果里带了这条边，图谱扩展又拉回来同一条
        let hits = vec![node("k1", "A", "a"), node("k2", "B", "b"), edge.clone()];
        let graph_edges = vec![edge];

        let lines = build_knowledge_lines(&hits, &[], &graph_edges);

        let edge_lines: Vec<&String> = lines.iter().filter(|l| l.contains("-->")).collect();
        assert_eq!(edge_lines.len(), 1, "同一条边只渲染一次: {lines:?}");
    }

    /// 两端都命中的边优先于「weight 更高但远端没命中」的边
    ///
    /// 「A 依赖 B」只有 A、B 都在本轮上下文里才闭环；远端缺席时读者拿到的是
    /// 半条信息，weight 再高也不该挤掉完整的那条。
    #[test]
    fn edge_with_both_ends_hit_outranks_heavier_dangling_edge() {
        let hits = vec![node("k1", "A", "a"), node("k2", "B", "b")];
        // k9 只在邻居里（用于取名），不算命中
        let neighbors = vec![node("k9", "远端", "d")];
        let relations = vec![
            relation_with_weight("k1", "k9", "weak_link", Some(0.95)),
            relation_with_weight("k1", "k2", "strong_link", Some(0.10)),
        ];

        let lines = build_knowledge_lines(&hits, &neighbors, &relations);

        assert!(lines[2].contains("strong_link"), "完整边应排前: {lines:?}");
        assert!(lines[3].contains("weak_link"));
    }

    /// 同样两端命中的两条边，按 weight 降序
    #[test]
    fn edges_of_equal_completeness_are_ordered_by_weight() {
        let hits = vec![node("k1", "A", "a"), node("k2", "B", "b")];
        let relations = vec![
            relation_with_weight("k1", "k2", "light", Some(0.1)),
            relation_with_weight("k1", "k2", "heavy", Some(0.9)),
        ];

        let lines = build_knowledge_lines(&hits, &[], &relations);

        assert!(lines[2].contains("heavy"), "weight 高的排前: {lines:?}");
        assert!(lines[3].contains("light"));
    }
}
