pub mod agent_loop_consumer;
pub mod aop_stats_collector;
pub mod aop_stats_hook;
pub mod message;
pub mod scheduler;
pub mod think_round_stats_consumer;
pub mod tool_exec_log_consumer;
pub mod tool_exec_stats_consumer;

use common::error::Result;
use std::sync::Arc;

use crate::pkg::aop;

pub async fn init() -> Result<()> {
    sys_info!("registering business consumers to AOP event center...");

    aop::registry().register_consumer(Arc::new(message::MessageConsumer::new()))?;

    aop::registry().register_consumer(Arc::new(scheduler::CronTriggerConsumer::new()))?;

    aop::registry()
        .register_consumer(Arc::new(tool_exec_log_consumer::ToolExecLogConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(
        tool_exec_stats_consumer::ToolExecStatsConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(agent_loop_consumer::AgentLoopConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(
        think_round_stats_consumer::ThinkRoundStatsConsumer::new(),
    ))?;

    sys_info!("all business consumers registered");
    Ok(())
}

pub use aop_stats_collector::{
    AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint,
};
pub use aop_stats_hook::AopStatsHook;
