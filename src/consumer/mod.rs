pub mod agent_loop_consumer;
pub mod aop_stats_collector;
pub mod aop_stats_hook;
pub mod federation_directory;
pub mod federation_inbound_task;
pub mod federation_ws_outbound;
pub mod lark_inbound;
pub mod message;
pub mod scheduler;
pub mod task_event_consumer;
pub mod think_round_stats_consumer;
pub mod tool_exec_log_consumer;
pub mod tool_exec_stats_consumer;

use common::error::Result;
use std::sync::Arc;

use crate::pkg::aop;

pub async fn init() -> Result<()> {
    sys_info!("registering business consumers to AOP event center...");

    aop::registry().register_consumer(Arc::new(message::MessageConsumer::new()))?;

    // 飞书入站消息：DAL 单例在 service::init 阶段已就位，此处注入弱引用
    aop::registry().register_consumer(Arc::new(lark_inbound::LarkInboundConsumer::new(
        Arc::downgrade(&crate::service::dal::lark::dal()),
    )))?;

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
    aop::registry().register_consumer(Arc::new(task_event_consumer::TaskEventConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(
        federation_directory::FederationDirectoryConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(
        federation_inbound_task::FederationInboundTaskConsumer::new(),
    ))?;
    aop::registry().register_consumer(Arc::new(
        federation_ws_outbound::FederationWsOutboundConsumer::new(),
    ))?;

    sys_info!("all business consumers registered");

    Ok(())
}

pub use aop_stats_collector::{
    AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint,
};
pub use aop_stats_hook::AopStatsHook;
