use crate::pkg::aop::{Event, EventKind};
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use serde::{Deserialize, Serialize};

/// 工具执行完成事件（取代 ToolCallLoggingDecorator 的日志+统计职责）
///
/// 由 ToolCallDao::execute 在工具执行完成后通过 AOP 同步发布。
/// 订阅者：
/// - ToolExecLogConsumer：写入 JSONL 日志（取代 decorator 的 log_call）
/// - ToolExecStatsConsumer：记录统计事件（取代 decorator 的 record_tool_call_stat）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecEvent {
    pub event_id: String,
    /// 完整的工具调用条目（与原 ToolCallEntry 结构一致）
    pub entry: ToolCallEntry,
    /// 从 ctx 提取的组织 ID（统计用）
    pub organization_id: Option<String>,
    /// 从 ctx 提取的用户 ID（统计用）
    pub user_id: Option<String>,
    /// 原始参数 JSON 长度（统计用）
    pub args_len: u64,
    /// 结果 JSON 长度（统计用）
    pub result_len: u64,
    pub created_at: i64,
}

impl ToolExecEvent {
    pub fn new(
        entry: ToolCallEntry,
        organization_id: Option<String>,
        user_id: Option<String>,
        args_len: u64,
        result_len: u64,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            entry,
            organization_id,
            user_id,
            args_len,
            result_len,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for ToolExecEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.tool.executed")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        // 按 agent_id 串行，保证同一 Agent 的工具日志顺序
        self.entry.agent_id.as_deref().unwrap_or("")
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
