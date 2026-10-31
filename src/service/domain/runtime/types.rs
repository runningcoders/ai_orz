//! Runtime Awakening 类型定义
//!
//! 集中存放 think loop / 场景选项 / 意图分析等共享类型，
//! 供 awakening / think_loop / intent_analyze / compaction 子模块复用。

use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use common::enums::ThinkingScene;
use std::sync::{Arc, Mutex};

// ==================== 循环进度快照 ====================

/// 单轮思考的进度摘要
///
/// `transcript` 已是可直接入档的文本行（由 [`ChatMessage::to_summary_text`] 生成），
/// 存文本而非存 `ChatMessage` 的原因：快照要跨 `Err` / 超时存活，
/// 存原始消息会让循环每轮多一次全量 clone。
#[derive(Debug, Clone, Default)]
pub struct RoundDigest {
    /// 轮次编号（1-based，跨压缩累计，面向阅读）
    pub round: usize,
    /// 本轮模型输出的文本（思考过程 / 中途说明），可能为空
    pub assistant_text: Option<String>,
    /// 本轮调用的工具名（按调用顺序，含执行失败与未找到的）
    pub tool_names: Vec<String>,
    /// 本轮新增消息的摘要行（assistant 输出 + 工具调用 + 工具结果）
    pub transcript: Vec<String>,
}

/// 思考循环进度快照（跨 `Err` / 超时存活）
///
/// # 为什么需要它
///
/// `messages` 是 `run_think_loop` 内部的局部变量，以下两条退出路径都拿不到它：
///
/// - **模型调用失败**（429 限流 / 5xx / 鉴权 / 内容过滤）：`?` 提前返回，`messages` 被 drop
/// - **`tokio::time::timeout` 超时**：整个 future 被丢弃，`messages` 一起消失
///
/// 而这两条恰恰是最需要「兜底总结」的场景 —— 循环白跑若干轮，用户诉求彻底丢失，
/// 短期记忆里没有、trace 里也没有。
///
/// 快照由**调用方**持有并通过 [`ThinkLoopParams::with_progress`] 传入，
/// 因此上述两条路径下它都存活，调用方可据此生成兜底摘要（见 `abort_summary` 模块）。
#[derive(Debug, Clone, Default)]
pub struct ThinkLoopProgress {
    inner: Arc<Mutex<ProgressInner>>,
}

#[derive(Debug, Default)]
struct ProgressInner {
    /// 已完成的有效轮次（模型成功返回并处理完毕的轮次）
    rounds: Vec<RoundDigest>,
    /// 累计工具调用次数
    total_tool_calls: usize,
    /// 按工具名累计调用次数
    tool_call_counts: std::collections::HashMap<String, usize>,
}

impl ThinkLoopProgress {
    /// 创建空快照
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一轮已完成的思考（工具调用与结果都已落地到 messages 之后调用）
    pub fn record_round(&self, digest: RoundDigest) {
        // 持锁期间只做纯内存计入，不做任何 I/O，避免阻塞循环
        match self.inner.lock() {
            Ok(mut inner) => {
                inner.total_tool_calls += digest.tool_names.len();
                for name in &digest.tool_names {
                    *inner.tool_call_counts.entry(name.clone()).or_insert(0) += 1;
                }
                inner.rounds.push(digest);
            }
            // 锁中毒：放弃本轮记录。兜底摘要是尽力而为的能力，
            // 绝不能因为记不下来就让思考循环失败。
            Err(_) => {
                crate::log_warn!(
                    "think loop progress lock poisoned, round {} not recorded",
                    digest.round
                );
            }
        }
    }

    /// 取一份独立副本（不持锁），供兜底摘要构造使用
    pub(crate) fn snapshot(&self) -> ProgressSnapshot {
        match self.inner.lock() {
            Ok(inner) => ProgressSnapshot {
                rounds: inner.rounds.clone(),
                total_tool_calls: inner.total_tool_calls,
                tool_call_counts: inner.tool_call_counts.clone(),
            },
            Err(_) => ProgressSnapshot::default(),
        }
    }

    /// 清空快照：上下文压缩成功后调用
    ///
    /// 压缩前的工作已由压缩流程归档成短期记忆，兜底摘要只需覆盖
    /// 压缩之后的进度，避免下次中断时同一段工作在记忆里出现两份。
    pub(crate) fn reset(&self) {
        match self.inner.lock() {
            Ok(mut inner) => *inner = ProgressInner::default(),
            Err(_) => {
                // 锁中毒时无法安全清空：保留旧快照只会让兜底摘要略有冗余，
                // 但绝不能让这个可忽略的瑕疵影响主循环。
                crate::log_warn!("think loop progress lock poisoned, reset skipped");
            }
        }
    }
}

/// [`ThinkLoopProgress`] 的只读副本
#[derive(Debug, Clone, Default)]
pub(crate) struct ProgressSnapshot {
    pub rounds: Vec<RoundDigest>,
    pub total_tool_calls: usize,
    pub tool_call_counts: std::collections::HashMap<String, usize>,
}

impl ProgressSnapshot {
    /// 已完成的有效轮次数
    pub fn rounds_used(&self) -> usize {
        self.rounds.len()
    }

    /// 模型最后一次输出的文本（兜底摘要里作为「中断前的最后一句话」）
    pub fn last_assistant_text(&self) -> Option<&str> {
        self.rounds
            .iter()
            .rev()
            .find_map(|r| r.assistant_text.as_deref())
            .filter(|s| !s.trim().is_empty())
    }

    /// 工具调用统计，格式 `name xN`，按调用次数降序、同次数按名称升序
    pub fn tool_call_stats(&self) -> Vec<String> {
        let mut items: Vec<_> = self.tool_call_counts.iter().collect();
        items.sort_by(|(a_name, a_cnt), (b_name, b_cnt)| {
            b_cnt.cmp(a_cnt).then_with(|| a_name.cmp(b_name))
        });
        items
            .into_iter()
            .map(|(name, count)| format!("{name} x{count}"))
            .collect()
    }
}

// ==================== think loop 入参 ====================

/// 单次唤醒的用量计量（**跨思考周期累计**，绝对值口径）
///
/// 唤醒链里一次任务可能经历多个思考周期（溢出→压缩→重来），每个周期都是一次
/// 独立的 `run_think_loop`。预算必须跨周期累计，否则「上限 10 轮」会变成
/// 「每个周期各 10 轮」，实际可用量被放大成 `周期数 × 上限` —— 这正是
/// 重构前 `start_round` 恒传 0 造成的口径偏差。
///
/// 三个字段都由 `start_round` + 本周期的本地增量算出，由思考循环每轮写回，
/// 调用方（`DeliberateNode`）在周期结束后读出来落进 `BudgetState`。
///
/// - `rounds`：已开始的思考轮次数（绝对编号，1 基）
/// - `tokens`：已消耗 token（input + output）
/// - `elapsed_secs`：已消耗墙钟秒数
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WakeUsage {
    pub rounds: usize,
    pub tokens: u64,
    pub elapsed_secs: u64,
}

impl WakeUsage {
    /// 新建一个「本场景独立计量」的空表（compact / intent / settle / 测试用）
    pub fn shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }

    /// 读取当前快照（锁中毒时退回零值：计量失败不应拖垮思考）
    pub fn snapshot(meter: &Arc<Mutex<Self>>) -> Self {
        match meter.lock() {
            Ok(guard) => *guard,
            Err(_) => Self::default(),
        }
    }

    /// 覆盖写回（同上，锁中毒时静默丢弃）
    pub fn store(meter: &Arc<Mutex<Self>>, value: Self) {
        if let Ok(mut guard) = meter.lock() {
            *guard = value;
        }
    }
}

/// 思考循环入参
///
/// `run_think_loop` 是 awaken / sleep_and_settle / compact_context / intent_analyze
/// 四个场景共用的多轮思考引擎。原先直接铺 12 个参数，既触发 clippy
/// `too_many_arguments`，也让调用点难以核对，故收敛为结构体。
///
/// # 关于 brain
/// 不作为入参：四个场景取的都是 `agent.brain`，改由 `run_think_loop` 内部解析，
/// 省去每个调用点重复写一遍「大脑未唤醒」的判空。
///
/// # 典型用法
/// ```rust,ignore
/// let params = ThinkLoopParams::new(ctx, agent, scene, trace_id, initial_messages, &tools)
///     .with_rounds(max_rounds, start_round)
///     .with_monitoring(&think_runtime, &policy);
/// let result = self.run_think_loop(params).await?;
/// ```
pub struct ThinkLoopParams<'a> {
    /// 请求上下文
    pub ctx: crate::pkg::request_context::RequestContext,
    /// 执行思考的 Agent（brain 在循环内部由其解析）
    pub agent: &'a Agent,
    /// 思考场景：决定事件 scene 字段与策略组
    pub scene: ThinkingScene,
    /// 本次思考的 trace id（事件与运行时快照上报用）
    ///
    /// awaken / settle / compact 下即 MemoryTrace.id，两条记录靠这个 ID 关联。
    pub trace_id: &'a str,
    /// 起始消息
    ///
    /// - awaken / settle：builder 拼出的 `[System, User]`
    /// - compact：主循环已有完整对话 + 追加的压缩指令
    pub initial_messages: Vec<ChatMessage>,
    /// 可用工具（由 `build_scene_tool_descriptors` 按场景过滤）
    pub tool_descriptors: &'a [ToolDescriptor],
    /// 总轮次上限（跨压缩累计）
    pub max_rounds: usize,
    /// 本次循环的起始轮次编号（跨压缩累计），新起一段循环时为 0
    pub start_round: usize,
    /// 超时秒数，0 = 不限制
    pub timeout_secs: u64,
    /// 思考运行时快照上报点（None = 不上报，前端查不到进度）
    ///
    /// 持有所有权：轮级环图的节点是 `'static`，必须能脱离 `params` 借用存活。
    pub think_runtime: Option<Arc<AgentThinkRuntime>>,
    /// 策略引擎接入点（None = 退化为旧行为，仅靠 max_rounds + timeout 控制）
    pub policy: Option<Arc<dyn crate::pkg::policy::Policy>>,
    /// 进度快照收集点（None = 不记录）
    ///
    /// 传入后循环每轮会把「做了什么」写进快照。异常中断（`Err` / 超时）时
    /// 调用方靠它生成兜底总结 —— 这是目前唯一能在这两条路径上保留工作成果的手段。
    pub progress: Option<ThinkLoopProgress>,
    /// 意图识别产出的「后续阶段思考强度动态偏移」（分）
    ///
    /// 叠加在场景偏移之上（`场景偏移 + 动态偏移`），由调用方在拿到
    /// `IntentAnalysis` 后填入；意图阶段缺失/降级时保持 0，行为与未改造前一致。
    pub thinking_intensity_offset: i32,
    /// 唤醒级用量计量器（跨周期累计）
    ///
    /// - `Some`：awaken 传入 run 级共享表 ⇒ 轮次/token/时长在周期之间累计，
    ///   策略阈值（`max_thinking_rounds` / `token_budget` / `think_timeout_secs`）
    ///   比较的是**整次唤醒**的已用量，而不是单周期。
    /// - `None`：compact / intent / settle 等固定开销场景 ⇒ 本次循环自建零表，
    ///   计量口径仍是单周期（这些场景没有「再来一次」的回环）。
    pub usage: Option<Arc<Mutex<WakeUsage>>>,
}

impl<'a> ThinkLoopParams<'a> {
    /// 构造思考循环入参
    ///
    /// 轮次上限与超时按 Agent 配置填默认值，调用方用
    /// [`with_rounds`](Self::with_rounds) / [`with_monitoring`](Self::with_monitoring)
    /// 覆盖差异项。
    pub fn new(
        ctx: crate::pkg::request_context::RequestContext,
        agent: &'a Agent,
        scene: ThinkingScene,
        trace_id: &'a str,
        initial_messages: Vec<ChatMessage>,
        tool_descriptors: &'a [ToolDescriptor],
    ) -> Self {
        Self {
            ctx,
            agent,
            scene,
            trace_id,
            initial_messages,
            tool_descriptors,
            max_rounds: config_resolve::max_thinking_rounds(agent),
            start_round: 0,
            timeout_secs: config_resolve::think_timeout_secs(agent),
            think_runtime: None,
            policy: None,
            progress: None,
            thinking_intensity_offset: 0,
            usage: None,
        }
    }

    /// 覆盖轮次参数：总上限 + 本次循环的起始轮次
    ///
    /// ⚠️ awaken 主链的 `start_round` **不是常数 0**：必须传「本次唤醒已消耗的
    /// 轮次数」（见 [`WakeUsage`]），否则轮次上限会被每个周期重新发放一次。
    pub fn with_rounds(mut self, max_rounds: usize, start_round: usize) -> Self {
        self.max_rounds = max_rounds;
        self.start_round = start_round;
        self
    }

    /// 接入唤醒级用量计量器（跨周期累计）
    ///
    /// 传入后，本次循环的指标（`round_number` / `total_tokens` / `elapsed_secs`）
    /// 都是**整次唤醒的绝对值**，策略阈值直接按唤醒级预算比较；循环内每轮
    /// 会把新用量写回该表，供下一个周期接着算。
    pub fn with_usage(mut self, usage: &Arc<Mutex<WakeUsage>>) -> Self {
        self.usage = Some(Arc::clone(usage));
        self
    }

    /// 接入思考运行时快照上报 + 策略引擎
    ///
    /// 两者通常成对出现：策略的 cancel_flag 由 think_runtime 提供，
    /// 命中策略时也需要 runtime 上报最后一轮快照。
    pub fn with_monitoring(
        mut self,
        think_runtime: &Arc<AgentThinkRuntime>,
        policy: &Arc<dyn crate::pkg::policy::Policy>,
    ) -> Self {
        self.think_runtime = Some(Arc::clone(think_runtime));
        self.policy = Some(Arc::clone(policy));
        self
    }

    /// 接入进度快照收集器
    ///
    /// 需要「异常中断也能留存工作成果」的调用方（目前是 awaken）应传入。
    /// 压缩 / 沉淀 / 意图识别这类内部小循环不传 —— 它们的产出本就只供内部使用。
    pub fn with_progress(mut self, progress: &ThinkLoopProgress) -> Self {
        self.progress = Some(progress.clone());
        self
    }

    /// 叠加意图识别产出的动态思考强度偏移（分）
    ///
    /// 仅主循环（Awaken 场景）使用：意图阶段判定后，用这个偏移让后续几个
    /// 步骤的思考强度随「任务难度 / 置信度」浮动。内部小循环（压缩/沉淀/
    /// 意图识别自身）不传，保持 0。
    pub fn with_thinking_intensity_offset(mut self, offset: i32) -> Self {
        self.thinking_intensity_offset = offset;
        self
    }
}

// ==================== think loop 返回结果 ====================

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
/// - `Cancelled`: 用户主动取消（通过 cancel-thinking 接口设置 cancel_flag）
///   - `messages`: 当前对话历史（可能为空，用于审计）
///   - `total_rounds`: 取消时已消耗的轮次
#[derive(Debug)]
pub enum ThinkLoopResult {
    Final {
        content: String,
        messages: Vec<crate::models::cortex_types::ChatMessage>,
        /// 本次循环结束时的**绝对轮次**（= 起始轮次 + 本循环跑掉的轮次）
        ///
        /// 必须随结果一起回传：唤醒级轮次记账以它为准，而不是另设一份侧信道
        /// 计量表 —— 后者对「不经过 run_think_loop 的后端」永远不增长。
        rounds_used: usize,
    },
    ContextOverflow {
        messages: Vec<crate::models::cortex_types::ChatMessage>,
        input_tokens: u64,
        rounds_used: usize,
    },
    MaxRoundsExceeded {
        messages: Vec<crate::models::cortex_types::ChatMessage>,
        total_rounds: usize,
    },
    Cancelled {
        messages: Vec<crate::models::cortex_types::ChatMessage>,
        total_rounds: usize,
    },
}

// ==================== 思考场景与选项 ====================

/// 从系统配置 + Agent runtime_config 解析思考轮次和超时
///
/// 优先级：Agent runtime_config（非 0）> 系统配置 [agent] > 硬编码兜底
pub(crate) mod config_resolve {
    use crate::config;
    use crate::models::agent::Agent;

    /// 单次唤醒的思考**总**轮次上限（跨压缩 / 跨周期累计）
    ///
    /// 这里是「轮次预算」的唯一解析入口：awaken 主链把它当作整次唤醒的天花板，
    /// 每个周期的可用量 = 该值 − 已消耗（见 [`WakeUsage`]）。
    pub fn max_thinking_rounds(agent: &Agent) -> usize {
        let rc = agent.po.get_runtime_config();
        if rc.max_thinking_rounds > 0 {
            return rc.max_thinking_rounds;
        }
        config::get().agent.max_thinking_rounds
    }

    /// 意图识别阶段最大思考轮次
    pub fn intent_analyze_max_rounds(agent: &Agent) -> usize {
        let rc = agent.po.get_runtime_config();
        if rc.intent_analyze_max_rounds > 0 {
            return rc.intent_analyze_max_rounds;
        }
        config::get().agent.intent_analyze_max_rounds
    }

    /// 收尾总结阶段最大思考轮次（轮次耗尽后的「给用户一个交代」那一段）
    ///
    /// 消费者是 `CompactNode(with_user_reply=true)`：它复用压缩场景的 prompt 与
    /// 工具白名单，但预算是独立的一份 —— 用户看到的「总结轮次」配置项指的就是这里。
    pub fn summary_max_rounds(agent: &Agent) -> usize {
        let rc = agent.po.get_runtime_config();
        if rc.summary_max_rounds > 0 {
            return rc.summary_max_rounds;
        }
        config::get().agent.summary_max_rounds
    }

    /// 思考超时（秒），0 = 不限制
    ///
    /// Agent 级配置 > 系统配置，两者都 0 = 不限制
    pub fn think_timeout_secs(agent: &Agent) -> u64 {
        let rc = agent.po.get_runtime_config();
        if rc.think_timeout_secs > 0 {
            return rc.think_timeout_secs;
        }
        config::get().agent.think_timeout_secs
    }

    /// 记忆分流阈值（本轮问答估算 token 数）
    ///
    /// 低于它原文写入短期记忆，达到/超过才交给 LLM 压缩。
    /// **不存在「不压缩」场景**，所以 `AgentRuntimeConfig` 的默认值就是 256，
    /// 不是 0；只有显式填 0 才表示「跟随系统配置」。
    pub fn compaction_threshold_tokens(agent: &Agent) -> usize {
        let rc = agent.po.get_runtime_config();
        if rc.compaction_threshold_tokens > 0 {
            return rc.compaction_threshold_tokens;
        }
        config::get().agent.compaction_threshold_tokens
    }
}

// ==================== 场景选项 ====================

/// 唤醒/沉睡的统一选项
///
/// 用于在不同场景传递业务上下文和场景标识，避免频繁修改方法签名。
/// awaken 和 sleep_and_settle 都接收此结构体，scene 字段决定工具过滤行为。
///
/// # 字段说明
/// - `scene`：场景标识（Awaken/Settle/Summary），决定工具过滤行为
/// - `project` / `task`：awaken 场景下，消息关联的项目/任务实体，注入 prompt 作为业务上下文
/// - `max_thinking_rounds`：awaken 场景最大思考轮次（跨压缩累计），None 时取系统配置 `[agent].max_thinking_rounds`（默认 365）
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

    /// 获取有效最大思考轮次（None 时取系统配置 [agent].max_thinking_rounds）
    pub fn effective_max_rounds(&self) -> usize {
        self.max_thinking_rounds
            .unwrap_or_else(|| crate::config::get().agent.max_thinking_rounds)
    }
}

// ==================== 意图分析结果 ====================

/// 结构化意图分析结果
///
/// 由 `RuntimeAwakening::analyze_input_intent()` 输出，供：
/// - awaken 正式阶段 PromptBuilder 渲染【输入理解结果】区块
/// - 外部入站适配器（飞书/WS/HTTP 回调）路由消息前的预分析
/// - 简单问题直答：`direct_answer` 非空时图从 `intent` 直连 `consolidate`，
///   跳过 retrieve / prepare / deliberate（省掉一次完整 LLM 调用）
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
    /// 需要进一步追问澄清的问题（空列表 = 理解充分）
    #[serde(default)]
    pub need_clarification: Vec<String>,
    /// 简单问题直答：给用户的最终答复（空串 = 必须走完整唤醒链路）
    ///
    /// 意图阶段判定「无需检索 / 工具 / 澄清即可作答」时填写。非空即视为直答，
    /// 图会从 `intent` 直接进 `consolidate`；`summary` 仍只放理解结论，不混入答复。
    #[serde(default)]
    pub direct_answer: String,
    /// 一句话总结：Agent 最终确认自己理解用户想要什么
    #[serde(default)]
    pub summary: String,
}

impl IntentAnalysis {
    /// 是否在意图阶段就给出了最终答复（非空即直答）
    pub fn has_direct_answer(&self) -> bool {
        !self.direct_answer.trim().is_empty()
    }

    /// 意图识别产出的「后续阶段思考强度动态偏移」（分）
    ///
    /// 目标：让「这一步要花多少思考」由意图本身决定，而不是所有会话都用同一个分数。
    ///
    /// 规则（确定性启发式，先覆盖能明确判断的三种情形，其余保持 0）：
    /// - **低置信（< 0.5）** → `+20`：连意图都没把握，后续更需要深度推理，
    ///   提高预算避免「没想明白就草率作答」
    /// - **任务 / 代码 / 计划类** → `+10`：多步执行、易出错，适度提强度
    /// - **简单且高置信（≥ 0.8）** → `-10`：闲聊/问答/确认，降强度更快更省
    /// - **其余** → `0`
    ///
    /// 结果再叠加场景偏移后由 `ThinkingIntensity::offset` 饱和钳制到 [0, 100]，
    /// 因此这里不需要自带边界；但总幅度刻意压在 ±20 内 —— 动态偏移是「微调」，
    /// 不该盖过用户在 Provider/Agent 上的显式配置。
    pub fn thinking_intensity_offset(&self) -> i32 {
        if self.confidence < 0.5 {
            return 20;
        }

        let ty = self.intent_type.trim().to_ascii_lowercase();
        let is_task_like = [
            "task",
            "task_request",
            "code",
            "coding",
            "plan",
            "planning",
            "mixed",
            "任务",
            "代码",
            "编程",
            "计划",
            "混合",
        ]
        .iter()
        .any(|k| ty.contains(k));
        if is_task_like {
            return 10;
        }

        let is_simple_like = [
            "chat",
            "question",
            "followup",
            "follow_up",
            "confirm",
            "闲聊",
            "聊天",
            "问答",
            "确认",
            "追问",
        ]
        .iter()
        .any(|k| ty.contains(k));
        if self.confidence >= 0.8 && is_simple_like {
            return -10;
        }

        0
    }
}

#[cfg(test)]
mod tests {
    use super::IntentAnalysis;

    fn intent(intent_type: &str, confidence: f32) -> IntentAnalysis {
        IntentAnalysis {
            intent_type: intent_type.to_string(),
            confidence,
            ..Default::default()
        }
    }

    /// 低置信优先：连意图都没把握时一律提强度（即使类型看着像闲聊）
    #[test]
    fn low_confidence_raises_intensity() {
        assert_eq!(intent("Chat", 0.3).thinking_intensity_offset(), 20);
        assert_eq!(intent("", 0.0).thinking_intensity_offset(), 20);
    }

    /// 任务/代码/计划类：多步执行，适度提强度
    #[test]
    fn task_like_raises_intensity() {
        for ty in [
            "TaskRequest",
            "Coding",
            "Plan",
            "Mixed",
            "任务",
            "代码",
            "编程",
        ] {
            assert_eq!(
                intent(ty, 0.9).thinking_intensity_offset(),
                10,
                "intent_type={ty} 应命中任务类"
            );
        }
    }

    /// 简单且高置信：降强度更快更省
    #[test]
    fn simple_high_confidence_lowers_intensity() {
        for ty in [
            "Chat", "Question", "FollowUp", "Confirm", "闲聊", "问答", "确认",
        ] {
            assert_eq!(
                intent(ty, 0.9).thinking_intensity_offset(),
                -10,
                "intent_type={ty} 应命中简单类"
            );
        }
    }

    /// 简单类但置信不足，或不认识的类型：保持基准
    #[test]
    fn ambiguous_or_unknown_intent_keeps_base() {
        assert_eq!(intent("Chat", 0.6).thinking_intensity_offset(), 0);
        assert_eq!(intent("Unknown", 0.9).thinking_intensity_offset(), 0);
        assert_eq!(intent("", 0.9).thinking_intensity_offset(), 0);
    }
}
