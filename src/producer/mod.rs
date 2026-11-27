pub mod a2a_polling;
pub mod cron_dispatch_queue;
pub mod cron_trigger;
pub mod inbound_filter;
pub mod message_channel;

use crate::pkg::aop;
use common::error::Result;
use std::sync::Arc;

pub async fn init() -> Result<()> {
    sys_info!("registering business producers to AOP event center...");

    // 同一实例双身份：既是 `cron.trigger` 的生产者，也是 `schedule.changed` 的消费者。
    // 派发队列是这个类型的私有字段，consumer 入队与 producer 出队共享同一份状态。
    let cron = Arc::new(cron_trigger::CronTriggerProducer::new());
    aop::registry().register_producer(cron.clone())?;
    aop::registry().register_consumer(cron)?;

    aop::registry().register_producer(Arc::new(a2a_polling::A2aPollingProducer::new()))?;

    sys_info!("all business producers registered");

    sys_info!("starting message channel producers...");
    message_channel::init().await?;

    Ok(())
}
