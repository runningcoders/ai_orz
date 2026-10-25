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
use crate::service::dao::memory::MemorySearch;
use crate::service::domain::runtime::RuntimeMemory;
use crate::service::domain::runtime::types::IntentAnalysis;
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_RETRIEVE: &str = "retrieve";

/// 知识命中条数上限（机械 Top-K）
const KNOWLEDGE_TOP_K: i32 = 5;

/// 关系边命中条数上限
///
/// 与知识节点**分开**截断：边是结构信息（「谁和谁什么关系」），
/// 和节点抢同一个名额会让图谱在提示里被挤掉。
const RELATION_TOP_K: usize = 4;

/// 方法技能条数上限
const METHOD_SKILLS_TOP_K: usize = 2;

/// 图谱扩展的种子节点数（从关键词命中里取前 N 个知识节点）
const GRAPH_SEED_TOP_K: usize = 3;

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
/// ⚠️ 关键词路不受 `top_k` 约束（`search` 只在**向量召回**用 top_k，FTS 命中
/// 全量返回），所以这里必须自己截断，否则几十行噪声会整段灌进 prompt。
///
/// `neighbors` 只用于**边两端的取名**，不渲染正文：扩展到一跳邻居的意义是
/// 「看清谁和谁有关系」，把邻居描述也塞进来就又回到噪声问题了——需要正文时
/// Agent 可以自己再检索。
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
    for m in relations {
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

    node_lines.truncate(KNOWLEDGE_TOP_K as usize);
    relation_lines.truncate(RELATION_TOP_K);
    node_lines.extend(relation_lines);
    node_lines
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
fn memory_id(m: &Memory) -> Option<&str> {
    match &m.po {
        MemoryPo::Trace(t) => Some(t.id.as_str()),
        MemoryPo::ShortTerm(st) => Some(st.id.as_str()),
        MemoryPo::KnowledgeNode(k) => Some(k.id.as_str()),
        MemoryPo::Relation(r) => Some(r.id.as_str()),
    }
}

/// 截取前 `max` 个字符（按字符而不是字节，避免把中文切坏）
fn truncate_chars(text: &str, max: usize) -> String {
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
        let keyword = new_terms.join(" ");
        let knowledge_hits = match deps
            .domain
            .search(
                deps.ctx.clone(),
                MemorySearch {
                    keyword: Some(keyword),
                    top_k: Some(KNOWLEDGE_TOP_K),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(memories) => {
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
        Memory::new(MemoryPo::Relation(KnowledgeNodeRelationPo {
            id: format!("rel-{src}-{dst}"),
            source_node_id: src.to_string(),
            target_node_id: dst.to_string(),
            relation_type: kind.to_string(),
            weight: None,
            status: KnowledgeRelationStatus::Active,
            created_at: 0,
            updated_at: 0,
        }))
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

    /// 关键词路不受 search.top_k 约束，必须在渲染侧硬截断
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
            KNOWLEDGE_TOP_K as usize + RELATION_TOP_K,
            "应为 5 个节点 + 4 条边"
        );
        assert!(lines[KNOWLEDGE_TOP_K as usize].contains("--rel-->"));
    }
}
