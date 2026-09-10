//! 共享 think loop 引擎
//!
//! `run_think_loop` 是 awaken / sleep_and_settle / summary / intent_analyze
//! 共用的多轮思考循环，封装：超时控制 + 多轮迭代 + 工具调用分发 + 策略评估。

use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, ThinkResult};
use crate::models::events::ThinkRoundEvent;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeToolExecution};
use common::enums::ThinkingScene;
use common::error::{Result, err};
use std::sync::Arc;

use super::types::{RoundDigest, ThinkLoopResult};

// ==================== 策略映射 ====================

/// 构造单轮策略评估用的 Metrics（错误路径与轮末评估点共用）
fn round_metrics(
    round_number: usize,
    max_rounds: usize,
    elapsed_secs: u64,
    total_tokens: u64,
    context_tokens: u64,
    tool_call_counts: &std::collections::HashMap<String, usize>,
    llm_consecutive_errors: u64,
) -> crate::pkg::policy::Metrics {
    let mut metrics = crate::pkg::policy::Metrics::new()
        .with("round_number", round_number as u64)
        .with("max_rounds", max_rounds as u64)
        .with("elapsed_secs", elapsed_secs)
        .with("total_tokens", total_tokens)
        .with("context_tokens", context_tokens)
        .with("llm_consecutive_errors", llm_consecutive_errors);
    // 按工具名上报各自累计调用次数（NoProgressPolicy 按工具差异化检测）
    for (name, count) in tool_call_counts {
        metrics = metrics.with(&format!("tool_calls.{name}"), *count as u64);
    }
    metrics
}

/// 策略裁决：评估当前 Metrics，命中则映射为退出结果，未命中返回 None（继续循环）
///
/// think_loop 的统一退出裁决点：Final 完成 / 取消 / 溢出 / 预算耗尽等
/// 所有退出情况都经此分派，优先级 = 策略组声明顺序。
fn policy_exit(
    ctx: &crate::pkg::request_context::RequestContext,
    policy: Option<&dyn crate::pkg::policy::Policy>,
    metrics: &crate::pkg::policy::Metrics,
    messages: &[ChatMessage],
    round_number: usize,
    input_tokens: u64,
    final_content: Option<String>,
) -> Option<ThinkLoopResult> {
    let triggered = policy?.evaluate(metrics);
    if triggered.is_empty() {
        return None;
    }
    log_info!(
        ctx,
        "think_loop",
        "policy triggered: {:?} at round={}",
        triggered,
        round_number
    );
    Some(map_triggered_to_result(
        &triggered,
        messages.to_vec(),
        round_number,
        input_tokens,
        final_content,
    ))
}

/// 将命中的策略 id 列表映射为 ThinkLoopResult
///
/// 多个策略命中时按优先级取第一个匹配的——优先级 = `policy_set!`
/// 声明顺序（见 `build_policy_for_scene`）：
/// 用户取消 > Final 完成 > 上下文溢出 > 轮次上限 > 超时 > token 预算 > 无进展
///
/// 兜底返回 MaxRoundsExceeded（不应发生，防御性）。
pub(crate) fn map_triggered_to_result(
    triggered: &[String],
    messages: Vec<ChatMessage>,
    round_number: usize,
    input_tokens: u64,
    final_content: Option<String>,
) -> ThinkLoopResult {
    for id in triggered {
        match id.as_str() {
            "user_cancel" => {
                return ThinkLoopResult::Cancelled {
                    messages,
                    total_rounds: round_number,
                };
            }
            // 正常完成：final_answer 策略命中即正常退出，进入正常总结路径
            "final_answer" => {
                return ThinkLoopResult::Final {
                    content: final_content.unwrap_or_default(),
                    messages,
                };
            }
            "context_overflow" => {
                return ThinkLoopResult::ContextOverflow {
                    messages,
                    input_tokens,
                    rounds_used: round_number,
                };
            }
            // no_progress / token_budget 与轮次耗尽同路：进入调用方的总结退出流程，
            // 保证用户最终能收到一条兜底回复（而非静默失败）
            "token_budget" | "no_progress" | "max_rounds" | "timeout" => {
                return ThinkLoopResult::MaxRoundsExceeded {
                    messages,
                    total_rounds: round_number,
                };
            }
            _ => {}
        }
    }
    // 未知策略 id 兜底（不应发生）
    ThinkLoopResult::MaxRoundsExceeded {
        messages,
        total_rounds: round_number,
    }
}

/// 判断是否为「沉淀/压缩循环中递归触发沉淀」的工具调用
///
/// 处于 Settle / Compact 场景时，模型再调 `settle_memory` 即为自我递归，需拦截。
/// 其他场景（含 Awaken）不拦截——主循环里 Agent 主动触发一次沉淀是合法需求。
fn is_recursive_settle_call(scene: common::enums::ThinkingScene, tool_name: &str) -> bool {
    use common::enums::ThinkingScene;
    matches!(scene, ThinkingScene::Settle | ThinkingScene::Compact)
        && tool_name == super::compaction::RECURSIVE_SETTLE_TOOL
}

/// 进度快照中单条消息摘要的最大字符数
///
/// 兜底摘要是「信息保全」而非「信息复现」，超长的工具返回（搜索结果、文件全文）
/// 截断即可 —— 真要回看细节还有 trace 的 JSONL 原文。
const ROUND_DIGEST_MAX_CONTENT: usize = 800;

/// 连续工具调用轮数的疲劳提示阈值：达到后注入一条 System 提醒，逼模型收尾
///
/// 业界通用做法（OpenAI Assistants / LangChain max_iterations 同类机制）：
/// 模型连续多轮只调工具不给最终回复时，大概率已陷入循环，注入提醒给一次自纠机会。
const TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS: usize = 8;

/// LLM 连续失败重试预算：连续失败达到该次数即命中策略、终止循环
///
/// 预算内立即重试（无退避，YAGNI）；命中后错误照常向上传播，
/// 下游 abort_summary 走无 LLM 兜底存档（异常总结路径）。
const LLM_ERROR_RETRY_BUDGET: u64 = 3;

/// 按场景构造策略组（Or 关系：任一策略命中即退出循环）
///
/// **声明顺序即优先级**（evaluate 按声明顺序返回命中，map_triggered_to_result
/// 按列表顺序取首个可分派的）：
/// 用户取消 > Final 完成 > 上下文溢出 > 轮次上限 > 超时 > 无进展 > token 预算 > LLM 错误预算
///
/// - UserCancelPolicy：始终注入，由 AgentThinkRuntime.cancel_flag() 驱动
/// - FinalAnswerPolicy：模型输出 Final 即正常退出（正常完成也是策略裁决的一部分）
/// - ContextOverflowPolicy：上下文溢出（基于 ModelProvider 配置，0 = 未启用）；
///   优先于轮次/超时——溢出可恢复（沉淀压缩后重试），恢复动作更有价值
/// - MaxRoundsPolicy：轮次上限，所有场景均启用
/// - TimeoutPolicy：超时保护，所有场景均启用（0 = 不限制）
/// - NoProgressPolicy：单工具累计调用上限（0 = 不启用），防同工具反复调用死循环
/// - TokenBudgetPolicy：token 预算（0 = 不启用）
/// - ConsecutiveLlmErrorsPolicy：LLM 连续失败预算（run_think_loop 的错误路径评估）
pub(crate) fn build_policy_for_scene(
    agent: &Agent,
    _scene: ThinkingScene,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Box<dyn crate::pkg::policy::Policy> {
    use super::types::config_resolve;
    use crate::pkg::policy::builtin::{
        ConsecutiveLlmErrorsPolicy, ContextOverflowPolicy, FinalAnswerPolicy, MaxRoundsPolicy,
        NoProgressPolicy, TimeoutPolicy, TokenBudgetPolicy, UserCancelPolicy,
    };
    use crate::pkg::policy::policy_set;

    let max_rounds = config_resolve::max_thinking_rounds(agent);
    let timeout_secs = config_resolve::think_timeout_secs(agent);
    // 无进展检测数据源：每个工具自身的运行时配置（po.config.no_progress_max_calls），
    // 未配置该键的工具不参与限制；token 预算 try_get 兜底（测试环境可能未初始化全局配置）
    let tool_limits: std::collections::HashMap<String, usize> = agent
        .tools()
        .iter()
        .filter_map(|t| {
            t.po.config_no_progress_max_calls()
                .map(|limit| (t.po.name.clone(), limit))
        })
        .collect();
    let token_budget = crate::config::try_get()
        .map(|cfg| cfg.agent.token_budget)
        .unwrap_or(0);

    // 上下文压缩触发阈值（占最大上下文窗口的比例）：
    // 优先 recommended_context_length > max_context_length * 60% > 未启用
    const CONTEXT_OVERFLOW_RATIO: f64 = 0.6;
    let overflow_threshold = agent.brain.as_ref().and_then(|brain| {
        brain.model_provider().and_then(|po| {
            let config = po.config();
            if let Some(rec) = config.recommended_context_length
                && rec > 0
            {
                return Some(rec as u64);
            }
            config
                .max_context_length
                .filter(|&v| v > 0)
                .map(|v| (v as f64 * CONTEXT_OVERFLOW_RATIO) as u64)
        })
    });
    // 阈值同步进运行时内存（纯内存、不入库），供前端把上下文长度渲染成进度。
    // 与 ContextOverflowPolicy 同源，避免两处各算一遍导致展示与裁决口径不一致。
    AgentRuntimeStateManager::global()
        .record_context_length_threshold(&agent.po.id, overflow_threshold.unwrap_or(0));

    policy_set! {
        OR {
            UserCancelPolicy(cancel_flag),
            FinalAnswerPolicy(),
            ContextOverflowPolicy(overflow_threshold.unwrap_or(0)),
            MaxRoundsPolicy(max_rounds),
            TimeoutPolicy(timeout_secs),
            NoProgressPolicy(tool_limits),
            TokenBudgetPolicy(token_budget),
            ConsecutiveLlmErrorsPolicy(LLM_ERROR_RETRY_BUDGET),
        }
    }
}

// ==================== 共享 think loop ====================

impl RuntimeDomainImpl {
    /// 执行 think 循环（awaken/sleep_and_settle/summary 共用）
    ///
    /// 统一封装：超时控制 + 多轮迭代 + 工具调用分发。
    /// 每轮 think 后发布 ThinkRoundEvent（通过 AOP 同步转发）。
    ///
    /// # 退出条件（统一策略裁决）
    /// - `FinalAnswerPolicy` 命中（模型输出 Final）→ 正常退出，`ThinkLoopResult::Final`
    /// - 其他策略命中（取消/溢出/轮次/超时等）→ `map_triggered_to_result` 按声明顺序分派
    /// - LLM 调用失败：计入 `llm_consecutive_errors` 因子交由策略裁决——
    ///   预算内立即重试，`ConsecutiveLlmErrorsPolicy` 命中则传播错误
    ///   （下游 abort_summary 走无 LLM 兑底存档）
    ///
    /// 参数见 [`ThinkLoopParams`]：`start_round` 为本次循环的起始轮次编号（跨压缩累计），
    /// `max_rounds` 为总轮次上限（跨压缩累计二者详见调用方 awaken 的压缩循环）。
    ///
    /// `params.think_runtime` / `params.policy` 为可选的策略引擎接入点：
    /// - `think_runtime`：每轮上报运行时快照（供前端 cancel-thinking/runtime-status 查询）
    /// - `policy`：每轮评估策略（用户取消/轮次上限/超时），命中即退出循环
    ///
    /// 两者都为 None 时退化为旧行为（仅靠 max_rounds + timeout_secs 控制）。
    pub(crate) async fn run_think_loop(
        &self,
        params: super::types::ThinkLoopParams<'_>,
    ) -> Result<ThinkLoopResult> {
        // 解构入参：后续循环体按字段名使用，避免 params. 前缀噪声
        let super::types::ThinkLoopParams {
            ctx,
            agent,
            scene,
            trace_id,
            initial_messages,
            tool_descriptors,
            max_rounds,
            start_round,
            timeout_secs,
            think_runtime,
            policy,
            progress,
        } = params;

        // brain 统一在此解析：四个场景取的都是 agent.brain，
        // 不必让每个调用点各自判空一遍「大脑未唤醒」。
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_agent_brain()"))?;

        let think_future = async {
            let mut messages = initial_messages;
            // 提取模型提供商信息（所有轮次共用）
            let (model_provider_id, model_name) = match brain.model_provider() {
                Some(po) => (Some(po.id.clone()), Some(po.model_name.clone())),
                None => (None, None),
            };
            // 本次循环可用轮次 = max_rounds - start_round
            let available_rounds = max_rounds.saturating_sub(start_round);
            // 策略评估用的起始时间（用于 elapsed_secs）
            let loop_start = std::time::Instant::now();
            // 累计 token 用量（用于策略评估 + 运行时快照上报）
            let mut total_input_tokens: u64 = 0;
            let mut total_output_tokens: u64 = 0;
            let mut total_tool_calls: usize = 0;
            // 无进展检测状态：按工具名累计调用次数（模型会换参数绕过指纹，按名字计数更鲁棒）
            let mut tool_call_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            // 连续工具调用轮计数 + 疲劳提示是否已注入（只注入一次）
            let mut consecutive_tool_rounds: usize = 0;
            let mut nudge_injected = false;
            // LLM 连续失败计数（成功即清零；预算见 ConsecutiveLlmErrorsPolicy）
            let mut llm_consecutive_errors: u64 = 0;
            let scene_str = scene.as_str();
            for offset in 0..available_rounds {
                // 循环开始前先检查 cancel_flag（避免无意义地调用 LLM）
                if let Some(tr) = think_runtime
                    && tr.is_cancelled()
                {
                    log_info!(
                        &ctx,
                        "think_loop",
                        "cancel flag detected before round={}, exiting loop",
                        start_round + offset
                    );
                    return Ok(ThinkLoopResult::Cancelled {
                        messages,
                        total_rounds: start_round + offset,
                    });
                }

                let round = start_round + offset;
                let round_start = std::time::Instant::now();
                // LLM 调用 + 错误预算：失败不再直接传播，计入因子交由策略裁决——
                // 预算内立即重试（无退避，YAGNI）；策略命中则传播错误，
                // 下游 abort_summary 走无 LLM 兜底存档（异常总结路径）
                let result = loop {
                    match self
                        .brain_dal()
                        .think(ctx.clone(), brain, &messages, tool_descriptors)
                        .await
                    {
                        Ok(r) => {
                            llm_consecutive_errors = 0;
                            break r;
                        }
                        Err(e) => {
                            llm_consecutive_errors += 1;
                            let metrics = round_metrics(
                                round + 1,
                                max_rounds,
                                loop_start.elapsed().as_secs(),
                                total_input_tokens.saturating_add(total_output_tokens),
                                0,
                                &tool_call_counts,
                                llm_consecutive_errors,
                            );
                            if let Some(p) = policy
                                && !p.evaluate(&metrics).is_empty()
                            {
                                log_info!(
                                    &ctx,
                                    "think_loop",
                                    "llm error budget exhausted at round={}, propagating",
                                    round + 1
                                );
                                return Err(e);
                            }
                            log_info!(
                                &ctx,
                                "think_loop",
                                "llm call failed ({llm_consecutive_errors} consecutive), retrying"
                            );
                        }
                    }
                };
                let round_duration_ms = round_start.elapsed().as_millis() as u64;

                match result {
                    ThinkResult::Final { content, usage } => {
                        // 累计 token 用量
                        total_input_tokens = total_input_tokens.saturating_add(usage.input_tokens);
                        total_output_tokens =
                            total_output_tokens.saturating_add(usage.output_tokens);
                        // 上下文长度：本轮实际送进模型的 prompt token 数。
                        // 纯内存指标（不入统计表），每轮覆盖，用于观察上下文思考强度。
                        AgentRuntimeStateManager::global()
                            .record_context_length(&agent.po.id, usage.input_tokens);
                        // 上报最终轮运行时快照
                        if let Some(tr) = think_runtime {
                            tr.report_round(
                                trace_id,
                                scene,
                                round + 1,
                                max_rounds,
                                total_input_tokens,
                                total_output_tokens,
                                total_input_tokens.saturating_add(total_output_tokens),
                                total_tool_calls,
                            );
                            tr.finish();
                        }
                        // 发布 ThinkRoundEvent（无工具调用，最终轮）
                        let _ = crate::pkg::aop::publish(
                            &ctx,
                            ThinkRoundEvent::new(
                                &agent.po.id,
                                trace_id,
                                scene_str,
                                round,
                                round_duration_ms,
                                false,
                                0,
                            )
                            .with_model_usage(
                                model_provider_id.clone(),
                                model_name.clone(),
                                usage.input_tokens,
                                usage.output_tokens,
                                usage.total(),
                            )
                            .with_context(
                                ctx.organization_id().cloned(),
                                ctx.user_id().cloned(),
                                ctx.task_id().cloned(),
                                ctx.project_id().cloned(),
                            ),
                        )
                        .await;
                        // 统一评估点：Final 也经策略裁决（final_answer 命中 → 正常退出；
                        // 若同轮取消/溢出等也命中，按声明顺序优先级分派）
                        let metrics = round_metrics(
                            round + 1,
                            max_rounds,
                            loop_start.elapsed().as_secs(),
                            total_input_tokens.saturating_add(total_output_tokens),
                            usage.input_tokens,
                            &tool_call_counts,
                            llm_consecutive_errors,
                        )
                        .with("output_kind", "final");
                        if let Some(exit) = policy_exit(
                            &ctx,
                            policy,
                            &metrics,
                            &messages,
                            round + 1,
                            usage.input_tokens,
                            Some(content.clone()),
                        ) {
                            return Ok(exit);
                        }
                        // fail-safe：自定义策略组未含 final_answer 时保持原行为
                        return Ok(ThinkLoopResult::Final { content, messages });
                    }
                    ThinkResult::ToolCall {
                        content,
                        tool_calls,
                        usage,
                    } => {
                        let tc_count = tool_calls.len();
                        total_tool_calls = total_tool_calls.saturating_add(tc_count);
                        total_input_tokens = total_input_tokens.saturating_add(usage.input_tokens);
                        total_output_tokens =
                            total_output_tokens.saturating_add(usage.output_tokens);
                        // 上下文长度：本轮实际送进模型的 prompt token 数（同上，每轮覆盖）
                        AgentRuntimeStateManager::global()
                            .record_context_length(&agent.po.id, usage.input_tokens);
                        consecutive_tool_rounds = consecutive_tool_rounds.saturating_add(1);
                        // 按工具名累计调用次数（无进展检测数据源）
                        for tc in &tool_calls {
                            *tool_call_counts.entry(tc.name.clone()).or_insert(0) += 1;
                        }
                        // 进度快照：记录本轮起点与本轮模型输出，稍后据此截取本轮新增消息。
                        // 必须在 push 之前取，push 会 move 掉 content / tool_calls。
                        let round_start_idx = messages.len();
                        let digest_assistant_text = content.clone();
                        let mut round_tool_names: Vec<String> = Vec::with_capacity(tc_count);
                        // 追加助手消息（含 tool_calls），让模型在下一轮看到自己发起的调用
                        messages.push(ChatMessage::Assistant {
                            content,
                            tool_calls: Some(tool_calls.clone()),
                        });
                        // 按 control_mode 分发执行（D26 入口统一：Auto → call_tool
                        // 协议路由（含凭据编排，Auto-MCP 亦可执行）；Manual →
                        // dispatch_manual_tool 特殊工具转发）
                        for tc in tool_calls {
                            // 沉淀/压缩场景：拦截对 settle_memory 的递归调用。
                            //
                            // `settle_memory` 会直接触发一整套 sleep_and_settle，在沉淀或压缩
                            // 循环里再调它属于自我递归（一次压缩可能放大成 N 次完整沉淀）。
                            // 这里选择「运行时拦截」而非收窄工具白名单：工具仍对其他场景可见，
                            // 且拦截时能即时给模型一句可理解的反馈，比静默过滤更容易让它改道。
                            if is_recursive_settle_call(scene, &tc.name) {
                                log_info!(
                                    &ctx,
                                    "think_loop",
                                    "blocked recursive {} call in scene={}",
                                    tc.name,
                                    scene_str
                                );
                                messages.push(ChatMessage::tool(
                                    tc.id,
                                    "你当前已经在沉淀/压缩流程中，无需再调用 settle_memory —— \
                                     重复调用会嵌套触发新的沉淀循环。\
                                     请直接继续当前任务（写入短期记忆后输出 Final 文本结束）。"
                                        .to_string(),
                                ));
                                continue;
                            }
                            // 进度快照统计的是「尝试过哪些工具」：执行失败与工具不存在
                            // 同样要计入，它们对判断中断前的进展很关键。
                            round_tool_names.push(tc.name.clone());
                            match agent.tools().iter().find(|t| t.po.name == tc.name) {
                                Some(tool) => {
                                    let call_result = match tool.po.control_mode {
                                        common::enums::tool::ControlMode::Auto => {
                                            self.call_tool(ctx.clone(), tool, tc.arguments).await
                                        }
                                        common::enums::tool::ControlMode::Manual => {
                                            self.dispatch_manual_tool(
                                                ctx.clone(),
                                                tool,
                                                tc.arguments,
                                            )
                                            .await
                                        }
                                    };
                                    match call_result {
                                        Ok(result) => {
                                            messages.push(ChatMessage::tool(
                                                tc.id,
                                                format!("{}", result.result),
                                            ));
                                        }
                                        Err(e) => {
                                            messages.push(ChatMessage::tool(
                                                tc.id,
                                                format!("Error: {}", e),
                                            ));
                                        }
                                    }
                                }
                                None => {
                                    messages.push(ChatMessage::tool(
                                        tc.id,
                                        format!("Error: tool {} not found", tc.name),
                                    ));
                                }
                            }
                        }

                        // 记录本轮进度到快照。
                        //
                        // 位置刻意选在「工具执行完、策略评估前」：上下文溢出检测与策略
                        // 命中（轮次耗尽 / token 预算 / 无进展）都会立即 return，
                        // 放在这里才能保证每一轮已完成的工具调用都不漏记。
                        //
                        // 下一轮 `brain_dal().think()` 失败（429 限流等）时本轮已落袋，
                        // 调用方才能凭快照生成兜底摘要。
                        if let Some(progress) = progress {
                            progress.record_round(RoundDigest {
                                round: round + 1,
                                assistant_text: digest_assistant_text,
                                tool_names: round_tool_names,
                                transcript: messages[round_start_idx..]
                                    .iter()
                                    .map(|m| m.to_summary_text(ROUND_DIGEST_MAX_CONTENT))
                                    .collect(),
                            });
                        }

                        // 发布 ThinkRoundEvent（有工具调用）
                        let _ = crate::pkg::aop::publish(
                            &ctx,
                            ThinkRoundEvent::new(
                                &agent.po.id,
                                trace_id,
                                scene_str,
                                round,
                                round_duration_ms,
                                true,
                                tc_count,
                            )
                            .with_model_usage(
                                model_provider_id.clone(),
                                model_name.clone(),
                                usage.input_tokens,
                                usage.output_tokens,
                                usage.total(),
                            )
                            .with_context(
                                ctx.organization_id().cloned(),
                                ctx.user_id().cloned(),
                                ctx.task_id().cloned(),
                                ctx.project_id().cloned(),
                            ),
                        )
                        .await;

                        // 疲劳提示：连续多轮只调工具不给最终回复时，注入 System 提醒给模型一次自纠机会
                        // （只注入一次，避免 System 消息堆积）
                        if consecutive_tool_rounds >= TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS
                            && !nudge_injected
                        {
                            log_info!(
                                &ctx,
                                "think_loop",
                                "no final response for {} consecutive tool rounds, injecting nudge",
                                consecutive_tool_rounds
                            );
                            messages.push(ChatMessage::system(format!(
                                "【系统提醒】你已经连续 {} 轮调用工具但尚未给出最终回复。请立即评估当前进度：\n\
                                 1. 如果已有足够信息，直接输出最终文本回复用户，不要再调用任何工具；\n\
                                 2. 如果任务确实无法完成（如检索始终无结果），直接向用户坦诚说明情况并停止；\n\
                                 3. 不要再继续重复或无意义的工具调用。",
                                consecutive_tool_rounds
                            )));
                            nudge_injected = true;
                        }

                        // 策略评估 + 运行时快照上报
                        let total_tokens = total_input_tokens.saturating_add(total_output_tokens);
                        let elapsed_secs = loop_start.elapsed().as_secs();

                        // 上报运行时快照（每轮都更新，供前端实时查询）
                        if let Some(tr) = think_runtime {
                            tr.report_round(
                                trace_id,
                                scene,
                                round + 1,
                                max_rounds,
                                total_input_tokens,
                                total_output_tokens,
                                total_tokens,
                                total_tool_calls,
                            );
                        }

                        // 统一评估点：任一策略命中即退出循环（含 ContextOverflowPolicy 收编
                        // 原内联溢出检测；优先级 = build_policy_for_scene 声明顺序）
                        let metrics = round_metrics(
                            round + 1,
                            max_rounds,
                            elapsed_secs,
                            total_tokens,
                            usage.input_tokens,
                            &tool_call_counts,
                            llm_consecutive_errors,
                        )
                        .with("output_kind", "tool_calls");
                        if let Some(exit) = policy_exit(
                            &ctx,
                            policy,
                            &metrics,
                            &messages,
                            round + 1,
                            usage.input_tokens,
                            None,
                        ) {
                            return Ok(exit);
                        }
                    }
                }
            }
            // 循环耗尽所有可用轮次，未得到 Final 回答
            Ok(ThinkLoopResult::MaxRoundsExceeded {
                messages,
                total_rounds: max_rounds,
            })
        };

        // timeout_secs = 0 → 不限制；非 0 → 超时保护
        if timeout_secs == 0 {
            think_future.await
        } else {
            match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), think_future)
                .await
            {
                Ok(inner) => inner,
                Err(_elapsed) => Err(err!(
                    Internal,
                    "brain think timeout after {}s",
                    timeout_secs
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::ThinkingScene;

    #[test]
    fn map_final_answer_returns_final_with_content() {
        let r = map_triggered_to_result(
            &["final_answer".to_string()],
            Vec::new(),
            3,
            100,
            Some("done".to_string()),
        );
        assert!(matches!(r, ThinkLoopResult::Final { content, .. } if content == "done"));
    }

    #[test]
    fn map_priority_follows_triggered_order() {
        // 声明顺序即优先级：user_cancel 先于 final_answer → 取消胜出
        let r = map_triggered_to_result(
            &["user_cancel".to_string(), "final_answer".to_string()],
            Vec::new(),
            3,
            100,
            Some("done".to_string()),
        );
        assert!(matches!(r, ThinkLoopResult::Cancelled { .. }));

        // 同理：溢出优先于轮次上限
        let r = map_triggered_to_result(
            &["context_overflow".to_string(), "max_rounds".to_string()],
            Vec::new(),
            5,
            9000,
            None,
        );
        assert!(matches!(r, ThinkLoopResult::ContextOverflow { .. }));
    }

    #[test]
    fn map_unknown_id_falls_back_to_max_rounds() {
        let r = map_triggered_to_result(&["no_such_policy".to_string()], Vec::new(), 2, 0, None);
        assert!(matches!(r, ThinkLoopResult::MaxRoundsExceeded { .. }));
    }

    #[test]
    fn recursive_settle_blocked_in_settle_and_compact() {
        // 沉淀/压缩流程中再调 settle_memory 属自我递归，必须拦截
        assert!(is_recursive_settle_call(
            ThinkingScene::Settle,
            "settle_memory"
        ));
        assert!(is_recursive_settle_call(
            ThinkingScene::Compact,
            "settle_memory"
        ));
    }

    #[test]
    fn recursive_settle_guard_does_not_affect_other_cases() {
        // 主循环里 Agent 主动触发一次沉淀是合法需求，不拦
        assert!(!is_recursive_settle_call(
            ThinkingScene::Awaken,
            "settle_memory"
        ));
        // 其他工具在任何场景都不受影响
        assert!(!is_recursive_settle_call(
            ThinkingScene::Compact,
            "save_short_term_memory"
        ));
        assert!(!is_recursive_settle_call(
            ThinkingScene::Settle,
            "search_memory"
        ));
    }
}
