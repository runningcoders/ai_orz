pub mod aop_stats_collector;
pub mod aop_stats_hook;
pub mod message;
pub mod scheduler;

use common::error::Result;
use std::sync::Arc;

use crate::pkg::aop;

pub async fn init() -> Result<()> {
    sys_info!("registering business consumers to AOP event center...");

    aop::registry().register_consumer(Arc::new(message::MessageConsumer::new()))?;

    aop::registry().register_consumer(Arc::new(scheduler::CronTriggerConsumer::new()))?;

    sys_info!("all business consumers registered");
    Ok(())
}

pub use aop_stats_collector::{
    AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint,
};
pub use aop_stats_hook::AopStatsHook;
