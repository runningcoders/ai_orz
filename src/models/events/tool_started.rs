//! 工具调用开始事件（`agent.tool.started`）
//!
//! 与既有 `ToolExecEvent`（结束）配对：开始事件带「模型想干什么」（intent）与
//! 实际入参，结束事件带结果与耗时。两者都是运行时消息，不落消息库。

use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde_json::Value;

pub use common::api::focus::ToolStartedEvent;

#[allow(clippy::too_many_arguments)]
pub fn tool_started_event(
    agent_id: &str,
    call_id: &str,
    tool_id: &str,
    tool_name: &str,
    intent: Option<String>,
    args: Value,
    task_id: Option<String>,
    project_id: Option<String>,
) -> ToolStartedEvent {
    ToolStartedEvent {
        event_id: common::id::TimeId::generate().to_string(),
        agent_id: agent_id.to_string(),
        call_id: call_id.to_string(),
        tool_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        intent,
        args,
        task_id,
        project_id,
        created_at: common::constants::utils::current_timestamp_ms(),
    }
}

impl Event for ToolStartedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::AgentToolStarted
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
