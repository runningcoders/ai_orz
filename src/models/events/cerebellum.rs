//! 小脑决策事件（`agent.cerebellum.decided`）
//!
//! 让专注浮层能看到「小脑刚判了什么」：场景、结论摘要、耗时与降级原因。
//! 只做展示，不参与业务判定。

use crate::pkg::aop::Event;
use common::enums::EventTopic;

pub use common::api::focus::{CerebellumAnswerBrief, CerebellumDecidedEvent};

pub fn cerebellum_decided_event(
    agent_id: &str,
    scenes: Vec<String>,
    outcome: &str,
    degraded: Option<String>,
    latency_ms: u64,
    answers: Vec<CerebellumAnswerBrief>,
) -> CerebellumDecidedEvent {
    CerebellumDecidedEvent {
        event_id: common::id::TimeId::generate().to_string(),
        agent_id: agent_id.to_string(),
        scenes,
        outcome: outcome.to_string(),
        degraded,
        latency_ms,
        answers,
        created_at: common::constants::utils::current_timestamp_ms(),
    }
}

impl Event for CerebellumDecidedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::AgentCerebellumDecided
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
