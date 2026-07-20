pub mod cron_trigger;
pub mod message_channel;

use common::error::Result;
use crate::pkg::aop;
use std::sync::Arc;

pub async fn init() -> Result<()> {
    sys_info!("registering business producers to AOP event center...");

    aop::registry()
        .register_producer(Arc::new(cron_trigger::CronTriggerProducer::new()))
        .await?;

    sys_info!("all business producers registered");

    sys_info!("starting message channel producers...");
    message_channel::init().await?;

    Ok(())
}