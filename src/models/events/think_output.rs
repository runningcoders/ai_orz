//! 轮次级思考输出事件（`agent.think.output`）
//!
//! 骨架之外的细节层：一轮思考结束后把推理摘要与助手输出预览推给专注浮层。
//! 只做展示，不落消息库。

use crate::pkg::aop::Event;
use common::enums::EventTopic;

pub use common::api::focus::ThinkOutputEvent;

#[allow(clippy::too_many_arguments)]
pub fn think_output_event(
    agent_id: &str,
    scene: &str,
    graph_id: &str,
    run_id: &str,
    round: usize,
    reasoning: String,
    content: String,
) -> ThinkOutputEvent {
    ThinkOutputEvent {
        event_id: common::id::TimeId::generate().to_string(),
        agent_id: agent_id.to_string(),
        scene: scene.to_string(),
        graph_id: graph_id.to_string(),
        run_id: run_id.to_string(),
        round,
        reasoning,
        content,
        created_at: common::constants::utils::current_timestamp_ms(),
    }
}

impl Event for ThinkOutputEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::AgentThinkOutput
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
