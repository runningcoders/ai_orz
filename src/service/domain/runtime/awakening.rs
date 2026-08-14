//! Runtime Awakening 具体实现

use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, ThinkResult, ToolDescriptor, messages_to_summary};
use crate::models::events::{AgentLoopEvent, ThinkRoundEvent};
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::AgentAwakeEvent;
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};
use common::error::{Result, err};

/// think loop 的返回结果
///
/// - `Final`: 模型返回了最终回答，循环正常结束
///   - `content`: 最终回答内容
///   - `messages`: 当前完整的对话历史（用于总结流程写入短期记忆）
/// - `ContextOverflow`: 上下文超限，需要调用方执行压缩后重试
///   - `messages`: 当前完整的对话历史（用于生成沉淀摘要）
///   - `input_tokens`: 触发超限时的输入 token 数
///   - `rounds_used`: 本次 think loop 消耗的轮次
/// - `MaxRoundsExceeded`: 思考轮次耗尽，需要进入总结退出流程
///   - `messages`: 当前完整的对话历史（用于总结）
///   - `total_rounds`: 累计消耗的轮次
#[derive(Debug)]
pub enum ThinkLoopResult {
    Final {
        content: String,
        messages: Vec<ChatMessage>,
    },
    ContextOverflow {
        messages: Vec<ChatMessage>,
        input_tokens: u64,
        rounds_used: usize,
    },
    MaxRoundsExceeded {
        messages: Vec<ChatMessage>,
        total_rounds: usize,
    },
}

use crate::enrich_ctx;
use crate::record_event;

// ==================== 思考场景与选项 ====================

/// 思考场景类型
///
/// 用于区分唤醒（awaken）、沉睡（sleep_and_settle）和总结退出（summary）三种场景，
/// 不同场景根据 tag 过滤可用工具和技能。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingScene {
    /// 唤醒场景：响应外部消息，加载全部工具
    #[default]
    Awaken,
    /// 沉睡场景：沉淀记忆，只加载记忆相关工具（neural/memory tag）
    Settle,
    /// 总结退出场景：思考轮次耗尽后总结当前工作，允许消息和任务管理工具
    /// （neural + memory + messaging + project_management tag）
    Summary,
    /// 意图识别 + 上下文补充阶段
    ///
    /// 思考目标：只理解，不执行任何业务动作
    /// 工具约束：严格禁止执行类工具
    /// 最终输出：IntentAnalysis 结构化 JSON
    IntentAnalyze,
}

impl ThinkingScene {
    /// 判断工具是否在此场景可用
    ///
    /// - Awaken 场景：全部可用
    /// - Settle 场景：只有 tags 含 "neural" 或 "memory" 的工具可用
    /// - Summary 场景：允许 neural / memory / messaging / project_management
    /// - IntentAnalyze 场景：允许 tags 包含 neural/memory/query/search/analyze（理解类工具）
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => true,
            ThinkingScene::Settle => tags.iter().any(|t| t == "neural" || t == "memory"),
            ThinkingScene::Summary => tags.iter().any(|t| {
                t == "neural" || t == "memory" || t == "messaging" || t == "project_management"
            }),
            ThinkingScene::IntentAnalyze => tags.iter().any(|t| {
                t.contains("neural")
                    || t.contains("memory")
                    || t.contains("query")
                    || t.contains("search")
                    || t.contains("analyze")
            }),
        }
    }
}

/// 思考轮次默认上限（跨压缩轮次累计）
const DEFAULT_MAX_THINKING_ROUNDS: usize = 90;

/// 唤醒/沉睡的统一选项
///
/// 用于在不同场景传递业务上下文和场景标识，避免频繁修改方法签名。
/// awaken 和 sleep_and_settle 都接收此结构体，scene 字段决定工具过滤行为。
///
/// # 字段说明
/// - `scene`：场景标识（Awaken/Settle/Summary），决定工具过滤行为
/// - `project` / `task`：awaken 场景下，消息关联的项目/任务实体，注入 prompt 作为业务上下文
/// - `max_thinking_rounds`：awaken 场景最大思考轮次（跨压缩累计），None 时用默认值 90
/// - `user_profile`：用户画像（消息发送者的 UserPo，含自述偏好，注入 Prompt 的【用户画像】区块）
#[derive(Debug, Clone, Default)]
pub struct ThinkingOptions {
    /// 场景标识
    pub scene: ThinkingScene,
    /// 消息关联的项目实体（awaken 场景使用）
    pub project: Option<crate::models::project::Project>,
    /// 消息关联的任务实体（awaken 场景使用）
    pub task: Option<crate::models::task::Task>,
    /// 最大思考轮次（跨压缩累计），None 时使用默认值 90
    pub max_thinking_rounds: Option<usize>,
    /// 用户画像（消息发送者的 UserPo，awaken 场景注入【用户画像】区块）
    pub user_profile: Option<crate::models::user::UserPo>,
}

impl ThinkingOptions {
    /// 创建唤醒场景的选项
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建指定场景的选项
    pub fn for_scene(scene: ThinkingScene) -> Self {
        Self {
            scene,
            ..Default::default()
        }
    }

    /// 设置项目上下文
    pub fn with_project(mut self, project: crate::models::project::Project) -> Self {
        self.project = Some(project);
        self
    }

    /// 设置任务上下文
    pub fn with_task(mut self, task: crate::models::task::Task) -> Self {
        self.task = Some(task);
        self
    }

    /// 设置最大思考轮次
    pub fn with_max_thinking_rounds(mut self, max_rounds: usize) -> Self {
        self.max_thinking_rounds = Some(max_rounds);
        self
    }

    /// 设置用户画像（消息发送者的 UserPo）
    pub fn with_user_profile(mut self, user: crate::models::user::UserPo) -> Self {
        self.user_profile = Some(user);
        self
    }

    /// 获取有效最大思考轮次（None 时返回默认值）
    pub fn effective_max_rounds(&self) -> usize {
        self.max_thinking_rounds
            .unwrap_or(DEFAULT_MAX_THINKING_ROUNDS)
    }
}

/// 结构化意图分析结果
///
/// 由 `RuntimeAwakening::analyze_input_intent()` 输出，供：
/// - awaken 正式阶段 PromptBuilder 渲染【输入理解结果】区块
/// - 外部入站适配器（飞书/WS/HTTP 回调）路由消息前的预分析
/// - 澄清短路判断（need_clarification=true 时，可选择不进入执行阶段直接追问）
///
/// 说明：除了 confidence 用 f32，其余均为自由文本/数组，不做强枚举约束，
/// 避免未来新意图类型导致编译期改动；解析失败时降级为 Default::default()
/// 空结构，保证不阻塞主流程。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct IntentAnalysis {
    /// 主意图类型（推荐取值：Question / TaskRequest / Confirm /
    /// FollowUp / ClarificationResponse / Chat / Mixed）
    /// Agent 自主判断，不做强枚举
    pub intent_type: String,
    /// 意图置信度 0.0~1.0（Agent 自己打分）
    #[serde(default)]
    pub confidence: f32,
    /// 关键词/关键实体抽取（直接可复用于 search_memory 的 query）
    #[serde(default)]
    pub key_terms: Vec<String>,
    /// 指代消歧结果（自由文本数组，每条 Agent 写清楚"X → Y"）
    /// 例如：["\"上次那个方案\" → project=proj_123, task=task_456"]
    #[serde(default)]
    pub resolutions: Vec<String>,
    /// 检索补充上下文摘要（search_memory/recommend_seed_nodes 结果）
    #[serde(default)]
    pub retrieved_context: Vec<String>,
    /// 需要进一步追问澄清的问题（空列表表示理解充分）
    #[serde(default)]
    pub need_clarification: Vec<String>,
    /// 一句话总结：Agent 最终确认自己理解用户想要什么
    #[serde(default)]
    pub summary: String,
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
    /// - 上下文超限（input_tokens >= 阈值）→ 返回 `ContextOverflow`
    /// - 累计轮次达到 `max_rounds` → 返回 `MaxRoundsExceeded`
    /// - 超时 300s → 返回错误
    ///
    /// `start_round` 为本次循环的起始轮次编号（跨压缩累计）。
    /// `max_rounds` 为总轮次上限（跨压缩累计）。
    #[allow(clippy::too_many_arguments)]
    async fn run_think_loop(
        &self,
        ctx: RequestContext,
        brain: &crate::models::brain::Brain,
        prompt: &str,
        tool_descriptors: &[ToolDescriptor],
        agent: &Agent,
        scene_str: &str,
        trace_id: &str,
        max_rounds: usize,
        start_round: usize,
    ) -> Result<ThinkLoopResult> {
        const THINK_TIMEOUT_SECS: u64 = 300;
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

        let result =
            tokio::time::timeout(std::time::Duration::from_secs(THINK_TIMEOUT_SECS), async {
                let mut messages = vec![ChatMessage::user(prompt.to_string())];
                // 提取模型提供商信息（所有轮次共用）
                let (model_provider_id, model_name) = match brain.model_provider() {
                    Some(po) => (Some(po.id.clone()), Some(po.model_name.clone())),
                    None => (None, None),
                };
                // 本次循环可用轮次 = max_rounds - start_round
                let available_rounds = max_rounds.saturating_sub(start_round);
                for offset in 0..available_rounds {
                    let round = start_round + offset;
                    let round_start = std::time::Instant::now();
                    let result = self
                        .brain_dal()
                        .think(ctx.clone(), brain, &messages, tool_descriptors)
                        .await?;
                    let round_duration_ms = round_start.elapsed().as_millis() as u64;

                    match result {
                        ThinkResult::Final { content, usage } => {
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
                            // 追加助手消息（含 tool_calls），让模型在下一轮看到自己发起的调用
                            messages.push(ChatMessage::Assistant {
                                content,
                                tool_calls: Some(tool_calls.clone()),
                            });
                            // 按 control_mode 分发执行
                            for tc in tool_calls {
                                match agent.tools().iter().find(|t| t.po.name == tc.name) {
                                    Some(tool) => {
                                        let call_result = match tool.po.control_mode {
                                            common::enums::tool::ControlMode::Auto => {
                                                self.tool_dal()
                                                    .execute_auto(ctx.clone(), tool, tc.arguments)
                                                    .await
                                            }
                                            common::enums::tool::ControlMode::Manual => {
                                                self.tool_dal()
                                                    .execute_manual(ctx.clone(), tool, tc.arguments)
                                                    .await
                                            }
                                        };
                                        match call_result {
                                            Ok((value, _entry)) => {
                                                messages.push(ChatMessage::tool(
                                                    tc.id,
                                                    format!("{}", value),
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
                        }
                    }
                }
                // 循环耗尽所有可用轮次，未得到 Final 回答
                Ok(ThinkLoopResult::MaxRoundsExceeded {
                    messages,
                    total_rounds: max_rounds,
                })
            })
            .await;

        match result {
            Ok(inner) => inner,
            Err(_elapsed) => Err(err!(
                Internal,
                "brain think timeout after {}s",
                THINK_TIMEOUT_SECS
            )),
        }
    }
}

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
        AgentRuntimeStateManager::global().set_busy(&agent.po.id, &message.po.id);
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

        // Step 2: 发布循环启动事件（AOP 同步转发）
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "awaken",
            Some(&message.po.id),
        ))
        .await;

        // Step 2.5: 工具已由 hr_domain.get_agent(with_tools=true) 加载到 agent.tools
        // 工具列表通过 OpenAI tools API 协议层传递（ToolDescriptor），不注入 Prompt

        // Step 2.6: 技能已由 hr_domain.get_agent(with_skills=true) 加载到 agent.skills
        // 技能只在 Agent 已安装的副本范围内（author_id = agent_id，排除 Expired）
        // 不匹配 match_keys 的技能不展示在 Prompt，由 Agent 通过 search_skill 神经工具按需加载
        let skill_pos: Vec<crate::models::skill::SkillPo> =
            agent.skills().iter().map(|s| s.po.clone()).collect();

        // Step 3: 调用大脑思考（带工具调用循环 + 上下文压缩）
        // 统一走 BrainDal.think() 入口，方便审计、统计、监控
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

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

        loop {
            // 重新读取最近短期记忆（首次 + 每次压缩后都会获取最新的记忆）
            let recent_memories = self
                .memory()
                .get_recent_context(ctx.clone(), &agent.po.id, 20)
                .await?;

            // 拼装 Prompt（通过工厂方法获取对应 Agent 类型的 builder）
            let mut builder = self.prompt_builder(agent);
            builder.current_trace_id(&trace_id);
            builder.system_prompt(agent);
            builder.skills(&skill_pos);
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
            builder.current_message(message);
            prompt = builder.build();

            // 调用共享 think loop（传入累计轮次和上限）
            let think_result = self
                .run_think_loop(
                    ctx.clone(),
                    brain,
                    &prompt,
                    &tool_descriptors,
                    agent,
                    "awaken",
                    &trace_id,
                    max_rounds,
                    total_rounds,
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
                        "context overflow (total_rounds={}, tokens={}), triggering compaction via sleep_and_settle",
                        total_rounds,
                        input_tokens
                    );

                    // 将当前工作对话序列化为摘要，传给 sleep_and_settle 沉淀
                    let summary = messages_to_summary(&messages, 500);
                    let settle_options = ThinkingOptions::for_scene(ThinkingScene::Settle);
                    // 复用休息流程沉淀记忆（内部会 set_resting → think → set_idle via RAII）
                    // 注意：sleep_and_settle 内部会设置 Resting 状态，完成后自动恢复
                    // 传入 pending_trace_ids 让沉淀 prompt 携带本次总结依赖的 trace 列表，
                    // 要求 Agent 写入短期记忆时填入 trace_ids
                    let settle_result = self
                        .sleep_and_settle(
                            ctx.clone(),
                            agent,
                            &summary,
                            &settle_options,
                            &pending_trace_ids,
                        )
                        .await;
                    if let Err(e) = settle_result {
                        log_warn!(
                            &ctx,
                            "awaken",
                            "compaction sleep_and_settle failed: {:?}, continuing with retry",
                            e
                        );
                    } else if let Ok(ref result) = settle_result {
                        // 压缩后重置 pending_trace_ids 为本次 settle 的 trace_id
                        // 下次总结的范围 = 自上次压缩以来
                        pending_trace_ids = result.trace_ids.clone();
                    }
                    // 压缩完成后循环继续，重新获取 recent_memories 并重建 prompt
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
                        "max rounds exceeded (total={}), entering summary exit flow",
                        total_rounds
                    );

                    // 进入总结退出流程：让 agent 总结当前工作并发送/记录
                    // 传入 pending_trace_ids + awaken trace_id 作为本次总结依赖的 trace 列表
                    let mut summary_trace_ids = pending_trace_ids.clone();
                    if summary_trace_ids.last() != Some(&trace_id) {
                        summary_trace_ids.push(trace_id.clone());
                    }
                    let summary_output = self
                        .awaken_for_summary(
                            ctx.clone(),
                            agent,
                            &messages,
                            options,
                            &trace_id,
                            &summary_trace_ids,
                        )
                        .await
                        .unwrap_or_else(|e| {
                            log_warn!(&ctx, "awaken", "summary exit failed: {:?}", e);
                            String::new()
                        });

                    // 总结完成后，用 summary 输出作为 raw_output
                    raw_output = if summary_output.is_empty() {
                        "任务因思考轮次耗尽而终止，已执行总结退出流程。".to_string()
                    } else {
                        summary_output
                    };
                    break;
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
                        }
                    ) {
                        log_warn!(
                            &ctx,
                            "awaken",
                            "record_event failed on error path, stats may be incomplete: {:?}",
                            stats_err
                        );
                    }
                    let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
                        &agent.po.id,
                        &trace_id,
                        "awaken",
                        &format!("failed: {}", e),
                        duration_ms,
                        Some(&message.po.id),
                    ))
                    .await;
                    return Err(e);
                }
            }
        }

        // Step 5: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // Step 6: 通过 RuntimeMemory 子模块写入
        // 架构：awakening → RuntimeMemory → MemoryDal → MemoryDao
        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 6.5: 正常完成时触发总结流程，写入短期记忆
        // 仅在 Final 分支（正常完成）时触发，MaxRoundsExceeded 已在循环内执行过总结
        // 目的：将本次工作对话总结为短期记忆，trace_ids 记录依赖的 trace 列表
        if let Some(messages) = final_messages {
            // 构造总结依赖的 trace_ids = pending_trace_ids（已含 awaken trace_id）
            let summary_trace_ids = pending_trace_ids.clone();
            let _ = self
                .awaken_for_summary(
                    ctx.clone(),
                    agent,
                    &messages,
                    options,
                    &trace_id,
                    &summary_trace_ids,
                )
                .await
                .map_err(|e| {
                    log_warn!(
                        &ctx,
                        "awaken",
                        "post-completion summary failed: {:?}, continuing (non-fatal)",
                        e
                    );
                });
            // 总结失败不影响业务返回（awaken 已成功），仅记录警告
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
        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "awaken",
            "success",
            duration_ms,
            Some(&message.po.id),
        ))
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
    /// - current_message 用沉淀场景 prompt 构造的虚拟系统消息替代真实用户消息
    /// - 统计事件的 message_id 为 None（沉淀无关联消息）
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

        // Step 1: 读取最近短期记忆作为 history
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, 20)
            .await?;

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

        // 发布循环启动事件（AOP 同步转发）
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "settle",
            None,
        ))
        .await;

        // Step 3: 加载技能（已由 hr_domain.get_agent 加载到 agent）
        // Settle 场景过滤：只保留记忆相关 skill（tags 含 neural 或 memory），
        // 确保沉淀模式下只能接触记忆类工具。
        // 睡觉是对自身知识的沉淀积累，不应触发消息流程导致异步唤醒自己。
        let scene = options.scene;
        let skill_pos: Vec<crate::models::skill::SkillPo> = agent
            .skills()
            .iter()
            .filter(|s| {
                let tags = s.po.parse_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(|s| s.po.clone())
            .collect();

        // Step 4: 拼装 Prompt（复用 builder 挂载链路，调用 build_sleep_prompt 生成沉淀模板）
        // 与 awaken 的区别：不构造虚拟 System Message，约束模板内聚在 builder.build_sleep_prompt
        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(agent);
        builder.skills(&skill_pos);
        builder.history(&recent_memories);

        let prompt = builder.build_sleep_prompt(pending_memories_summary, trace_ids);

        // Step 5: 调用大脑思考（带工具调用循环，与 awaken 对称）
        // sleep 场景传递过滤后的记忆工具，Agent 可通过 function calling 调用记忆工具完成沉淀
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        // 构建 ToolDescriptor 列表（从 agent.tools 按场景过滤后派生，供模型 function calling）
        let tool_descriptors: Vec<ToolDescriptor> = agent
            .tools()
            .iter()
            .filter(|t| {
                let tags = t.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(ToolDescriptor::from)
            .collect();

        // 调用共享 think loop（沉淀场景不限制轮次，给一个较大的上限）
        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                &prompt,
                &tool_descriptors,
                agent,
                "settle",
                &trace_id,
                DEFAULT_MAX_THINKING_ROUNDS,
                0,
            )
            .await;

        // 展开 Result，失败时也记录事件
        // sleep_and_settle 场景不处理 ContextOverflow/MaxRoundsExceeded（沉淀工具量小）
        let raw_output = match think_result {
            Ok(ThinkLoopResult::Final { content, .. }) => content,
            Ok(ThinkLoopResult::ContextOverflow { .. })
            | Ok(ThinkLoopResult::MaxRoundsExceeded { .. }) => {
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
                let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
                    &agent.po.id,
                    &trace_id,
                    "settle",
                    &format!("settle failed: {}", e),
                    duration_ms,
                    None,
                ))
                .await;
                return Err(e);
            }
        };

        // Step 6: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());
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
        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "settle",
            "settle success",
            duration_ms,
            None,
        ))
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
        // 1. 强制构造出 IntentAnalyze 场景专用 options（覆盖 scene），
        //    思考轮次严格限制 1-2 轮（只做理解，不需要多轮执行）
        let mut analyze_opts = options.clone();
        analyze_opts.scene = ThinkingScene::IntentAnalyze;
        analyze_opts.max_thinking_rounds = Some(2);
        let scene = analyze_opts.scene;

        // 补充 Agent 上下文到 ctx（与 awaken 对齐）
        let ctx = enrich_ctx!(&ctx, agent);

        // 构造分析阶段的 trace_id：复用父 ctx 的 log_id 加前缀，避免与主 awaken trace 冲突
        let trace_id = format!("intent-analyze-{}", ctx.log_id);

        // 2. 查最近 20 条短期记忆做上下文（与 awaken 相同窗口，保证 Agent 有历史可读做消歧）
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, 20)
            .await?;

        // 3. 按 IntentAnalyze 场景过滤技能（严格只保留理解类标签）
        let skill_pos: Vec<crate::models::skill::SkillPo> = agent
            .skills()
            .iter()
            .filter(|s| {
                let tags = s.po.parse_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(|s| s.po.clone())
            .collect();

        // 4. 构造 PromptBuilder（与 awaken 相同挂载链路，保证背景知识一致）
        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(agent);
        builder.skills(&skill_pos);
        if let Some(project) = &analyze_opts.project {
            builder.project_context(project);
        }
        if let Some(task) = &analyze_opts.task {
            builder.task_context(task);
        }
        if let Some(user) = &analyze_opts.user_profile {
            builder.user_profile(user);
        }
        builder.history(&recent_memories);
        builder.current_message(message);

        // 5. 组装专用 Prompt（不是普通 build()）
        let prompt = builder.build_intent_analyze_prompt();

        // 6. 取 Agent Brain（调用方需已通过 wake_agent_brain 装配）
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_agent_brain()"))?;

        // 7. 按场景构建工具描述符列表（严格白名单，只允许理解类工具）
        let tool_descriptors: Vec<ToolDescriptor> = agent
            .tools()
            .iter()
            .filter(|t| {
                let tags = t.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(ToolDescriptor::from)
            .collect();

        // 8. 运行 think loop（严格最多 2 轮，快速理解后退出）
        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                &prompt,
                &tool_descriptors,
                agent,
                "intent-analyze",
                &trace_id,
                2,
                0,
            )
            .await?;

        // 9. 取最终回答文本
        let final_text = match think_result {
            ThinkLoopResult::Final { content, .. } => content,
            ThinkLoopResult::ContextOverflow { .. } => {
                return Err(err!(
                    Internal,
                    "analyze_input_intent context overflow (unexpected for 2-round limit)"
                ));
            }
            ThinkLoopResult::MaxRoundsExceeded { .. } => {
                return Err(err!(
                    Internal,
                    "analyze_input_intent max rounds exceeded without Final (Agent failed to output JSON)"
                ));
            }
        };

        // 10. 解析 IntentAnalysis JSON（5 级降级，全部失败则返回 Err，由调用方降级）
        parse_intent_analysis_json(&final_text)
    }
}

/// 总结退出流程的独立实现块
///
/// `awaken_for_summary` 是 `RuntimeDomainImpl` 的私有辅助方法，
/// 不属于 `RuntimeAwakening` trait（只在 awaken 内部调用）。
impl RuntimeDomainImpl {
    /// 总结退出流程
    ///
    /// 当思考轮次耗尽时，或正常完成时，让 Agent 总结当前工作进展并写入短期记忆。
    /// 内部构建 summary prompt，调用 think loop 让 Agent 自主完成总结，
    /// 可通过 send_message / update_task_progress 等工具发送通知（仅 MaxRoundsExceeded 场景）。
    ///
    /// `trace_ids` 为本次总结依赖的 trace 列表，写入 prompt 要求 Agent 调用
    /// save_short_term_memory 时填入此字段。
    ///
    /// 返回 Agent 的总结文本（作为 raw_output 记录到 trace）。
    async fn awaken_for_summary(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        work_messages: &[ChatMessage],
        options: &ThinkingOptions,
        parent_trace_id: &str,
        trace_ids: &[String],
    ) -> Result<String> {
        use common::enums::MemoryRole;

        let scene = ThinkingScene::Summary;

        // 1. 读取最近短期记忆
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, 20)
            .await?;

        // 2. 构造 summary trace
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            format!("summary-{}", parent_trace_id),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(),
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // 3. 发布循环启动事件
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "summary",
            None,
        ))
        .await;

        // 4. 按场景过滤技能
        let skill_pos: Vec<crate::models::skill::SkillPo> = agent
            .skills()
            .iter()
            .filter(|s| {
                let tags = s.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(|s| s.po.clone())
            .collect();

        // 5. 构建 summary prompt
        let work_summary = messages_to_summary(work_messages, 500);
        let total_rounds = options.effective_max_rounds();

        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(agent);
        builder.skills(&skill_pos);
        if let Some(project) = &options.project {
            builder.project_context(project);
        }
        if let Some(task) = &options.task {
            builder.task_context(task);
        }
        builder.history(&recent_memories);
        let prompt = builder.build_summary_prompt(&work_summary, total_rounds, trace_ids);

        // 6. 构建 Summary 场景的 ToolDescriptor（只允许消息和任务管理工具）
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        let tool_descriptors: Vec<ToolDescriptor> = agent
            .tools()
            .iter()
            .filter(|t| {
                let tags = t.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .map(ToolDescriptor::from)
            .collect();

        // 7. 调用 think loop（Summary 场景给少量轮次，最多 10 轮）
        const MAX_SUMMARY_ROUNDS: usize = 10;
        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                &prompt,
                &tool_descriptors,
                agent,
                "summary",
                &trace_id,
                MAX_SUMMARY_ROUNDS,
                0,
            )
            .await;

        let raw_output = match think_result {
            Ok(ThinkLoopResult::Final { content, .. }) => content,
            Ok(ThinkLoopResult::ContextOverflow { .. })
            | Ok(ThinkLoopResult::MaxRoundsExceeded { .. }) => {
                // 总结场景兜底：即使超限或轮次耗尽也返回已有内容
                String::new()
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "awaken_for_summary",
                    "summary think loop failed: {:?}",
                    e
                );
                String::new()
            }
        };

        // 8. 写入 trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());
        let _ = self.memory().write_thinking_trace(ctx.clone(), trace).await;

        // 9. 发布循环完成事件
        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "summary",
            "success",
            0,
            None,
        ))
        .await;

        Ok(raw_output)
    }
}

/// 从 Agent Final 文本中按 5 级降级策略尽量提取并解析 IntentAnalysis JSON
///
/// # 降级策略
/// 1. 整段文本直接 JSON 反序列化
/// 2. 手动查找 ```json ... ``` 或 ``` ... ``` 代码块，提取内容再解析
/// 3. 查找 INTENT_ANALYSIS_START/END 锚点标记之间的内容
/// 4. 平衡括号法：从第一个 { 开始找到匹配的顶层 }，提取中间内容再解析
///    (含字段类型宽容修复：confidence 字符串→数字、缺省字段 Default)
/// 5. 取第一个 { 与最后一个 } 之间的子串尝试解析
/// 6. 全部失败 → 返回 Err（错误信息含文本前缀，便于调试日志）
pub fn parse_intent_analysis_json(text: &str) -> Result<IntentAnalysis> {
    let text = text.trim();
    if text.is_empty() {
        return Err(err!(
            Internal,
            "parse_intent_analysis_json: empty text"
        ));
    }

    // ===== Level 1: 整段文本直接解析 =====
    if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(text) {
        return Ok(ia);
    }

    // ===== Level 2: 手动查找 ```json ... ``` 或 ``` ... ``` 代码块 =====
    let mut cursor = text;
    while let Some(start) = cursor.find("```") {
        let after_first = &cursor[start + 3..];
        // 跳过可选的 "json" 标识符 + 空白
        let after_lang = if let Some(rest) = after_first.strip_prefix("json") {
            rest.trim_start_matches([' ', '\n', '\r', '\t'])
        } else {
            after_first.trim_start_matches([' ', '\n', '\r', '\t'])
        };
        if let Some(end) = after_lang.find("```") {
            let inner = after_lang[..end].trim();
            if !inner.is_empty() {
                if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
                    return Ok(ia);
                }
                // 如果直接 IntentAnalysis 失败，可能是锚点包裹或字段类型问题，
                // 进入宽容解析流程
                if let Some(ia) = try_lenient_parse(inner) {
                    return Ok(ia);
                }
            }
            // 继续在剩余文本中寻找下一组 ```
            cursor = &after_lang[end.saturating_add(3)..];
            continue;
        }
        break;
    }

    // ===== Level 3: 查找 INTENT_ANALYSIS_START/END 锚点之间的内容 =====
    if let Some(start_marker) = text.find("--- INTENT_ANALYSIS_START ---") {
        let after_start = &text[start_marker + "--- INTENT_ANALYSIS_START ---".len()..];
        if let Some(end_marker) = after_start.find("--- INTENT_ANALYSIS_END ---") {
            let inner = after_start[..end_marker].trim();
            if !inner.is_empty() {
                if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
                    return Ok(ia);
                }
                if let Some(ia) = try_lenient_parse(inner) {
                    return Ok(ia);
                }
            }
        }
    }

    // ===== Level 4: 平衡括号法提取第一个完整 JSON 对象 =====
    if let Some(json_obj) = extract_first_json_object(text) {
        if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(json_obj) {
            return Ok(ia);
        }
        if let Some(ia) = try_lenient_parse(json_obj) {
            return Ok(ia);
        }
    }

    // ===== Level 5: 取第一个 { 到最后一个 } 之间的子串 =====
    let first_brace = text.find('{');
    let last_brace = text.rfind('}');
    if let (Some(first), Some(last)) = (first_brace, last_brace)
        && first < last
    {
        let inner = &text[first..=last];
        if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
            return Ok(ia);
        }
        if let Some(ia) = try_lenient_parse(inner) {
            return Ok(ia);
        }
    }

    // ===== Level 6 (全部失败): 返回 Err 含文本前缀便于调试 =====
    let preview: String = text.chars().take(120).collect();
    Err(err!(
        Internal,
        "parse_intent_analysis_json: all strategies failed. Text prefix: {}",
        preview
    ))
}

/// 宽容解析：先 parse 成 serde_json::Value，再手动按字段提取并做类型宽容转换
/// （解决 Agent 偶发把 confidence 写成字符串、数组里混非字符串等问题）
fn try_lenient_parse(s: &str) -> Option<IntentAnalysis> {
    use serde_json::Value;

    let val: Value = serde_json::from_str(s).ok()?;
    let intent_type = val
        .get("intent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let confidence = val
        .get("confidence")
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
        .unwrap_or(0.0) as f32;
    let extract_str_arr = |key: &str| -> Vec<String> {
        val.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| match x {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        Value::Bool(b) => Some(b.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let key_terms = extract_str_arr("key_terms");
    let resolutions = extract_str_arr("resolutions");
    let retrieved_context = extract_str_arr("retrieved_context");
    let need_clarification = extract_str_arr("need_clarification");
    let summary = val
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 至少要有 intent_type 或 summary 任一非空，才算解析出有效结果
    if intent_type.is_empty() && summary.is_empty() {
        return None;
    }

    Some(IntentAnalysis {
        intent_type,
        confidence,
        key_terms,
        resolutions,
        retrieved_context,
        need_clarification,
        summary,
    })
}

/// 简易括号匹配：从字符串中找到第一个顶层的 { ... } 完整 JSON 对象
///
/// 支持字符串内部出现大括号的情况：遇到未转义的双引号进入字符串模式，
/// 字符串内部的 {} 不计入括号计数。
pub fn extract_first_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let mut depth = 0;
            let start = i;
            let mut in_string = false;
            let mut escape = false;
            while i < bytes.len() {
                let b = bytes[i];
                if escape {
                    escape = false;
                    i += 1;
                    continue;
                }
                if b == b'\\' {
                    escape = true;
                    i += 1;
                    continue;
                }
                if b == b'"' {
                    in_string = !in_string;
                    i += 1;
                    continue;
                }
                if !in_string {
                    if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            let end = i + 1;
                            return Some(&s[start..end]);
                        }
                    }
                }
                i += 1;
            }
            return None;
        }
        i += 1;
    }
    None
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
            // 从 messages 中提取最后一条 user 消息作为 prompt 捕获
            let prompt = messages
                .iter()
                .rev()
                .find_map(|m| match m {
                    crate::models::cortex_types::ChatMessage::User { content } => {
                        Some(content.as_str())
                    }
                    _ => None,
                })
                .unwrap_or("");
            *self.captured_prompt.lock().unwrap() = Some(prompt.to_string());
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
        use super::ThinkingScene;

        let scene = ThinkingScene::IntentAnalyze;

        // 允许：工具名 tag 包含 vector_search / query_memory / search / analyze
        let allowed_tags: Vec<String> = vec![
            "vector_search".into(),
            "query_memory".into(),
            "search_memory".into(),
            "analyze_text".into(),
        ];
        for tag in &allowed_tags {
            assert!(
                scene.is_tool_allowed(&[tag.clone()]),
                "tag '{}' should be allowed in IntentAnalyze scene",
                tag
            );
        }

        // 禁止：工具名 tag 包含 shell_exec / lark_push
        let forbidden_tags: Vec<String> =
            vec!["shell_exec".into(), "lark_push".into(), "send_message".into()];
        for tag in &forbidden_tags {
            assert!(
                !scene.is_tool_allowed(&[tag.clone()]),
                "tag '{}' should be forbidden in IntentAnalyze scene",
                tag
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
        assert_eq!(
            ia.summary,
            "用户想知道项目 X 中方案 A 的推进情况"
        );
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
        assert_eq!(
            ia.summary,
            "用户询问项目X的排期，需要澄清版本信息"
        );
    }
}
