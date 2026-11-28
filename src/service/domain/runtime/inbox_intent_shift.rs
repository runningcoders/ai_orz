//! 收件箱新输入的「意图是否改变」判定（poll 收集后的第二层判据）
//!
//! 第一层 [`super::inbox_relevance`] 已回答「这些新消息是不是在补同一件事」，
//! 本模块回答下一问：**用户是在否定当前意图、要换个方向重干，还是只是补充**。
//!
//! - **一次二选一**：整批新输入作为一次 `Pick` 的两个选项
//!   （`reinterpret` / `supplement`），结果直接决定后续走哪条边：
//!   `reinterpret` → `set_revision` 回 intent 做增量修订；`supplement` → 尾插追加；
//! - **隐式启用**：Brain 由 Agent 携带，有小脑即判断；无小脑 / 配额耗尽 /
//!   降级 / 无答案一律回 [`IntentShift::Supplement`]（`supplement` 成本最低，
//!   不重跑意图、不作废检索结果）；
//! - 现场上下文复用 [`InboxJudgeContext`] —— 它已含「输入理解结果」（即当前意图），
//!   再附上本批新消息正文，小脑据此比较「新输入 vs 当前意图」。
//!
//! 决策走统一的 [`crate::service::dal::brain::BrainDal::decide`]，生产入口
//! [`decide_with_default_dao`] 注入默认 DAL，核函数 [`decide`] 注入 `&dyn BrainDal`
//! 便于测试覆盖各降级路径（同 `thinking_strength::select` 范式）。

use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::decision::{DecisionRequest, DecisionValue, QuestionSpec};
use crate::service::dal::brain::BrainDal;
use crate::service::domain::runtime::inbox::InjectedMessage;
use crate::service::domain::runtime::inbox_relevance::InboxJudgeContext;
use std::collections::BTreeMap;

/// 决策场景标识（计量 / 日志口径，全局唯一）
pub const DECISION_SCENE: &str = "inbox_intent_shift";

/// 固定问题 ID（独立命名空间，避免覆盖相关性排序等场景）
pub(crate) const QUESTION_ID: &str = "inbox_intent_shift.verdict.primary";

/// 「用户改了意图」选项：否定当前理解，带旧结果 + 新消息重识别
const OPTION_REINTERPRET: &str = "reinterpret";
/// 「用户补充意图」选项：仍在做同一件事，尾插追加即可
const OPTION_SUPPLEMENT: &str = "supplement";

/// 单条新输入进判据的最大字符数（控制小脑 token，超出截断）
const NEW_INPUT_SNIPPET_CHARS: usize = 400;

/// 判定结论
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentShift {
    /// 用户否定了当前意图，要换方向重做 —— 回 intent 节点做增量修订
    Reinterpret,
    /// 用户仍在补充当前意图 —— 直接尾插追加，不重跑意图
    Supplement,
}

impl IntentShift {
    /// 是否需要回 intent 重识别
    pub fn needs_reinterpret(self) -> bool {
        matches!(self, Self::Reinterpret)
    }
}

/// 二选一判据：比较本批新输入与当前意图（已在现场上下文里）
const INSTRUCTIONS: &str = "「当前思考现场」里的【输入理解结果】是系统对当前消息的理解（当前意图）；\
「本批新输入」是同一用户在之后发来的补充消息。\
请判断这些新输入是**否定了当前意图、要求换一个方向重新处理**（选 reinterpret），\
还是**仍在推进、补充、追问当前这件事**（选 supplement）。\
只有用户明确改变了要做的事、或要求推翻重来时才选 reinterpret；\
补充细节、追加条件、催促、补充材料都算 supplement。";

/// 截断正文为判据片段（按字符计，避免中英混排按字节切出半个汉字）
fn snippet(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}…（已截断）")
}

/// 组装小脑 state：现场要点（含当前意图）+ 本批新输入正文
fn build_state(judge: &InboxJudgeContext, batch: &[InjectedMessage]) -> String {
    let new_inputs = batch
        .iter()
        .enumerate()
        .map(|(index, item)| {
            format!(
                "{}. {}",
                index + 1,
                snippet(&item.content, NEW_INPUT_SNIPPET_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut bundle = crate::service::domain::runtime::cerebellum_context::ContextBundle::new()
        .with_identity(&judge.agent_id, &judge.agent_name)
        .with_scene(&judge.scene);
    if !judge.situation.is_empty() {
        bundle.push("当前思考现场", judge.situation.join("\n"));
    }
    bundle.push("本批新输入", new_inputs);
    bundle.render()
}

/// 判定本批新输入是否改变了当前意图
///
/// 无小脑 / 配额耗尽 / 调用降级 / 无结论一律返回 [`IntentShift::Supplement`]，
/// 因为尾插追加成本最低 —— 不重跑意图、不作废已完成的检索与装配。
/// DAL 由调用方注入，便于测试替换成 mock 覆盖各降级路径。
pub async fn decide(
    ctx: &RequestContext,
    judge: &InboxJudgeContext,
    batch: &[InjectedMessage],
    brain: Option<&Brain>,
    dal: &dyn BrainDal,
) -> IntentShift {
    // Brain 未装配（外部 Agent / 尚未唤醒）或没有小脑 ⇒ 零调用降级
    if brain.and_then(|brain| brain.cerebellum.as_ref()).is_none()
        || !brain.is_some_and(|brain| brain.cerebellum_quota.try_acquire())
    {
        return IntentShift::Supplement;
    }
    let brain = brain.expect("上面已确认 Brain 存在");

    let mut options = BTreeMap::new();
    options.insert(
        OPTION_REINTERPRET.to_string(),
        "用户否定了当前意图，要换方向重新处理".to_string(),
    );
    options.insert(
        OPTION_SUPPLEMENT.to_string(),
        "用户仍在补充、追问当前这件事".to_string(),
    );
    let mut questions = BTreeMap::new();
    questions.insert(
        QUESTION_ID.to_string(),
        QuestionSpec::Pick {
            instructions: INSTRUCTIONS.to_string(),
            options,
        },
    );
    let request = DecisionRequest {
        scene: DECISION_SCENE,
        state: serde_json::json!({
            crate::service::domain::runtime::cerebellum_context::STATE_KEY: build_state(judge, batch)
        }),
        questions,
        timeout_ms: None,
    };

    let outcome = dal.decide(ctx.clone(), brain, vec![request]).await;
    if outcome.degraded.is_some() {
        return IntentShift::Supplement;
    }
    let Some(DecisionValue::Pick { chosen, .. }) = outcome.answers.get(QUESTION_ID) else {
        return IntentShift::Supplement;
    };
    if chosen == OPTION_REINTERPRET {
        IntentShift::Reinterpret
    } else {
        IntentShift::Supplement
    }
}

/// 生产入口：注入默认 Brain DAL（与 `rank_related_with_default_dao` 同范式）
pub async fn decide_with_default_dao(
    ctx: &RequestContext,
    judge: &InboxJudgeContext,
    batch: &[InjectedMessage],
    brain: Option<&Brain>,
) -> IntentShift {
    let dal = crate::service::dal::brain::dal();
    decide(ctx, judge, batch, brain, dal.as_ref()).await
}

#[cfg(test)]
#[path = "inbox_intent_shift_tests.rs"]
mod tests;
