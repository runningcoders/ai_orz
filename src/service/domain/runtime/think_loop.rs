//! 共享 think loop 引擎
//!
//! `run_think_loop` 是 awaken / sleep_and_settle / summary / intent_analyze
//! 共用的多轮思考循环，封装：超时控制 + 多轮迭代 + 工具调用分发 + 策略评估。

use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, ThinkResult, ToolDescriptor};
use crate::models::events::ThinkRoundEvent;
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeToolExecution};
use common::enums::ThinkingScene;
use common::error::{Result, err};
use std::sync::Arc;

use super::types::ThinkLoopResult;

// ==================== 策略映射 ====================

/// 将命中的策略 id 列表映射为 ThinkLoopResult
///
/// 多个策略命中时按优先级取第一个匹配的：
/// 用户取消 > 上下文溢出 > token 预算 > 轮次上限 > 超时
///
/// 兜底返回 MaxRoundsExceeded（不应发生，防御性）。
pub(crate) fn map_triggered_to_result(
    triggered: &[String],
    messages: Vec<ChatMessage>,
    round_number: usize,
    input_tokens: u64,
) -> ThinkLoopResult {
    for id in triggered {
        match id.as_str() {
            "user_cancel" => {
                return ThinkLoopResult::Cancelled {
                    messages,
                    total_rounds: round_number,
                };
            }
            "context_overflow" => {
                return ThinkLoopResult::ContextOverflow {
                    messages,
                    input_tokens,
                    rounds_used: round_number,
                };
            }
            "token_budget" | "max_rounds" | "timeout" => {
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

/// 按场景构造策略组（Or 关系：任一策略命中即退出循环）
///
/// 内置策略：
/// - UserCancelPolicy：始终注入，由 AgentThinkRuntime.cancel_flag() 驱动
/// - MaxRoundsPolicy：轮次上限，所有场景均启用
/// - TimeoutPolicy：超时保护，所有场景均启用（0 = 不限制）
///
/// 注意：ContextOverflowPolicy 暂不在此处使用，因为 run_think_loop 已有
/// 独立的上下文溢出检测逻辑（基于 ModelProvider 配置），后续可整合。
pub(crate) fn build_policy_for_scene(
    agent: &Agent,
    _scene: ThinkingScene,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Box<dyn crate::pkg::policy::Policy> {
    use super::types::config_resolve;
    use crate::pkg::policy::builtin::{MaxRoundsPolicy, TimeoutPolicy, UserCancelPolicy};
    use crate::pkg::policy::policy_set;

    let max_rounds = config_resolve::max_thinking_rounds(agent);
    let timeout_secs = config_resolve::think_timeout_secs(agent);

    policy_set! {
        OR {
            UserCancelPolicy(cancel_flag),
            MaxRoundsPolicy(max_rounds),
            TimeoutPolicy(timeout_secs),
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
    /// # 退出条件
    /// - `ThinkResult::Final` → 返回 `ThinkLoopResult::Final(content)`
    /// - 策略命中（用户取消/轮次上限/超时等）→ 通过 `map_triggered_to_result` 映射
    /// - 上下文超限（input_tokens >= 阈值）→ 返回 `ContextOverflow`
    /// - 累计轮次达到 `max_rounds` → 返回 `MaxRoundsExceeded`
    /// - 超时 → 返回错误
    ///
    /// `start_round` 为本次循环的起始轮次编号（跨压缩累计）。
    /// `max_rounds` 为总轮次上限（跨压缩累计）。
    ///
    /// `think_runtime` / `policy` 为可选的策略引擎接入点：
    /// - `think_runtime`：每轮上报运行时快照（供前端 cancel-thinking/runtime-status 查询）
    /// - `policy`：每轮评估策略（用户取消/轮次上限/超时），命中即退出循环
    ///
    /// 两者都为 None 时退化为旧行为（仅靠 max_rounds + timeout_secs 控制）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_think_loop(
        &self,
        ctx: RequestContext,
        brain: &crate::models::brain::Brain,
        initial_messages: Vec<ChatMessage>,
        tool_descriptors: &[ToolDescriptor],
        agent: &Agent,
        scene: ThinkingScene,
        trace_id: &str,
        max_rounds: usize,
        start_round: usize,
        timeout_secs: u64,
        think_runtime: Option<&Arc<AgentThinkRuntime>>,
        policy: Option<&dyn crate::pkg::policy::Policy>,
    ) -> Result<ThinkLoopResult> {
        /// 上下文压缩触发阈值（占最大上下文窗口的比例）
        const CONTEXT_OVERFLOW_RATIO: f64 = 0.6;

        // 从 ModelProvider 配置中获取上下文压缩阈值
        // 优先级：recommended_context_length > max_context_length * 60% > 不检测
        let overflow_threshold: Option<u64> = brain.model_provider().and_then(|po| {
            let config = po.config();
            // 优先使用推荐上下文长度
            if let Some(rec) = config.recommended_context_length
                && rec > 0
            {
                return Some(rec as u64);
            }
            // fallback：max_context_length * 60%
            config
                .max_context_length
                .filter(|&v| v > 0)
                .map(|v| (v as f64 * CONTEXT_OVERFLOW_RATIO) as u64)
        });

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
                let result = self
                    .brain_dal()
                    .think(ctx.clone(), brain, &messages, tool_descriptors)
                    .await?;
                let round_duration_ms = round_start.elapsed().as_millis() as u64;

                match result {
                    ThinkResult::Final { content, usage } => {
                        // 累计 token 用量
                        total_input_tokens = total_input_tokens.saturating_add(usage.input_tokens);
                        total_output_tokens =
                            total_output_tokens.saturating_add(usage.output_tokens);
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
                        // 追加助手消息（含 tool_calls），让模型在下一轮看到自己发起的调用
                        messages.push(ChatMessage::Assistant {
                            content,
                            tool_calls: Some(tool_calls.clone()),
                        });
                        // 按 control_mode 分发执行（D26 入口统一：Auto → call_tool
                        // 协议路由（含凭据编排，Auto-MCP 亦可执行）；Manual →
                        // dispatch_manual_tool 特殊工具转发）
                        for tc in tool_calls {
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
                        // 发布 ThinkRoundEvent（有工具调用）
                        let _ = crate::pkg::aop::publish(
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

                        // 上下文压缩检测：当输入 token 超过阈值时中断循环，
                        // 由调用方（awaken）执行 sleep_and_settle 沉淀后重试
                        if let Some(threshold) = overflow_threshold
                            && usage.input_tokens >= threshold
                        {
                            log_info!(
                                &ctx,
                                "think_loop",
                                "context overflow detected: input_tokens={} >= threshold={}",
                                usage.input_tokens,
                                threshold
                            );
                            return Ok(ThinkLoopResult::ContextOverflow {
                                messages,
                                input_tokens: usage.input_tokens,
                                rounds_used: offset + 1,
                            });
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

                        // 评估策略：任一命中即退出循环
                        if let Some(policy) = policy {
                            let metrics = crate::pkg::policy::Metrics::new()
                                .with("round_number", (round + 1) as u64)
                                .with("max_rounds", max_rounds as u64)
                                .with("elapsed_secs", elapsed_secs)
                                .with("total_tokens", total_tokens)
                                .with("context_tokens", usage.input_tokens);
                            let triggered = policy.evaluate(&metrics);
                            if !triggered.is_empty() {
                                log_info!(
                                    &ctx,
                                    "think_loop",
                                    "policy triggered: {:?} at round={}",
                                    triggered,
                                    round + 1
                                );
                                return Ok(map_triggered_to_result(
                                    &triggered,
                                    messages,
                                    round + 1,
                                    usage.input_tokens,
                                ));
                            }
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
