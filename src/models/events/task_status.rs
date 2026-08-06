use crate::pkg::aop::{Event, EventKind};
use common::enums::task::TaskStatus;
use serde::{Deserialize, Serialize};

/// 任务状态变更事件
///
/// 由 Task DAL 层 `update_status` 在 SQL UPDATE 成功后发布。
/// 订阅者：
/// - TaskEventConsumer：异步消费，对 Completed 状态变更触发 Owner Agent 通知
///
/// 通过 AOP 异步发布，确保所有 status 变更都触发通知，无论调用方是 domain 还是 handler。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusChangedEvent {
    pub event_id: String,
    pub task_id: String,
    pub task_title: String,
    pub project_id: Option<String>,
    pub assignee_id: String,
    pub old_status: TaskStatus,
    pub new_status: TaskStatus,
    pub progress: i32,
    pub created_at: i64,
}

impl TaskStatusChangedEvent {
    pub fn new(
        task_id: &str,
        task_title: &str,
        project_id: Option<&str>,
        assignee_id: &str,
        old_status: TaskStatus,
        new_status: TaskStatus,
        progress: i32,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            task_id: task_id.to_string(),
            task_title: task_title.to_string(),
            project_id: project_id.map(|s| s.to_string()),
            assignee_id: assignee_id.to_string(),
            old_status,
            new_status,
            progress,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for TaskStatusChangedEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("task.status_changed")
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
