//! 轮级环图的三个节点实现
//!
//! 节点顺序等价于老 `run_think_loop` 的循环体，只是按「模型调用 / 工具执行 / 策略裁决」
//! 切成三段。所有跨段状态经 [`RoundState`] 传递；节点之间互不引用后继。

use super::blueprint::{
    NODE_DISPATCH, NODE_EVALUATE, NODE_THINK, SIGNAL_CANCELLED, SIGNAL_CONTINUE, SIGNAL_EXIT,
    SIGNAL_FINAL, SIGNAL_TOOL_CALLS,
};
use super::deps::RoundDeps;
use super::state::{OutputKind, RoundState};
use crate::models::cortex_types::{ChatMessage, ThinkResult};
use crate::models::events::ThinkRoundEvent;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::think_path::{
    ActivationBudget, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus, ThinkNode,
};
use crate::service::domain::runtime::RuntimeToolExecution;
use crate::service::domain::runtime::think_loop::{
    ROUND_DIGEST_MAX_CONTENT, TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS, is_recursive_settle_call,
    policy_exit, round_metrics,
};
use crate::service::domain::runtime::types::{RoundDigest, ThinkLoopResult};
use async_trait::async_trait;
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
}

/// 轮级环图节点（三角色共用一个实现，避免三份样板）
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
            Role::Dispatch | Role::Evaluate => PhaseId::Review,
        }
    }

    fn purpose(&self) -> &str {
        match self.role {
            Role::Think => "调用模型进行一轮思考（含失败重试）",
            Role::Dispatch => "执行本轮工具调用并记录进度",
            Role::Evaluate => "策略裁决：继续下一轮或退出循环",
        }
    }

    /// 三个节点都按「可用轮次」放开预算：环的终止性由轮次上限 + 图步数护栏保证
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
        }
    }
}

/// 按蓝图顺序构造节点池
pub(crate) fn build_nodes(deps: &Arc<RoundDeps>) -> Vec<Arc<dyn ThinkNode>> {
    vec![
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
    ]
}

// ==================== think ====================

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
    let round_start = std::time::Instant::now();
    let result = loop {
        match d
            .domain
            .brain_dal()
            .think(ctx.clone(), &d.brain, &messages, &d.tool_descriptors)
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

    for tc in pending_tool_calls {
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
                 请直接继续当前任务（写入短期记忆后输出 Final 文本结束）。"
                    .to_string(),
            ));
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
                        messages.push(ChatMessage::tool(tc.id, format!("{}", result.result)));
                    }
                    Err(e) => {
                        messages.push(ChatMessage::tool(tc.id, format!("Error: {}", e)));
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
    state.messages = messages;
    state.nudge_injected = nudge_injected;
    Ok(NodeSignal::Done.into())
}

// ==================== evaluate ====================

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
                state.result = Some(exit);
                Ok(NodeSignal::Yield(SIGNAL_EXIT.to_string()).into())
            } else {
                Ok(NodeSignal::Yield(SIGNAL_CONTINUE.to_string()).into())
            }
        }
    }
}
