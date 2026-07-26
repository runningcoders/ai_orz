use common::error::{Result, err};
use std::sync::Arc;

use crate::pkg::RequestContext;
use crate::pkg::adapter::AdaptedMessage;
use crate::pkg::adapter::message::MessageAdapterCallback;
use crate::service::domain::hr::HrDomain;
use crate::service::domain::message::{MessageDomain, SendToAgentCommand};

struct MessageChannelProducer {
    hr_domain: Arc<dyn HrDomain>,
    message_domain: Arc<dyn MessageDomain>,
}

impl MessageChannelProducer {
    pub fn new() -> Self {
        Self {
            hr_domain: crate::service::domain::hr::domain(),
            message_domain: crate::service::domain::message::domain(),
        }
    }
}

#[async_trait::async_trait]
impl MessageAdapterCallback for MessageChannelProducer {
    async fn on_message(&self, msg: AdaptedMessage) -> Result<()> {
        let ctx = RequestContext::new(None, None);

        let to_agent_id = match msg.to_agent_id {
            Some(id) => id,
            None => match self.hr_domain.resolve_agent(ctx.clone()).await? {
                Some(agent) => agent.po.id,
                None => {
                    log_warn!(
                        &ctx,
                        "message_channel_producer",
                        "no available onboarded agent for routing from_user={}",
                        msg.from_id
                    );
                    return Ok(());
                }
            },
        };

        let cmd = SendToAgentCommand {
            from_id: &msg.from_id,
            from_role: msg.from_role,
            to_agent_id: &to_agent_id,
            content: &msg.content,
            project_id: msg.project_id.as_deref(),
            task_id: msg.task_id.as_deref(),
            reply_to_id: msg.reply_to_id.as_deref(),
            attachment_ids: None,
        };

        self.message_domain
            .delivery()
            .send_to_agent(ctx.clone(), cmd)
            .await
            .map_err(|e| {
                err!(
                    Internal,
                    "message channel producer send_to_agent failed from={} to_agent={}: {}",
                    msg.from_id,
                    to_agent_id,
                    e
                )
            })?;

        log_info!(
            &ctx,
            "message_channel_producer",
            "message dispatched: from={} to_agent={}",
            msg.from_id,
            to_agent_id
        );
        Ok(())
    }
}

pub async fn init() -> Result<()> {
    let registry = crate::pkg::adapter::message::registry();

    if registry.is_empty() {
        sys_info!("no message channel adapters registered, skip init");
        return Ok(());
    }

    let producer = Arc::new(MessageChannelProducer::new());
    registry.start_all(producer).await?;

    sys_info!(
        "message channel producers started, total adapters: {}",
        registry.len()
    );
    Ok(())
}

pub async fn shutdown() -> Result<()> {
    let registry = crate::pkg::adapter::message::registry();
    registry.stop_all().await?;
    Ok(())
}
