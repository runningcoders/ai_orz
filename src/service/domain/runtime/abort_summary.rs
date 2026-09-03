//! 思考循环异常中断的兜底总结
//!
//! # 为什么需要它
//!
//! 主循环的总结能力（`compact_context`）本质上是**再发起一次 LLM 调用**让模型自己归纳。
//! 但循环之所以异常退出，往往正是因为 LLM 调不通 —— 429 限流、5xx、超时。此时再发起
//! 一次压缩调用，大概率同样失败，于是本轮已经做完的工作彻底蒸发：短期记忆里没有，
//! trace 里也没有，用户只收到一句「模型服务请求过于频繁」。
//!
//! 更糟的是 `messages` 只活在 `Ok(ThinkLoopResult::*)` 里。`?` 一传播（429 等）就被 drop，
//! `tokio::time::timeout` 掐断 future 时更是连函数都出不去。
//!
//! 因此框架需要一条**完全不依赖 LLM** 的兜底路径：
//!
//! ```text
//! 用户原始消息 + 每轮进度快照（ThinkLoopProgress）+ 真实错误原因
//!     → 规则化拼接 → 短期记忆
//! ```
//!
//! 它不追求文采，只保证**信息不丢**：下次这个 Agent 被唤醒时，能从记忆里看到
//! 「上次做到哪一步、为什么断了」，接着做而不是从零开始。
//!
//! # 边界
//!
//! - **不改运行时状态**（不 set_resting / set_idle）：与 `compact_context` 一致，
//!   awaken 的 `BusyGuard` 负责收尾。
//! - **不改错误语义**：兜底只做「存档」，失败照常向上抛，重试/通知决策仍在消费者侧。
//! - **全链路尽力而为**：任何一步失败都只记 warn，绝不把兜底失败变成新的错误。

use super::types::{ProgressSnapshot, RoundDigest, ThinkLoopProgress};
use crate::models::agent::Agent;
use crate::models::memory::{MemoryCreateParams, MemoryTrace, ShortTermMemoryIndexPo};
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{RuntimeDomain, RuntimeDomainImpl};
use common::enums::MemoryStatus;
use common::error::Error;
use std::borrow::Cow;

/// 框架落库时给中断存档打的标签（便于识别/统计这类记忆，与 `compaction` 区分）
const ABORT_TAG: &str = "loop_abort";

/// 错误 field 中存放「给用户的一句话进度概览」的 key
///
/// 消费者侧（`MessageConsumer::notify_agent_failure`）据此把进度附在失败通知里，
/// 让用户知道工作没白做。放在错误上传递，是为了不为此新增返回值字段。
pub const ABORT_NOTICE_FIELD: &str = "abort_notice";

/// 兜底摘要的字符预算
///
/// 与 `settle_memory::DEFAULT_PENDING_BUDGET_CHARS` 同量级。短期记忆最终要进 Prompt，
/// 无上限的存档会把下一轮上下文直接撑爆 —— 宁可截断，也不能劣化模型表现。
const SUMMARY_BUDGET_CHARS: usize = 12_000;

/// 用户原始消息的截断上限
const USER_MESSAGE_MAX_CHARS: usize = 4_000;

/// 错误详情的截断上限（provider 的原始报错体可能极长）
const ERROR_MESSAGE_MAX_CHARS: usize = 1_000;

// ==================== 中断类型 ====================

/// 循环非正常退出的两种情形
///
/// 两者都不走 `compact_context`，但性质不同：一个是「失败了」，一个是「人让停的」。
/// 摘要里分开表述，避免下次 Agent 读到"被取消"误判成故障。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AbortKind {
    /// 循环执行失败（模型错误 / 超时 / 内部异常）
    Error,
    /// 用户主动取消
    Cancelled,
}

impl AbortKind {
    /// 摘要标题措辞
    fn title(self) -> &'static str {
        match self {
            AbortKind::Error => "中断的任务",
            AbortKind::Cancelled => "被取消的任务",
        }
    }

    /// 写进 trace metadata 与记忆 tag 的短标识
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            AbortKind::Error => "error",
            AbortKind::Cancelled => "cancelled",
        }
    }
}

// ==================== 摘要构造 ====================

/// 构造兜底摘要所需的全部信息
pub(crate) struct AbortContext<'a> {
    /// 中断类型
    pub kind: AbortKind,
    /// 真实错误原因（Cancelled 时为 None）
    pub error: Option<&'a Error>,
    /// 用户原始消息内容
    pub user_message: &'a str,
    /// 循环进度快照
    pub progress: &'a ProgressSnapshot,
    /// 已消耗的轮次（跨压缩累计，可能大于快照里的轮次数）
    pub total_rounds: usize,
}

/// 兜底摘要的两种产物
#[derive(Debug, Clone)]
pub(crate) struct AbortSummary {
    /// 入库全文：结构化存档，供下次唤醒检索/阅读
    pub detail: String,
    /// 给用户的一句话进度概览（附在失败通知里）
    pub brief: String,
}

/// 按字符预算截断（保字符边界，尾部标注原始长度）
fn truncate_chars(s: &str, max: usize) -> Cow<'_, str> {
    if s.chars().count() <= max {
        return Cow::Borrowed(s);
    }
    let end = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
    Cow::Owned(format!(
        "{}…（已截断，原文 {} 字符）",
        &s[..end],
        s.chars().count()
    ))
}

/// 从最近一轮往前挑，直到用尽预算
///
/// 保留**最近**的轮次而非最早的：对接续更有价值的是"最后一刻做到哪了"，
/// 开头部分即便被裁掉，用户诉求也在摘要开头单独成段，不会丢。
///
/// 至少保留一轮，避免预算极小时产出空进度。
fn pick_rounds_within_budget(rounds: &[RoundDigest], budget: usize) -> (Vec<&RoundDigest>, usize) {
    let mut kept: Vec<&RoundDigest> = Vec::new();
    let mut used = 0usize;
    let mut omitted = 0usize;
    for rd in rounds.iter().rev() {
        let size: usize = rd.transcript.iter().map(|l| l.chars().count() + 1).sum();
        if !kept.is_empty() && used.saturating_add(size) > budget {
            omitted += 1;
            continue;
        }
        used = used.saturating_add(size);
        kept.push(rd);
    }
    kept.reverse();
    (kept, omitted)
}

/// 规则化拼出兜底摘要（纯函数，不触碰任何外部状态）
pub(crate) fn build_abort_summary(input: &AbortContext<'_>) -> AbortSummary {
    let rounds_used = input.progress.rounds_used();
    let tool_calls = input.progress.total_tool_calls;

    let mut s = String::new();

    // 标题：先说清楚这不是一份正常的工作总结
    s.push_str(&format!(
        "【{}】本轮工作未完成，以下是中断时刻的进度存档。\n\n",
        input.kind.title()
    ));

    // 1. 用户原始诉求 —— 单独成段，保证任何情况下都不被预算裁掉
    let user_message = input.user_message.trim();
    s.push_str("## 用户原始消息\n\n");
    if user_message.is_empty() {
        s.push_str("（空消息）");
    } else {
        s.push_str(&truncate_chars(user_message, USER_MESSAGE_MAX_CHARS));
    }
    s.push_str("\n\n");

    // 2. 真实中断原因 —— 错误码 + 原始消息，不吞任何信息
    s.push_str("## 中断原因\n\n");
    match input.error {
        Some(e) => {
            s.push_str(&format!(
                "[{}] {}",
                e.code(),
                truncate_chars(&e.msg, ERROR_MESSAGE_MAX_CHARS)
            ));
        }
        None => match input.kind {
            AbortKind::Error => s.push_str("（未知错误）"),
            AbortKind::Cancelled => s.push_str("用户在思考过程中主动取消了本次任务。"),
        },
    }
    s.push_str("\n\n");

    // 3. 已完成进度
    s.push_str(&format!(
        "## 已完成进度（{} 轮思考 / {} 次工具调用）\n\n",
        input.total_rounds.max(rounds_used),
        tool_calls
    ));
    if rounds_used == 0 {
        s.push_str("尚未产生任何有效轮次：模型在第一轮调用时即失败。\n\n");
    } else {
        let (kept, omitted) =
            pick_rounds_within_budget(&input.progress.rounds, SUMMARY_BUDGET_CHARS);
        for rd in &kept {
            s.push_str(&format!("### 第 {} 轮\n", rd.round));
            if rd.transcript.is_empty() {
                s.push_str("（本轮无文本产出）\n");
            } else {
                for line in &rd.transcript {
                    s.push_str(line);
                    s.push('\n');
                }
            }
            s.push('\n');
        }
        if omitted > 0 {
            s.push_str(&format!(
                "（受字符预算限制，已省略最早的 {omitted} 轮）\n\n"
            ));
        }
    }

    // 4. 工具调用统计
    let tool_stats = input.progress.tool_call_stats();
    if !tool_stats.is_empty() {
        s.push_str("## 工具调用统计\n\n");
        s.push_str(&tool_stats.join("、"));
        s.push_str("\n\n");
    }

    // 5. 中断前模型最后的话 —— 常常就是它打算接下来做什么
    if let Some(last) = input.progress.last_assistant_text() {
        s.push_str("## 中断前模型的最后输出\n\n");
        s.push_str(&truncate_chars(last.trim(), 2_000));
        s.push_str("\n\n");
    }

    // 6. 给下次自己的指引
    s.push_str("## 后续注意\n\n");
    s.push_str(
        "用户的原始诉求**尚未得到回复**。再次接到相关任务时，请先核对上述进度，\
         直接接着做，不要重复已完成的工作；若失败原因已解除，可从中断处继续。",
    );

    AbortSummary {
        detail: s,
        brief: build_brief(input.kind, rounds_used, tool_calls, input.error),
    }
}

/// 给用户的一句话进度概览
///
/// 只说「做了多少、存没存下」，不铺细节 —— 用户要的是安抚与可行动性，不是日志。
fn build_brief(
    kind: AbortKind,
    rounds_used: usize,
    tool_calls: usize,
    error: Option<&Error>,
) -> String {
    if rounds_used == 0 && tool_calls == 0 {
        return match kind {
            AbortKind::Error => "本次任务尚未产生有效进展，未留存中途结果。".to_string(),
            AbortKind::Cancelled => "任务在开始执行前即被取消。".to_string(),
        };
    }
    let head = format!("已完成 {rounds_used} 轮思考、{tool_calls} 次工具调用");
    match kind {
        AbortKind::Error => {
            // 限流/服务端错误是可以稍后重试的，明确告诉用户工作没白做
            let retry_hint = if error.is_some_and(|e| e.is_retryable()) {
                "，稍后重试时可从中断处继续"
            } else {
                ""
            };
            format!("{head}，中断前的进度已记入记忆{retry_hint}。")
        }
        AbortKind::Cancelled => format!("{head}，已完成的进度已记入记忆。"),
    }
}

// ==================== 落库与收尾 ====================

/// `finalize_abort` 的入参
pub(crate) struct AbortFinalizeParams<'a> {
    /// 请求上下文
    pub ctx: RequestContext,
    /// 执行思考的 Agent
    pub agent: &'a Agent,
    /// 调用方预生成的 trace（已含 id / agent_id / user_id 等，此处只补中断相关信息）
    pub trace: MemoryTrace,
    /// 本轮思考使用的完整 Prompt（写入 trace.input）
    pub prompt: &'a str,
    /// 用户原始消息内容
    pub user_message: &'a str,
    /// 触发本次思考的消息 id（写进 trace metadata，便于按消息反查）
    pub message_id: &'a str,
    /// 进度快照
    pub progress: &'a ThinkLoopProgress,
    /// 已消耗轮次（跨压缩累计）
    pub total_rounds: usize,
    /// 本段工作涉及的 trace id 列表（写入记忆的 trace_ids）
    pub trace_ids: &'a [String],
    /// 中断类型
    pub kind: AbortKind,
    /// 真实错误（Cancelled 时为 None）
    pub error: Option<&'a Error>,
}

/// 把「给用户的一句话进度概览」挂到错误上
///
/// 错误语义不变（code / msg / error_type 原样保留），只在 `field.extra` 里追加一个
/// `abort_notice` key。消费者侧（`notify_agent_failure`）据此在失败通知里告知用户
/// 「工作没白做」。放在错误上传递，是为不为此新增返回值字段。
///
/// 若错误已携带 field 则就地扩展（不覆盖已有 extra 内容）；否则新建。
pub(crate) fn attach_abort_notice(mut error: Error, brief: String) -> Error {
    let mut field = match error.field {
        Some(b) => *b,
        None => common::error::ErrorField::new(),
    };
    field.extra.insert(
        ABORT_NOTICE_FIELD.to_string(),
        serde_json::Value::String(brief),
    );
    error.field = Some(Box::new(field));
    error
}

/// 兜底收尾的产出
#[derive(Debug, Clone, Default)]
pub(crate) struct AbortOutcome {
    /// 落库的兜底记忆 id（None = 落库失败，工作未能留存）
    pub memory_id: Option<String>,
    /// 给用户的一句话进度概览
    pub brief: String,
}

impl RuntimeDomainImpl {
    /// 异常中断收尾：写 trace + 落库兜底短期记忆
    ///
    /// # 与 `compact_context` 的分工
    ///
    /// | | `compact_context` | 本方法 |
    /// |---|---|---|
    /// | 摘要来源 | LLM 自己归纳 | 规则化拼接 |
    /// | 需要模型可用 | 是 | **否** |
    /// | 触发时机 | 正常退出 / 上下文溢出 | 异常中断 |
    ///
    /// 两者写入同一张表（`short_term_memory_index`），靠 `tags` 区分：
    /// 压缩记忆是 `["compaction"]`，中断存档是 `["loop_abort", "error"|"cancelled"]`。
    ///
    /// # 失败语义
    ///
    /// 全流程尽力而为：trace 写失败、记忆落库失败都只记 warn，返回 `memory_id: None`。
    /// 兜底本身绝不能成为新的失败源。
    pub(crate) async fn finalize_abort(&self, params: AbortFinalizeParams<'_>) -> AbortOutcome {
        let AbortFinalizeParams {
            ctx,
            agent,
            mut trace,
            prompt,
            user_message,
            message_id,
            progress,
            total_rounds,
            trace_ids,
            kind,
            error,
        } = params;

        let snapshot = progress.snapshot();
        let summary = build_abort_summary(&AbortContext {
            kind,
            error,
            user_message,
            progress: &snapshot,
            total_rounds,
        });

        // ---- 1. 落库短期记忆 ----
        let memory_id = self
            .persist_abort_memory(
                ctx.clone(),
                &agent.po.id,
                &summary.detail,
                kind,
                trace_ids,
                &trace.id,
            )
            .await;

        // ---- 2. 写 trace（中断态：output 保持 None，语义即"没有产出"）----
        trace.input = prompt.to_string();
        trace.metadata.insert("scene".into(), "awaken".into());
        trace
            .metadata
            .insert("message_id".into(), message_id.to_string());
        trace
            .metadata
            .insert("exit_reason".into(), kind.as_str().into());
        trace
            .metadata
            .insert("rounds_used".into(), total_rounds.to_string());
        if let Some(e) = error {
            trace.metadata.insert("error_code".into(), e.code().into());
            trace.metadata.insert(
                "error_message".into(),
                truncate_chars(&e.msg, ERROR_MESSAGE_MAX_CHARS).into_owned(),
            );
        }
        if let Some(id) = &memory_id {
            trace.metadata.insert("abort_memory_id".into(), id.clone());
        }
        if let Some(task_id) = ctx.task_id() {
            trace.metadata.insert("task_id".into(), task_id.clone());
        }
        if let Some(project_id) = ctx.project_id() {
            trace
                .metadata
                .insert("project_id".into(), project_id.clone());
        }
        if let Err(e) = self.memory().write_thinking_trace(ctx.clone(), trace).await {
            crate::log_warn!(
                &ctx,
                "abort",
                agent_id = %agent.po.id,
                error = ?e,
                "中断兜底：写入思考 trace 失败（记忆已单独尝试落库）"
            );
        }

        match &memory_id {
            Some(id) => crate::log_info!(
                &ctx,
                "abort",
                "agent_id={}, kind={}, rounds={}, 中断兜底记忆已落库 memory_id={}",
                agent.po.id,
                kind.as_str(),
                total_rounds,
                id
            ),
            None => crate::log_warn!(
                &ctx,
                "abort",
                agent_id = %agent.po.id,
                kind = %kind.as_str(),
                "中断兜底记忆落库失败，本轮工作未能留存"
            ),
        }

        AbortOutcome {
            memory_id,
            brief: summary.brief,
        }
    }

    /// 把中断存档写入短期记忆
    ///
    /// 不复用 `compaction::persist_compacted_memory`：两者 `tags` 与失败语义不同，
    /// 且中断存档后续还要挂告警/重试提示，各自内聚比共用参数更易演化。
    ///
    /// 失败只记 warn 返回 None：兜底不该阻断主流程。
    async fn persist_abort_memory(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        summary: &str,
        kind: AbortKind,
        trace_ids: &[String],
        parent_trace_id: &str,
    ) -> Option<String> {
        let now = common::constants::utils::current_timestamp_ms();
        let memory_id = uuid::Uuid::now_v7().to_string();
        let tags = vec![ABORT_TAG.to_string(), kind.as_str().to_string()];
        let index = ShortTermMemoryIndexPo {
            id: memory_id.clone(),
            agent_id: agent_id.to_string(),
            task_id: ctx.task_id().cloned(),
            role: "assistant".to_string(),
            summary: summary.to_string(),
            tags: serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string()),
            trace_ids: serde_json::to_string(trace_ids).unwrap_or_else(|_| "[]".to_string()),
            status: MemoryStatus::Active,
            created_at: now,
            updated_at: now,
        };

        match self
            .memory()
            .create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index))
            .await
        {
            Ok(_) => Some(memory_id),
            Err(e) => {
                crate::log_warn!(
                    &ctx,
                    "abort",
                    parent_trace_id = %parent_trace_id,
                    error = ?e,
                    "框架写入中断兜底记忆失败"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::domain::runtime::types::ThinkLoopProgress;

    fn progress_with(rounds: &[(&str, &[&str])]) -> ThinkLoopProgress {
        let p = ThinkLoopProgress::new();
        for (i, (text, tools)) in rounds.iter().enumerate() {
            p.record_round(RoundDigest {
                round: i + 1,
                assistant_text: Some((*text).to_string()),
                tool_names: tools.iter().map(|t| (*t).to_string()).collect(),
                transcript: vec![format!("[assistant] {text}")],
            });
        }
        p
    }

    #[test]
    fn abort_summary_keeps_user_message_and_error_code() {
        let p = progress_with(&[("先查一下记忆", &["search_memory"])]);
        let snapshot = p.snapshot();
        let err = Error::new(
            common::error::ErrorCode::ModelRateLimited,
            "chat completions rate limited (429): quota exceeded",
        );
        let out = build_abort_summary(&AbortContext {
            kind: AbortKind::Error,
            error: Some(&err),
            user_message: "帮我把上周的方案整理成一页纸",
            progress: &snapshot,
            total_rounds: 2,
        });

        assert!(out.detail.contains("帮我把上周的方案整理成一页纸"));
        assert!(out.detail.contains("model_rate_limited"));
        assert!(out.detail.contains("quota exceeded"));
        assert!(out.detail.contains("第 1 轮"));
        assert!(out.detail.contains("search_memory x1"));
        assert!(out.detail.contains("尚未得到回复"));
    }

    #[test]
    fn abort_summary_handles_zero_progress() {
        let p = ThinkLoopProgress::new();
        let snapshot = p.snapshot();
        let err = Error::new(
            common::error::ErrorCode::ModelRateLimited,
            "rate limited (429)",
        );
        let out = build_abort_summary(&AbortContext {
            kind: AbortKind::Error,
            error: Some(&err),
            user_message: "你好",
            progress: &snapshot,
            total_rounds: 0,
        });

        assert!(out.detail.contains("尚未产生任何有效轮次"));
        assert_eq!(out.brief, "本次任务尚未产生有效进展，未留存中途结果。");
    }

    #[test]
    fn brief_mentions_retryable_for_rate_limit() {
        let p = progress_with(&[("开工", &["read_file"]), ("继续", &["read_file"])]);
        let snapshot = p.snapshot();
        let err = Error::new(common::error::ErrorCode::ModelRateLimited, "429");
        let out = build_abort_summary(&AbortContext {
            kind: AbortKind::Error,
            error: Some(&err),
            user_message: "读文件",
            progress: &snapshot,
            total_rounds: 2,
        });
        assert!(out.brief.contains("2 轮思考"));
        assert!(out.brief.contains("2 次工具调用"));
        assert!(out.brief.contains("稍后重试"));
    }

    #[test]
    fn cancelled_kind_reads_differently_from_error() {
        let p = progress_with(&[("做到一半", &["search_memory"])]);
        let snapshot = p.snapshot();
        let out = build_abort_summary(&AbortContext {
            kind: AbortKind::Cancelled,
            error: None,
            user_message: "继续跑",
            progress: &snapshot,
            total_rounds: 1,
        });
        assert!(out.detail.contains("被取消的任务"));
        assert!(out.detail.contains("主动取消"));
        assert!(!out.brief.contains("稍后重试"));
    }

    #[test]
    fn budget_drops_oldest_rounds_and_says_so() {
        // 每轮 3000 字符，预算 12000 → 只能留下最近 4 轮中的若干轮
        let p = ThinkLoopProgress::new();
        for i in 0..6 {
            p.record_round(RoundDigest {
                round: i + 1,
                assistant_text: Some("x".to_string()),
                tool_names: vec![],
                transcript: vec!["y".repeat(3_000)],
            });
        }
        let snapshot = p.snapshot();
        let out = build_abort_summary(&AbortContext {
            kind: AbortKind::Error,
            error: None,
            user_message: "跑批",
            progress: &snapshot,
            total_rounds: 6,
        });
        assert!(out.detail.contains("已省略最早的"));
        // 最近一轮必须保留
        assert!(out.detail.contains("### 第 6 轮"));
    }

    #[test]
    fn truncate_keeps_char_boundary() {
        let s = "中文字符串截断测试";
        assert_eq!(truncate_chars(s, 100), s);
        let out = truncate_chars(s, 3);
        assert!(out.starts_with("中文字"));
        assert!(out.contains("已截断"));
    }
}
