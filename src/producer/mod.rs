pub mod a2a_polling;
pub mod cron_trigger;
pub mod message_channel;

use crate::pkg::aop;
use common::error::Result;
use std::sync::Arc;

pub async fn init() -> Result<()> {
    sys_info!("registering business producers to AOP event center...");

    // `register_producer` 随 `Producer::register` 的删除已退化为**同步**纯 push
    aop::registry().register_producer(Arc::new(cron_trigger::CronTriggerProducer::new()))?;

    aop::registry().register_producer(Arc::new(a2a_polling::A2aPollingProducer::new()))?;

    sys_info!("all business producers registered");

    sys_info!("starting message channel producers...");
    message_channel::init().await?;

    Ok(())
}
