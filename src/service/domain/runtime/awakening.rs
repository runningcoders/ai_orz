//! Runtime Awakening 具体实现
//!
//! 本文件只保留 `RuntimeAwakening` trait 的四大入口方法：
//! - `wake_agent_brain`：装配 Brain
//! - `awaken`：唤醒并执行思考（Phase 1 意图分析 + Phase 2 正式执行）
//! - `sleep_and_settle`：沉睡沉淀记忆
//! - `analyze_input_intent`：意图分析（Phase 1，stats 包裹 + 委托 inner）
//!
//! 共享类型在 `types.rs`，think loop 引擎在 `think_loop.rs`，
//! 意图分析核心 + JSON 解析在 `intent_analyze.rs`，总结退出在 `summary.rs`。

use super::types::ThinkLoopParams;
use crate::enrich_ctx;
use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, ToolDescriptor};
use crate::models::events::AgentLoopEvent;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, AgentThinkRuntime};
use crate::pkg::paths;
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::AgentAwakeEvent;
use crate::record_event;
use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};
use crate::service::domain::runtime::compaction::{CompactOutcome, strip_summary_block};
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};
use common::enums::ThinkingScene;
use common::enums::{MemoryStatus, MemoryType};
use common::error::Result;
use std::sync::Arc;

use super::think_loop::build_policy_for_scene;

// ==================== re-export（保持外部引用路径不变）====================
// pub use 同时完成导入（供本文件内部使用）和 re-export（供外部 awakening::xxx 访问）

pub use super::intent_analyze::{extract_first_json_object, parse_intent_analysis_json};
pub use super::types::{IntentAnalysis, ThinkLoopResult, ThinkingOptions};

// ==================== 场景辅助函数（消除重复逻辑）====================

/// 创建思考运行时并注册到 StateManager，同时构造场景策略组
///
/// 在 awaken / sleep_and_settle / intent_analyze / summary 中重复出现的初始化逻辑：
/// 1. 创建 AgentThinkRuntime（绑定 trace_id）
/// 2. 注册到 AgentRuntimeStateManager（供 cancel-thinking / runtime-status 查询）
/// 3. 按场景构造策略组（用户取消 + 轮次上限 + 超时）
pub(super) fn init_think_runtime_and_policy(
    agent: &Agent,
    scene: ThinkingScene,
    trace_id: &str,
) -> (Arc<AgentThinkRuntime>, Box<dyn crate::pkg::policy::Policy>) {
    let think_runtime = Arc::new(AgentThinkRuntime::new(
        agent.po.id.clone(),
        trace_id.to_string(),
    ));
    AgentRuntimeStateManager::global().set_think_runtime(&agent.po.id, think_runtime.clone());
    let policy = build_policy_for_scene(agent, scene, think_runtime.cancel_flag());
    (think_runtime, policy)
}

/// 按场景过滤工具，构建 ToolDescriptor 列表
///
/// sleep_and_settle / intent_analyze / summary 中重复出现的工具过滤逻辑。
/// awaken 场景不过滤（用全量工具），不调用此函数。
pub(super) fn build_scene_tool_descriptors(
    agent: &Agent,
    scene: ThinkingScene,
) -> Vec<ToolDescriptor> {
    agent
        .tools()
        .iter()
        .filter(|t| {
            let tags = t.po.get_tags();
            scene.is_tool_allowed(&tags)
        })
        .map(ToolDescriptor::from)
        .collect()
}

/// 按场景过滤技能，返回 SkillPo 列表
///
/// sleep_and_settle / intent_analyze / summary 中重复出现的技能过滤逻辑。
/// awaken 场景不过滤技能（全量加载），不调用此函数。
/// 沉淀场景的「近期已沉淀记忆」参考条数
///
/// 只取少量：主要价值是衔接上次沉淀被预算截断的情况，让 Agent 看到「上一段沉淀到哪了」。
/// 完整上下文由 Agent 用 `search_memory` 按需检索，不在这里铺开。
const SETTLED_REFERENCE_LIMIT: usize = 5;

/// 压缩后下一轮补充的「更早的记忆」条数
///
/// 只给少量：主体上下文是【上一轮工作压缩结果】，这里仅作连续性线索，
/// 多给会挤占预算。完整历史由 Agent 用 `search_memory` 按需检索。
const PAST_MEMORIES_LIMIT: usize = 5;

impl RuntimeDomainImpl {
    /// 查询最近若干条短期记忆摘要，作为压缩后下一轮的「更早的记忆」参考
    ///
    /// `exclude_id` 为本次压缩刚落库那条记忆的 id —— 它的内容已经在
    /// 【上一轮工作压缩结果】里，必须排除，否则同一份内容出现两次。
    ///
    /// 查询失败不阻断：参考块是锦上添花，缺了也能正常工作。
    async fn query_past_memories(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        exclude_id: Option<&str>,
    ) -> Vec<String> {
        // 多取一条：被排除的那条大概率是最新的一条，多取一条才能凑够 PAST_MEMORIES_LIMIT
        let result = self
            .memory()
            .query(
                ctx.clone(),
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    memory_type: Some(MemoryType::ShortTerm),
                    limit: Some(PAST_MEMORIES_LIMIT + 1),
                    order: MemorySortOrder::RecentFirst,
                    ..Default::default()
                },
            )
            .await;

        match result {
            Ok(memories) => memories
                .iter()
                .filter(|m| match (&m.po, exclude_id) {
                    (crate::models::memory::MemoryPo::ShortTerm(st), Some(id)) => st.id != id,
                    _ => true,
                })
                .filter_map(|m| m.to_prompt_summary())
                .take(PAST_MEMORIES_LIMIT)
                .collect(),
            Err(e) => {
                log_warn!(
                    &ctx,
                    "awaken",
                    error = ?e,
                    "查询更早记忆失败，跳过参考区块"
                );
                Vec::new()
            }
        }
    }

    /// 查询最近若干条**已沉淀**短期记忆的摘要，作为沉淀 prompt 的参考线索
    ///
    /// 与 `get_recent_context` 的区别：
    /// - 只取 `status = Settled` 的（已整合进图谱，不是待处理对象）
    /// - 条数远少于历史对话（5 vs 20），避免与【待沉淀的短期记忆】重复占用预算
    ///
    /// 查询失败不阻断沉淀流程——参考块是锦上添花，缺了也能正常工作。
    async fn query_settled_reference(&self, ctx: RequestContext, agent_id: &str) -> Vec<String> {
        let result = self
            .memory()
            .query(
                ctx.clone(),
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    status: Some(MemoryStatus::Settled),
                    memory_type: Some(MemoryType::ShortTerm),
                    limit: Some(SETTLED_REFERENCE_LIMIT),
                    order: MemorySortOrder::RecentFirst,
                    ..Default::default()
                },
            )
            .await;

        match result {
            Ok(memories) => memories
                .iter()
                .filter_map(|m| m.to_prompt_summary())
                .collect(),
            Err(e) => {
                log_warn!(
                    &ctx,
                    "sleep_and_settle",
                    error = ?e,
                    "查询近期已沉淀记忆失败，跳过参考区块"
                );
                Vec::new()
            }
        }
    }
}

pub(super) fn build_scene_skills(
    agent: &Agent,
    scene: ThinkingScene,
) -> Vec<crate::models::skill::SkillPo> {
    agent
        .skills()
        .iter()
        .filter(|s| {
            let tags = s.po.parse_tags();
            scene.is_tool_allowed(&tags)
        })
        .map(|s| s.po.clone())
        .collect()
}

// ==================== RuntimeAwakening trait 实现 ====================

#[async_trait::async_trait]
impl RuntimeAwakening for RuntimeDomainImpl {
    /// 装配 Agent 的 Brain
    ///
    /// 根据 agent.kind 构造对应的 Brain：
    /// - Local: 通过 BrainDal.wake_brain 构造带 Cortex 的 Brain
    /// - External（Cli/Remote）: 构造不带 Cortex 的虚拟 Brain
    ///
    /// 工具不再在此层处理——agent.tools 保留全量工具，
    /// awaken/sleep_and_settle 时按场景过滤构建 ToolDescriptor 列表。
    ///
    /// 返回 enriched ctx：wake_brain 内部查询 ModelProvider 后会补充
    /// `model_provider_id` / `model_name`，调用方应使用返回的 ctx 替换原 ctx，
    /// 保证后续 awaken/think 链路的 ctx 字段完整（避免监控日志缺 model_name）。
    async fn wake_agent_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
    ) -> Result<RequestContext> {
        // 幂等：brain 已装配则直接返回原 ctx（无需再 enrich provider 字段）
        if agent.brain.is_some() {
            return Ok(ctx);
        }

        let ctx = enrich_ctx!(&ctx, &*agent);

        // 工具不再分离：所有工具保留在 agent.tools 中，
        // awaken/sleep_and_settle 时按场景过滤展示，ToolDescriptor 列表在 think 时构建。
        //
        // TODO(brain-cache): 目前每条消息都重新加载 agent 并重建 Brain，
        // 存在性能浪费。若未来引入 brain 缓存，需重新评估 ctx 新鲜度问题。
        let brain = self
            .brain_dal()
            .wake_brain(ctx.clone(), &agent.po, Vec::new())
            .await?;

        // 从 brain.model_provider 提取配置重新 enrich ctx（仅 Local agent 有值），
        // 保证返回的 ctx 含 model_provider_id / model_name（供 awaken 的统计/trace 使用）。
        // 外部 agent（Cli/Remote）无 model_provider，ctx 保持原样。
        let ctx = match brain.model_provider.as_ref() {
            Some(provider) => enrich_ctx!(&ctx, provider),
            None => ctx,
        };

        agent.set_brain(brain);

        // 返回 enriched ctx（含 ModelProvider 字段：model_provider_id / model_name）
        Ok(ctx)
    }

    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
    ) -> Result<AwakeningResult> {
        let start_time = std::time::SystemTime::now();

        // 设置 Agent 为忙碌状态
        // 使用 RAII guard 确保 set_idle 一定被执行
        // 修复：之前 set_busy 与 set_idle 之间多处 ? 提早返回（get_recent_context、
        // brain 缺失等）会导致 Agent 永远 Busy，后续消息被 is_unavailable 挡住
        AgentRuntimeStateManager::global().set_busy(
            &agent.po.id,
            &message.po.id,
            message.po.task_id.as_deref(),
            message.po.project_id.as_deref(),
        );
        let _busy_guard =
            crate::service::domain::runtime::busy_guard::BusyGuard::new(agent.po.id.clone());

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        // Step 1: 预先构造 MemoryTrace 拿到 trace_id
        // 调用方负责组装 trace，RuntimeMemory 负责写入和补全信息
        use common::enums::MemoryRole;
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(), // input 后续填充
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // 创建思考运行时（与 Busy 状态绑定，BusyGuard Drop 时自动清理）
        // 整个 awaken 流程（含 ContextOverflow 重试）共享同一个 think_runtime
        let (think_runtime, policy) =
            init_think_runtime_and_policy(agent, ThinkingScene::Awaken, &trace_id);

        // Step 2: 发布循环启动事件（AOP 同步转发）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::started(&agent.po.id, &trace_id, "awaken", Some(&message.po.id)),
        )
        .await;

        // Step 2.5: 工具已由 hr_domain.get_agent(with_tools=true) 加载到 agent.tools
        // 工具列表通过 OpenAI tools API 协议层传递（ToolDescriptor），不注入 Prompt

        // Step 2.6: 技能已由 hr_domain.get_agent(with_skills=true) 加载到 agent.skills
        // 技能只在 Agent 已安装的副本范围内（author_id = agent_id，排除 Expired）
        // 不匹配 match_keys 的技能不展示在 Prompt，由 Agent 通过 search_skill 神经工具按需加载
        let skill_pos: Vec<crate::models::skill::SkillPo> =
            agent.skills().iter().map(|s| s.po.clone()).collect();

        // 上一轮上下文压缩的产物。
        //
        // Some 时下一轮**不再查询历史记忆** —— 需要回顾的内容都在这份摘要里了，
        // 再查一遍既浪费预算又会与摘要重复（详见 compaction 模块说明）。
        // 若需要更早期的记忆，Agent 可用 search_memory 自行检索。
        let mut compacted_context: Option<String> = None;
        // 刚落库那条压缩记忆的 id：组装「更早的记忆」参考块时要排除它，
        // 否则它的内容会在【上一轮工作压缩结果】和参考块里各出现一次。
        let mut compacted_memory_id: Option<String> = None;

        // Step 3: 调用大脑思考（带工具调用循环 + 上下文压缩）
        // 统一走 BrainDal.think() 入口，方便审计、统计、监控
        // brain 由 run_think_loop 内部从 agent.brain 解析（四个场景一致）

        // 构建 ToolDescriptor 列表（从 agent.tools 直接派生，供模型 function calling）
        let tool_descriptors: Vec<ToolDescriptor> =
            agent.tools().iter().map(ToolDescriptor::from).collect();

        // 上下文压缩循环：当 think loop 返回 ContextOverflow 时，
        // 执行 sleep_and_settle 沉淀当前对话，然后重建 prompt 重新开始工作循环。
        // 轮次限制由 max_rounds 控制（跨压缩累计），超过则进入总结退出流程。
        let max_rounds = options.effective_max_rounds();
        let mut total_rounds: usize = 0;

        // 跟踪自上次压缩以来产生的 trace_id 列表（用于总结流程写入短期记忆）
        // 初始化为 awaken 自身的 trace_id（预生成，Step 6 才写入完整内容）
        let mut pending_trace_ids: Vec<String> = vec![trace_id.clone()];

        let mut prompt;
        let raw_output;
        // 正常完成时保存对话历史，用于触发总结流程写入短期记忆
        let mut final_messages: Option<Vec<ChatMessage>> = None;

        // =============== 移除强制两阶段唤醒（Phase 1）===============
        // 原 Phase 1 IntentAnalyze 有三个硬伤：
        //   1) 每条消息多花 1~3 轮模型调用，简单问候也付出 2x 首字延迟/token 成本；
        //   2) skip_memory_fetch = ia.is_some() 导致主循环首轮【历史对话】为空，
        //      多轮指代消解（这/那/上次/那个）直接失败；
        //   3) 唯一的潜在收益"澄清短路"只是 P4 TODO，从未启用；
        //      保留的【输入理解结果】区块还反复写了"仅供参考，可以忽略"。
        // 移除后，单 think_loop + System 角色的回复指引直接承担全部职责，
        // 流程与成本模型恢复直观。`analyze_input_intent` 接口仍保留在 trait，
        // 可在入站消息路由、跨 Agent 协作分发等需要结构化理解的场景独立复用。
        // =============== Phase 1 移除结束 ===============

        loop {
            // 上下文来源二选一：
            // - 刚做过压缩 → 直接用压缩结果，不再查历史记忆（避免与摘要重复占用预算）；
            //   同时补少量「更早的记忆」作为连续性线索
            // - 否则 → 取最近 20 条短期记忆，保证指代消解始终有上下文
            let recent_memories: Vec<_> = match &compacted_context {
                Some(_) => Vec::new(),
                None => {
                    self.memory()
                        .get_recent_context(ctx.clone(), &agent.po.id, 20)
                        .await?
                }
            };
            let past_memories: Vec<String> = match &compacted_context {
                Some(_) => {
                    self.query_past_memories(
                        ctx.clone(),
                        &agent.po.id,
                        compacted_memory_id.as_deref(),
                    )
                    .await
                }
                None => Vec::new(),
            };

            // 拼装 Prompt（通过工厂方法获取对应 Agent 类型的 builder）
            let mut builder = self.prompt_builder(agent);
            builder.current_trace_id(&trace_id);
            builder.system_prompt(agent);
            builder.skills(&skill_pos);
            let base = crate::config::get().base_data_path();
            let uid = ctx.uid();
            let uid_ref = if uid.is_empty() {
                None
            } else {
                Some(uid.as_str())
            };
            let default_workspace = paths::default_workspace(&base, uid_ref, Some(&agent.po.id))
                .to_string_lossy()
                .to_string();
            let user_home = if uid.is_empty() {
                paths::users_root_dir(&base).to_string_lossy().to_string()
            } else {
                paths::user_home(&base, &uid).to_string_lossy().to_string()
            };
            let user_shared_workspace = if uid.is_empty() {
                default_workspace.clone()
            } else {
                paths::user_shared_workspace(&base, &uid)
                    .to_string_lossy()
                    .to_string()
            };
            let user_agent_workspace = if uid.is_empty() {
                None
            } else {
                Some(
                    paths::user_agent_workspace(&base, &uid, &agent.po.id)
                        .to_string_lossy()
                        .to_string(),
                )
            };
            let agent_workspace = Some(
                paths::agent_workspace(&base, &agent.po.id)
                    .to_string_lossy()
                    .to_string(),
            );
            let project_workspace =
                if let (Some(project), true) = (&options.project, !uid.is_empty()) {
                    Some(
                        paths::user_project_workspace(&base, &uid, &project.po.id)
                            .to_string_lossy()
                            .to_string(),
                    )
                } else {
                    None
                };
            builder.workspace_context(
                default_workspace,
                user_home,
                user_shared_workspace,
                user_agent_workspace,
                agent_workspace,
                project_workspace,
            );
            if let Some(project) = &options.project {
                builder.project_context(project);
            }
            if let Some(task) = &options.task {
                builder.task_context(task);
            }
            if let Some(user) = &options.user_profile {
                builder.user_profile(user);
            }
            builder.history(&recent_memories);
            // 压缩产物直接注入：告诉模型「这是你上一轮工作的压缩结果」，
            // 无需再回顾历史记忆；需要更早的记忆时用 search_memory 检索。
            if let Some(summary) = &compacted_context {
                builder.compacted_context(summary);
            }
            if !past_memories.is_empty() {
                builder.past_memories_reference(&past_memories);
            }
            builder.current_message(message);
            prompt = builder.build();
            // P0-b：用 System + User 双角色分离的初始消息（而非整段塞进一条 User），
            // 配合 P0-a 的回复规则指引，让模型正确决定何时 Final 何时 ToolCall。
            // `prompt` 仍保留用于 trace/stat 原始输入记录（raw_input 字段）。
            let initial_messages = builder.build_initial_messages();

            // 调用共享 think loop（传入累计轮次和上限）
            // 注意：每次循环都重新设置 think_runtime，因为 sleep_and_settle 的 BusyGuard
            // Drop 会清理 think_runtime（set_resting → set_idle 链路）
            AgentRuntimeStateManager::global()
                .set_think_runtime(&agent.po.id, think_runtime.clone());
            let think_result = self
                .run_think_loop(
                    ThinkLoopParams::new(
                        ctx.clone(),
                        agent,
                        ThinkingScene::Awaken,
                        &trace_id,
                        initial_messages,
                        &tool_descriptors,
                    )
                    .with_rounds(max_rounds, total_rounds)
                    .with_monitoring(&think_runtime, policy.as_ref()),
                )
                .await;

            match think_result {
                Ok(ThinkLoopResult::Final { content, messages }) => {
                    raw_output = content;
                    final_messages = Some(messages);
                    break;
                }
                Ok(ThinkLoopResult::ContextOverflow {
                    messages,
                    input_tokens,
                    rounds_used,
                }) => {
                    total_rounds += rounds_used;

                    log_info!(
                        &ctx,
                        "awaken",
                        "context overflow (total_rounds={}, tokens={}), triggering compaction",
                        total_rounds,
                        input_tokens
                    );

                    // 上下文压缩：复用主循环完整上下文，尾部追加压缩指令。
                    //
                    // 不重建 Prompt 而选择「追加」的原因：追加前的前缀与上一次模型调用
                    // 逐字节一致，可命中 provider 侧 prefix caching；同时模型看到的是
                    // 完整原始对话，而非被按条截断过的二手摘要。
                    //
                    // 与 sleep_and_settle 的关键差异：**不操作 Agent 运行时状态**。
                    // 压缩发生在 awaken 主循环内部，Agent 必须保持 Busy —— 若走
                    // sleep_and_settle 会被切成 Resting 再掉成 Idle，导致主循环仍在
                    // 思考时 `is_unavailable()` 返回 false，新消息可并发唤醒同一 Agent。
                    match self
                        .compact_context(
                            ctx.clone(),
                            &messages,
                            agent,
                            &trace_id,
                            &pending_trace_ids,
                            false,
                        )
                        .await
                    {
                        Ok(outcome) => {
                            // 压缩成功：重置待沉淀范围，下次压缩/总结的边界 = 自本次压缩起
                            pending_trace_ids = vec![trace_id.clone()];
                            // 压缩结果直接交给下一轮，不再重新查询历史记忆
                            compacted_context = outcome.compacted_summary;
                            compacted_memory_id = outcome.compacted_memory_id;
                        }
                        Err(e) => {
                            log_warn!(
                                &ctx,
                                "awaken",
                                "compaction failed: {:?}, continuing with retry",
                                e
                            );
                        }
                    }
                    // 压缩完成后循环继续：下一轮用【上一轮工作压缩结果】重建 prompt
                    continue;
                }
                Ok(ThinkLoopResult::MaxRoundsExceeded {
                    messages,
                    total_rounds: rounds,
                }) => {
                    total_rounds = rounds;

                    log_info!(
                        &ctx,
                        "awaken",
                        "max rounds exceeded (total={}), entering compaction exit flow",
                        total_rounds
                    );

                    // 轮次耗尽 → 走压缩流程。压缩本身就是一次完整总结，
                    // 无需再跑一遍 awaken_for_summary：既省一次模型调用，
                    // 也避免同一段对话被两套提示词重复总结。
                    //
                    // with_user_reply = true：压缩产出会作为 raw_output 发给用户，
                    // 指令里要求模型最后用一句话说明进展与停止原因。
                    let mut compact_trace_ids = pending_trace_ids.clone();
                    if compact_trace_ids.last() != Some(&trace_id) {
                        compact_trace_ids.push(trace_id.clone());
                    }
                    let compact_outcome = self
                        .compact_context(
                            ctx.clone(),
                            &messages,
                            agent,
                            &trace_id,
                            &compact_trace_ids,
                            true,
                        )
                        .await
                        .unwrap_or_else(|e| {
                            log_warn!(&ctx, "awaken", "compaction exit failed: {:?}", e);
                            CompactOutcome {
                                final_text: String::new(),
                                compacted_summary: None,
                                compacted_memory_id: None,
                            }
                        });

                    // 用 Final 文本作为 raw_output（会发送给用户）。
                    // 摘要在 <compacted_summary> 标记内，属内部产物，发给用户前剥掉。
                    let user_text = strip_summary_block(&compact_outcome.final_text);
                    raw_output = if user_text.is_empty() {
                        "任务因思考轮次耗尽而终止，已执行上下文压缩退出流程。".to_string()
                    } else {
                        user_text
                    };
                    break;
                }
                Ok(ThinkLoopResult::Cancelled {
                    total_rounds: rounds,
                    ..
                }) => {
                    // 用户取消：直接返回，不触发总结流程
                    log_info!(
                        &ctx,
                        "awaken",
                        "agent thinking cancelled by user, rounds={}",
                        rounds
                    );
                    let duration_ms = start_time
                        .elapsed()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    if let Err(stats_err) = record_event!(
                        ctx,
                        AgentAwakeEvent {
                            agent_id: agent.po.id.clone(),
                            project_id: ctx.project_id().cloned(),
                            task_id: ctx.task_id().cloned(),
                            organization_id: ctx.organization_id.clone(),
                            user_id: Some(ctx.uid()),
                            message_id: Some(message.po.id.clone()),
                            call_count: 1,
                            duration_ms: duration_ms,
                            status: "cancelled".to_string(),
                            exit_reason: "cancelled".to_string(),
                        }
                    ) {
                        log_warn!(
                            &ctx,
                            "awaken",
                            "record_event failed on cancel path: {:?}",
                            stats_err
                        );
                    }
                    let _ = crate::pkg::aop::publish(
                        &ctx,
                        AgentLoopEvent::finished(
                            &agent.po.id,
                            &trace_id,
                            "awaken",
                            "cancelled",
                            duration_ms,
                            Some(&message.po.id),
                        ),
                    )
                    .await;
                    return Ok(AwakeningResult {
                        agent_id: agent.po.id.clone(),
                        trace_ids: vec![trace_id.clone()],
                        raw_input: prompt.clone(),
                        raw_output: String::new(),
                    });
                }
                Err(e) => {
                    // think loop 执行失败
                    let duration_ms = start_time
                        .elapsed()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    if let Err(stats_err) = record_event!(
                        ctx,
                        AgentAwakeEvent {
                            agent_id: agent.po.id.clone(),
                            project_id: ctx.project_id().cloned(),
                            task_id: ctx.task_id().cloned(),
                            organization_id: ctx.organization_id.clone(),
                            user_id: Some(ctx.uid()),
                            message_id: Some(message.po.id.clone()),
                            call_count: 1,
                            duration_ms: duration_ms,
                            status: format!("failed: {}", e),
                            exit_reason: "error".to_string(),
                        }
                    ) {
                        log_warn!(
                            &ctx,
                            "awaken",
                            "record_event failed on error path, stats may be incomplete: {:?}",
                            stats_err
                        );
                    }
                    let _ = crate::pkg::aop::publish(
                        &ctx,
                        AgentLoopEvent::finished(
                            &agent.po.id,
                            &trace_id,
                            "awaken",
                            &format!("failed: {}", e),
                            duration_ms,
                            Some(&message.po.id),
                        ),
                    )
                    .await;
                    return Err(e);
                }
            }
        }

        // Step 5: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // 补充运行时元数据，便于 trace 检索与场景区分
        let exit_reason = if final_messages.is_some() {
            "Final"
        } else {
            "MaxRoundsExceeded"
        };
        trace.metadata.insert("scene".into(), "awaken".into());
        trace
            .metadata
            .insert("message_id".into(), message.po.id.clone());
        trace
            .metadata
            .insert("exit_reason".into(), exit_reason.into());
        trace
            .metadata
            .insert("rounds_used".into(), total_rounds.to_string());
        if let Some(task_id) = ctx.task_id() {
            trace.metadata.insert("task_id".into(), task_id.clone());
        }
        if let Some(project_id) = ctx.project_id() {
            trace
                .metadata
                .insert("project_id".into(), project_id.clone());
        }

        // Step 6: 通过 RuntimeMemory 子模块写入
        // 架构：awakening → RuntimeMemory → MemoryDal → MemoryDao
        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 6.5: 正常完成时压缩本次对话，写入短期记忆
        // 仅在 Final 分支（正常完成）时触发，MaxRoundsExceeded 已在循环内执行过压缩
        // 目的：将本次工作对话压缩为一条短期记忆，trace_ids 记录依赖的 trace 列表
        if let Some(messages) = final_messages {
            let _ = self
                .compact_context(
                    ctx.clone(),
                    &messages,
                    agent,
                    &trace_id,
                    &pending_trace_ids,
                    false,
                )
                .await
                .map_err(|e| {
                    log_warn!(
                        &ctx,
                        "awaken",
                        "post-completion compaction failed: {:?}, continuing (non-fatal)",
                        e
                    );
                });
            // 压缩失败不影响业务返回（awaken 已成功），仅记录警告
        }

        // Step 7: 记录 Agent 唤醒统计事件
        // 统计写入失败不应阻塞业务返回（awaken 已成功），仅记录警告
        let duration_ms = start_time
            .elapsed()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if let Err(stats_err) = record_event!(
            ctx,
            AgentAwakeEvent {
                agent_id: agent.po.id.clone(),
                project_id: ctx.project_id().cloned(),
                task_id: ctx.task_id().cloned(),
                organization_id: ctx.organization_id.clone(),
                user_id: Some(ctx.uid()),
                message_id: Some(message.po.id.clone()),
                call_count: 1,
                duration_ms: duration_ms,
                status: "success".to_string(),
                exit_reason: exit_reason.to_lowercase(),
            }
        ) {
            log_warn!(
                &ctx,
                "awaken",
                "record_event failed on success path, stats may be incomplete: {:?}",
                stats_err
            );
        }

        // 发布循环完成事件（成功）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::finished(
                &agent.po.id,
                &trace_id,
                "awaken",
                "success",
                duration_ms,
                Some(&message.po.id),
            ),
        )
        .await;

        // Step 8: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id],
            raw_input: prompt,
            raw_output,
        })
    }

    /// 让 Agent 进入沉睡模式，执行记忆沉淀（与 awaken 对称）
    ///
    /// awaken 是醒来响应外部消息，sleep_and_settle 是沉睡整理内部记忆。
    /// 流程：set_resting → 读取历史 → 拼装沉淀 Prompt → think → 写 Trace → set_idle
    ///
    /// 与 awaken 的关键差异：
    /// - 状态用 Resting（而非 Busy），通过 BusyGuard 的 set_idle 恢复（语义一致）
    /// - 统计事件的 message_id 为 None（沉淀无关联消息）
    ///
    /// **只服务「真正的睡觉」**（定时任务 `agent_rest` / 神经工具触发）：
    /// 把近期未沉淀的短期记忆整理进知识图谱。
    /// 主循环的上下文压缩走 `compact_context()`，不复用本方法——压缩要保持 Busy
    /// 状态且复用主循环上下文，语义与成本模型都不同。
    async fn sleep_and_settle(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        pending_memories_summary: &str,
        options: &ThinkingOptions,
        trace_ids: &[String],
    ) -> Result<AwakeningResult> {
        let start_time = std::time::SystemTime::now();

        // 使用 Resting 状态（而非 Busy），RAII guard 恢复 Idle
        // BusyGuard 的 Drop 行为是 set_idle，与 Resting 恢复语义一致，直接复用
        AgentRuntimeStateManager::global().set_resting(&agent.po.id);
        let _rest_guard =
            crate::service::domain::runtime::busy_guard::BusyGuard::new(agent.po.id.clone());

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        // Step 1: 取少量「已沉淀」记忆作为参考线索
        //
        // 刻意**不取**【历史对话】全量（原为最近 20 条短期记忆）：它与本次
        // 【待沉淀的短期记忆】大面积重复（Active ≤ 20 条时完全重复），
        // 白白吃掉上下文预算，还会让刚做完的预算自适应打折。
        //
        // 图谱里有什么、曾经沉淀过什么，技能已要求 Agent 用 search_memory 按需检索；
        // 这里只留少量「顺手可见」的线索，用于衔接上次沉淀被预算截断的情况。
        let settled_reference = self
            .query_settled_reference(ctx.clone(), &agent.po.id)
            .await;

        // Step 2: 预先构造 MemoryTrace 拿到 trace_id
        use common::enums::MemoryRole;
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(), // input 后续填充
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // 创建沉淀场景的思考运行时（覆盖 awaken 的，因为这是一个独立思考阶段）
        let (think_runtime, policy) =
            init_think_runtime_and_policy(agent, ThinkingScene::Settle, &trace_id);

        // 发布循环启动事件（AOP 同步转发）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::started(&agent.po.id, &trace_id, "settle", None),
        )
        .await;

        // Step 3: 加载技能（已由 hr_domain.get_agent 加载到 agent）
        // Settle 场景过滤：只保留记忆相关 skill（tags 含 neural 或 memory），
        // 确保沉淀模式下只能接触记忆类工具。
        // 睡觉是对自身知识的沉淀积累，不应触发消息流程导致异步唤醒自己。
        let scene = options.scene;
        let skill_pos = build_scene_skills(agent, scene);

        // Step 4: 拼装 Prompt（复用 builder 挂载链路，调用 build_sleep_prompt 生成沉淀模板）
        // 与 awaken 的区别：不构造虚拟 System Message，约束模板内聚在 builder.build_sleep_prompt
        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(agent);
        builder.skills(&skill_pos);
        let base = crate::config::get().base_data_path();
        let uid = ctx.uid();
        let uid_ref = if uid.is_empty() {
            None
        } else {
            Some(uid.as_str())
        };
        let default_workspace = paths::default_workspace(&base, uid_ref, Some(&agent.po.id))
            .to_string_lossy()
            .to_string();
        let user_home = if uid.is_empty() {
            paths::users_root_dir(&base).to_string_lossy().to_string()
        } else {
            paths::user_home(&base, &uid).to_string_lossy().to_string()
        };
        let user_shared_workspace = if uid.is_empty() {
            default_workspace.clone()
        } else {
            paths::user_shared_workspace(&base, &uid)
                .to_string_lossy()
                .to_string()
        };
        let user_agent_workspace = if uid.is_empty() {
            None
        } else {
            Some(
                paths::user_agent_workspace(&base, &uid, &agent.po.id)
                    .to_string_lossy()
                    .to_string(),
            )
        };
        let agent_workspace = Some(
            paths::agent_workspace(&base, &agent.po.id)
                .to_string_lossy()
                .to_string(),
        );
        let project_workspace = if let (Some(project), true) = (&options.project, !uid.is_empty()) {
            Some(
                paths::user_project_workspace(&base, &uid, &project.po.id)
                    .to_string_lossy()
                    .to_string(),
            )
        } else {
            None
        };
        builder.workspace_context(
            default_workspace,
            user_home,
            user_shared_workspace,
            user_agent_workspace,
            agent_workspace,
            project_workspace,
        );
        // 沉淀场景不装配 history（避免与待沉淀列表重复），只挂参考条目
        builder.settled_reference(&settled_reference);

        let prompt = builder.build_sleep_prompt(pending_memories_summary, trace_ids);
        // P0-b：角色拆分版初始消息；prompt 仍保留用于 trace/stat 记录
        let initial_messages =
            builder.build_sleep_initial_messages(pending_memories_summary, trace_ids);

        // Step 5: 调用大脑思考（带工具调用循环，与 awaken 对称）
        // sleep 场景传递过滤后的记忆工具，Agent 可通过 function calling 调用记忆工具完成沉淀
        // brain 由 run_think_loop 内部从 agent.brain 解析

        // 构建 ToolDescriptor 列表（从 agent.tools 按场景过滤后派生，供模型 function calling）
        let tool_descriptors = build_scene_tool_descriptors(agent, scene);

        // 调用共享 think loop（轮次与超时由 ThinkLoopParams::new 按 Agent 配置填充）
        let think_result = self
            .run_think_loop(
                ThinkLoopParams::new(
                    ctx.clone(),
                    agent,
                    ThinkingScene::Settle,
                    &trace_id,
                    initial_messages,
                    &tool_descriptors,
                )
                .with_monitoring(&think_runtime, policy.as_ref()),
            )
            .await;

        // 展开 Result，失败时也记录事件
        // sleep_and_settle 场景不处理 ContextOverflow/MaxRoundsExceeded/Cancelled（沉淀工具量小）
        let raw_output = match think_result {
            Ok(ThinkLoopResult::Final { content, .. }) => content,
            Ok(ThinkLoopResult::ContextOverflow { .. })
            | Ok(ThinkLoopResult::MaxRoundsExceeded { .. })
            | Ok(ThinkLoopResult::Cancelled { .. }) => {
                // 沉淀场景理论不会触发（记忆工具量小），兜底处理
                String::new()
            }
            Err(e) => {
                let duration_ms = start_time
                    .elapsed()
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                if let Err(stats_err) = record_event!(
                    ctx,
                    AgentAwakeEvent {
                        agent_id: agent.po.id.clone(),
                        project_id: None,
                        task_id: None,
                        organization_id: ctx.organization_id.clone(),
                        user_id: Some(ctx.uid()),
                        message_id: None,
                        call_count: 1,
                        duration_ms: duration_ms,
                        status: format!("settle failed: {}", e),
                        exit_reason: "error".to_string(),
                    }
                ) {
                    log_warn!(
                        &ctx,
                        "sleep_and_settle",
                        "record_event failed on error path, stats may be incomplete: {:?}",
                        stats_err
                    );
                }
                // 发布循环完成事件（失败）
                let _ = crate::pkg::aop::publish(
                    &ctx,
                    AgentLoopEvent::finished(
                        &agent.po.id,
                        &trace_id,
                        "settle",
                        &format!("settle failed: {}", e),
                        duration_ms,
                        None,
                    ),
                )
                .await;
                return Err(e);
            }
        };

        // Step 6: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // 补充运行时元数据
        trace.metadata.insert("scene".into(), "settle".into());
        trace.metadata.insert(
            "depended_trace_ids".into(),
            serde_json::to_string(trace_ids).unwrap_or_default(),
        );
        if let Some(task_id) = ctx.task_id() {
            trace.metadata.insert("task_id".into(), task_id.clone());
        }
        if let Some(project_id) = ctx.project_id() {
            trace
                .metadata
                .insert("project_id".into(), project_id.clone());
        }

        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 7: 记录沉淀统计事件
        let duration_ms = start_time
            .elapsed()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if let Err(stats_err) = record_event!(
            ctx,
            AgentAwakeEvent {
                agent_id: agent.po.id.clone(),
                project_id: None,
                task_id: None,
                organization_id: ctx.organization_id.clone(),
                user_id: Some(ctx.uid()),
                message_id: None,
                call_count: 1,
                duration_ms: duration_ms,
                status: "settle success".to_string(),
                exit_reason: "settle".to_string(),
            }
        ) {
            log_warn!(
                &ctx,
                "sleep_and_settle",
                "record_event failed on success path, stats may be incomplete: {:?}",
                stats_err
            );
        }

        // 发布循环完成事件（成功）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::finished(
                &agent.po.id,
                &trace_id,
                "settle",
                "settle success",
                duration_ms,
                None,
            ),
        )
        .await;

        // Step 8: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id],
            raw_input: prompt,
            raw_output,
        })
    }

    async fn analyze_input_intent(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
    ) -> Result<IntentAnalysis> {
        let start_time = std::time::SystemTime::now();
        let ctx = enrich_ctx!(&ctx, agent);
        let trace_id = format!("intent-analyze-{}", ctx.log_id);

        // 发布循环启动事件（与 awaken/sleep_and_settle 对齐，监控可区分 Phase 1）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::started(&agent.po.id, &trace_id, "intent-analyze", None),
        )
        .await;

        let result = self
            .analyze_input_intent_inner(ctx.clone(), agent, message, options, &trace_id)
            .await;

        let duration_ms = start_time
            .elapsed()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let status = match &result {
            Ok(_) => "success".to_string(),
            Err(e) => format!("failed: {}", e),
        };

        // 发布循环完成事件（成功/失败均发布，监控可追踪 Phase 1 耗时与状态）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::finished(
                &agent.po.id,
                &trace_id,
                "intent-analyze",
                &status,
                duration_ms,
                None,
            ),
        )
        .await;

        result
    }
}

#[cfg(test)]
mod tests {
    // ==================== awaken 集成测试 ====================

    use super::ThinkingOptions;
    use crate::models::agent::{Agent, AgentPo};
    use crate::models::brain::Brain;
    use crate::models::file::FileMeta;
    use crate::models::message::Message;
    use crate::models::model_provider::ModelProvider;
    use crate::models::skill::SkillPo;
    use crate::pkg::RequestContext;
    use crate::pkg::tool_tracing::logger::ToolCallLogger;
    use crate::service::dal::brain::BrainDal;
    use crate::service::domain::runtime::tool_execution_test::credential_stubs::{
        StubLarkCredentialDal, StubUserDal,
    };
    use async_trait::async_trait;
    use common::enums::skill::SkillAuthorType;
    use common::enums::{AgentStatus, MessageRole, MessageType, ModelCapability, ProviderType};
    use sqlx::SqlitePool;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use uuid::Uuid;

    /// 捕获 Prompt 的 BrainDal Stub
    ///
    /// 在 think() 调用时捕获传入的 prompt，返回固定响应
    struct CapturingBrainDal {
        captured_prompt: Arc<Mutex<Option<String>>>,
    }

    impl CapturingBrainDal {
        fn new(captured_prompt: Arc<Mutex<Option<String>>>) -> Self {
            Self { captured_prompt }
        }
    }

    #[async_trait]
    impl BrainDal for CapturingBrainDal {
        async fn wake_brain(
            &self,
            _ctx: RequestContext,
            _agent: &AgentPo,
            _memories: Vec<crate::models::memory::Memory>,
        ) -> common::error::Result<Brain> {
            unimplemented!("not needed by awaken skill tests")
        }

        async fn test_connection(
            &self,
            _ctx: RequestContext,
            _provider: &ModelProvider,
            _prompt: &str,
        ) -> common::error::Result<String> {
            unimplemented!("not needed by awaken skill tests")
        }

        async fn think(
            &self,
            _ctx: RequestContext,
            _brain: &Brain,
            messages: &[crate::models::cortex_types::ChatMessage],
            _tools: &[crate::models::cortex_types::ToolDescriptor],
        ) -> common::error::Result<crate::models::cortex_types::ThinkResult> {
            // 把所有初始消息的 content 按顺序拼接（等价旧版扁平 build() 输出），
            // 保证原有断言（soul 是否注入 / skills 是否出现 / user_profile 是否携带）
            // 在 System/User 角色拆分后仍能通过。
            use crate::models::cortex_types::ChatMessage;
            let prompt: String = messages
                .iter()
                .filter_map(|m| match m {
                    ChatMessage::System { content } => Some(content.as_str()),
                    ChatMessage::User { content } => Some(content.as_str()),
                    ChatMessage::Assistant { content, .. } => content.as_deref(),
                    ChatMessage::Tool { content, .. } => Some(content.as_str()),
                })
                .collect::<Vec<_>>()
                .join("\n");
            *self.captured_prompt.lock().unwrap() = Some(prompt);
            Ok(crate::models::cortex_types::ThinkResult::Final {
                content: "mock response".to_string(),
                usage: crate::models::cortex_types::TokenUsage::default(),
            })
        }

        async fn embed_entity(
            &self,
            _ctx: RequestContext,
            _entity: &dyn crate::models::vector::Vectorizable,
        ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
            Ok(None)
        }

        async fn embed_text_for_search(
            &self,
            _ctx: RequestContext,
            _text: &str,
        ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
            Ok(None)
        }
    }

    /// 初始化测试环境：所有 DAO + DAL 单例
    fn init_awaken_test_env(pool: SqlitePool) -> RequestContext {
        // 必须先初始化 config（文件操作需要 base_data_path）
        let _ = crate::config::init();

        // 初始化所有 DAO
        crate::service::dao::agent::init();
        crate::service::dao::tool::init();
        crate::service::dao::skill::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::cortex::init();
        crate::service::dao::memory::init();
        crate::service::dao::mcp_server::init();

        // 初始化所有 DAL
        crate::service::dal::agent::init();
        crate::service::dal::tool::init();
        crate::service::dal::skill::init();
        crate::service::dal::model_provider::init();
        crate::service::dal::memory::init();
        crate::service::dal::mcp_tool::init();
        crate::service::dal::brain::init();

        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    /// 创建带 Brain 的测试 Agent
    fn make_test_agent(agent_id: &str) -> Agent {
        let mut po = AgentPo::new(
            "Test Agent".to_string(),
            vec!["assistant".to_string()],
            "Test description".to_string(),
            vec!["chat".to_string()],
            "Test soul".to_string(),
            "provider-001".to_string(),
            "test-user".to_string(),
        );
        po.id = agent_id.to_string();
        po.status = AgentStatus::Onboarded;

        let mut agent = Agent::from_po(po);
        let model_provider_po = crate::models::model_provider::ModelProviderPo {
            id: "mock-provider".to_string(),
            name: "Mock Provider".to_string(),
            provider_type: ProviderType::OpenAI,
            model_name: "gpt-4".to_string(),
            capability: ModelCapability::Agent,
            api_key: "fake-key".to_string(),
            base_url: None,
            description: None,
            config: "{}".to_string(),
            status: common::enums::ModelProviderStatus::Normal,
            created_by: "test-user".to_string(),
            modified_by: "test-user".to_string(),
            created_at: 0,
            updated_at: 0,
        };
        let runtime_config = crate::models::agent::AgentRuntimeConfig::default();
        agent.brain = Some(Brain::new_local(
            agent_id.to_string(),
            "Test Agent".to_string(),
            runtime_config,
            model_provider_po,
            vec![],
        ));
        agent
    }

    /// 创建测试文本消息
    fn make_test_message(content: &str) -> Message {
        Message::new_with_context(
            Uuid::now_v7().to_string(),
            None,
            None,
            "test-user".to_string(),
            "test-agent".to_string(),
            MessageRole::User,
            MessageRole::Agent,
            MessageType::Text,
            content.to_string(),
            None,
            FileMeta::default(),
            None,
            None,
            None,
            "test-user".to_string(),
        )
    }

    /// 在数据库中为 Agent 创建技能副本
    ///
    /// skills tags 包含 "assistant" 以匹配 Agent 的 role，确保出现在"必加载技能"区块
    async fn create_skill_for_agent(
        ctx: RequestContext,
        agent_id: &str,
        name: &str,
        description: &str,
    ) {
        let skill_po = SkillPo::new(
            format!("skill-{}--{}", name.to_lowercase(), Uuid::new_v4()),
            name.to_string(),
            description.to_string(),
            vec!["assistant".to_string()],
            "test".to_string(),
            String::new(),
            agent_id.to_string(),
            SkillAuthorType::Agent,
            format!("skills/{}", name.to_lowercase()),
        );
        crate::service::dal::skill::dal()
            .create(ctx, &skill_po)
            .await
            .expect("创建测试技能失败");
    }

    /// 模拟 hr_domain.get_agent(with_skills=true) 的技能加载
    ///
    /// 生产路径由 hr_domain 加载 Skill 业务实体写入 agent.skills，测试中直接查 DB 填充
    async fn load_skills_to_agent(ctx: RequestContext, agent: &mut Agent) {
        use common::enums::SkillStatus;
        let skills = crate::service::dal::skill::dal()
            .query(
                ctx,
                crate::service::dao::skill::SkillQuery {
                    author_id: Some(agent.po.id.clone()),
                    exclude_status: Some(SkillStatus::Expired),
                    ..Default::default()
                },
            )
            .await
            .expect("加载技能失败");
        agent.set_skills(skills.items);
    }

    #[sqlx::test]
    async fn test_awaken_with_skills(pool: SqlitePool) {
        let ctx = init_awaken_test_env(pool);

        let agent_id = format!("agent-with-skills-{}", Uuid::now_v7());
        let mut agent = make_test_agent(&agent_id);

        // 为 Agent 创建 2 个技能副本
        create_skill_for_agent(
            ctx.clone(),
            &agent_id,
            "CodeReview",
            "审查代码质量并给出改进建议",
        )
        .await;
        create_skill_for_agent(ctx.clone(), &agent_id, "DocWriting", "编写清晰的技术文档").await;

        // 模拟 hr_domain.get_agent(with_skills=true) 加载技能到 agent.skills
        load_skills_to_agent(ctx.clone(), &mut agent).await;

        let message = make_test_message("请帮我审查这段代码");

        let captured_prompt = Arc::new(Mutex::new(None));
        let temp_dir = tempdir().expect("tempdir should be created");
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
            crate::service::dal::tool::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::agent::dal(),
            Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        let result = runtime
            .awakening()
            .awaken(ctx.clone(), &agent, &message, &ThinkingOptions::new())
            .await
            .expect("awaken 应该成功");

        let prompt = captured_prompt
            .lock()
            .unwrap()
            .clone()
            .expect("应该捕获到 prompt");

        // 验证 Prompt 包含"【必加载技能】"部分（tags 匹配 agent role "assistant"）
        assert!(
            prompt.contains("【必加载技能】"),
            "Prompt 应该包含【必加载技能】部分，实际: {}",
            prompt
        );
        // 验证两个技能都出现在 Prompt 中
        assert!(
            prompt.contains("CodeReview"),
            "Prompt 应该包含技能 CodeReview"
        );
        assert!(
            prompt.contains("审查代码质量并给出改进建议"),
            "Prompt 应该包含 CodeReview 的描述"
        );
        assert!(
            prompt.contains("DocWriting"),
            "Prompt 应该包含技能 DocWriting"
        );
        assert!(
            prompt.contains("编写清晰的技术文档"),
            "Prompt 应该包含 DocWriting 的描述"
        );

        // 验证返回结果
        assert_eq!(result.agent_id, agent_id);
        assert!(!result.raw_input.is_empty());
        assert_eq!(result.raw_output, "mock response");
    }

    #[sqlx::test]
    async fn test_awaken_without_skills(pool: SqlitePool) {
        let ctx = init_awaken_test_env(pool);

        let agent_id = format!("agent-no-skills-{}", Uuid::now_v7());
        let agent = make_test_agent(&agent_id);

        // 不为 Agent 创建任何技能
        let message = make_test_message("你好");

        let captured_prompt = Arc::new(Mutex::new(None));
        let temp_dir = tempdir().expect("tempdir should be created");
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
            crate::service::dal::tool::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::agent::dal(),
            Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        let result = runtime
            .awakening()
            .awaken(ctx.clone(), &agent, &message, &ThinkingOptions::new())
            .await
            .expect("awaken 应该成功");

        let prompt = captured_prompt
            .lock()
            .unwrap()
            .clone()
            .expect("应该捕获到 prompt");

        // 验证 Prompt 不包含技能相关区块（Agent 无技能）
        assert!(
            !prompt.contains("【必加载技能】") && !prompt.contains("【神经技能】"),
            "Prompt 不应该包含技能区块（Agent 无技能），实际: {}",
            prompt
        );

        // 验证返回结果仍然正常
        assert_eq!(result.agent_id, agent_id);
        assert!(!result.raw_input.is_empty());
        assert_eq!(result.raw_output, "mock response");
    }

    #[sqlx::test]
    async fn test_awaken_with_user_profile(pool: SqlitePool) {
        let ctx = init_awaken_test_env(pool);

        let agent_id = format!("agent-user-profile-{}", Uuid::now_v7());
        let agent = make_test_agent(&agent_id);
        let message = make_test_message("你好");

        // 构造带自述偏好的用户画像
        let mut user_po = crate::models::user::UserPo::new(
            "test-user".to_string(),
            "org-1".to_string(),
            "tester".to_string(),
            "测试用户".to_string(),
            "tester@example.com".to_string(),
            "hash".to_string(),
            common::enums::UserRole::Member,
            "system".to_string(),
        );
        user_po.preferences = "- 回复请用中文".to_string();

        let captured_prompt = Arc::new(Mutex::new(None));
        let temp_dir = tempdir().expect("tempdir should be created");
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
            crate::service::dal::tool::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::agent::dal(),
            Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        runtime
            .awakening()
            .awaken(
                ctx.clone(),
                &agent,
                &message,
                &ThinkingOptions::new().with_user_profile(user_po),
            )
            .await
            .expect("awaken 应该成功");

        let prompt = captured_prompt
            .lock()
            .unwrap()
            .clone()
            .expect("应该捕获到 prompt");

        // 【用户画像】区块含基础信息 + 自述偏好
        assert!(
            prompt.contains("【用户画像】"),
            "Prompt 应该包含【用户画像】区块，实际: {}",
            prompt
        );
        assert!(
            prompt.contains("【用户偏好】- 回复请用中文"),
            "【用户画像】应包含用户自述偏好，实际: {}",
            prompt
        );
    }

    #[test]
    fn thinking_scene_tool_whitelist() {
        use common::enums::ThinkingScene;

        let scene = ThinkingScene::IntentAnalyze;

        // 允许：工具名 tag 包含 vector_search / query_memory / search / analyze
        let allowed_tags: Vec<String> = vec![
            "vector_search".into(),
            "query_memory".into(),
            "search_memory".into(),
            "analyze_text".into(),
        ];
        for tag in allowed_tags {
            assert!(
                scene.is_tool_allowed(&[tag]),
                "tag should be allowed in IntentAnalyze scene"
            );
        }

        // 禁止：工具名 tag 包含 shell_exec / lark_push
        let forbidden_tags: Vec<String> = vec![
            "shell_exec".into(),
            "lark_push".into(),
            "send_message".into(),
        ];
        for tag in forbidden_tags {
            assert!(
                !scene.is_tool_allowed(&[tag]),
                "tag should be forbidden in IntentAnalyze scene"
            );
        }
    }

    #[test]
    fn intent_analysis_json_roundtrip() {
        use super::IntentAnalysis;
        use serde_json;

        let ia = IntentAnalysis {
            intent_type: "TaskRequest".into(),
            confidence: 0.85,
            key_terms: vec!["项目X".into(), "方案A".into(), "进度".into()],
            resolutions: vec!["\"上次那个方案\" → project=123, task=456".into()],
            retrieved_context: vec![
                "2026-08-10 方案 A/B 比较结论，推荐方案 A（相似度 0.88）".into(),
            ],
            need_clarification: vec![],
            summary: "用户想知道项目 X 方案 A 的当前推进进度".into(),
        };

        let json_str = serde_json::to_string(&ia).expect("serialize IntentAnalysis");
        let ia2: IntentAnalysis =
            serde_json::from_str(&json_str).expect("deserialize IntentAnalysis");

        assert_eq!(ia.intent_type, ia2.intent_type);
        assert!((ia.confidence - ia2.confidence).abs() < 0.0001);
        assert_eq!(ia.key_terms, ia2.key_terms);
        assert_eq!(ia.resolutions, ia2.resolutions);
        assert_eq!(ia.retrieved_context, ia2.retrieved_context);
        assert_eq!(ia.need_clarification, ia2.need_clarification);
        assert_eq!(ia.summary, ia2.summary);
    }

    #[test]
    fn parse_intent_analysis_json_level4_fallback() {
        use super::{IntentAnalysis, parse_intent_analysis_json};

        // 模拟 LLM 输出：大量中文思考 + JSON 代码块包裹（Level 2 代码块降级）
        let input = r#"我先分析一下用户的意图...好的，现在整理成结构化结果：
用户明显是在追问之前的内容，我归类为 FollowUp 型。
以下是 JSON 输出：
```json
{
  "intent_type": "FollowUp",
  "confidence": 0.82,
  "key_terms": ["项目X", "方案A", "进度", "上次那个方案"],
  "resolutions": ["\"上次那个方案\" → project=proj_123, task=task_456"],
  "retrieved_context": ["通过 search_memory 查到 2026-08-10 记忆：推荐方案 A"],
  "need_clarification": [],
  "summary": "用户想知道项目 X 中方案 A 的推进情况"
}
```
好的，以上就是我的分析结论。"#;

        let ia: IntentAnalysis = parse_intent_analysis_json(input)
            .expect("level4 fallback should parse successfully via code block extraction");

        assert_eq!(ia.intent_type, "FollowUp");
        assert!((ia.confidence - 0.82).abs() < 0.0001);
        assert_eq!(ia.key_terms.len(), 4);
        assert_eq!(ia.key_terms[0], "项目X");
        assert_eq!(ia.resolutions.len(), 1);
        assert!(ia.need_clarification.is_empty());
        assert_eq!(ia.summary, "用户想知道项目 X 中方案 A 的推进情况");
    }

    #[test]
    fn parse_intent_analysis_json_balanced_braces() {
        use super::{IntentAnalysis, extract_first_json_object, parse_intent_analysis_json};

        // 0. 测试 extract_first_json_object 基础能力：多个 JSON 时提取第一个
        let multi = r#"prefix {"a":1} middle {"b":2} suffix"#;
        assert_eq!(extract_first_json_object(multi), Some(r#"{"a":1}"#));

        // 1. 测试字符串内部的大括号不会干扰括号计数
        let with_inner_braces = r#"文本开头 {"key":"val{ue}"} 文本结尾"#;
        assert_eq!(
            extract_first_json_object(with_inner_braces),
            Some(r#"{"key":"val{ue}"}"#)
        );

        // 2. 平衡括号降级测试：有效 JSON 埋在中文散文里，无代码块
        let input = r#"经过 Step1 到 Step5 的仔细思考，我得出以下理解结论。
首先对用户意图进行归类，认为属于 Question 类型（问答型），置信度较高。
指代消解部分：没有明显的歧义短语，上下文清晰。
关键词抽取完毕。语义检索已完成，有如下结果摘要。
最终 JSON 结果如下：{"intent_type":"Question","confidence":0.9,"key_terms":["排期","项目X"],"resolutions":[],"retrieved_context":["查到项目X的排期计划：周五截止"],"need_clarification":["排期是指哪个版本的？（A：V1.2；B：V1.3）"],"summary":"用户询问项目X的排期，需要澄清版本信息"}如果还需要补充信息请及时告诉我。"#;

        let ia: IntentAnalysis = parse_intent_analysis_json(input)
            .expect("balanced braces fallback should parse successfully");

        assert_eq!(ia.intent_type, "Question");
        assert!((ia.confidence - 0.9).abs() < 0.0001);
        assert_eq!(ia.key_terms, vec!["排期", "项目X"]);
        assert_eq!(ia.need_clarification.len(), 1);
        assert!(ia.need_clarification[0].contains("排期是指哪个版本"));
        assert_eq!(ia.summary, "用户询问项目X的排期，需要澄清版本信息");
    }
}
