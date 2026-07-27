use crate::models::events::CronTriggerEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{Producer, Registry};
use crate::service::domain::system;
use common::error::Result;
use std::sync::{Arc, RwLock};

pub struct CronTriggerProducer {
    registry: RwLock<Option<Arc<Registry>>>,
}

impl Default for CronTriggerProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl CronTriggerProducer {
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl Producer for CronTriggerProducer {
    fn name(&self) -> &str {
        "cron_trigger"
    }

    async fn register(&self, registry: Arc<Registry>) -> Result<()> {
        let mut reg = self.registry.write().unwrap();
        *reg = Some(registry);
        Ok(())
    }

    fn poll_interval_secs(&self) -> u64 {
        60
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
        let now = common::constants::utils::current_timestamp();

        let triggers = system::domain()
            .cron_manager()
            .list_due_triggers(ctx.clone(), now, 100)
            .await?;

        if triggers.is_empty() {
            return Ok(());
        }

        log_debug!("cron producer found {} due triggers", triggers.len());

        for trigger in &triggers {
            let event = CronTriggerEvent {
                event_id: format!("{}-{}", trigger.id, now),
                trigger_id: trigger.id.clone(),
                trigger_name: trigger.name.clone(),
                payload: trigger.payload.clone(),
                created_at: common::constants::utils::current_timestamp(),
            };

            registry.publish(event).await;

            system::domain()
                .cron_manager()
                .mark_trigger_executed(ctx.clone(), &trigger.id, now)
                .await?;
        }

        log_info!("cron producer published {} trigger events", triggers.len());

        Ok(())
    }
}
