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

/// 方法技能条数上限
const METHOD_SKILLS_TOP_K: usize = 2;

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

/// 单条知识命中 → 一行摘要（截前 160 chars）
///
/// `Memory` 没有 `content` 字段（计划里的字段名以实际模型为准）：可读文本
/// 存在于 PO 变体里——短期记忆取 summary、知识节点取 node_name + 描述；
/// Relation / Trace 没有可直接引用的文本（边的两端会以 KnowledgeNode 形式
/// 单独命中），跳过不产生行。
fn hit_line(m: &Memory) -> Option<String> {
    let text = match &m.po {
        MemoryPo::ShortTerm(st) if !st.summary.is_empty() => st.summary.clone(),
        MemoryPo::KnowledgeNode(k) => {
            let body = if k.node_description.is_empty() {
                &k.summary
            } else {
                &k.node_description
            };
            if body.is_empty() {
                return None;
            }
            format!("{}：{}", k.node_name, body)
        }
        _ => return None,
    };
    let line: String = text.chars().take(160).collect();
    Some(format!("- {line}"))
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

        // 3. visited 差集为空 → 无新息不查库：写空 pack 短路（保留旧 pack 的思考摘要）
        let new_terms = diff_terms(&ia.key_terms, &queried_snapshot);
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
            Ok(memories) => memories.iter().filter_map(hit_line).collect::<Vec<_>>(),
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
