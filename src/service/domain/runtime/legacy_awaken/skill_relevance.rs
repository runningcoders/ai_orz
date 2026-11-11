//! 方法技能精选（同 tag 已发布候选 → 小脑一次 Pick 全序 → 阈值截断 → 渲染预算）
//!
//! 老路径：`thinking_method` 已发布技能按「description 与新词词面重叠」布尔排序取
//! 前 [`METHOD_SKILLS_TOP_K`](super::retrieve_node::METHOD_SKILLS_TOP_K) 条。词面重叠
//! 只回答「描述里有没有提到这个词」，回答不了「这一条该不该用」——而技能描述写的
//! 正是「什么时候用这个技能」，这是模型比字符串匹配擅长的判断。
//!
//! # 形态复用
//!
//! 排序本身不在这里实现：[`RankPolicy`] 已经是「候选 → 一次 Pick → 有序保留集」的
//! 通用形态。本模块只负责业务侧两端——把 [`SkillPo`] 转成候选（[`Candidate`]），
//! 以及把保留的 id 映射回 [`SkillPo`]。
//!
//! # 为什么是一次 Pick，而不是 N 道题
//!
//! 同 `memory_relevance`：答案带**全部选项概率**（`DecisionValue::Pick::weights`），
//! 候选逐条作选项，权重即适用性序。逐条各发一题既多花 token，又让模型看不到彼此的
//! 相对差异——那是分类，不是排序。
//!
//! # 降级口径
//!
//! 关闭开关、无小脑、超时、答案缺失、模型编造 id —— 一律返回 `None`，调用方退回
//! 「词面重叠优先 + 稳定排序」取前 2（= 老路径）。精选是增强，不是闸门。

use crate::models::brain::Brain;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Candidate, KEY_CANDIDATES, KEY_QUESTION, Metrics, Policy, RankPolicy};
use crate::service::dal::brain::{BrainDal, dal};
use crate::service::domain::runtime::cerebellum_context;
use crate::service::domain::runtime::compaction::truncate_to_tokens;
use std::collections::HashMap;
use std::sync::Arc;

/// 场景标识（日志/计量口径，也是 qid 命名空间前缀）
pub(crate) const SCENE: &str = "skill_relevance";

/// 权重阈值：低于此值视为不适用（被模型选中的那条无论如何保留）
pub(crate) const WEIGHT_THRESHOLD: f32 = 0.08;

/// 重排判据（prompt 存调用方，服务端不持有）
const RANK_INSTRUCTIONS: &str = "下面每条候选是一个可用的方法技能，描述里写的是「什么时候该用这个技能」。\
请判断哪一条最适合解决当前问题。你的判断会按各选项概率从高到低决定保留哪些技能，\
因此请让明显无关的技能概率尽量低；若一条都不适用，请把所有选项概率都给得很低。";

/// 本场景的排序形态实例（无状态，可反复构造）
///
/// `keep` = 技能召回位的渲染预算，由调用方给（当前
/// [`METHOD_SKILLS_TOP_K`](super::retrieve_node::METHOD_SKILLS_TOP_K)）。
fn policy(keep: usize) -> RankPolicy {
    RankPolicy::new(
        SCENE,
        "方法技能精选",
        "已发布方法技能 ≥ 2 条且小脑可用时按适用性重排",
        RANK_INSTRUCTIONS,
        WEIGHT_THRESHOLD,
        keep,
    )
}

/// 候选的判据文本（名称 + 何时用 + 分类），全空 = 不可判，直接跳过
///
/// 三个字段都是技能的「可判别面」：`description` 是「什么时候用」，
/// `category` 给出领域归属，`name` 兜底标识同一族的不同粒度。
fn snippet(s: &SkillPo, snippet_tokens: Option<usize>) -> String {
    let mut text = s.name.trim().to_string();
    // 描述正文按 token 预算截断（`share::SNIPPET_BP` × 输入预算）；
    // 容量未配置 ⇒ 不截断。名称 / 分类不吃这段预算（它们是判别面，不是正文）。
    let desc = s.description.trim();
    let desc = match snippet_tokens {
        Some(budget) => truncate_to_tokens(desc, budget, ""),
        None => desc.to_string(),
    };
    if !desc.is_empty() {
        if !text.is_empty() {
            text.push('：');
        }
        text.push_str(&desc);
    }
    let category = s.category.trim();
    if !category.is_empty() {
        if text.is_empty() {
            text.push_str(category);
        } else {
            text.push('（');
            text.push_str(category);
            text.push('）');
        }
    }
    text
}

/// 候选 → 决策视图（保持召回序；空判据直接丢弃，不占选项位）
fn candidate_views(skills: &[SkillPo], snippet_tokens: Option<usize>) -> Vec<Candidate> {
    skills
        .iter()
        .filter_map(|s| {
            let id = s.id.trim();
            let text = snippet(s, snippet_tokens);
            (!id.is_empty() && !text.is_empty()).then(|| Candidate::new(id, text))
        })
        .collect()
}

/// 本场景的事实桶（问题 + 候选）
fn facts(question: &str, skills: &[SkillPo], snippet_tokens: Option<usize>) -> Metrics {
    let views = candidate_views(skills, snippet_tokens);
    Metrics::new()
        .with(KEY_QUESTION, question)
        .with(KEY_CANDIDATES, views)
}

/// 生产入口：一次决策 → 精选后的技能（`None` = 保持老路径）
pub(crate) async fn rank(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    question: &str,
    skills: &[SkillPo],
    keep: usize,
) -> (Option<Vec<SkillPo>>, Option<u64>) {
    let policy = policy(keep);
    let snippet_tokens =
        cerebellum_context::snippet_budget(brain, cerebellum_context::share::SNIPPET_BP);
    let metrics = facts(question, skills, snippet_tokens);

    // 阶段一：策略声明要问什么（纯函数，未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        // 续-3：被丢弃的请求必须留痕。候选不足 2 条属常态 ⇒ info（warn 会刷屏）。
        // `usable` 直接读事实桶里已过滤好的选项数，零重算。
        let usable = metrics
            .get_as::<Vec<Candidate>>(KEY_CANDIDATES)
            .map_or(0, |v| v.len());
        let dropped = skills.len().saturating_sub(usable);
        log_info!(
            ctx,
            "skill_relevance",
            decision = "empty_requests",
            scene = SCENE,
            candidates = skills.len(),
            usable = usable,
            dropped = dropped,
            keep = keep,
            "skill rerank asked nothing, keep original order"
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
    let candidate_count = skills.len();
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    // 轨迹口径：`None` = 本次没真正发出请求（空请求门 / 三个零调用门）
    let traced_latency_ms = outcome.traced_latency_ms();
    let input_tokens = outcome.usage.input_tokens;
    let output_tokens = outcome.usage.output_tokens;

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "skill_relevance",
            decision = "degrade",
            degraded_reason = reason.as_str(),
            candidates = candidate_count,
            keep = keep,
            latency_ms = latency_ms,
            "skill rerank degraded"
        );
        return (None, traced_latency_ms);
    }

    // 阶段三：答案注入事实桶，形态负责排序与截断
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(ids) = policy.ranked(&metrics) else {
        log_info!(
            ctx,
            "skill_relevance",
            decision = "no_answer",
            candidates = candidate_count,
            keep = keep,
            latency_ms = latency_ms,
            "skill rerank produced no usable answer"
        );
        return (None, traced_latency_ms);
    };

    let by_id: HashMap<&str, &SkillPo> = skills.iter().map(|s| (s.id.as_str(), s)).collect();
    let kept: Vec<SkillPo> = ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .cloned()
        .collect();

    log_info!(
        ctx,
        "skill_relevance",
        decision = "select",
        candidates = candidate_count,
        keep = keep,
        kept = kept.len(),
        latency_ms = latency_ms,
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        "skill rerank selected"
    );
    ((!kept.is_empty()).then_some(kept), traced_latency_ms)
}

/// 单例 DAL 的生产封装
pub(crate) async fn rank_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    question: &str,
    skills: &[SkillPo],
    keep: usize,
) -> (Option<Vec<SkillPo>>, Option<u64>) {
    rank(ctx, dal(), brain, question, skills, keep).await
}

#[cfg(test)]
#[path = "skill_relevance_tests.rs"]
mod tests;
