use async_trait::async_trait;
use common::api::a2a::{A2aMessagePart, A2aTask, A2aTaskState};
use common::enums::TaskStatus;
use common::error::{Error, Result};
use serde_json::Value;
use std::sync::Arc;

use crate::models::events::A2aTaskUpdateEvent;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::pkg::RequestContext;
use crate::service::domain::message::{self as message_domain, MessageDomain, SendToUserCommand};
use crate::service::domain::project::{self as project_domain, ProjectDomain};

const A2A_SYNCED_MSG_COUNT_PREFIX: &str = "a2a_synced_msgs:";

pub struct A2aTaskUpdateConsumer {
    message_domain: Arc<dyn MessageDomain>,
    project_domain: Arc<dyn ProjectDomain>,
}

impl A2aTaskUpdateConsumer {
    pub fn new() -> Self {
        Self {
            message_domain: message_domain::domain(),
            project_domain: project_domain::domain(),
        }
    }

    fn build_ctx(&self, event: &A2aTaskUpdateEvent) -> RequestContext {
        let mut builder = RequestContext::builder();
        builder = builder.agent_id(event.remote_agent_id.clone());
        builder = builder.task_id(event.local_task_id.clone());
        builder.build()
    }

    async fn process_update(
        &self,
        event: &A2aTaskUpdateEvent,
        remote_task: &A2aTask,
    ) -> Result<()> {
        let ctx = self.build_ctx(event);

        let Some(mut local_task) = self.project_domain
            .task_manage()
            .get(ctx.clone(), &event.local_task_id)
            .await?
        else {
            log_warn!(
                &ctx,
                "a2a_task_update",
                "Local task {} not found, skipping update",
                event.local_task_id
            );
            return Ok(());
        };

        if matches!(
            local_task.po.status,
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Archived
        ) {
            log_debug!(
                &ctx,
                "a2a_task_update",
                "Task {} is already in terminal state {:?}, skipping",
                event.local_task_id,
                local_task.po.status
            );
            return Ok(());
        }

        let new_agent_msg_count = self.process_messages(&ctx, event, remote_task, &local_task).await?;

        self.process_status(&ctx, event, remote_task, &mut local_task).await?;

        if new_agent_msg_count > 0 {
            self.update_synced_msg_count(&ctx, event, &local_task, new_agent_msg_count).await?;
        }

        Ok(())
    }

    fn get_synced_msg_count(tags: &[String]) -> usize {
        tags.iter()
            .find(|t| t.starts_with(A2A_SYNCED_MSG_COUNT_PREFIX))
            .and_then(|t| t[A2A_SYNCED_MSG_COUNT_PREFIX.len()..].parse::<usize>().ok())
            .unwrap_or(0)
    }

    fn update_tags_with_synced_count(tags: &[String], new_count: usize) -> Vec<String> {
        let mut new_tags: Vec<String> = tags
            .iter()
            .filter(|t| !t.starts_with(A2A_SYNCED_MSG_COUNT_PREFIX))
            .cloned()
            .collect();
        new_tags.push(format!("{}{}", A2A_SYNCED_MSG_COUNT_PREFIX, new_count));
        new_tags
    }

    async fn process_messages(
        &self,
        ctx: &RequestContext,
        event: &A2aTaskUpdateEvent,
        remote_task: &A2aTask,
        local_task: &crate::models::task::Task,
    ) -> Result<usize> {
        let to_user_id = &local_task.po.root_user_id;
        let tags = local_task.po.get_tags();
        let already_synced = Self::get_synced_msg_count(&tags);

        let agent_messages: Vec<_> = remote_task.messages.iter()
            .filter(|msg| msg.role == "agent" || msg.role == "assistant")
            .collect();

        let total_agent_msgs = agent_messages.len();
        if total_agent_msgs <= already_synced {
            return Ok(0);
        }

        let new_messages = &agent_messages[already_synced..];
        let mut sent_count = 0usize;

        for msg in new_messages {
            let text = extract_text_from_parts(&msg.parts);
            if text.is_empty() {
                continue;
            }

            let cmd = SendToUserCommand {
                from_agent_id: &event.remote_agent_id,
                to_user_id,
                content: &text,
                project_id: local_task.po.project_id.as_deref(),
                task_id: Some(&event.local_task_id),
                reply_to_id: None,
            };

            self.message_domain
                .delivery()
                .send_to_user(ctx.clone(), cmd)
                .await?;
            sent_count += 1;
        }

        Ok(sent_count)
    }

    async fn update_synced_msg_count(
        &self,
        ctx: &RequestContext,
        event: &A2aTaskUpdateEvent,
        local_task: &crate::models::task::Task,
        new_sent_count: usize,
    ) -> Result<()> {
        let tags = local_task.po.get_tags();
        let current_synced = Self::get_synced_msg_count(&tags);
        let new_total = current_synced + new_sent_count;
        let new_tags = Self::update_tags_with_synced_count(&tags, new_total);

        self.project_domain
            .task_manage()
            .update_basic(
                ctx.clone(),
                &event.local_task_id,
                None,
                None,
                None,
                Some(new_tags),
                None,
                None,
            )
            .await?;

        Ok(())
    }

    async fn process_status(
        &self,
        ctx: &RequestContext,
        event: &A2aTaskUpdateEvent,
        remote_task: &A2aTask,
        local_task: &mut crate::models::task::Task,
    ) -> Result<()> {
        let target_status = match remote_task.status.state {
            A2aTaskState::Completed => Some(TaskStatus::Completed),
            A2aTaskState::Failed => Some(TaskStatus::Cancelled),
            A2aTaskState::Canceled => Some(TaskStatus::Cancelled),
            A2aTaskState::Working | A2aTaskState::Submitted | A2aTaskState::InputRequired => {
                if local_task.po.status == TaskStatus::Pending {
                    Some(TaskStatus::InProgress)
                } else {
                    None
                }
            }
        };

        if let Some(target) = target_status {
            if local_task.po.status != target {
                self.project_domain
                    .task_manage()
                    .transition_status(ctx.clone(), local_task, target)
                    .await?;

                log_info!(
                    ctx,
                    "a2a_task_update",
                    "Task {} transitioned to {:?} via {:?}",
                    event.local_task_id,
                    target,
                    event.source
                );
            }
        }

        Ok(())
    }
}

fn extract_text_from_parts(parts: &[A2aMessagePart]) -> String {
    let mut texts = Vec::new();
    for part in parts {
        if let A2aMessagePart::Text { text } = part {
            texts.push(text.clone());
        }
    }
    texts.join("\n")
}

#[async_trait]
impl Consumer for A2aTaskUpdateConsumer {
    fn name(&self) -> &str {
        "a2a.task.update"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("a2a.task.update")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, event: Value) -> Result<()> {
        let update: A2aTaskUpdateEvent = serde_json::from_value(event)?;

        let remote_task: A2aTask = serde_json::from_str(&update.task_json)
            .map_err(|e| Error::internal(format!("Failed to parse A2aTask JSON: {}", e)))?;

        self.process_update(&update, &remote_task).await
    }

    async fn ack(&self, _event_id: &str) -> Result<()> {
        Ok(())
    }

    async fn nack(&self, _event_id: &str) -> Result<()> {
        Ok(())
    }

    fn concurrency(&self) -> usize {
        2
    }

    fn empty_queue_sleep_ms(&self) -> u64 {
        500
    }

    fn error_retry_sleep_ms(&self) -> u64 {
        3000
    }
}
