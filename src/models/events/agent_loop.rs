use crate::pkg::aop::{Event, EventKind};
use serde::{Deserialize, Serialize};

/// Agent 循环生命周期事件（awaken/sleep_and_settle 的启动与完成）
///
/// 通过 AOP 同步发布，订阅者可记录循环耗时、状态等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLoopEvent {
    pub event_id: String,
    pub agent_id: String,
    pub trace_id: String,
    /// "awaken" 或 "settle"
    pub scene: String,
    /// "started" 或 "finished"
    pub phase: String,
    /// 完成时才有值："success" 或 "failed: {error}"
    pub status: Option<String>,
    /// 完成时才有值（毫秒）
    pub duration_ms: Option<u64>,
    /// awaken 场景关联的消息 ID
    pub message_id: Option<String>,
    pub created_at: i64,
}

impl AgentLoopEvent {
    pub fn started(agent_id: &str, trace_id: &str, scene: &str, message_id: Option<&str>) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            trace_id: trace_id.to_string(),
            scene: scene.to_string(),
            phase: "started".to_string(),
            status: None,
            duration_ms: None,
            message_id: message_id.map(|s| s.to_string()),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }

    pub fn finished(
        agent_id: &str,
        trace_id: &str,
        scene: &str,
        status: &str,
        duration_ms: u64,
        message_id: Option<&str>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id: agent_id.to_string(),
            trace_id: trace_id.to_string(),
            scene: scene.to_string(),
            phase: "finished".to_string(),
            status: Some(status.to_string()),
            duration_ms: Some(duration_ms),
            message_id: message_id.map(|s| s.to_string()),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for AgentLoopEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("agent.loop")
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
