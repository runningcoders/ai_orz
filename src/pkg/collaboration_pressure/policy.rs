//! 压力档位规则表：**策略判据 + 行为动作 + 配套提示词 + 回血话术** 的单一真相
//!
//! 照 `message_route_policy::STATIC_ROUTE_DEFS` 先例：规则按 Or 组合，
//! 声明顺序即优先级（`evaluate` 返回全部命中，取 `first()` 即最高优先级档位）。
//!
//! ⚠️ 阈值**不写进规则对象**：`FatigueAtLeast` / `WindowDepthAtLeast` 从
//! [`Metrics`] 里按 key 双键读取，规则本身无状态。因此规则组可以安全地
//! `OnceLock` 缓存，而系统配置改了阈值立刻生效（旧 `chain_policy()` 把
//! `MAX_AGENT_REPLY_CHAIN` 固化进 `OnceLock<ThresholdPolicy>` 是反例）。

use std::sync::OnceLock;

use serde::Serialize;

use crate::pkg::policy::{Metrics, Policy, PolicyBuilder};

use super::keys;

/// 压力档位（记录与提示词共用的语义标签）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PressureStage {
    /// 正常协作
    Fresh,
    /// 发生回血后的可续协作
    Recovered,
    /// 压力带：施压但未终止
    Pressured,
    /// 硬红线：本轮回发收口为知会（最后一跳）并升级给归属方
    Exhausted,
}

impl PressureStage {
    pub fn label(self) -> &'static str {
        match self {
            PressureStage::Fresh => "fresh",
            PressureStage::Recovered => "recovered",
            PressureStage::Pressured => "pressured",
            PressureStage::Exhausted => "exhausted",
        }
    }
}

/// 档位对应的行为动作（业务侧据此映射路由 / 循环控制，组件不感知具体协议）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PressureAction {
    /// 正常放行
    Continue,
    /// 放行但伴随提示词施压，让模型主动收敛
    Nudge,
    /// 升级：本轮回发**收口为知会**（最后一跳，信息照常送达）并通知归属方
    ///
    /// ⚠️ 不是「丢回包」也不是「照常回发」：前者让结论凭空消失，后者把乒乓推高一跳。
    /// 行为落点在 `message_route_policy::AutoReplyRoute::CloseChainAsNotify`，
    /// 文案必须同步说明「本轮仍会送达」，否则模型会以为发不出去。
    Escalate,
}

impl PressureAction {
    pub fn label(self) -> &'static str {
        match self {
            PressureAction::Continue => "continue",
            PressureAction::Nudge => "nudge",
            PressureAction::Escalate => "escalate",
        }
    }
}

/// 档位判据（全部读 [`Metrics`]，无构造期阈值）
#[derive(Debug, Clone, Copy)]
pub(super) enum Matcher {
    /// `fatigue >= metrics[threshold_key]`；阈值 0 = 未启用
    FatigueAtLeast(&'static str),
    /// `window_depth >= metrics[HARD_WINDOW_DEPTH]`
    WindowDepthAtLeast,
    /// 发生过有效回血
    Recovered,
    /// 兜底规则不参与判定
    Never,
}

impl Matcher {
    fn matches(self, metrics: &Metrics) -> bool {
        match self {
            Matcher::FatigueAtLeast(threshold_key) => {
                let threshold = metrics.get_u64(threshold_key).unwrap_or(0);
                threshold > 0 && metrics.get_u64(keys::FATIGUE).unwrap_or(0) >= threshold
            }
            Matcher::WindowDepthAtLeast => {
                let hard = metrics.get_u64(keys::HARD_WINDOW_DEPTH).unwrap_or(0);
                hard > 0 && metrics.get_u64(keys::WINDOW_DEPTH).unwrap_or(0) >= hard
            }
            Matcher::Recovered => metrics.get_u64(keys::RECOVERY_COUNT).unwrap_or(0) > 0,
            Matcher::Never => false,
        }
    }

    fn required(self) -> Vec<String> {
        match self {
            Matcher::FatigueAtLeast(key) => {
                vec![keys::FATIGUE.to_string(), key.to_string()]
            }
            Matcher::WindowDepthAtLeast => vec![
                keys::WINDOW_DEPTH.to_string(),
                keys::HARD_WINDOW_DEPTH.to_string(),
            ],
            Matcher::Recovered => vec![keys::RECOVERY_COUNT.to_string()],
            Matcher::Never => vec![],
        }
    }
}

/// 一条档位规则：判据、动作、提示词、回血话术**同行声明**
#[derive(Debug, Clone, Copy)]
pub(super) struct StageRule {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) condition_desc: &'static str,
    pub(super) matcher: Matcher,
    pub(super) stage: PressureStage,
    pub(super) action: PressureAction,
    /// 给模型的本轮指引
    pub(super) prompt_hint: &'static str,
    /// 回血机制说明（与提示词成对维护，避免「动作改了提示词没改」）
    pub(super) recovery_hint: &'static str,
}

impl Policy for StageRule {
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &str {
        self.name
    }

    fn condition_desc(&self) -> &str {
        self.condition_desc
    }

    fn required_metrics(&self) -> Vec<String> {
        self.matcher.required()
    }

    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        if self.matcher.matches(metrics) {
            vec![self.id.to_string()]
        } else {
            Vec::new()
        }
    }
}

/// 档位规则表（声明顺序 = 优先级：硬红线在最前，回血态在未命中压力时兜底）
pub(super) static STAGE_DEFS: &[StageRule] = &[
    StageRule {
        id: "pressure_fatigue_exhausted",
        name: "疲劳度触顶",
        condition_desc: "fatigue >= hard_fatigue",
        matcher: Matcher::FatigueAtLeast(keys::HARD_FATIGUE),
        stage: PressureStage::Exhausted,
        action: PressureAction::Escalate,
        prompt_hint: "协作已连续多轮无有效新信息、疲劳度触顶。本轮是本链最后一跳：Final 仍会作为**知会**完整送达来源方（对方不会再自动回发，框架也不会再唤醒你），请一次性给出可独立读懂的最终结论，不要期待下一轮，也不要把上一轮换个说法重发；若后续工作与来源 Agent 无关，让 Final 恰好只输出 NO_REPLY（那就不发知会）。",
        recovery_hint: "只有用户补充、工具/任务结果或新参与方注入才能开启新窗口；确需继续这段讨论，由一方主动重新发起新消息（新链从第 1 跳重新计数），而不是在旧链上回发。",
    },
    StageRule {
        id: "pressure_window_exhausted",
        name: "连续窗口触顶",
        condition_desc: "window_depth >= hard_window_depth",
        matcher: Matcher::WindowDepthAtLeast,
        stage: PressureStage::Exhausted,
        action: PressureAction::Escalate,
        prompt_hint: "连续无新信息窗口已达上限，本轮回发会**收口为知会**（本链最后一跳，结果照常送达对方）。请直接给出完整结论；若来源是 Agent 且后续无关，让 Final 恰好只输出 NO_REPLY。",
        recovery_hint: "回血会把连续窗口清零，因此带新信息的往返不会撞这条硬墙；收口后要续接，请主动发起新消息重新开链。",
    },
    StageRule {
        id: "pressure_nudge",
        name: "信息疲劳施压",
        condition_desc: "fatigue >= soft_fatigue",
        matcher: Matcher::FatigueAtLeast(keys::SOFT_FATIGUE),
        stage: PressureStage::Pressured,
        action: PressureAction::Nudge,
        prompt_hint: "信息疲劳度偏高：连续往返尚未引入新信息。请优先给出结论或引入新事实，不要把上一轮换个说法再发一次；若来源是 Agent 且后续工作与来源 Agent 无关，让 Final 恰好只输出 NO_REPLY。",
        recovery_hint: "本轮若带来工具/任务结果或新参与方，疲劳度回血且连续窗口清零，协作可以合法继续；纯文本重复往返会继续累积疲劳度。",
    },
    StageRule {
        id: "pressure_recovered",
        name: "新信息回血",
        condition_desc: "recovery_count > 0",
        matcher: Matcher::Recovered,
        stage: PressureStage::Recovered,
        action: PressureAction::Continue,
        prompt_hint: "本轮已注入新信息，疲劳度回血、连续窗口重新计数，可以继续协作；但请只在确实有新结论或新事实时才回复。",
        recovery_hint: "已发生回血，当前连续窗口从 0 重新计数；再次进入纯文本往返会重新累积疲劳度。",
    },
];

/// 未命中任何规则的兜底档位（不参与规则组判定）
const FALLBACK: StageRule = StageRule {
    id: "pressure_fresh",
    name: "正常协作",
    condition_desc: "fatigue < soft_fatigue && window_depth < hard_window_depth",
    matcher: Matcher::Never,
    stage: PressureStage::Fresh,
    action: PressureAction::Continue,
    prompt_hint: "协作链刚开始，正常回复即可；若来源是 Agent 且后续工作与来源 Agent 无关，让 Final 恰好只输出 NO_REPLY。",
    recovery_hint: "引入工具/任务结果、新参与方或用户输入会降低疲劳度并重置连续窗口，允许在硬墙之外继续协作。",
};

/// 规则组：Or 组合，阈值来自 Metrics 所以规则对象无状态、可安全缓存
fn ruleset() -> &'static dyn Policy {
    static RULESET: OnceLock<Box<dyn Policy>> = OnceLock::new();
    RULESET
        .get_or_init(|| {
            let mut builder = PolicyBuilder::new();
            for rule in STAGE_DEFS {
                builder = builder.with_policy(*rule);
            }
            builder.or()
        })
        .as_ref()
}

/// 一次裁决的档位产出（父模块据此组装 [`super::PressureDecision`]）
pub(super) struct StageOutcome {
    pub stage: PressureStage,
    pub action: PressureAction,
    pub policy_id: &'static str,
    pub condition_desc: &'static str,
    pub prompt_hint: &'static str,
    pub recovery_hint: &'static str,
    pub policy_hits: Vec<String>,
}

/// 跑策略引擎，取声明序最高的命中档位
pub(super) fn judge_stage(metrics: &Metrics) -> StageOutcome {
    let hits = ruleset().evaluate(metrics);
    let rule = hits
        .first()
        .and_then(|id| STAGE_DEFS.iter().find(|r| r.id == id))
        .unwrap_or(&FALLBACK);
    StageOutcome {
        stage: rule.stage,
        action: rule.action,
        policy_id: rule.id,
        condition_desc: rule.condition_desc,
        prompt_hint: rule.prompt_hint,
        recovery_hint: rule.recovery_hint,
        policy_hits: hits,
    }
}
