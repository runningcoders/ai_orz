use common::enums::{AssigneeType, TaskStatus};
use common::error::Result;
use crate::models::agent::AgentPo;
use crate::models::events::{A2aTaskUpdateEvent, A2aUpdateSource};
use crate::pkg::RequestContext;
use crate::pkg::aop::{Producer, Registry};
use crate::service::dao::agent_runtime::a2a::{A2aRuntimeConfig, A2aRuntimeDao};
use crate::service::domain::hr as hr_domain;
use crate::service::domain::project as project_domain;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

pub struct A2aPollingProducer {
    registry: RwLock<Option<Arc<Registry>>>,
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
        let registry = {
            let reg = self.registry.read().unwrap();
            reg.clone()
        };

        let Some(registry) = registry else {
            return Err(common::error::err!(Internal, "registry not registered"));
        };

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

        let mut published_count = 0usize;

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
                let Some(remote_task_id) = A2aTaskUpdateEvent::extract_a2a_task_id(&tags) else {
                    continue;
                };

                match a2a_dao.fetch_task(&remote_task_id).await {
                    Ok(remote_task) => {
                        let task_json = serde_json::to_string(&remote_task)
                            .unwrap_or_default();

                        let event = A2aTaskUpdateEvent {
                            event_id: Uuid::now_v7().to_string(),
                            local_task_id: task.po.id.clone(),
                            remote_agent_id: agent.po.id.clone(),
                            remote_task_id: remote_task_id.clone(),
                            source: A2aUpdateSource::Polling,
                            task_json,
                            created_at: common::constants::utils::current_timestamp(),
                        };

                        registry.publish(event).await;
                        published_count += 1;
                    }
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
                    }
                }
            }
        }

        if published_count > 0 {
            log_info!("a2a polling published {} task update events", published_count);
        }

        Ok(())
    }
}
