use crate::models::cortex_types::TokenUsage;
use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

/// 每轮 think 事件（记录轮次、耗时、是否触发工具调用、token 用量）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkRoundEvent {
    pub event_id: String,
    pub agent_id: String,
    pub trace_id: String,
    /// "awaken" 或 "settle"
    pub scene: String,
    /// 第几轮（从 0 开始）
    pub round_number: usize,
    /// 本轮 think 耗时（毫秒）
    pub duration_ms: u64,
    /// 是否触发了工具调用
    pub has_tool_calls: bool,
    /// 工具调用数量
    pub tool_call_count: usize,
    /// 模型提供商 ID（Local agent 有值，外部 agent 为 None）
    pub model_provider_id: Option<String>,
    /// 模型名称
    pub model_name: Option<String>,
    /// 输入 token 数
    pub tokens_input: u64,
    /// 输出 token 数
    pub tokens_output: u64,
    /// 总 token 数
    pub total_tokens: u64,
    /// 提示缓存命中的输入 token 数（provider 未上报时为 0）
    pub cached_tokens: u64,
    /// 思考/推理消耗的输出 token 数（provider 未上报时为 0）
    pub reasoning_tokens: u64,
    /// 组织 ID
    pub organization_id: Option<String>,
    /// 用户 ID
    pub user_id: Option<String>,
    /// 任务 ID
    pub task_id: Option<String>,
    /// 项目 ID
    pub project_id: Option<String>,
    pub created_at: i64,
}

impl ThinkRoundEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_id: &str,
        trace_id: &str,
        scene: &str,
        round_number: usize,
        duration_ms: u64,
        has_tool_calls: bool,
        tool_call_count: usize,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            trace_id: trace_id.to_string(),
            scene: scene.to_string(),
            round_number,
            duration_ms,
            has_tool_calls,
            tool_call_count,
            model_provider_id: None,
            model_name: None,
            tokens_input: 0,
            tokens_output: 0,
            total_tokens: 0,
            cached_tokens: 0,
            reasoning_tokens: 0,
            organization_id: None,
            user_id: None,
            task_id: None,
            project_id: None,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }

    /// 链式设置模型提供商信息 + token 用量
    ///
    /// 收 `&TokenUsage` 而非散装的 u64：明细字段（缓存命中 / 思考开销）加入后
    /// 再逐个传参会让签名持续膨胀，且调用侧容易漏传某一路。
    pub fn with_model_usage(
        mut self,
        model_provider_id: Option<String>,
        model_name: Option<String>,
        usage: &TokenUsage,
    ) -> Self {
        self.model_provider_id = model_provider_id;
        self.model_name = model_name;
        self.tokens_input = usage.input_tokens;
        self.tokens_output = usage.output_tokens;
        self.total_tokens = usage.total();
        self.cached_tokens = usage.cached_tokens;
        self.reasoning_tokens = usage.reasoning_tokens;
        self
    }

    /// 链式设置上下文信息（组织/用户/任务/项目）
    pub fn with_context(
        mut self,
        organization_id: Option<String>,
        user_id: Option<String>,
        task_id: Option<String>,
        project_id: Option<String>,
    ) -> Self {
        self.organization_id = organization_id;
        self.user_id = user_id;
        self.task_id = task_id;
        self.project_id = project_id;
        self
    }
}

impl Event for ThinkRoundEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::AgentThinkRound
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.agent_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
