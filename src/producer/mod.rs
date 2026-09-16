pub mod a2a_polling;
pub mod agent_settle;
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

    // 无业务收尾、只为「失败后重试到第几次就放弃」兜底 —— 见该模块文档
    aop::registry().register_producer(Arc::new(agent_settle::AgentSettleProducer::new()))?;

    sys_info!("all business producers registered");

    sys_info!("starting message channel producers...");
    message_channel::init().await?;

    Ok(())
}
