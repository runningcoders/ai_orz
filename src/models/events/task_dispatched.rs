use crate::pkg::aop::Event;
use common::enums::EventTopic;
use serde::{Deserialize, Serialize};

/// 任务派发事件（动作已落库，请求派发 `TaskAssignment` 消息）
///
/// 与 [`super::TaskStatusChangedEvent`] 的分工：那个描述「任务状态变了」，
/// 这个描述「有一个新任务需要被交给执行者」。二者都是**动作先行、消息后发**
/// 的伴生事件，不是 Agent 的回复。
///
/// ⚠️ 消费者是 `ConsumeMode::Sync`：`aop::publish` 会内联执行派发，事件不进
/// 内存队列。这是可靠性的前提 —— 「任务已落库但派单消息没发出去」是一个
/// 永远不会有人报错的死任务，不能容忍进程重启把队列清空。若将来改成 Async，
/// **必须**同时补一条「扫 Pending 且无 TaskAssignment 的任务行」的补偿扫描，
/// 否则等于重新引入静默死任务窗口。
///
/// 交付身份与链锚点在发布时快照，消费者不再反查任务行。
///
/// ⚠️ 这里**没有** `from_id` / `from_role`：派发者由 `initiator_agent_id` /
/// `root_user_id` 推导（见 `consumer::task_sender_identity`）。历史版本把
/// 「归属用户」当中继写进 `from_id`，抹掉了真实派发者，已废弃。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDispatchedEvent {
    pub event_id: String,
    pub task_id: String,
    pub task_title: String,
    pub task_description: String,
    pub project_id: Option<String>,
    pub assignee_id: String,
    /// 归属用户：结果最终关联给谁，也是被唤醒 Agent 的工具凭据来源
    pub root_user_id: String,
    /// 任务发起者 Agent：谁派发、谁对交付收尾负责；空串 = 无发起者
    pub initiator_agent_id: String,
    /// 触发任务诞生的消息 ID：派单挂回这条链；空串 = 另起新链
    pub trigger_message_id: String,
    pub created_at: i64,
}

impl TaskDispatchedEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_id: &str,
        task_title: &str,
        task_description: &str,
        project_id: Option<&str>,
        assignee_id: &str,
        root_user_id: &str,
        initiator_agent_id: &str,
        trigger_message_id: &str,
    ) -> Self {
        Self {
            event_id: common::id::TimeId::generate().to_string(),
            task_id: task_id.to_string(),
            task_title: task_title.to_string(),
            task_description: task_description.to_string(),
            project_id: project_id.map(str::to_string),
            assignee_id: assignee_id.to_string(),
            root_user_id: root_user_id.to_string(),
            initiator_agent_id: initiator_agent_id.to_string(),
            trigger_message_id: trigger_message_id.to_string(),
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for TaskDispatchedEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::TaskDispatched
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.task_id
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}
