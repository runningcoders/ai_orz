use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// Agent 运行时状态变更事件（Idle/Busy/Resting 切换）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateEvent {
    pub event_id: String,
    pub agent_id: String,
    /// 变更前状态："idle" / "busy" / "resting"
    pub from_state: String,
    /// 变更后状态："idle" / "busy" / "resting"
    pub to_state: String,
    /// Busy 时关联的消息 ID
    pub message_id: Option<String>,
    pub created_at: i64,
}

impl AgentStateEvent {
    pub fn new(
        agent_id: &str,
        from_state: &str,
        to_state: &str,
        message_id: Option<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            from_state: from_state.to_string(),
            to_state: to_state.to_string(),
            message_id,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for AgentStateEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.state.changed")
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
