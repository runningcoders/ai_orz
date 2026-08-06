pub mod agent_loop_consumer;
pub mod aop_stats_collector;
pub mod aop_stats_hook;
pub mod message;
pub mod scheduler;
pub mod task_event_consumer;
pub mod think_round_stats_consumer;
pub mod tool_exec_log_consumer;
pub mod tool_exec_stats_consumer;

use common::error::Result;
use std::sync::Arc;

use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::domain::system;

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
    aop::registry().register_consumer(Arc::new(task_event_consumer::TaskEventConsumer::new()))?;

    sys_info!("all business consumers registered");

    // 创建系统级默认定时任务（agent_rest + project_followup），幂等：
    // 已有同 action 的触发器则跳过。失败仅记录日志，不影响启动。
    let ctx = RequestContext::new_system();
    if let Err(e) = system::ensure_system_cron_triggers(&ctx).await {
        sys_warn!("创建系统级定时任务失败: {}", e);
    }

    Ok(())
}

pub use aop_stats_collector::{
    AopDistributionItem, AopOverview, AopStatsCollector, AopTimeSeriesPoint,
};
pub use aop_stats_hook::AopStatsHook;
