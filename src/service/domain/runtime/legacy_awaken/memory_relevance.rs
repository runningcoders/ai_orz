//! 检索结果精选（宽召回 → 小脑重排 → 阈值截断）
//!
//! 老路径：关键词召回后全部塞进 prompt——坑位即上限，多一条都进不来。
//! 这里保留上游的宽召回候选池，再由小脑**一次性**对整批候选排序，按权重
//! 阈值截回**调用方传入的渲染预算**，prompt 预算不变、召回命中率更高。
//!
//! ⚠️ `keep` 必须是入参：同一个排序形态同时服务「长期知识宽召回 → 5 条」和
//! 「短期记忆宽召回 → 3 条」两路独立预算，在这里写死任一 Top-K 都会让另一路
//! 的精选结果越过自己的渲染上限。
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

use super::retrieve_node::{memory_id, truncate_chars};
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
///
/// 只约束**正文**部分：来源标签是前缀，不吃这段预算（见 [`snippet`]）。
const SNIPPET_CHARS: usize = 160;

/// 候选来源标签（与 [`MemoryPo`] 的四个变体一一对应）
///
/// 四类候选的**可信度与用途不同**：长期知识是已沉淀的结论、短期记忆是工作记忆、
/// trace 是原始思考记录、关系是图谱边。模型无法只从正文可靠区分这四种来源，
/// 所以来源属于「判断所需的事实」，不是装饰。
///
/// ⚠️ 只加标签、**不加分数**：入参层（[`rank`] 的签名）没有任何召回分数可用；
/// 真正的相关性分数在**输出侧**（`DecisionValue::Pick::weights` 给出全部选项
/// 概率，`RankPolicy` 用它做阈值截断）。在候选文本里塞一个先验分反而会锚定
/// 模型，破坏「一次 Pick 看相对差异」的设计。
const SOURCE_KNOWLEDGE_NODE: &str = "[知识节点]";
const SOURCE_SHORT_TERM: &str = "[短期记忆]";
const SOURCE_TRACE: &str = "[trace]";
const SOURCE_RELATION: &str = "[关系]";

/// 重排判据（prompt 存调用方，服务端不持有）
const RANK_INSTRUCTIONS: &str = "下面每条候选是按关键词从记忆库召回的条目。请判断哪一条与用户问题最相关。\
你的判断会按各选项概率从高到低决定保留哪些条目，因此请让明显无关的候选概率尽量低。";

/// 本场景的排序形态实例（无状态，可反复构造）
///
/// `keep` = 该路召回的渲染预算，由调用方按类型给（长期 5 / 短期 3）。
fn policy(keep: usize) -> RankPolicy {
    RankPolicy::new(
        SCENE,
        "检索结果精选",
        "召回候选 ≥ 2 条且小脑可用时按相关性重排",
        RANK_INSTRUCTIONS,
        WEIGHT_THRESHOLD,
        keep,
    )
}

/// 候选的判据文本（来源标签 + 名称/摘录），空描述 = 不可判，直接跳过
///
/// 标签只加在**有内容**的候选上：空描述必须继续产出空串（[`candidate_views`]
/// 靠 `is_empty()` 判定「不可判」并丢弃）。否则标签会把「无内容」伪装成有
/// 内容，让空判据白占一个选项位 —— 那是行为变更，不是本次要的。
fn snippet(m: &Memory) -> String {
    let (label, text) = match &m.po {
        MemoryPo::KnowledgeNode(k) => {
            let body = if k.node_description.is_empty() {
                &k.summary
            } else {
                &k.node_description
            };
            if body.is_empty() {
                return String::new();
            }
            (
                SOURCE_KNOWLEDGE_NODE,
                format!("{}：{}", k.node_name, truncate_chars(body, SNIPPET_CHARS)),
            )
        }
        MemoryPo::ShortTerm(st) => (
            SOURCE_SHORT_TERM,
            truncate_chars(&st.summary, SNIPPET_CHARS),
        ),
        // trace 没有摘要字段：优先思考输出（短且有信息量），回退输入
        MemoryPo::Trace(t) => {
            let body = t.output.as_deref().unwrap_or(t.input.as_str());
            (SOURCE_TRACE, truncate_chars(body, SNIPPET_CHARS))
        }
        MemoryPo::Relation(r) => (
            SOURCE_RELATION,
            truncate_chars(&r.relation_type, SNIPPET_CHARS),
        ),
    };
    // 纯空白正文同样视为不可判（`summary` / `relation_type` 落库可能是空串）
    if text.trim().is_empty() {
        return String::new();
    }
    format!("{label}{text}")
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
    keep: usize,
) -> (Option<Vec<Memory>>, Option<u64>) {
    let policy = policy(keep);
    let metrics = facts(question, candidates);

    // 阶段一：策略声明要问什么（纯函数，未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        // 续-3：被丢弃的请求必须留痕。候选不足 2 条属常态 ⇒ info（warn 会刷屏）。
        // `usable` 直接读事实桶里已过滤好的选项数，零重算。
        let usable = metrics
            .get_as::<Vec<Candidate>>(KEY_CANDIDATES)
            .map_or(0, |v| v.len());
        let dropped = candidates.len().saturating_sub(usable);
        log_info!(
            ctx,
            "memory_relevance",
            decision = "empty_requests",
            scene = SCENE,
            candidates = candidates.len(),
            usable = usable,
            dropped = dropped,
            keep = keep,
            "memory rerank asked nothing, keep original order"
        );
        return (None, None);
    }

    // 本 run 的调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
    if !brain.cerebellum_quota.try_acquire() {
        log_info!(
            ctx,
            "cerebellum_quota",
            scene = SCENE,
            limit = brain.cerebellum_quota.limit(),
            used = brain.cerebellum_quota.used(),
            "cerebellum call budget exhausted, fall back to original logic"
        );
        return (None, None);
    }

    // 阶段二：唯一 async 点
    let candidate_count = candidates.len();
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    // 轨迹口径：`None` = 本次没真正发出请求（空请求门 / 三个零调用门）
    let traced_latency_ms = outcome.traced_latency_ms();
    let input_tokens = outcome.usage.input_tokens;
    let output_tokens = outcome.usage.output_tokens;

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "memory_relevance",
            decision = "degrade",
            degraded_reason = reason.as_str(),
            candidates = candidate_count,
            keep = keep,
            latency_ms = latency_ms,
            "memory rerank degraded"
        );
        return (None, traced_latency_ms);
    }

    // 阶段三：答案注入事实桶，形态负责排序与截断
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(ids) = policy.ranked(&metrics) else {
        log_info!(
            ctx,
            "memory_relevance",
            decision = "no_answer",
            candidates = candidate_count,
            keep = keep,
            latency_ms = latency_ms,
            "memory rerank produced no usable answer"
        );
        return (None, traced_latency_ms);
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
        keep = keep,
        kept = kept.len(),
        latency_ms = latency_ms,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        "memory rerank selected"
    );
    ((!kept.is_empty()).then_some(kept), traced_latency_ms)
}

/// 单例 DAL 的生产封装
pub(crate) async fn rank_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    question: &str,
    candidates: &[Memory],
    keep: usize,
) -> (Option<Vec<Memory>>, Option<u64>) {
    rank(ctx, dal(), brain, question, candidates, keep).await
}
#[cfg(test)]
#[path = "memory_relevance_tests.rs"]
mod tests;
