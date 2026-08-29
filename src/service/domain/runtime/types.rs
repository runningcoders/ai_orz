//! Runtime Awakening 类型定义
//!
//! 集中存放 think loop / 场景选项 / 意图分析等共享类型，
//! 供 awakening / think_loop / intent_analyze / compaction 子模块复用。

use crate::models::agent::Agent;
use crate::models::cortex_types::{ChatMessage, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use common::enums::ThinkingScene;
use std::sync::Arc;

// ==================== think loop 入参 ====================

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
///     .with_monitoring(&think_runtime, policy.as_ref());
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
    pub think_runtime: Option<&'a Arc<AgentThinkRuntime>>,
    /// 策略引擎接入点（None = 退化为旧行为，仅靠 max_rounds + timeout 控制）
    pub policy: Option<&'a dyn crate::pkg::policy::Policy>,
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
        }
    }

    /// 覆盖轮次参数：总上限 + 本次循环的起始轮次
    pub fn with_rounds(mut self, max_rounds: usize, start_round: usize) -> Self {
        self.max_rounds = max_rounds;
        self.start_round = start_round;
        self
    }

    /// 接入思考运行时快照上报 + 策略引擎
    ///
    /// 两者通常成对出现：策略的 cancel_flag 由 think_runtime 提供，
    /// 命中策略时也需要 runtime 上报最后一轮快照。
    pub fn with_monitoring(
        mut self,
        think_runtime: &'a Arc<AgentThinkRuntime>,
        policy: &'a dyn crate::pkg::policy::Policy,
    ) -> Self {
        self.think_runtime = Some(think_runtime);
        self.policy = Some(policy);
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

    /// 单次唤醒最大思考轮次（awaken + sleep_and_settle 共用）
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
