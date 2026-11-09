//! 思考强度选择（小脑直接按 0–100 分选）
//!
//! 2026-11-09 AMan 拍板：强度值交给小脑选，平台天然支持 0–100 分
//! （[`ThinkingIntensity`] 本身就是 0–100 的分数，六档只是下游 wire 的映射）。
//! 本模块只负责「问什么、答案怎么读」，执行单点仍是 `BrainDal::decide`。
//!
//! # 降级（业务方忽略错误、走原逻辑）
//!
//! 无小脑 / 调用失败 / 超时 / 低置信 / 答案不可用 → 一律 `None`，调用方退回
//! 原逻辑（Provider 基准 + Agent 覆盖 + 场景偏移 + 意图确定性偏移）。
//! 小脑是可选增强，绝不阻塞主流程，也不把「没判断出来」当成「判断为最低强度」。
//!
//! # Provider 未配置 `thinking_intensity` 时
//!
//! 分数照常算出并**写进思考轨迹**（`applied=false`），但请求体不下发该字段——
//! 能力开关的唯一判定点在 `think_round::runner::resolve_call_options`，这里
//! 不重复判。这样「算出了但没生效」是可观测的，不是静默失效。

use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, PickOnePolicy, Policy};
use crate::service::dal::brain::{BrainDal, dal};
use common::enums::ThinkingIntensity;
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 qid 命名空间前缀）
pub const SCENE: &str = "thinking_strength";

/// 置信阈值：低于此值按「没结论」处理
///
/// 比路由的 0.85 宽松：强度是「程度判断」，判偏一档只是贵一点/浅一点，
/// 不像路由那样直接决定「进不进主循环」。判不出来时必须走原逻辑。
pub const MIN_CONFIDENCE: f32 = 0.5;

/// 判据（prompt 存调用方，服务端不持有）
const INSTRUCTIONS: &str = "请判断用户这条消息需要多深的思考强度，在 0–100 分之间选一档。\
分数越高代表需要越多的推理与工具调用；越低代表越可以直接回答。\
只按「这条消息本身需要多深的处理」判断，不要按身份、历史表现或情绪判断。";

/// 11 档（选项 id 就是分数字符串，直接映射 0–100）
///
/// 为什么不是 Grade：协议的 Grade 只支持 2–10 级，0–100 的分数用它表达要
/// 人为切档再反推；Pick 的选项 id 可以就是分数本身，读回来零换算。
const LEVELS: [(&str, &str); 11] = [
    ("0", "0 分：寒暄 / 客套 / 纯格式确认，无需任何推理"),
    ("10", "10 分：一句话事实或简单确认"),
    ("20", "20 分：简单解释或单步操作指引"),
    ("30", "30 分：需要少量检索或一步工具调用"),
    ("40", "40 分：需要综合几条已知信息作答"),
    ("50", "50 分：常规任务，需要若干轮推理与工具调用"),
    ("60", "60 分：多步任务，需要计划与自我验证"),
    ("70", "70 分：复杂任务，涉及代码 / 数据 / 多文件改动"),
    ("80", "80 分：高难度任务，需要深入分析与反复验证"),
    ("90", "90 分：跨模块 / 跨项目深链推理，错一步代价高"),
    ("100", "100 分：最高强度，需要穷尽式排查与充分自证"),
];

/// 透传给模型的上下文事实（与路由同口径：只带轻量上下文，不取全量记忆）
const STATE_KEYS: &[&str] = &["agent_id", "agent_name", "scene", "last_user_message"];

/// 一次强度选择的结果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrengthChoice {
    /// 小脑选定的强度（0–100 分）
    pub strength: ThinkingIntensity,
    /// 该选项的概率
    pub confidence: f32,
}

/// 本场景的策略形态（无状态，可反复构造）
fn policy() -> PickOnePolicy {
    PickOnePolicy::new(
        SCENE,
        "思考强度选择",
        "这条用户消息需要多深的思考强度（0–100 分）",
        INSTRUCTIONS,
        LEVELS,
        MIN_CONFIDENCE,
    )
    .with_state_keys(STATE_KEYS)
}

/// 生产入口：小脑选定本次思考强度（`None` = 保持原逻辑）
pub async fn select(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    scene: &str,
    last_user_message: Option<&str>,
) -> Option<StrengthChoice> {
    // 没有用户消息就没有可判断的输入：直接走原逻辑，零调用。
    let user_text = last_user_message?;

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
        return None;
    }

    let policy = policy();
    let metrics = Metrics::new()
        .with("agent_id", &brain.agent_id)
        .with("agent_name", &brain.agent_name)
        .with("scene", scene)
        .with("last_user_message", user_text);

    // 阶段一：策略声明要问什么（纯函数；未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        return None;
    }

    // 阶段二：唯一 async 点（门禁 / 超时 / 协议转换 / 降级全在 DAL）
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "thinking_strength",
            decision = "fallback",
            degraded_reason = reason.as_str(),
            scene = scene,
            latency_ms = latency_ms,
            "thinking strength degraded to original logic"
        );
        return None;
    }

    // 阶段三：答案注入事实桶，形态负责「选一个 + 置信达标」
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(pick) = policy.chosen(&metrics) else {
        log_info!(
            ctx,
            "thinking_strength",
            decision = "no_answer",
            scene = scene,
            latency_ms = latency_ms,
            "thinking strength produced no usable answer"
        );
        return None;
    };
    // 选项 id 就是分数；形态已保证在选项集内，解析失败只可能是配置被改坏。
    let Ok(score) = pick.option.parse::<u8>() else {
        log_warn!(
            ctx,
            "thinking_strength",
            decision = "bad_option",
            option = pick.option.as_str(),
            "thinking strength option is not a score"
        );
        return None;
    };
    let strength = ThinkingIntensity::new(score);
    log_info!(
        ctx,
        "thinking_strength",
        decision = "select",
        score = strength.score(),
        confidence = pick.confidence,
        scene = scene,
        latency_ms = latency_ms,
        input_tokens = outcome.usage.input_tokens,
        output_tokens = outcome.usage.output_tokens,
        "thinking strength selected"
    );
    Some(StrengthChoice {
        strength,
        confidence: pick.confidence,
    })
}

/// 单例 DAL 的生产封装
pub async fn select_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    scene: &str,
    last_user_message: Option<&str>,
) -> Option<StrengthChoice> {
    select(ctx, dal(), brain, scene, last_user_message).await
}
