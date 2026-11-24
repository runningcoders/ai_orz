//! 思考图骨架事件（`agent.think.graph`）
//!
//! 线格式定义在 `common::api::focus`（前后端共享）；本文件只做服务端适配：
//! 补 `event_id` / `created_at` 并实现 [`Event`]。
//!
//! 一条事件 = 图上的一次动作；前端把它们叠加成一张实时生长的思考图。

use crate::pkg::aop::Event;
use common::enums::EventTopic;

pub use common::api::focus::{FocusGraphEdge, FocusGraphNode, ThinkGraphAction, ThinkGraphEvent};

/// 构造一条骨架事件（event_id / created_at 的唯一补全点）
#[allow(clippy::too_many_arguments)]
pub fn graph_event(
    agent_id: &str,
    scene: &str,
    graph_id: &str,
    run_id: &str,
    path: &str,
    task_id: Option<String>,
    project_id: Option<String>,
    action: ThinkGraphAction,
) -> ThinkGraphEvent {
    ThinkGraphEvent {
        event_id: common::id::TimeId::generate().to_string(),
        agent_id: agent_id.to_string(),
        scene: scene.to_string(),
        graph_id: graph_id.to_string(),
        run_id: run_id.to_string(),
        path: path.to_string(),
        task_id,
        project_id,
        created_at: common::constants::utils::current_timestamp_ms(),
        action,
    }
}

impl Event for ThinkGraphEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::AgentThinkGraph
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
