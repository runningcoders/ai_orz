//! 共享 think loop 引擎
//!
//! `run_think_loop` 是 awaken / sleep_and_settle / summary / intent_analyze
//! 共用的多轮思考循环，封装：超时控制 + 小脑路由 + 轮级环图驱动。
//!
//! ## 轮级环图
//!
//! 每一轮思考不再是主循环里的一段直线代码，而是图上的三个节点：
//!
//! ```text
//! think --tool_calls--> dispatch --done--> evaluate
//!   ^                      （工具执行）        │
//!   └──────────── continue ───────────────────┘
//!   │
//!   └── final ───────────────────────────> evaluate --exit--> 结束
//! ```
//!
//! 拓扑定义在 [`think_round::blueprint`]，节点实现在 [`think_round::nodes`]。
//! 本文件只保留小脑路由、超时控制与跨场景共用的策略/指标工具函数。

use super::cerebellum_router::{self, RouteDecision};
use super::think_round::{self, RoundRun};
use super::types::ThinkLoopResult;
use crate::models::agent::Agent;
use crate::models::cortex_types::ChatMessage;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::runtime::RuntimeDomainImpl;
use common::enums::ThinkingScene;
use common::error::{Result, err};
use std::sync::Arc;

// ==================== 策略映射 ====================

/// 构造单轮策略评估用的 Metrics（错误路径与轮末评估点共用）
pub(crate) fn round_metrics(
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
pub(crate) fn policy_exit(
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
/// 入参 `tool_name` 是模型回传的 wire name（`to_wire_tool_name` 输出），
/// 常量侧同样过一遍该函数，两侧禁各写一份字面量。
pub(crate) fn is_recursive_settle_call(
    scene: common::enums::ThinkingScene,
    tool_name: &str,
) -> bool {
    use common::enums::ThinkingScene;
    matches!(scene, ThinkingScene::Settle | ThinkingScene::Compact)
        && tool_name
            == common::llm_schema::to_wire_tool_name(super::compaction::RECURSIVE_SETTLE_TOOL)
}

/// 进度快照中单条消息摘要的最大字符数
///
/// 兜底摘要是「信息保全」而非「信息复现」，超长的工具返回（搜索结果、文件全文）
/// 截断即可 —— 真要回看细节还有 trace 的 JSONL 原文。
pub(crate) const ROUND_DIGEST_MAX_CONTENT: usize = 800;

/// 连续工具调用轮数的疲劳提示阈值：达到后注入一条 System 提醒，逼模型收尾
///
/// 业界通用做法（OpenAI Assistants / LangChain max_iterations 同类机制）：
/// 模型连续多轮只调工具不给最终回复时，大概率已陷入循环，注入提醒给一次自纠机会。
pub(crate) const TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS: usize = 8;

/// LLM 连续失败重试预算：连续失败达到该次数即命中策略、终止循环
///
/// 预算内立即重试（无退避，YAGNI）；命中后错误照常向上传播，
/// 下游 abort_summary 走无 LLM 兜底存档（异常总结路径）。
pub(crate) const LLM_ERROR_RETRY_BUDGET: u64 = 3;

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
) -> Arc<dyn crate::pkg::policy::Policy> {
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
    // key 用 wire name（与 tool_call_counts 的 tc.name 同空间，见回站匹配处）
    let tool_limits: std::collections::HashMap<String, usize> = agent
        .tools()
        .iter()
        .filter_map(|t| {
            t.po.config_no_progress_max_calls()
                .map(|limit| (common::llm_schema::to_wire_tool_name(&t.po.id), limit))
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

    // 转为 Arc：轮级环图节点需要 `'static` 共享同一策略组
    Arc::from(policy_set! {
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
    })
}

// ==================== 共享 think loop ====================

impl RuntimeDomainImpl {
    /// 执行 think 循环（awaken/sleep_and_settle/summary 共用）
    ///
    /// 统一封装：小脑快判断路由 + 超时控制 + 轮级环图（think → dispatch → evaluate）。
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
        // 克隆一份 owned Brain：轮级环图节点是 'static，不能借用 params。
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_agent_brain()"))?
            .clone();

        // 入口留痕（早于小脑路由）：「think loop entered」被小脑路由挡在后面，
        // 小脑那一步一旦挂住/降级，现场就只剩 brain woken → 再无下文。
        // 本行在路由之前，把这段盲区切成两半。
        log_info!(
            &ctx,
            "think_loop",
            "think loop entered (pre-cerebellum), scene={}, agent={}, has_cerebellum={}, enable_route={}, max_rounds={}, start_round={}, timeout_secs={}",
            scene.as_str(),
            agent.po.id,
            brain.cerebellum.is_some(),
            brain.runtime_config.enable_cerebellum_route,
            max_rounds,
            start_round,
            timeout_secs,
        );

        let think_future = async {
            let mut messages = initial_messages;
            // ---- B3 小脑快判断路由（单点接线；四场景自动全覆盖）----
            // 三层降级全部收敛为「静默跳过 = 现状」：无启用小脑/总开关关、
            // 800ms 超时/调用失败、低置信。只有 Inject 会改变消息序列。
            if let Some(cerebellum) = brain.cerebellum.as_ref() {
                let rc = &brain.runtime_config;
                let last_user_text = messages.iter().rev().find_map(|m| match m {
                    ChatMessage::User { content } => Some(content.as_str()),
                    // 批4 披露②：带图消息取文本 part（防小脑快判断静默跳过）
                    ChatMessage::UserMultimodal { text, .. } => Some(text.as_str()),
                    _ => None,
                });
                log_info!(
                    &ctx,
                    "think_loop",
                    "cerebellum route start, agent={}, provider={}, enable={}",
                    agent.po.id,
                    cerebellum.id,
                    rc.enable_cerebellum_route,
                );
                let decision = cerebellum_router::route_with_default_dao(
                    &ctx,
                    cerebellum,
                    rc.enable_cerebellum_route,
                    &brain.agent_id,
                    &brain.agent_name,
                    last_user_text,
                )
                .await;
                log_info!(
                    &ctx,
                    "think_loop",
                    "cerebellum route done, agent={}, direct={}, inject={}, skip={}",
                    agent.po.id,
                    matches!(decision, RouteDecision::Direct { .. }),
                    matches!(decision, RouteDecision::Inject { .. }),
                    matches!(decision, RouteDecision::Skip { .. }),
                );
                match decision {
                    // TRIVIAL 高置信直回：默认关；开则跳过主循环提前返回
                    RouteDecision::Direct {
                        content,
                        confidence,
                    } => {
                        if rc.cerebellum_trivial_direct {
                            log_info!(
                                &ctx,
                                "think_loop",
                                "cerebellum trivial direct short-circuit, confidence={}",
                                confidence
                            );
                            return Ok(ThinkLoopResult::Final { content, messages });
                        }
                        // 直回开关关 → 不直回也不注入（保守口径）
                    }
                    // 增强注入（默认开）：路由结论作为首轮 System 提示插入消息头
                    RouteDecision::Inject { system_prompt, .. } => {
                        messages.insert(
                            0,
                            ChatMessage::System {
                                content: system_prompt,
                            },
                        );
                    }
                    // 静默跳过 = 现状
                    RouteDecision::Skip { .. } => {}
                }
            }
            // 提取模型提供商信息（入口日志用；事件发布由轮级节点自行取）
            let model_provider_id = brain.model_provider().map(|po| po.id.clone());
            // 本次循环可用轮次 = max_rounds - start_round
            let available_rounds = max_rounds.saturating_sub(start_round);
            // 策略评估用的起始时间（用于 elapsed_secs）
            let loop_start = std::time::Instant::now();
            // 入口留痕：这一行是「Agent 置忙但零进展」类故障的定位检查点。
            // 它缺失 ⇒ 冻结在上游（wake_brain / ThinkingOptions / 引擎启动）；
            // 它在而后续无日志 ⇒ 冻结在 LLM 调用本身。scene 字段区分
            // awaken / compact / settle 各自一次。
            log_info!(
                &ctx,
                "think_loop",
                "think loop entered, scene={}, agent={}, max_rounds={}, start_round={}, available_rounds={}, provider={}",
                scene.as_str(),
                agent.po.id,
                max_rounds,
                start_round,
                available_rounds,
                model_provider_id.as_deref().unwrap_or("<none>"),
            );
            // 零可用轮次 = 配置把思考禁光了：循环体一次都不进，没有任何 LLM
            // 请求、也不会有后续任何日志。不显式警告的话，「Agent 忙碌但
            // 什么都没做」在现场只剩沉默 —— 而它的成因（max_rounds 解析为 0）
            // 在另一台机器上完全可以由一份不同的 config / runtime_config 触发。
            if available_rounds == 0 {
                crate::log_warn!(
                    &ctx,
                    "think_loop",
                    "no available rounds (max_rounds={} <= start_round={}), exiting without any LLM call",
                    max_rounds,
                    start_round,
                );
                return Ok(ThinkLoopResult::MaxRoundsExceeded {
                    messages,
                    total_rounds: max_rounds,
                });
            }

            // 轮级环图：think → dispatch → evaluate，由引擎按节点信号驱动
            think_round::run(
                self.clone(),
                RoundRun {
                    ctx,
                    agent,
                    brain,
                    scene,
                    trace_id,
                    messages,
                    tool_descriptors,
                    max_rounds,
                    start_round,
                    available_rounds,
                    think_runtime,
                    policy,
                    progress,
                    loop_start,
                },
            )
            .await
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
