//! 轮级环图的三个节点实现
//!
//! 节点顺序等价于老 `run_think_loop` 的循环体，只是按「模型调用 / 工具执行 / 策略裁决」
//! 切成三段。所有跨段状态经 [`RoundState`] 传递；节点之间互不引用后继。

use super::blueprint::{
    NODE_DISPATCH, NODE_EVALUATE, NODE_REFLECT, NODE_THINK, SIGNAL_CANCELLED, SIGNAL_CONTINUE,
    SIGNAL_EXIT, SIGNAL_FINAL, SIGNAL_REDIRECT, SIGNAL_REFLECT, SIGNAL_TOOL_CALLS,
};
use super::deps::RoundDeps;
use super::state::{
    OutputKind, RoundState, TRACE_TEXT_MAX_CHARS, pending_tool_trace, tool_result_preview,
    truncate_trace_text,
};
use crate::models::cortex_types::{ChatMessage, ThinkResult};
use crate::models::events::ThinkRoundEvent;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::think_path::{
    ActivationBudget, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus, ThinkNode,
};
use crate::pkg::think_trace::ThinkTraceRound;
use crate::service::domain::runtime::RuntimeToolExecution;
use crate::service::domain::runtime::think_loop::{
    ROUND_DIGEST_MAX_CONTENT, TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS, is_recursive_settle_call,
    is_settle_queue_write, policy_exit, round_metrics,
};
use crate::service::domain::runtime::types::{RoundDigest, ThinkLoopResult};
use async_trait::async_trait;
use common::enums::ThinkingIntensity;
use common::error::{Result, err};
use std::sync::{Arc, Mutex, MutexGuard};

/// 取状态锁：锁中毒属于编程错误，直接中止本次思考
fn lock(state: &Mutex<RoundState>) -> Result<MutexGuard<'_, RoundState>> {
    state
        .lock()
        .map_err(|_| err!(Internal, "think round state lock poisoned"))
}

/// 节点角色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Think,
    Dispatch,
    Evaluate,
    Reflect,
}

/// 轮级环图节点（各角色共用一个实现，避免多份样板）
#[derive(Debug)]
struct RoundNode {
    id: NodeId,
    role: Role,
    deps: Arc<RoundDeps>,
}

impl RoundNode {
    fn new(id: &'static str, role: Role, deps: Arc<RoundDeps>) -> Self {
        Self {
            id: NodeId::new(id),
            role,
            deps,
        }
    }
}

#[async_trait]
impl ThinkNode for RoundNode {
    fn id(&self) -> NodeId {
        self.id.clone()
    }

    fn phase(&self) -> PhaseId {
        match self.role {
            Role::Think => PhaseId::Deliberate,
            Role::Dispatch | Role::Evaluate | Role::Reflect => PhaseId::Review,
        }
    }

    fn purpose(&self) -> &str {
        match self.role {
            Role::Think => "调用模型进行一轮思考（含失败重试）",
            Role::Dispatch => "执行本轮工具调用并记录进度",
            Role::Evaluate => "策略裁决：继续下一轮或退出循环",
            Role::Reflect => "自省：重复调用时注入换方向提示",
        }
    }

    /// 各节点都按「可用轮次」放开预算：环的终止性由轮次上限 + 图步数护栏保证
    fn budget(&self) -> ActivationBudget {
        ActivationBudget {
            max_activations: self.deps.available_rounds,
            cooldown_steps: 0,
        }
    }

    async fn run(&self, _bus: &mut ThinkBus) -> Result<NodeOutcome> {
        match self.role {
            Role::Think => run_think(&self.deps).await,
            Role::Dispatch => run_dispatch(&self.deps).await,
            Role::Evaluate => run_evaluate(&self.deps).await,
            Role::Reflect => run_reflect(&self.deps),
        }
    }
}

/// 按蓝图顺序构造节点池（reflect 仅在开关打开时加入）
pub(crate) fn build_nodes(deps: &Arc<RoundDeps>) -> Vec<Arc<dyn ThinkNode>> {
    let mut nodes: Vec<Arc<dyn ThinkNode>> = vec![
        Arc::new(RoundNode::new(NODE_THINK, Role::Think, Arc::clone(deps))),
        Arc::new(RoundNode::new(
            NODE_DISPATCH,
            Role::Dispatch,
            Arc::clone(deps),
        )),
        Arc::new(RoundNode::new(
            NODE_EVALUATE,
            Role::Evaluate,
            Arc::clone(deps),
        )),
    ];
    if deps.enable_reflect {
        nodes.push(Arc::new(RoundNode::new(
            NODE_REFLECT,
            Role::Reflect,
            Arc::clone(deps),
        )));
    }
    nodes
}

// ==================== think ====================

/// 从模型返回中提取可读推理摘要
///
/// 只取 `ReasoningItem.summary`（provider 主动给出的可读摘要）；
/// `encrypted_content` 是私有密文，绝不进入轨迹（沿用 RoundDigest 的排除红线）。
fn reasoning_summaries(result: &ThinkResult) -> Vec<String> {
    result
        .reasoning_items()
        .iter()
        .flat_map(|item| item.summary.iter())
        .map(|s| truncate_trace_text(s, TRACE_TEXT_MAX_CHARS))
        .filter(|s| !s.is_empty())
        .collect()
}

/// 组装本轮轨迹明细（工具结果由 dispatch 回填）
///
/// `thinking_intensity` 必须是**本轮实际传给模型的调用参数快照**，而不是 run 级
/// 默认值——未来按轮动态调整强度时，轨迹仍需如实反映每一轮实际下发的值。
#[allow(clippy::too_many_arguments)]
fn build_trace_round(
    round: usize,
    assistant_text: Option<String>,
    reasoning: Vec<String>,
    tool_calls: Vec<crate::pkg::think_trace::ThinkTraceToolCall>,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
    reasoning_tokens: u64,
    duration_ms: u64,
    thinking_intensity: Option<ThinkingIntensity>,
) -> ThinkTraceRound {
    ThinkTraceRound {
        round: round + 1,
        assistant_text,
        reasoning,
        tool_calls,
        input_tokens,
        output_tokens,
        cached_tokens,
        reasoning_tokens,
        duration_ms,
        decision: None,
        thinking_intensity,
    }
}

/// 把非空文本截断为可落盘的预览
fn assistant_preview(content: &str) -> Option<String> {
    let preview = truncate_trace_text(content, TRACE_TEXT_MAX_CHARS);
    if preview.is_empty() {
        None
    } else {
        Some(preview)
    }
}

/// 回填一条工具调用轨迹（工具执行期间只改本地副本，不持锁）
fn mark_trace_tool(
    traces: &mut [crate::pkg::think_trace::ThinkTraceToolCall],
    index: usize,
    call_id: Option<String>,
    status: &str,
    result: Option<String>,
) {
    if let Some(entry) = traces.get_mut(index) {
        entry.call_id = call_id;
        entry.status = status.to_string();
        entry.result = result;
    }
}

/// 调用模型一轮：取消检查 → LLM 重试预算 → 产出 Final / ToolCall
async fn run_think(deps: &Arc<RoundDeps>) -> Result<NodeOutcome> {
    let d = deps;
    let ctx = d.ctx.clone();

    // ---- 轮次编号 + 轮首取消检查（原循环体开头）----
    // 锁作用域必须收在内层块里：节点内后面有 LLM await，std 锁守卫不能跨 await 存活
    let (
        round,
        mut messages,
        mut total_input_tokens,
        mut total_output_tokens,
        mut total_tool_calls,
        mut tool_call_counts,
        mut llm_consecutive_errors,
    ) = {
        let mut state = lock(&d.state)?;
        if d.think_runtime.as_ref().is_some_and(|tr| tr.is_cancelled()) {
            let round = d.start_round + state.rounds_started;
            log_info!(
                &ctx,
                "think_loop",
                "cancel flag detected before round={}, exiting loop",
                round
            );
            let messages = std::mem::take(&mut state.messages);
            state.result = Some(ThinkLoopResult::Cancelled {
                messages,
                total_rounds: round,
            });
            return Ok(NodeSignal::Yield(SIGNAL_CANCELLED.to_string()).into());
        }
        let round = d.start_round + state.rounds_started;
        state.rounds_started += 1;
        // 取出可变状态：节点内异步调用期间不能持有 std 锁
        (
            round,
            std::mem::take(&mut state.messages),
            state.total_input_tokens,
            state.total_output_tokens,
            state.total_tool_calls,
            std::mem::take(&mut state.tool_call_counts),
            state.llm_consecutive_errors,
        )
    };

    // ---- LLM 调用 + 错误预算 ----
    // 失败不再直接传播，计入因子交由策略裁决：预算内立即重试（无退避，YAGNI），
    // 策略命中则传播错误，下游 abort_summary 走无 LLM 兜底存档（异常总结路径）。
    //
    // 本轮调用参数快照：轨迹按轮记录实际下发值。未来若支持逐轮动态调整
    // （如按上下文长度/重试次数收敛强度），只需在这里产出不同的参数。
    let round_call_options = d.call_options.clone();
    let round_start = std::time::Instant::now();
    let result = loop {
        match d
            .domain
            .brain_dal()
            .think(
                ctx.clone(),
                &d.brain,
                &messages,
                &d.tool_descriptors,
                &round_call_options,
            )
            .await
        {
            Ok(result) => {
                llm_consecutive_errors = 0;
                break result;
            }
            Err(e) => {
                llm_consecutive_errors += 1;
                let metrics = round_metrics(
                    round + 1,
                    d.max_rounds,
                    d.loop_start.elapsed().as_secs(),
                    total_input_tokens.saturating_add(total_output_tokens),
                    0,
                    &tool_call_counts,
                    llm_consecutive_errors,
                );
                if let Some(p) = d.policy.as_deref()
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

    // 推理摘要必须在 match 前取出：`result` 之后会被 move 分解
    let reasoning = reasoning_summaries(&result);

    match result {
        ThinkResult::Final { content, usage, .. } => {
            // 累计 token 用量
            total_input_tokens = total_input_tokens.saturating_add(usage.input_tokens);
            total_output_tokens = total_output_tokens.saturating_add(usage.output_tokens);
            // 上下文长度：本轮实际送进模型的 prompt token 数。
            // 纯内存指标（不入统计表），每轮覆盖，用于观察上下文思考强度。
            AgentRuntimeStateManager::global()
                .record_context_length(&d.agent.po.id, usage.input_tokens);
            // 上报最终轮运行时快照
            if let Some(tr) = d.think_runtime.as_ref() {
                tr.report_round(
                    &d.trace_id,
                    d.scene,
                    round + 1,
                    d.max_rounds,
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
                    &d.agent.po.id,
                    &d.trace_id,
                    d.scene.as_str(),
                    round,
                    round_duration_ms,
                    false,
                    0,
                )
                .with_model_usage(d.model_provider_id.clone(), d.model_name.clone(), &usage)
                .with_context(
                    ctx.organization_id().cloned(),
                    ctx.user_id().cloned(),
                    ctx.task_id().cloned(),
                    ctx.project_id().cloned(),
                ),
            )
            .await;

            // 策略裁决交给 evaluate：这里只登记本轮产出
            let mut state = lock(&d.state)?;
            state.trace_rounds.push(build_trace_round(
                round,
                assistant_preview(&content),
                reasoning,
                Vec::new(),
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_tokens,
                usage.reasoning_tokens,
                round_duration_ms,
                round_call_options.thinking_intensity,
            ));
            state.messages = messages;
            state.total_input_tokens = total_input_tokens;
            state.total_output_tokens = total_output_tokens;
            state.total_tool_calls = total_tool_calls;
            state.tool_call_counts = tool_call_counts;
            state.llm_consecutive_errors = llm_consecutive_errors;
            state.last_round = round;
            state.last_input_tokens = usage.input_tokens;
            state.output_kind = Some(OutputKind::Final);
            state.final_content = Some(content);
            Ok(NodeSignal::Yield(SIGNAL_FINAL.to_string()).into())
        }
        ThinkResult::ToolCall {
            content,
            tool_calls,
            usage,
            reasoning_items,
        } => {
            let tc_count = tool_calls.len();
            total_tool_calls = total_tool_calls.saturating_add(tc_count);
            total_input_tokens = total_input_tokens.saturating_add(usage.input_tokens);
            total_output_tokens = total_output_tokens.saturating_add(usage.output_tokens);
            // 上下文长度：本轮实际送进模型的 prompt token 数（同上，每轮覆盖）
            AgentRuntimeStateManager::global()
                .record_context_length(&d.agent.po.id, usage.input_tokens);
            // 按工具名累计调用次数（无进展检测数据源）
            for tc in &tool_calls {
                *tool_call_counts.entry(tc.name.clone()).or_insert(0) += 1;
            }
            // 本轮新增消息起点：必须在 push 之前取，push 会 move 掉 content / tool_calls
            let round_start_idx = messages.len();
            let digest_assistant_text = content.clone();
            // 轨迹用的助手文本预览（digest_assistant_text 稍后会被移入 state）
            let digest_assistant_text_for_trace =
                assistant_preview(digest_assistant_text.as_deref().unwrap_or_default());
            // usage 之后会被移入 state，token 数值先取出来给轨迹用
            let (
                round_input_tokens,
                round_output_tokens,
                round_cached_tokens,
                round_reasoning_tokens,
            ) = (
                usage.input_tokens,
                usage.output_tokens,
                usage.cached_tokens,
                usage.reasoning_tokens,
            );
            // 待执行的工具：先记 pending，dispatch 执行后回填 call_id / status / result
            let tool_traces: Vec<crate::pkg::think_trace::ThinkTraceToolCall> =
                tool_calls.iter().map(pending_tool_trace).collect();
            // Responses 协议：模型本轮产出的推理项必须原样回传给下一轮
            // （部分推理模型强制要求，否则 400）。顺序排在助手消息之前，
            // 与 Responses 的 output 数组顺序（reasoning → message / function_call）一致。
            for item in reasoning_items {
                messages.push(ChatMessage::reasoning(item));
            }
            // 追加助手消息（含 tool_calls），让模型在下一轮看到自己发起的调用
            messages.push(ChatMessage::Assistant {
                content,
                tool_calls: Some(tool_calls.clone()),
            });

            let last_input_tokens = usage.input_tokens;
            let mut state = lock(&d.state)?;
            state.messages = messages;
            state.total_input_tokens = total_input_tokens;
            state.total_output_tokens = total_output_tokens;
            state.total_tool_calls = total_tool_calls;
            state.tool_call_counts = tool_call_counts;
            state.llm_consecutive_errors = llm_consecutive_errors;
            state.consecutive_tool_rounds = state.consecutive_tool_rounds.saturating_add(1);
            state.round_start_idx = round_start_idx;
            state.pending_assistant_text = digest_assistant_text;
            state.pending_usage = Some(usage);
            state.pending_round_duration_ms = round_duration_ms;
            state.trace_rounds.push(build_trace_round(
                round,
                digest_assistant_text_for_trace,
                reasoning,
                tool_traces,
                round_input_tokens,
                round_output_tokens,
                round_cached_tokens,
                round_reasoning_tokens,
                round_duration_ms,
                round_call_options.thinking_intensity,
            ));
            state.pending_tool_calls = tool_calls;
            state.last_round = round;
            state.last_input_tokens = last_input_tokens;
            state.output_kind = Some(OutputKind::ToolCalls);
            Ok(NodeSignal::Yield(SIGNAL_TOOL_CALLS.to_string()).into())
        }
    }
}

// ==================== dispatch ====================

/// 执行本轮工具调用，并完成进度快照 / 事件 / 疲劳提示 / 运行时快照
async fn run_dispatch(deps: &Arc<RoundDeps>) -> Result<NodeOutcome> {
    let d = deps;
    let ctx = d.ctx.clone();

    let (
        mut messages,
        mut trace_tool_calls,
        pending_tool_calls,
        round_start_idx,
        round,
        round_duration_ms,
        digest_assistant_text,
        usage,
        total_input_tokens,
        total_output_tokens,
        total_tool_calls,
        consecutive_tool_rounds,
        nudge_injected,
    ) = {
        let mut state = lock(&d.state)?;
        (
            std::mem::take(&mut state.messages),
            // 本轮工具调用轨迹取出后本地回填，执行期间不持锁
            state
                .trace_rounds
                .last_mut()
                .map(|trace_round| std::mem::take(&mut trace_round.tool_calls))
                .unwrap_or_default(),
            std::mem::take(&mut state.pending_tool_calls),
            state.round_start_idx,
            state.last_round,
            state.pending_round_duration_ms,
            state.pending_assistant_text.take(),
            state.pending_usage.take(),
            state.total_input_tokens,
            state.total_output_tokens,
            state.total_tool_calls,
            state.consecutive_tool_rounds,
            state.nudge_injected,
        )
    };

    let tc_count = pending_tool_calls.len();
    // 进度快照统计的是「尝试过哪些工具」：执行失败与工具不存在同样要计入
    let mut round_tool_names: Vec<String> = Vec::with_capacity(tc_count);

    for (trace_idx, tc) in pending_tool_calls.into_iter().enumerate() {
        // 沉淀/压缩场景：拦截对 settle_memory 的递归调用。
        //
        // `settle_memory` 会直接触发一整套 sleep_and_settle，在沉淀或压缩
        // 循环里再调它属于自我递归（一次压缩可能放大成 N 次完整沉淀）。
        // 这里选择「运行时拦截」而非收窄工具白名单：工具仍对其他场景可见，
        // 且拦截时能即时给模型一句可理解的反馈，比静默过滤更容易让它改道。
        if is_recursive_settle_call(d.scene, &tc.name) {
            log_info!(
                &ctx,
                "think_loop",
                "blocked recursive {} call in scene={}",
                tc.name,
                d.scene.as_str()
            );
            messages.push(ChatMessage::tool(
                tc.id,
                "你当前已经在沉淀/压缩流程中，无需再调用 settle_memory —— \
                 重复调用会嵌套触发新的沉淀循环。\
                 请直接继续当前任务（完成当前工作后输出 Final 文本结束）。"
                    .to_string(),
            ));
            mark_trace_tool(
                &mut trace_tool_calls,
                trace_idx,
                None,
                "blocked",
                Some("已拦截：沉淀/压缩流程内不允许递归调用 settle_memory".to_string()),
            );
            continue;
        }
        // 沉淀场景：拦截往待沉淀队列写新短期记忆。
        //
        // 待沉淀队列是本轮的**输入**：框架刚把本批标记 Settled，若模型又写一条，
        // 回环的 `prepare` 会立刻取到它 → 队列永远扫不空，最终打满
        // `MAX_SETTLE_BATCHES` 并触发整轮重投（实测把一次夜间沉淀拖成 520 轮
        // 模型调用、持续 2.5 小时）。与上面同样选择运行时拦截：工具在其它场景
        // （主循环 / 压缩）必须可用，且拦截能把「产出该进图谱」即时告诉模型。
        if is_settle_queue_write(d.scene, &tc.name) {
            log_info!(
                &ctx,
                "think_loop",
                "blocked short-term write tool {} in scene={}",
                tc.name,
                d.scene.as_str()
            );
            messages.push(ChatMessage::tool(
                tc.id,
                "沉淀期间不要写新的短期记忆：待沉淀队列是本轮沉淀的输入，再写一条会让\
                 队列永远排不空。请把这次沉淀的结论写进知识图谱\
                 （save_long_term_memory / update_memory）或沉淀成技能；\
                 本次沉淀的 Trace 由框架记录，无需另记摘要。"
                    .to_string(),
            ));
            mark_trace_tool(
                &mut trace_tool_calls,
                trace_idx,
                None,
                "blocked",
                Some("已拦截：沉淀期间禁止写入新的短期记忆（防止待沉淀队列自续料）".to_string()),
            );
            continue;
        }
        round_tool_names.push(tc.name.clone());
        // 回站匹配必须与出站（ToolDescriptor::from → to_wire_tool_name）同一函数：
        // 模型回传的是 wire name，不是 DB 显示名。
        match d
            .agent
            .tools()
            .iter()
            .find(|t| common::llm_schema::to_wire_tool_name(&t.po.id) == tc.name)
        {
            Some(tool) => {
                let call_result = match tool.po.control_mode {
                    common::enums::tool::ControlMode::Auto => {
                        d.domain.call_tool(ctx.clone(), tool, tc.arguments).await
                    }
                    common::enums::tool::ControlMode::Manual => {
                        d.domain
                            .dispatch_manual_tool(ctx.clone(), tool, tc.arguments)
                            .await
                    }
                };
                match call_result {
                    Ok(result) => {
                        // call_id 是 think trace → tool_tracing 的关联键
                        let call_id = result.trace_ref.call_id.clone();
                        let content = format!("{}", result.result);
                        mark_trace_tool(
                            &mut trace_tool_calls,
                            trace_idx,
                            Some(call_id),
                            "ok",
                            Some(tool_result_preview(&content)),
                        );
                        messages.push(ChatMessage::tool(tc.id, content));
                    }
                    Err(e) => {
                        let content = format!("Error: {}", e);
                        mark_trace_tool(
                            &mut trace_tool_calls,
                            trace_idx,
                            None,
                            "error",
                            Some(tool_result_preview(&content)),
                        );
                        messages.push(ChatMessage::tool(tc.id, content));
                    }
                }
            }
            None => {
                let content = format!("Error: tool {} not found", tc.name);
                mark_trace_tool(
                    &mut trace_tool_calls,
                    trace_idx,
                    None,
                    "not_found",
                    Some(tool_result_preview(&content)),
                );
                messages.push(ChatMessage::tool(tc.id, content));
            }
        }
    }

    // 记录本轮进度到快照。
    //
    // 位置刻意选在「工具执行完、策略评估前」：上下文溢出检测与策略
    // 命中（轮次耗尽 / token 预算 / 无进展）都会立即终止，
    // 放在这里才能保证每一轮已完成的工具调用都不漏记。
    if let Some(progress) = &d.progress {
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
            &d.agent.po.id,
            &d.trace_id,
            d.scene.as_str(),
            round,
            round_duration_ms,
            true,
            tc_count,
        )
        .with_model_usage(
            d.model_provider_id.clone(),
            d.model_name.clone(),
            &usage.unwrap_or_default(),
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
    let mut nudge_injected = nudge_injected;
    if consecutive_tool_rounds >= TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS && !nudge_injected {
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

    // 上报运行时快照（每轮都更新，供前端实时查询）
    if let Some(tr) = d.think_runtime.as_ref() {
        tr.report_round(
            &d.trace_id,
            d.scene,
            round + 1,
            d.max_rounds,
            total_input_tokens,
            total_output_tokens,
            total_input_tokens.saturating_add(total_output_tokens),
            total_tool_calls,
        );
    }

    let mut state = lock(&d.state)?;
    // 工具结果回填到 think 节点创建的那条轮级轨迹（与本轮 pending_tool_calls 同序）
    if let Some(trace_round) = state.trace_rounds.last_mut() {
        trace_round.tool_calls = trace_tool_calls;
    }
    state.messages = messages;
    state.nudge_injected = nudge_injected;
    Ok(NodeSignal::Done.into())
}

// ==================== reflect ====================

/// 同一工具累计调用达到该次数即视为可能陷入重复路径
///
/// 与 `TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS`（连续 8 轮只调工具的全局疲劳提示）
/// 互补：这里按工具维度更早介入，且每个工具只提示一次。
const REFLECT_REPEAT_THRESHOLD: usize = 3;

/// 找出值得「换方向」的工具：累计次数达阈值且尚未自省过
///
/// 排序保证提示文案与自省记录稳定（次数降序，同次数按名字升序）。
fn reflect_targets(state: &RoundState) -> Vec<(String, usize)> {
    let mut targets: Vec<(String, usize)> = state
        .tool_call_counts
        .iter()
        .filter(|(name, count)| {
            **count >= REFLECT_REPEAT_THRESHOLD && !state.reflected_tools.contains(*name)
        })
        .map(|(name, count)| (name.clone(), *count))
        .collect();
    targets.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    targets
}

/// 自省节点：把「别再重复同一个工具」的提示注入下一轮上下文
///
/// 纯确定性、零 LLM 调用：只在 evaluate 已判定值得自省时进入，
/// 因此不会增加任何额外模型成本。每个工具只提示一次，防止 System 消息堆积。
fn run_reflect(deps: &Arc<RoundDeps>) -> Result<NodeOutcome> {
    let d = deps;
    let mut state = lock(&d.state)?;
    let targets = reflect_targets(&state);
    if targets.is_empty() {
        // 防御：evaluate 已确认非空才发 reflect 信号，正常不会走到这里
        return Ok(NodeSignal::Yield(SIGNAL_REDIRECT.to_string()).into());
    }

    let round = state.last_round;
    let summary = targets
        .iter()
        .map(|(name, count)| format!("{name}（{count} 次）"))
        .collect::<Vec<_>>()
        .join("、");
    log_info!(
        &d.ctx,
        "think_loop",
        "reflect: repeated tool calls at round={}: {}",
        round + 1,
        summary
    );

    state.messages.push(ChatMessage::system(format!(
        "【系统提醒】检测到以下工具被反复调用：{summary}。请先判断是否陷入了重复路径：\n\
         1. 如果这些调用没有带来新信息，请换一种工具，或换一个检索/执行方向；\n\
         2. 如果已有足够信息，直接输出最终回复，不要再调用这些工具；\n\
         3. 如果确实无法推进，向用户说明卡点与已尝试过的方案。"
    )));
    for (name, _) in &targets {
        state.reflected_tools.insert(name.clone());
    }
    Ok(NodeSignal::Yield(SIGNAL_REDIRECT.to_string()).into())
}

// ==================== evaluate ====================

/// 给本轮轨迹补上裁决结果（前端据此显示 final / continue / reflect / exit）
fn set_trace_decision(state: &mut RoundState, decision: &str) {
    if let Some(trace_round) = state.trace_rounds.last_mut() {
        trace_round.decision = Some(decision.to_string());
    }
}

/// 统一策略裁决：final 与 tool_calls 两套指标，命中即退出，否则继续下一轮
async fn run_evaluate(deps: &Arc<RoundDeps>) -> Result<NodeOutcome> {
    let d = deps;
    let mut state = lock(&d.state)?;
    let Some(kind) = state.output_kind else {
        // 防御：evaluate 只由 think / dispatch 的确定信号进入，正常不会走到这里
        return Ok(NodeSignal::Yield(SIGNAL_EXIT.to_string()).into());
    };

    let round = state.last_round;
    let metrics = round_metrics(
        round + 1,
        d.max_rounds,
        d.loop_start.elapsed().as_secs(),
        state
            .total_input_tokens
            .saturating_add(state.total_output_tokens),
        state.last_input_tokens,
        &state.tool_call_counts,
        state.llm_consecutive_errors,
    )
    .with("output_kind", kind.as_str());

    // 工具轮没有 Final 内容：显式传 None，避免任何历史残留影响策略映射
    let final_content = match kind {
        OutputKind::Final => state.final_content.clone(),
        OutputKind::ToolCalls => None,
    };
    let exit = policy_exit(
        &d.ctx,
        d.policy.as_deref(),
        &metrics,
        &state.messages,
        round + 1,
        state.last_input_tokens,
        final_content,
    );

    match kind {
        OutputKind::Final => {
            set_trace_decision(&mut state, "final");
            state.result = Some(match exit {
                Some(exit) => exit,
                None => {
                    // fail-safe：自定义策略组未含 final_answer 时保持原行为
                    let content = state.final_content.clone().unwrap_or_default();
                    let messages = std::mem::take(&mut state.messages);
                    ThinkLoopResult::Final { content, messages }
                }
            });
            Ok(NodeSignal::Yield(SIGNAL_EXIT.to_string()).into())
        }
        OutputKind::ToolCalls => {
            if let Some(exit) = exit {
                set_trace_decision(&mut state, "exit");
                state.result = Some(exit);
                Ok(NodeSignal::Yield(SIGNAL_EXIT.to_string()).into())
            } else if d.enable_reflect && !reflect_targets(&state).is_empty() {
                set_trace_decision(&mut state, "reflect");
                // 重复调用同一工具：插入自省节点，下一轮换方向
                log_info!(
                    &d.ctx,
                    "think_loop",
                    "evaluate: repeated tools detected at round={}, entering reflect",
                    round + 1
                );
                Ok(NodeSignal::Yield(SIGNAL_REFLECT.to_string()).into())
            } else {
                set_trace_decision(&mut state, "continue");
                Ok(NodeSignal::Yield(SIGNAL_CONTINUE.to_string()).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 达到阈值且未自省过的工具才进自省名单；自省过的工具不重复提示
    #[test]
    fn reflect_targets_picks_repeated_unreflected_tools() {
        let mut state = RoundState::new(vec![ChatMessage::system("hello")]);
        state.tool_call_counts.insert("search".to_string(), 3);
        state.tool_call_counts.insert("calc".to_string(), 2);

        assert_eq!(
            reflect_targets(&state),
            vec![("search".to_string(), 3)],
            "只有达到阈值且未自省的工具入选"
        );

        state.reflected_tools.insert("search".to_string());
        assert!(reflect_targets(&state).is_empty(), "同一工具只提示一次");
    }
}
