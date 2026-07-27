use crate::models::agent::AgentPo;
use crate::models::events::{
    A2A_SYNCED_MSG_COUNT_PREFIX, extract_a2a_task_id, extract_text_from_parts,
    get_synced_msg_count, make_synced_msg_tag,
};
use crate::pkg::RequestContext;
use crate::pkg::aop::{Producer, Registry};
use crate::service::dao::agent_runtime::a2a::{A2aRuntimeConfig, A2aRuntimeDao};
use crate::service::domain::hr as hr_domain;
use crate::service::domain::message::{self as message_domain, SendToUserCommand};
use crate::service::domain::project as project_domain;
use common::enums::{AssigneeType, TaskStatus};
use common::error::Result;
use std::sync::{Arc, RwLock};

pub struct A2aPollingProducer {
    registry: RwLock<Option<Arc<Registry>>>,
}

impl Default for A2aPollingProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aPollingProducer {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(None),
        }
    }

    fn build_a2a_dao(agent: &AgentPo) -> Option<A2aRuntimeDao> {
        let config = agent.get_remote_config()?;
        Some(A2aRuntimeDao::new(A2aRuntimeConfig {
            endpoint: config.endpoint,
            agent_name: config.agent_name,
            auth_token: config.auth_token,
            timeout_secs: config.timeout_secs,
        }))
    }
}

#[async_trait::async_trait]
impl Producer for A2aPollingProducer {
    fn name(&self) -> &str {
        "a2a_polling"
    }

    async fn register(&self, registry: Arc<Registry>) -> Result<()> {
        let mut reg = self.registry.write().unwrap();
        *reg = Some(registry);
        Ok(())
    }

    fn poll_interval_secs(&self) -> u64 {
        30
    }

    async fn poll(&self) -> Result<()> {
        let ctx = RequestContext::new(None, None);

        let all_agents = hr_domain::domain()
            .agent_manage()
            .list_agents(ctx.clone())
            .await?;

        let remote_agents: Vec<_> = all_agents
            .into_iter()
            .filter(|a| a.po.kind.is_remote())
            .collect();

        if remote_agents.is_empty() {
            return Ok(());
        }

        log_debug!("a2a polling: found {} remote agents", remote_agents.len());

        let mut processed_count = 0usize;

        for agent in &remote_agents {
            let tasks = project_domain::domain()
                .task_manage()
                .list(
                    ctx.clone(),
                    None,
                    Some(AssigneeType::Agent),
                    Some(&agent.po.id),
                    Some(TaskStatus::InProgress),
                    Some(100),
                )
                .await?;

            if tasks.is_empty() {
                continue;
            }

            let Some(a2a_dao) = Self::build_a2a_dao(&agent.po) else {
                log_warn!(
                    &ctx,
                    "a2a_polling",
                    "Remote agent {} has invalid or missing remote config, skipping",
                    agent.po.id
                );
                continue;
            };

            for task in &tasks {
                let tags = task.po.get_tags();
                let Some(remote_task_id) = extract_a2a_task_id(&tags) else {
                    continue;
                };

                let remote_task = match a2a_dao.fetch_task(&remote_task_id).await {
                    Ok(t) => t,
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "a2a_polling",
                            "Failed to fetch remote task {} for local task {} (agent {}): {}",
                            remote_task_id,
                            task.po.id,
                            agent.po.id,
                            e
                        );
                        continue;
                    }
                };

                let mut task_ctx_builder = RequestContext::builder();
                task_ctx_builder = task_ctx_builder.agent_id(agent.po.id.clone());
                task_ctx_builder = task_ctx_builder.task_id(task.po.id.clone());
                if let Some(pid) = &task.po.project_id {
                    task_ctx_builder = task_ctx_builder.project_id(pid.clone());
                }
                let task_ctx = task_ctx_builder.build();

                let already_synced = get_synced_msg_count(&tags);
                let agent_messages: Vec<_> = remote_task
                    .messages
                    .iter()
                    .filter(|msg| msg.role == "agent" || msg.role == "assistant")
                    .collect();
                let total_agent_msgs = agent_messages.len();
                let mut new_sent = 0usize;

                if total_agent_msgs > already_synced {
                    let new_messages = &agent_messages[already_synced..];
                    for msg in new_messages {
                        let text = extract_text_from_parts(&msg.parts);
                        if text.is_empty() {
                            continue;
                        }

                        let cmd = SendToUserCommand {
                            from_agent_id: &agent.po.id,
                            to_user_id: &task.po.root_user_id,
                            content: &text,
                            project_id: task.po.project_id.as_deref(),
                            task_id: Some(&task.po.id),
                            reply_to_id: None,
                        };

                        if let Err(e) = message_domain::domain()
                            .delivery()
                            .send_to_user(task_ctx.clone(), cmd)
                            .await
                        {
                            log_warn!(
                                &task_ctx,
                                "a2a_polling",
                                "Failed to send message for task {}: {}",
                                task.po.id,
                                e
                            );
                        } else {
                            new_sent += 1;
                        }
                    }
                }

                if new_sent > 0 {
                    let new_total = already_synced + new_sent;
                    let mut new_tags: Vec<String> = tags
                        .iter()
                        .filter(|t| !t.starts_with(A2A_SYNCED_MSG_COUNT_PREFIX))
                        .cloned()
                        .collect();
                    new_tags.push(make_synced_msg_tag(new_total));

                    if let Err(e) = project_domain::domain()
                        .task_manage()
                        .update_basic(
                            task_ctx.clone(),
                            &task.po.id,
                            None,
                            None,
                            None,
                            Some(new_tags),
                            None,
                            None,
                        )
                        .await
                    {
                        log_warn!(
                            &task_ctx,
                            "a2a_polling",
                            "Failed to update synced msg count for task {}: {}",
                            task.po.id,
                            e
                        );
                    }
                }

                let mut local_task = task.clone();
                let target_status = match remote_task.status.state {
                    common::api::a2a::A2aTaskState::Completed => Some(TaskStatus::Completed),
                    common::api::a2a::A2aTaskState::Failed => Some(TaskStatus::Cancelled),
                    common::api::a2a::A2aTaskState::Canceled => Some(TaskStatus::Cancelled),
                    common::api::a2a::A2aTaskState::Working
                    | common::api::a2a::A2aTaskState::Submitted
                    | common::api::a2a::A2aTaskState::InputRequired => {
                        if local_task.po.status == TaskStatus::Pending {
                            Some(TaskStatus::InProgress)
                        } else {
                            None
                        }
                    }
                };

                if let Some(target) = target_status
                    && local_task.po.status != target
                {
                    if let Err(e) = project_domain::domain()
                        .task_manage()
                        .transition_status(task_ctx.clone(), &mut local_task, target)
                        .await
                    {
                        log_warn!(
                            &task_ctx,
                            "a2a_polling",
                            "Failed to transition task {} to {:?}: {}",
                            task.po.id,
                            target,
                            e
                        );
                    } else {
                        log_info!(
                            &task_ctx,
                            "a2a_polling",
                            "Task {} transitioned to {:?}",
                            task.po.id,
                            target
                        );
                    }
                }

                processed_count += 1;
            }
        }

        if processed_count > 0 {
            log_info!("a2a polling processed {} tasks", processed_count);
        }

        Ok(())
    }
}
