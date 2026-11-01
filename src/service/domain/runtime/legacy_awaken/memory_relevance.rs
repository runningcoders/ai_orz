//! 检索结果精选（宽召回 → 小脑重排 → 阈值截断）
//!
//! 老路径：关键词召回后全部塞进 prompt——坑位即上限，多一条都进不来。
//! 这里保留上游的宽召回候选池（[`super::retrieve_node::recall_width`]），
//! 再由小脑**一次性**对整批候选排序，按权重阈值截回
//! [`super::retrieve_node::KNOWLEDGE_TOP_K`] 条，prompt 预算不变、召回命中率更高。
//!
//! # 形态复用
//!
//! 排序这件事本身不在这里实现：[`RankPolicy`] 已经是「候选 → 一次 Pick →
//! 有序保留集」的通用形态。本模块只负责业务侧的两端——
//! 把 `Memory` 转成候选（[`Candidate`]），以及把保留的 id 映射回 `Memory`。
//!
//! # 为什么是一次 Pick，而不是 N 道题
//!
//! 协议一次请求只能要一个 choice 结论，但它的答案带**全部选项概率**
//! （`DecisionValue::Pick::weights`），所以「N 条候选重排」用**一次** Pick 表达：
//! 候选逐条作为选项，权重即相关性序。逐条各发一题不只是多花 token，
//! 还会让模型看不到彼此的相对差异——那是分类，不是排序。
//!
//! # 降级口径
//!
//! 关闭开关、无小脑、超时、答案缺失、模型编造 id —— 一律返回 `None`，
//! 调用方按机械 Top-K 截断（= 老路径）。精选是增强，不是闸门。

use super::retrieve_node::{KNOWLEDGE_TOP_K, memory_id, truncate_chars};
use crate::models::brain::Brain;
use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::pkg::policy::{Candidate, KEY_CANDIDATES, KEY_QUESTION, Metrics, Policy, RankPolicy};
use crate::service::dal::brain::{BrainDal, dal};
use std::collections::HashMap;
use std::sync::Arc;

/// 场景标识（日志/计量口径，也是 qid 命名空间前缀）
pub(crate) const SCENE: &str = "memory_relevance";

/// 权重阈值：低于此值视为不相关（被模型选中的那条无论如何保留）
pub(crate) const WEIGHT_THRESHOLD: f32 = 0.08;

/// 单条候选进判据的字符上限
const SNIPPET_CHARS: usize = 160;

/// 重排判据（prompt 存调用方，服务端不持有）
const RANK_INSTRUCTIONS: &str = "下面每条候选是按关键词从记忆库召回的条目。请判断哪一条与用户问题最相关。\
你的判断会按各选项概率从高到低决定保留哪些条目，因此请让明显无关的候选概率尽量低。";

/// 本场景的排序形态实例（无状态，可反复构造）
fn policy() -> RankPolicy {
    RankPolicy::new(
        SCENE,
        "检索结果精选",
        "召回候选 ≥ 2 条且小脑可用时按相关性重排",
        RANK_INSTRUCTIONS,
        WEIGHT_THRESHOLD,
        KNOWLEDGE_TOP_K,
    )
}

/// 候选的判据文本（名称 + 摘录），空描述 = 不可判，直接跳过
fn snippet(m: &Memory) -> String {
    match &m.po {
        MemoryPo::KnowledgeNode(k) => {
            let body = if k.node_description.is_empty() {
                &k.summary
            } else {
                &k.node_description
            };
            if body.is_empty() {
                String::new()
            } else {
                format!("{}：{}", k.node_name, truncate_chars(body, SNIPPET_CHARS))
            }
        }
        MemoryPo::ShortTerm(st) => truncate_chars(&st.summary, SNIPPET_CHARS),
        // trace 没有摘要字段：优先思考输出（短且有信息量），回退输入
        MemoryPo::Trace(t) => {
            let body = t.output.as_deref().unwrap_or(t.input.as_str());
            truncate_chars(body, SNIPPET_CHARS)
        }
        MemoryPo::Relation(r) => truncate_chars(&r.relation_type, SNIPPET_CHARS),
    }
}

/// 候选 → 决策视图（保持召回序；空判据直接丢弃，不占选项位）
fn candidate_views(candidates: &[Memory]) -> Vec<Candidate> {
    candidates
        .iter()
        .filter_map(|m| {
            let id = memory_id(m)?;
            let text = snippet(m);
            (!text.is_empty()).then(|| Candidate::new(id, text))
        })
        .collect()
}

/// 本场景的事实桶（问题 + 候选）
fn facts(question: &str, candidates: &[Memory]) -> Metrics {
    let views = candidate_views(candidates);
    Metrics::new()
        .with(KEY_QUESTION, question)
        .with(KEY_CANDIDATES, views)
}

/// 生产入口：一次决策 → 精选后的候选（`None` = 保持老路径）
pub(crate) async fn rank(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    question: &str,
    candidates: &[Memory],
) -> Option<Vec<Memory>> {
    let policy = policy();
    let metrics = facts(question, candidates);

    // 阶段一：策略声明要问什么（纯函数，未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        return None;
    }

    // 阶段二：唯一 async 点
    let candidate_count = candidates.len();
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    let input_tokens = outcome.usage.input_tokens;
    let output_tokens = outcome.usage.output_tokens;

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "memory_relevance",
            decision = "degrade",
            degraded_reason = reason.as_str(),
            candidates = candidate_count,
            latency_ms = latency_ms,
            "memory rerank degraded"
        );
        return None;
    }

    // 阶段三：答案注入事实桶，形态负责排序与截断
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(ids) = policy.ranked(&metrics) else {
        log_info!(
            ctx,
            "memory_relevance",
            decision = "no_answer",
            candidates = candidate_count,
            latency_ms = latency_ms,
            "memory rerank produced no usable answer"
        );
        return None;
    };

    let by_id: HashMap<&str, &Memory> = candidates
        .iter()
        .filter_map(|m| memory_id(m).map(|id| (id, m)))
        .collect();
    let kept: Vec<Memory> = ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .cloned()
        .collect();

    log_info!(
        ctx,
        "memory_relevance",
        decision = "select",
        candidates = candidate_count,
        kept = kept.len(),
        latency_ms = latency_ms,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        "memory rerank selected"
    );
    (!kept.is_empty()).then_some(kept)
}

/// 单例 DAL 的生产封装
pub(crate) async fn rank_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    question: &str,
    candidates: &[Memory],
) -> Option<Vec<Memory>> {
    rank(ctx, dal(), brain, question, candidates).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::memory::LongTermKnowledgeNodePo;
    use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionValue, QuestionSpec};
    use common::enums::MemoryStatus;

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

    fn metrics_for(question: &str, cands: &[Memory]) -> Metrics {
        facts(question, cands)
    }

    /// 构造带答案的事实桶（同时带候选：`ranked` 要用它过滤幻觉 id）
    fn with_pick(cands: &[Memory], weights: &[(&str, f32)], chosen: &str) -> Metrics {
        let answers = DecisionAnswers::new().with(
            policy().primary_qid(),
            DecisionValue::Pick {
                chosen: chosen.to_string(),
                confidence: 0.9,
                weights: weights
                    .iter()
                    .map(|(id, w)| ((*id).to_string(), *w))
                    .collect(),
            },
        );
        metrics_for("q", cands).with_decisions(Arc::new(answers))
    }

    #[test]
    fn fewer_than_two_candidates_skips_the_call() {
        let cands = vec![node("m1", "订单状态机", "订单流转")];
        let m = metrics_for("订单怎么流转", &cands);
        // 一次调用都不该发
        assert!(policy().prepare(&m).is_empty());
        assert!(!policy().is_triggered(&m));
    }

    #[test]
    fn request_puts_every_candidate_into_one_question() {
        let cands = vec![
            node("m1", "订单状态机", "订单从创建到完成"),
            node("m2", "退款流程", "上次讨论过退款流程"),
        ];
        let m = metrics_for("订单怎么流转", &cands);
        let p = policy();
        let reqs = p.prepare(&m);

        // 一次请求、一道题：不是每条候选各问一次
        assert_eq!(reqs.len(), 1);
        match &reqs[0].questions[&p.primary_qid()] {
            QuestionSpec::Pick { options, .. } => {
                assert_eq!(options.len(), 2);
                assert!(options["m1"].contains("订单状态机"));
                assert!(options["m2"].contains("退款"));
            }
            other => panic!("expected Pick, got {other:?}"),
        }
        assert_eq!(reqs[0].scene, SCENE);
        assert_eq!(reqs[0].state["candidate_count"], 2);
    }

    #[test]
    fn candidates_without_snippet_do_not_become_options() {
        let cands = vec![
            node("m1", "A", ""),
            node("m2", "B", "b"),
            node("m3", "C", "c"),
        ];
        let m = metrics_for("q", &cands);
        let p = policy();
        let reqs = p.prepare(&m);
        match &reqs[0].questions[&p.primary_qid()] {
            QuestionSpec::Pick { options, .. } => {
                assert_eq!(options.len(), 2, "空判据不占选项位");
                assert!(!options.contains_key("m1"));
            }
            other => panic!("expected Pick, got {other:?}"),
        }
    }

    #[test]
    fn no_answer_falls_back_to_old_path() {
        let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
        let m = metrics_for("q", &cands);
        // 有请求但没答案 → 未命中，`ranked` 给 None（不是「全不相关」）
        assert!(!policy().is_triggered(&m));
        assert!(policy().ranked(&m).is_none());
    }

    #[test]
    fn usable_pick_triggers_the_policy() {
        let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
        let m = with_pick(&cands, &[("m1", 0.6), ("m2", 0.4)], "m1");
        assert_eq!(policy().evaluate(&m), vec![SCENE.to_string()]);
    }

    #[test]
    fn empty_weights_do_not_trigger() {
        let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
        let m = with_pick(&cands, &[], "m1");
        assert!(!policy().is_triggered(&m));
    }

    #[test]
    fn ranked_keeps_candidates_above_threshold() {
        let cands = vec![
            node("m1", "A", "a"),
            node("m2", "B", "b"),
            node("m3", "C", "c"),
        ];
        let m = with_pick(&cands, &[("m1", 0.02), ("m2", 0.6), ("m3", 0.38)], "m2");

        // 按权重降序，且 m1 低于阈值被剔除
        assert_eq!(
            policy().ranked(&m),
            Some(vec!["m2".to_string(), "m3".to_string()])
        );
    }

    #[test]
    fn chosen_survives_even_when_every_weight_is_low() {
        let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
        let m = with_pick(&cands, &[("m1", 0.01), ("m2", 0.02)], "m1");

        // 全批低于阈值也只保留被选中的那条，绝不清空召回
        assert_eq!(policy().ranked(&m), Some(vec!["m1".to_string()]));
    }

    #[test]
    fn unknown_ids_from_model_are_dropped() {
        let cands = vec![node("m1", "A", "a"), node("m2", "B", "b")];
        let m = with_pick(&cands, &[("m1", 0.2), ("ghost", 0.9)], "m1");

        assert_eq!(policy().ranked(&m), Some(vec!["m1".to_string()]));
    }

    #[test]
    fn keep_top_k_is_bounded() {
        let cands: Vec<Memory> = (0..10).map(|i| node(&format!("m{i}"), "n", "d")).collect();
        let weights: Vec<(&str, f32)> = vec![
            ("m0", 0.9),
            ("m1", 0.8),
            ("m2", 0.7),
            ("m3", 0.6),
            ("m4", 0.5),
            ("m5", 0.4),
            ("m6", 0.3),
            ("m7", 0.2),
            ("m8", 0.1),
            ("m9", 0.9),
        ];
        let m = with_pick(&cands, &weights, "m0");

        let kept = policy().ranked(&m).expect("selection");
        assert_eq!(kept.len(), KNOWLEDGE_TOP_K);
    }
}
