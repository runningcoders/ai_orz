//! 轮级环图的跨节点状态
//!
//! 图引擎的 `ThinkBus` 只装 `serde_json::Value`，而思考状态含 `ChatMessage` /
//! `ThinkLoopResult` 这类强类型对象（且节点需要「取出 → 异步处理 → 放回」以避免
//! 每轮整段消息 clone）。因此状态放在所有节点共享的 `Arc<Mutex<RoundState>>` 里，
//! 与 `ThinkLoopProgress` 同一套路：节点顺序执行，锁只在同步片段内持有。

use crate::models::cortex_types::{ChatMessage, TokenUsage, ToolCallRequest};
use crate::pkg::think_trace::{ThinkTraceRound, ThinkTraceToolCall};
use crate::service::domain::runtime::types::ThinkLoopResult;
use std::collections::{HashMap, HashSet};

/// 思考文本落盘上限（字符数）：reasoning / assistant 文本可能很长，
/// think trace 是列表扫描型数据，必须限制单条体积。
pub(crate) const TRACE_TEXT_MAX_CHARS: usize = 2000;
/// 工具入参 / 结果预览落盘上限（字符数）；完整内容在 tool_tracing 中
pub(crate) const TRACE_TOOL_PREVIEW_MAX_CHARS: usize = 500;

/// 按字符数截断（避免 UTF-8 边界 panic），超长时追加省略号
pub(crate) fn truncate_trace_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}…")
}

/// 工具调用入参预览：序列化失败时退化为空对象
pub(crate) fn tool_arguments_preview(arguments: &serde_json::Value) -> String {
    let raw = serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
    truncate_trace_text(&raw, TRACE_TOOL_PREVIEW_MAX_CHARS)
}

/// 结果预览
pub(crate) fn tool_result_preview(result: &str) -> String {
    truncate_trace_text(result, TRACE_TOOL_PREVIEW_MAX_CHARS)
}

/// 为待执行工具构造 trace 记录（结果由 dispatch 回填）
pub(crate) fn pending_tool_trace(tc: &ToolCallRequest) -> ThinkTraceToolCall {
    ThinkTraceToolCall {
        call_id: None,
        name: tc.name.clone(),
        arguments: tool_arguments_preview(&tc.arguments),
        result: None,
        status: "pending".to_string(),
    }
}

/// 本轮模型的产出类型（决定 evaluate 用哪套策略指标）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputKind {
    /// 模型给出最终回答
    Final,
    /// 模型要求调用工具
    ToolCalls,
}

impl OutputKind {
    /// 与 `round_metrics(...).with("output_kind", ...)` 使用的标签一致
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::ToolCalls => "tool_calls",
        }
    }
}

/// 一轮思考的完整状态（跨 think / dispatch / evaluate 三个节点流转）
#[derive(Debug)]
pub(crate) struct RoundState {
    /// 对话历史（think 取出调用模型，dispatch 追加工具结果后放回）
    pub messages: Vec<ChatMessage>,
    /// 已开始的轮次数（下一次 think 用它推算轮次编号）
    pub rounds_started: usize,
    /// 累计输入 token
    pub total_input_tokens: u64,
    /// 累计输出 token
    pub total_output_tokens: u64,
    /// 累计工具调用次数
    pub total_tool_calls: usize,
    /// 按 wire name 累计的工具调用次数（无进展检测数据源）
    pub tool_call_counts: HashMap<String, usize>,
    /// 连续只调工具的轮数（疲劳提示依据）
    pub consecutive_tool_rounds: usize,
    /// 疲劳提示是否已注入（只注入一次）
    pub nudge_injected: bool,
    /// 已做过「换方向」提示的工具（同一工具只提示一次，避免 System 消息堆积）
    pub reflected_tools: HashSet<String>,
    /// LLM 连续失败次数（成功即清零）
    pub llm_consecutive_errors: u64,

    /// 本轮新增消息在 `messages` 中的起始下标（进度快照截取用）
    pub round_start_idx: usize,
    /// 待 dispatch 执行的工具调用
    pub pending_tool_calls: Vec<ToolCallRequest>,
    /// 本轮助手文本（进度快照用）
    pub pending_assistant_text: Option<String>,
    /// 本轮 token 用量（事件上报用）
    pub pending_usage: Option<TokenUsage>,
    /// 本轮 LLM 调用耗时（事件上报用）
    pub pending_round_duration_ms: u64,

    /// 最近一轮的 0-based 轮次编号（与老循环里的 `round` 同口径）
    pub last_round: usize,
    /// 最近一轮送入模型的 prompt token 数（策略上下文指标）
    pub last_input_tokens: u64,
    /// 本轮产出类型（evaluate 分派依据）
    pub output_kind: Option<OutputKind>,
    /// Final 内容（evaluate 兜底时使用）
    pub final_content: Option<String>,
    /// 最终退出结果（终止后由调用方取走）
    pub result: Option<ThinkLoopResult>,

    /// 轮级明细（think 创建、dispatch 回填工具结果、evaluate 补裁决）
    pub trace_rounds: Vec<ThinkTraceRound>,
}

impl RoundState {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            rounds_started: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_tool_calls: 0,
            tool_call_counts: HashMap::new(),
            consecutive_tool_rounds: 0,
            nudge_injected: false,
            reflected_tools: HashSet::new(),
            llm_consecutive_errors: 0,
            round_start_idx: 0,
            pending_tool_calls: Vec::new(),
            pending_assistant_text: None,
            pending_usage: None,
            pending_round_duration_ms: 0,
            last_round: 0,
            last_input_tokens: 0,
            output_kind: None,
            final_content: None,
            result: None,
            trace_rounds: Vec::new(),
        }
    }
}
